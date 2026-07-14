use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::thread;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::fmt;
use serde::Deserialize;
use toml;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::shellapi::ShellExecuteW;
use winapi::um::winuser::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, SW_RESTORE,
    KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU, VK_SHIFT, VK_CAPITAL, VK_CONTROL,
    VK_TAB, VK_ESCAPE, VK_LWIN, VK_SPACE, VK_RETURN, VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN,
    VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12,
    VK_OEM_COMMA, VK_OEM_PERIOD, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7,
    VK_OEM_MINUS, VK_OEM_PLUS
};
use winapi::um::winbase::CREATE_NEW_PROCESS_GROUP;

use crate::apps::{find_reparse_points, get_windows_apps_path};
use crate::theme::WlinesTheme;
use crate::wlan;
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

#[derive(Debug)]
pub enum MenuCommand {
    Start(PathBuf),            // For Start menu shortcuts
    Configured(Vec<String>),  // For configured commands
    KeyCombo(Vec<String>),    // For key combinations like ALT+X
    ToggleCapsLock,           // For caps lock toggle
    WlanScan,                 // For WLAN network scan
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hotkey {
    keys: Vec<String>,
    poll_interval: u64, // Polling interval in milliseconds
}

impl Hotkey {
    fn is_pressed(&self) -> bool {
        unsafe {
            self.keys.iter().all(|key| {
                if let Ok(vk_code) = Menu::parse_key_name_to_vk_code(key) {
                    GetAsyncKeyState(vk_code as i32) & 0x8000u16 as i16 != 0
                } else {
                    false // If we can't parse the key, consider it not pressed
                }
            })
        }
    }

    pub fn poll<F>(&self, mut callback: F) -> !
    where
        F: FnMut(),
    {
        loop {
            if self.is_pressed() {
                callback();
            }
            thread::sleep(std::time::Duration::from_millis(self.poll_interval));
        }
    }
}

#[derive(Debug, Deserialize)]
struct MenuConfig {
    theme: Option<WlinesTheme>,
    commands: Option<Vec<CommandConfig>>,
    hotkey: Option<Vec<String>>, // Custom hotkey keys (e.g., ["WIN", "SPACE"])
    wlines_cli_path: Option<String>, // Path to wlines.exe for direct execution
    hotkey_poll_interval: Option<u64>, // Hotkey polling interval in milliseconds
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
    pub commands: HashMap<String, MenuCommand>,
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
            poll_interval: 50, // 50ms
        };
        let mut commands = HashMap::new();
        let mut settings = WlinesTheme::default().to_settings();

        // Add Start menu commands
        for path in get_start_menu_paths() {
            if let Ok(lnk_files) = find_lnk_files(&path) {
                for (name, path) in lnk_files {
                    commands.insert(name, MenuCommand::Start(path));
                }
            }
        }

        // Add Windows Apps reparse points
        if let Some(windows_apps_path) = get_windows_apps_path() {
            if let Ok(reparse_points) = find_reparse_points(&windows_apps_path) {
                for rp in reparse_points {
                    // Use the reparse point name without extension for the command key
                    let command_name = if let Some(stem) = rp.full_path.file_stem().and_then(|s| s.to_str()) {
                        stem.to_string()
                    } else {
                        rp.name.clone()
                    };
                    commands.insert(command_name, MenuCommand::Start(rp.full_path));
                }
            }
        }

        // Load config and process commands/options
        let config = MenuConfig::load().ok();

        if let Some((cfg, _config_dir)) = config {
            // Convert theme to renderer settings
            if let Some(ref theme) = &cfg.theme {
                settings = theme.to_settings();
            }
            // wlines_cli_path is obsolete: the renderer is now built in
            if cfg.wlines_cli_path.is_some() {
                eprintln!("Warning: 'wlines_cli_path' in windmenu.toml is no longer used; the menu renderer is built into windmenu");
            }
            // Update hotkey keys from config
            if let Some(ref keys) = &cfg.hotkey {
                hotkey.keys = keys.clone();
            }
            // Update poll interval from config
            if let Some(interval) = &cfg.hotkey_poll_interval {
                hotkey.poll_interval = *interval;
            }
            // Key commands
            if let Some(config_commands) = cfg.commands {
                for cmd in config_commands {
                let (key, command) = match cmd.command_type {
                    CommandType::Args { args } => (cmd.name, MenuCommand::Configured(args)),
                    CommandType::Keys { keys } => {
                        let key_sequence = keys.join(", ");
                        let formatted_key = format!("{} [{}]", cmd.name, key_sequence);
                        (formatted_key, MenuCommand::KeyCombo(keys))
                    }
                };
                commands.insert(key, command);
                }
            }
        }

        // Add built-in commands
        commands.insert("Toggle Caps Lock".to_string(), MenuCommand::ToggleCapsLock);
        commands.insert("WLAN Scan".to_string(), MenuCommand::WlanScan);

        Menu {
            process_running,
            commands,
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
        self.commands.keys().cloned().collect()
    }

    fn execute_command(&self, selected: &str) -> Result<(), MenuError> {
        match self.commands.get(selected) {
            Some(MenuCommand::Start(path)) => {
                println!("Executing Start command: {}", selected);
                Self::launch_program(path)
            },
            Some(MenuCommand::Configured(args)) => {
                println!("Executing command: {}", selected);
                Self::launch_command(args)
            },
            Some(MenuCommand::KeyCombo(keys)) => {
                println!("Executing key combination: {}", selected);
                Self::send_key_combination(keys)
            },
            Some(MenuCommand::ToggleCapsLock) => {
                println!("Toggling caps lock: {}", selected);
                Self::toggle_caps_lock()
            },
            Some(MenuCommand::WlanScan) => {
                println!("Performing WLAN scan: {}", selected);
                Self::perform_wlan_scan()
            },
            None => {
                if !selected.is_empty() {
                    Err(MenuError::CommandExecution(format!("No command found for selection: '{}'", selected)))
                } else {
                    Ok(()) // Empty selection is fine, user probably cancelled
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

    fn perform_wlan_scan() -> Result<(), MenuError> {
        println!("Starting WLAN scan...");

        // Run the scan in a separate thread to avoid blocking
        thread::spawn(|| {
            wlan::test_wlan_scan();
        });

        Ok(())
    }
}
