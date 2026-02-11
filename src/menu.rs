use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::io::Write;
use std::thread;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::fmt;
use serde::Deserialize;
use toml;
use winapi::um::fileapi::{CreateFileW, WriteFile, ReadFile, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{GENERIC_READ, GENERIC_WRITE};
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
use winapi::shared::minwindef::{DWORD, FALSE};

use crate::apps::{find_reparse_points, get_windows_apps_path};
use crate::daemon::{WlinesDaemon, Daemon};
use crate::theme::WlinesTheme;
use crate::wlan;

#[derive(Debug)]
pub enum MenuError {
    ConfigLoad(String),
    PipeConnection(u32),
    PipeWrite(u32),
    PipeRead(u32),
    CommandExecution(String),
    KeyParsing(String),
    KeyInput(u32),
    ProcessSpawn(std::io::Error),
    InvalidArguments(String),
    WindowsApi(u32),
    MenuAlreadyRunning,
    DaemonCommunicationFailed(String),
    DirectExecutionFailed(String),
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MenuError::ConfigLoad(msg) => write!(f, "Failed to load configuration: {}", msg),
            MenuError::PipeConnection(error) => write!(f, "Failed to connect to pipe: error {}", error),
            MenuError::PipeWrite(error) => write!(f, "Failed to write to pipe: error {}", error),
            MenuError::PipeRead(error) => write!(f, "Failed to read from pipe: error {}", error),
            MenuError::CommandExecution(msg) => write!(f, "Failed to execute command: {}", msg),
            MenuError::KeyParsing(key) => write!(f, "Unknown key: {}", key),
            MenuError::KeyInput(error) => write!(f, "Failed to send key input: error {}", error),
            MenuError::ProcessSpawn(error) => write!(f, "Failed to spawn process: {}", error),
            MenuError::InvalidArguments(msg) => write!(f, "Invalid arguments: {}", msg),
            MenuError::WindowsApi(error) => write!(f, "Windows API error: {}", error),
            MenuError::MenuAlreadyRunning => write!(f, "Menu is already running"),
            MenuError::DaemonCommunicationFailed(msg) => write!(f, "Daemon communication failed: {}", msg),
            MenuError::DirectExecutionFailed(msg) => write!(f, "Direct execution failed: {}", msg),
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
    pub wlines_args: Vec<String>,
    pub hotkey: Hotkey,
    pub wlines_cli_path: Option<PathBuf>,
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
        let mut wlines_args = vec![];
        let mut wlines_cli_path = None;

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

        if let Some((cfg, config_dir)) = config {
            // Update wlines args from theme
            if let Some(ref theme) = &cfg.theme {
                wlines_args = theme.to_args();
                // Generate wlines daemon config next to the loaded config file
                let wlines_config_path = config_dir.join("wlines-config.txt");
                if let Err(e) = WlinesTheme::generate_wlines_config(&wlines_config_path, Some(theme)) {
                    eprintln!("Warning: Failed to generate wlines config: {}", e);
                }
            }
            // Update wlines CLI path
            if let Some(ref path) = &cfg.wlines_cli_path {
                wlines_cli_path = Some(PathBuf::from(path));
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
            wlines_args,
            hotkey,
            wlines_cli_path,
        }
    }

    pub fn show(self: Arc<Self>, wlines_daemon: &WlinesDaemon) -> Result<(), MenuError> {
        // Check if already running
        {
            let running = self.process_running.lock().unwrap();
            if *running {
                return Err(MenuError::MenuAlreadyRunning);
            }
        }

        // Prepare command list
        let command_list = self.prepare_command_list();

        if wlines_daemon.is_running() {
            // Using preferred method: Daemon communication via named pipe
            self.show_via_daemon(&command_list)
            // You could also:
            // self.show_via_daemon(&command_list).or_else(|daemon_error| { ... })
            // To raise a windows dialog and alert the user in this case
        } else if self.wlines_cli_path.is_some() {
            // Using fallback method: Direct wlines.exe execution
            self.show_via_direct_execution(&command_list)
        } else {
            // No daemon running and no CLI path configured
            Err(MenuError::DirectExecutionFailed(
                "Wlines daemon is not running and no CLI path is configured. Either start the wlines daemon or configure 'wlines_cli_path' in windmenu.toml".to_string()
            ))
        }
    }

    fn prepare_command_list(&self) -> String {
        self.commands.keys()
                     .fold(String::new(), |acc, s| {
                         if acc.is_empty() { s.to_string() } else { acc + "\n" + s }
                     })
    }

    fn show_via_daemon(&self, command_list: &str) -> Result<(), MenuError> {
        match Self::send_to_wlines_daemon(command_list) {
            Ok(selected) => {
                self.execute_command(&selected)
            }
            Err(e) => {
                Err(MenuError::DaemonCommunicationFailed(e.to_string()))
            }
        }
    }

    fn show_via_direct_execution(self: Arc<Self>, command_list: &str) -> Result<(), MenuError> {
        // Clone before spawning a new thread
        let command_list = command_list.to_string();

        // Set running flag
        *self.process_running.lock().unwrap() = true;

        thread::spawn(move || {
            let result = (|| -> Result<(), MenuError> {
                let mut child = Command::new(self.wlines_cli_path.as_ref().unwrap())
                    .args(&self.wlines_args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| MenuError::DirectExecutionFailed(format!("Failed to spawn {}: {}", self.wlines_cli_path.as_ref().unwrap().display(), e)))?;

                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(command_list.as_bytes())
                        .map_err(|e| MenuError::DirectExecutionFailed(format!("Failed to write to stdin: {}", e)))?;
                }

                let output = child.wait_with_output()
                    .map_err(|e| MenuError::DirectExecutionFailed(format!("Failed to read output: {}", e)))?;

                let selected = std::str::from_utf8(&output.stdout)
                    .unwrap_or("")
                    .trim();

                self.execute_command(selected)
            })();

            // Always reset the running flag, regardless of success or failure
            *self.process_running.lock().unwrap() = false;

            // Log any errors that occurred
            if let Err(e) = result {
                eprintln!("Direct execution error: {}", e);
            }
        });

        Ok(())
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

    fn send_to_wlines_daemon(data: &str) -> Result<String, MenuError> {
        unsafe {
            // Convert pipe name to wide string
            let pipe_name_wide: Vec<u16> = OsStr::new(WlinesDaemon::PIPE_NAME)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            // Create file handle for the named pipe with read/write access
            let h_pipe = CreateFileW(
                pipe_name_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );

            if h_pipe == INVALID_HANDLE_VALUE {
                let error = GetLastError();
                println!("Failed to connect to pipe: error {}", error);
                return Err(MenuError::PipeConnection(error));
            }

            // Write data to pipe
            let data_bytes = data.as_bytes();
            let mut bytes_written: DWORD = 0;

            let write_success = WriteFile(
                h_pipe,
                data_bytes.as_ptr() as *const _,
                data_bytes.len() as DWORD,
                &mut bytes_written,
                std::ptr::null_mut(),
            );

            if write_success == FALSE {
                let error = GetLastError();
                println!("Failed to write to pipe: error {}", error);
                CloseHandle(h_pipe);
                return Err(MenuError::PipeWrite(error));
            }

            println!("Sent {} bytes to daemon", bytes_written);

            // Read response from pipe
            const BUFFER_SIZE: usize = 1024;
            let mut buffer = vec![0u8; BUFFER_SIZE];
            let mut bytes_read: DWORD = 0;

            let read_success = ReadFile(
                h_pipe,
                buffer.as_mut_ptr() as *mut _,
                BUFFER_SIZE as DWORD,
                &mut bytes_read,
                std::ptr::null_mut(),
            );

            CloseHandle(h_pipe);

            if read_success != FALSE && bytes_read > 0 {
                // Convert bytes to string, trimming null bytes and whitespace
                let response = String::from_utf8_lossy(&buffer[..bytes_read as usize])
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();

                if !response.is_empty() {
                    println!("Received: '{}'", response);
                    return Ok(response);
                }
            } else {
                let error = GetLastError();
                println!("Failed to read from pipe: error {}", error);
                return Err(MenuError::PipeRead(error));
            }
        }

        Err(MenuError::PipeRead(0))
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
