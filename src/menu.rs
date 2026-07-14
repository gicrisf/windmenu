use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::process::{Command, Stdio};
use std::thread;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::fmt;
use serde::Deserialize;
use toml;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::shellapi::ShellExecuteW;
use winapi::um::winuser::{
    DispatchMessageW, MessageBoxW, MsgWaitForMultipleObjects, PeekMessageW, RegisterHotKey,
    SendInput, TranslateMessage, INPUT, INPUT_KEYBOARD, MB_ICONERROR, MB_OK, MOD_ALT,
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, MSG, PM_REMOVE, QS_ALLINPUT,
    SW_RESTORE, WM_HOTKEY, WM_QUIT, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_MENU, VK_SHIFT, VK_CAPITAL, VK_CONTROL, VK_TAB, VK_ESCAPE, VK_LWIN, VK_SPACE,
    VK_RETURN, VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN, VK_F1, VK_F2, VK_F3, VK_F4, VK_F5,
    VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_OEM_COMMA, VK_OEM_PERIOD,
    VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7,
    VK_OEM_MINUS, VK_OEM_PLUS
};
use winapi::um::winbase::{CREATE_NEW_PROCESS_GROUP, INFINITE, WAIT_OBJECT_0};
use winapi::um::handleapi::CloseHandle;
use winapi::um::synchapi::CreateEventW;

use crate::apps::{find_reparse_points, get_windows_apps_path};
use crate::theme::WlinesTheme;
use crate::wlines;

#[derive(Debug)]
pub enum MenuError {
    ConfigLoad(String),
    CommandExecution(String),
    KeyParsing(String),
    KeyInput(u32),
    ProcessSpawn(std::io::Error),
    InvalidArguments(String),
    WindowsApi(u32),
    MenuAlreadyRunning,
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MenuError::ConfigLoad(msg) => write!(f, "Failed to load configuration: {}", msg),
            MenuError::CommandExecution(msg) => write!(f, "Failed to execute command: {}", msg),
            MenuError::KeyParsing(key) => write!(f, "Unknown key: {}", key),
            MenuError::KeyInput(error) => write!(f, "Failed to send key input: error {}", error),
            MenuError::ProcessSpawn(error) => write!(f, "Failed to spawn process: {}", error),
            MenuError::InvalidArguments(msg) => write!(f, "Invalid arguments: {}", msg),
            MenuError::WindowsApi(error) => write!(f, "Windows API error: {}", error),
            MenuError::MenuAlreadyRunning => write!(f, "Menu is already running"),
        }
    }
}

impl std::error::Error for MenuError {}

#[derive(Debug, Clone)]
pub enum MenuCommand {
    Start(PathBuf),            // For Start menu shortcuts
    Configured(Vec<String>),  // For configured commands
    KeyCombo(Vec<String>),    // For key combinations like ALT+X
    ToggleCapsLock,           // For caps lock toggle
    RefreshApps,              // Rescan Start Menu and Windows Store apps
    ReloadConfig,             // Reload commands from windmenu.toml
}

pub(crate) struct EntryStore {
    builtins: HashMap<String, MenuCommand>,
    config: HashMap<String, MenuCommand>,
    dynamic: HashMap<String, MenuCommand>,
}

impl EntryStore {
    fn empty() -> Self {
        let mut builtins = HashMap::new();
        builtins.insert("Toggle Caps Lock".to_string(), MenuCommand::ToggleCapsLock);
        builtins.insert("Refresh Apps".to_string(), MenuCommand::RefreshApps);
        builtins.insert("Reload Config".to_string(), MenuCommand::ReloadConfig);
        Self { builtins, config: HashMap::new(), dynamic: HashMap::new() }
    }

    fn apply_config_commands(&mut self, cmds: Vec<CommandConfig>) {
        let mut config = HashMap::new();
        for cmd in cmds {
            let (key, command) = match cmd.command_type {
                CommandType::Args { args } => (cmd.name, MenuCommand::Configured(args)),
                CommandType::Keys { keys } => {
                    let key_sequence = keys.join(", ");
                    let formatted_key = format!("{} [{}]", cmd.name, key_sequence);
                    (formatted_key, MenuCommand::KeyCombo(keys))
                }
            };
            config.insert(key, command);
        }
        self.config = config;
    }

    pub(crate) fn reload_config(&mut self) {
        if let Some((cfg, _)) = MenuConfig::load().ok() {
            if let Some(cmds) = cfg.commands {
                self.apply_config_commands(cmds);
            } else {
                self.config.clear();
            }
        }
    }

    pub(crate) fn rescan_dynamic(&mut self) {
        let mut dynamic = HashMap::new();

        for path in get_start_menu_paths() {
            if let Ok(lnk_files) = find_lnk_files(&path) {
                for (name, path) in lnk_files {
                    dynamic.insert(name, MenuCommand::Start(path));
                }
            }
        }

        if let Some(windows_apps_path) = get_windows_apps_path() {
            if let Ok(reparse_points) = find_reparse_points(&windows_apps_path) {
                for rp in reparse_points {
                    let command_name = if let Some(stem) = rp.full_path.file_stem().and_then(|s| s.to_str()) {
                        stem.to_string()
                    } else {
                        rp.name.clone()
                    };
                    dynamic.insert(command_name, MenuCommand::Start(rp.full_path));
                }
            }
        }

        self.dynamic = dynamic;
    }

    fn all_entries(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.builtins.keys()
            .chain(self.config.keys())
            .chain(self.dynamic.keys())
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn get(&self, name: &str) -> Option<&MenuCommand> {
        self.builtins.get(name)
            .or_else(|| self.config.get(name))
            .or_else(|| self.dynamic.get(name))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hotkey {
    keys: Vec<String>,
}

impl Hotkey {
    const HOTKEY_ID: i32 = 1;

    /// Map the configured keys to a RegisterHotKey (modifiers, vk) pair.
    /// Valid combos are any number of modifiers (WIN/CTRL/ALT/SHIFT) plus
    /// exactly one other key.
    fn to_registration(&self) -> Result<(u32, u32), MenuError> {
        let mut modifiers = MOD_NOREPEAT;
        let mut vk: Option<u32> = None;

        for key in &self.keys {
            match key.to_uppercase().as_str() {
                "WIN" | "WINDOWS" => modifiers |= MOD_WIN,
                "CTRL" | "CONTROL" => modifiers |= MOD_CONTROL,
                "ALT" => modifiers |= MOD_ALT,
                "SHIFT" => modifiers |= MOD_SHIFT,
                _ => {
                    let code = Menu::parse_key_name_to_vk_code(key)?;
                    if vk.is_some() {
                        return Err(MenuError::InvalidArguments(format!(
                            "hotkey {:?} has more than one non-modifier key; \
                             use any number of WIN/CTRL/ALT/SHIFT plus exactly one other key",
                            self.keys
                        )));
                    }
                    vk = Some(code as u32);
                }
            }
        }

        match vk {
            Some(vk) => Ok((modifiers as u32, vk)),
            None => Err(MenuError::InvalidArguments(format!(
                "hotkey {:?} has no non-modifier key; \
                 use any number of WIN/CTRL/ALT/SHIFT plus exactly one other key",
                self.keys
            ))),
        }
    }

    /// Wait for hotkey activations via RegisterHotKey, invoking `callback`
    /// on each WM_HOTKEY. Event-driven: the thread sleeps in GetMessageW,
    /// so idle CPU cost is zero and presses can't be missed or repeated
    /// (MOD_NOREPEAT). Fatal errors are shown in a message box because the
    /// detached daemon has no visible stderr.
    pub fn listen<F>(&self, mut callback: F)
    where
        F: FnMut(),
    {
        let (modifiers, vk) = self.to_registration().unwrap_or_else(|e| {
            Self::fatal(&format!("Invalid hotkey configuration: {}", e));
        });

        unsafe {
            if RegisterHotKey(std::ptr::null_mut(), Self::HOTKEY_ID, modifiers, vk) == 0 {
                Self::fatal(&format!(
                    "Failed to register hotkey {} (error {}). \
                     Another application may already use this combo; \
                     change 'hotkey' in windmenu.toml.",
                    self.keys.join("+"),
                    GetLastError()
                ));
            }

            let event_name: Vec<u16> = "windmenu-shutdown-event\0".encode_utf16().collect();
            let shutdown_event = CreateEventW(
                std::ptr::null_mut(),
                1, // manual-reset
                0, // initially non-signaled
                event_name.as_ptr(),
            );
            if shutdown_event.is_null() {
                Self::fatal("Failed to create shutdown event");
            }

            println!("Hotkey registered ({})", self.keys.join("+"));
            let mut msg: MSG = std::mem::zeroed();
            loop {
                let result = MsgWaitForMultipleObjects(
                    1,
                    &shutdown_event,
                    0,
                    INFINITE,
                    QS_ALLINPUT,
                );

                if result == WAIT_OBJECT_0 {
                    break;
                }

                if result == WAIT_OBJECT_0 + 1 {
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_QUIT {
                            break;
                        }
                        if msg.message == WM_HOTKEY && msg.wParam == Self::HOTKEY_ID as usize {
                            callback();
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                } else {
                    break;
                }
            }

            CloseHandle(shutdown_event);
        }
    }

    /// Report a fatal daemon error and exit. Uses a message box since the
    /// detached daemon process has stdout/stderr redirected to null.
    fn fatal(message: &str) -> ! {
        eprintln!("{}", message);
        unsafe {
            let text: Vec<u16> = OsStr::new(message).encode_wide().chain(std::iter::once(0)).collect();
            let caption: Vec<u16> = OsStr::new("windmenu").encode_wide().chain(std::iter::once(0)).collect();
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
        }
        std::process::exit(1);
    }
}

#[derive(Debug, Deserialize)]
struct MenuConfig {
    theme: Option<WlinesTheme>,
    commands: Option<Vec<CommandConfig>>,
    hotkey: Option<Vec<String>>, // Custom hotkey keys (e.g., ["WIN", "SPACE"])
}

impl MenuConfig {
    const DEFAULT_CONFIG_PATH: &'static str = "windmenu.toml";

    fn load_from_file(config_path: &Path) -> Result<MenuConfig, MenuError> {
        let config_content = fs::read_to_string(config_path)
            .map_err(|e| MenuError::ConfigLoad(format!("Failed to read config file: {}", e)))?;
        let config: MenuConfig = toml::from_str(&config_content)
            .map_err(|e| MenuError::ConfigLoad(format!("Failed to parse TOML: {}", e)))?;
        Ok(config)
    }

    fn load() -> Result<(MenuConfig, PathBuf), MenuError> {
        // Try CWD first (portable installs)
        let cwd_path = Path::new(Self::DEFAULT_CONFIG_PATH);
        if cwd_path.exists() {
            let config = Self::load_from_file(cwd_path)?;
            let config_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            return Ok((config, config_dir));
        }
        // Fall back to executable's directory (Scoop installs)
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let exe_config = exe_dir.join(Self::DEFAULT_CONFIG_PATH);
                if exe_config.exists() {
                    let config = Self::load_from_file(&exe_config)?;
                    return Ok((config, exe_dir.to_path_buf()));
                }
            }
        }
        // Neither found — return the CWD error for backward-compatible messaging
        let config = Self::load_from_file(cwd_path)?;
        Ok((config, env::current_dir().unwrap_or_else(|_| PathBuf::from("."))))
    }
}

pub fn print_config_debug() {
    let exe_path = env::current_exe().ok();
    println!("Exe path: {}", exe_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unknown".into()));
    println!("CWD: {}", env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "unknown".into()));

    let cwd_config = Path::new(MenuConfig::DEFAULT_CONFIG_PATH);
    println!("CWD config ({}): {}", cwd_config.display(), if cwd_config.exists() { "found" } else { "not found" });

    if let Some(exe_dir) = exe_path.as_ref().and_then(|p| p.parent()) {
        let exe_config = exe_dir.join(MenuConfig::DEFAULT_CONFIG_PATH);
        println!("Exe config ({}): {}", exe_config.display(), if exe_config.exists() { "found" } else { "not found" });
    }

    match MenuConfig::load() {
        Ok((_, config_dir)) => println!("Result: loaded from {}", config_dir.display()),
        Err(e) => println!("Result: {}", e),
    }
}


#[derive(Debug, Deserialize)]
struct CommandConfig {
    name: String,
    #[serde(flatten)]
    command_type: CommandType,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandType {
    Args { args: Vec<String> },
    Keys { keys: Vec<String> },
}

pub struct Menu {
    pub process_running: Mutex<bool>,
    pub entries: Arc<RwLock<EntryStore>>,
    pub settings: wlines::Settings,
    pub hotkey: Hotkey,
}

fn get_start_menu_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(appdata) = env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu"));
    }

    if let Ok(program_data) = env::var("ProgramData") {
        paths.push(PathBuf::from(program_data).join("Microsoft\\Windows\\Start Menu"));
    }

    paths
}

fn find_lnk_files(dir: &Path) -> std::io::Result<HashMap<String, PathBuf>> {
    let mut lnk_files = HashMap::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            lnk_files.extend(find_lnk_files(&path)?);
        } else if path.extension().map_or(false, |ext| ext == "lnk") {
            if let Some(file_name) = path.file_stem().and_then(|n| n.to_str()) {
                lnk_files.insert(file_name.to_lowercase().to_string(), path);
            }
        }
    }

    Ok(lnk_files)
}


impl Menu {
    pub fn new() -> Menu {
        let process_running = Mutex::new(false);

        let mut hotkey = Hotkey {
            keys: vec!["WIN".to_string(), "SPACE".to_string()],
        };
        let mut settings = WlinesTheme::default().to_settings();
        let entries = Arc::new(RwLock::new(EntryStore::empty()));

        if let Some((cfg, _config_dir)) = MenuConfig::load().ok() {
            if let Some(ref theme) = &cfg.theme {
                settings = theme.to_settings();
            }
            if let Some(ref keys) = &cfg.hotkey {
                hotkey.keys = keys.clone();
            }
            if let Some(cmds) = cfg.commands {
                entries.write().unwrap().apply_config_commands(cmds);
            }
        }

        Menu {
            process_running,
            entries,
            settings,
            hotkey,
        }
    }

    pub fn show(self: Arc<Self>) -> Result<(), MenuError> {
        // Check and set the running flag atomically
        {
            let mut running = self.process_running.lock().unwrap();
            if *running {
                return Err(MenuError::MenuAlreadyRunning);
            }
            *running = true;
        }

        let entries = self.prepare_entries();

        // Run the menu window and its message loop on a dedicated thread
        thread::spawn(move || {
            let result = match wlines::show(&self.settings, &entries) {
                Some(selected) => self.execute_command(&selected),
                None => Ok(()), // User cancelled
            };

            // Always reset the running flag, regardless of success or failure
            *self.process_running.lock().unwrap() = false;

            // Log any errors that occurred
            if let Err(e) = result {
                eprintln!("Menu error: {}", e);
            }
        });

        Ok(())
    }

    fn prepare_entries(&self) -> Vec<String> {
        self.entries.read().unwrap().all_entries()
    }

    fn execute_command(&self, selected: &str) -> Result<(), MenuError> {
        let cmd = {
            self.entries.read().unwrap().get(selected).cloned()
        };

        match cmd {
            Some(MenuCommand::Start(path)) => {
                println!("Executing Start command: {}", selected);
                Self::launch_program(&path)
            },
            Some(MenuCommand::Configured(args)) => {
                println!("Executing command: {}", selected);
                Self::launch_command(&args)
            },
            Some(MenuCommand::KeyCombo(keys)) => {
                println!("Executing key combination: {}", selected);
                Self::send_key_combination(&keys)
            },
            Some(MenuCommand::ToggleCapsLock) => {
                println!("Toggling caps lock: {}", selected);
                Self::toggle_caps_lock()
            },
            Some(MenuCommand::RefreshApps) => {
                let entries = self.entries.clone();
                thread::spawn(move || {
                    entries.write().unwrap().rescan_dynamic();
                });
                Ok(())
            },
            Some(MenuCommand::ReloadConfig) => {
                let entries = self.entries.clone();
                thread::spawn(move || {
                    entries.write().unwrap().reload_config();
                });
                Ok(())
            },
            None => {
                if !selected.is_empty() {
                    Err(MenuError::CommandExecution(format!("No command found for selection: '{}'", selected)))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn launch_program(path: &Path) -> Result<(), MenuError> {
        unsafe {
            // Convert path to wide string
            let path_wide: Vec<u16> = path.as_os_str()
                                          .encode_wide()
                                          .chain(std::iter::once(0))
                                          .collect();

            // Use ShellExecuteW to launch the program - SW_RESTORE brings window to foreground
            let result = ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_RESTORE,  // SW_RESTORE is more likely to bring window to foreground than SW_SHOWNORMAL
            );

            if result as usize <= 32 {
                Err(MenuError::WindowsApi(result as u32))
            } else {
                Ok(())
            }
        }
    }

    fn launch_command(args: &[String]) -> Result<(), MenuError> {
        if args.is_empty() {
            return Err(MenuError::InvalidArguments("No command provided".to_string()));
        }

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        // Hide console window for these commands
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(MenuError::ProcessSpawn(e)),
        }
    }

    fn send_key_combination(keys: &[String]) -> Result<(), MenuError> {
        if keys.is_empty() {
            return Err(MenuError::InvalidArguments("No keys provided".to_string()));
        }

        // Parse virtual key codes from key names
        let vk_codes: Result<Vec<u16>, MenuError> = keys.iter()
                                                     .map(|key| Self::parse_key_name_to_vk_code(key))
                                                     .collect();

        let vk_codes = vk_codes?;

        unsafe {
            let mut inputs = Vec::new();

            // Press all keys down
            for &vk_code in &vk_codes {
                let mut input: INPUT = std::mem::zeroed();
                input.type_ = INPUT_KEYBOARD;
                *input.u.ki_mut() = KEYBDINPUT {
                    wVk: vk_code,
                    wScan: 0,
                    dwFlags: 0, // Key down
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            // Release all keys up (in reverse order)
            for &vk_code in vk_codes.iter().rev() {
                let mut input: INPUT = std::mem::zeroed();
                input.type_ = INPUT_KEYBOARD;
                *input.u.ki_mut() = KEYBDINPUT {
                    wVk: vk_code,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            // Send all inputs
            let sent = SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );

            if sent == 0 {
                Err(MenuError::KeyInput(GetLastError()))
            } else {
                Ok(())
            }
        }
    }

    fn toggle_caps_lock() -> Result<(), MenuError> {
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.type_ = INPUT_KEYBOARD;
            *input.u.ki_mut() = KEYBDINPUT {
                wVk: VK_CAPITAL as u16,
                wScan: 0,
                dwFlags: 0, // Key down
                time: 0,
                dwExtraInfo: 0,
            };

            // Send key down
            if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) == 0 {
                return Err(MenuError::KeyInput(GetLastError()));
            }

            // Send key up
            input.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
            if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) == 0 {
                return Err(MenuError::KeyInput(GetLastError()));
            }

            println!("Caps lock toggled successfully");
            Ok(())
        }
    }

    fn parse_key_name_to_vk_code(key: &str) -> Result<u16, MenuError> {
        match key.to_uppercase().as_str() {
            "ALT" => Ok(VK_MENU as u16),
            "CTRL" | "CONTROL" => Ok(VK_CONTROL as u16),
            "SHIFT" => Ok(VK_SHIFT as u16),
            "WIN" | "WINDOWS" => Ok(VK_LWIN as u16),
            "TAB" => Ok(VK_TAB as u16),
            "ESC" | "ESCAPE" => Ok(VK_ESCAPE as u16),
            "SPACE" => Ok(VK_SPACE as u16),
            "ENTER" => Ok(VK_RETURN as u16),
            "CAPS" => Ok(VK_CAPITAL as u16),
            // Function keys
            "F1" => Ok(VK_F1 as u16), "F2" => Ok(VK_F2 as u16), "F3" => Ok(VK_F3 as u16), "F4" => Ok(VK_F4 as u16),
            "F5" => Ok(VK_F5 as u16), "F6" => Ok(VK_F6 as u16), "F7" => Ok(VK_F7 as u16), "F8" => Ok(VK_F8 as u16),
            "F9" => Ok(VK_F9 as u16), "F10" => Ok(VK_F10 as u16), "F11" => Ok(VK_F11 as u16), "F12" => Ok(VK_F12 as u16),
            // Arrow keys
            "LEFT" => Ok(VK_LEFT as u16), "UP" => Ok(VK_UP as u16), "RIGHT" => Ok(VK_RIGHT as u16), "DOWN" => Ok(VK_DOWN as u16),
            // Number keys
            "0" => Ok(0x30), "1" => Ok(0x31), "2" => Ok(0x32), "3" => Ok(0x33), "4" => Ok(0x34),
            "5" => Ok(0x35), "6" => Ok(0x36), "7" => Ok(0x37), "8" => Ok(0x38), "9" => Ok(0x39),
            // Special punctuation keys
            "COMMA" | "," => Ok(VK_OEM_COMMA as u16),
            "PERIOD" | "." => Ok(VK_OEM_PERIOD as u16),
            "SEMICOLON" | ";" => Ok(VK_OEM_1 as u16),
            "SLASH" | "/" => Ok(VK_OEM_2 as u16),
            "BACKSLASH" | "\\" => Ok(VK_OEM_5 as u16),
            "QUOTE" | "'" => Ok(VK_OEM_7 as u16),
            "BACKTICK" | "`" => Ok(VK_OEM_3 as u16),
            "MINUS" | "-" => Ok(VK_OEM_MINUS as u16),
            "EQUALS" | "PLUS" | "=" | "+" => Ok(VK_OEM_PLUS as u16),
            "LBRACKET" | "[" => Ok(VK_OEM_4 as u16),
            "RBRACKET" | "]" => Ok(VK_OEM_6 as u16),
            // Letter keys (A-Z)
            key if key.len() == 1 && key.chars().next().unwrap().is_ascii_alphabetic() => {
                Ok(key.chars().next().unwrap() as u16)
            },
            _ => Err(MenuError::KeyParsing(key.to_string())),
        }
    }
}
