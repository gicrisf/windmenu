use std::fs;
use std::env;
use std::thread;
use std::time;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;

use winapi::um::winuser::{
    GetAsyncKeyState, SendInput, 
    INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, 
    VK_LWIN, VK_SPACE, SW_RESTORE, VK_MENU, VK_SHIFT, VK_CAPITAL
};
use winapi::um::fileapi::{CreateFileW, WriteFile, ReadFile, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{GENERIC_READ, GENERIC_WRITE};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, 
    PROCESSENTRY32W, TH32CS_SNAPPROCESS
};
use winapi::um::winbase::{DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP};
use winapi::um::shellapi::ShellExecuteW;
use winapi::shared::minwindef::{DWORD, FALSE};

use serde::Deserialize;
use toml;

// Named pipe name for communicating with wlines daemon
const PIPE_NAME: &str = r"\\.\pipe\wlines_pipe";

fn launch_program(path: &Path) -> Result<(), String> {
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
            Err(format!("Failed to launch program: {}", result as usize))
        } else {
            Ok(())
        }
    }
}

fn launch_command(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("No command provided".to_string());
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
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}

#[derive(Debug)]
enum AppCommand {
    Start(PathBuf),            // For Start menu shortcuts
    Configured(Vec<String>),  // For configured commands
    KeyCombo(Vec<String>),    // For key combinations like ALT+X
    ToggleCapsLock,           // For caps lock toggle
}

#[derive(Debug, Deserialize)]
struct Config {
    options: WlinesConfig,
    commands: Vec<CommandConfig>,
}

#[derive(Debug, Deserialize)]
struct WlinesConfig {
    l: Option<usize>,       // Lines to show
    p: Option<String>,      // Prompt text
    fm: Option<String>,     // Filter mode
    si: Option<usize>,      // Selected index
    px: Option<usize>,      // Window padding
    wx: Option<usize>,      // Window width
    bg: Option<String>,     // Background color
    fg: Option<String>,     // Foreground color
    sbg: Option<String>,    // Selected bg color
    sfg: Option<String>,    // Selected fg color
    tbg: Option<String>,    // Text input bg
    tfg: Option<String>,    // Text input fg
    f: Option<String>,      // Font name
    fs: Option<usize>,      // Font size
}

impl WlinesConfig {
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        
        if let Some(l) = self.l {
            args.extend(["-l".to_string(), l.to_string()]);
        }
        if let Some(p) = &self.p {
            args.extend(["-p".to_string(), p.clone()]);
        }
        if let Some(fm) = &self.fm {
            args.extend(["-fm".to_string(), fm.clone()]);
        }
        if let Some(si) = self.si {
            args.extend(["-si".to_string(), si.to_string()]);
        }
        if let Some(px) = self.px {
            args.extend(["-px".to_string(), px.to_string()]);
        }
        if let Some(wx) = self.wx {
            args.extend(["-wx".to_string(), wx.to_string()]);
        }
        if let Some(bg) = &self.bg {
            args.extend(["-bg".to_string(), bg.clone()]);
        }
        if let Some(fg) = &self.fg {
            args.extend(["-fg".to_string(), fg.clone()]);
        }
        if let Some(sbg) = &self.sbg {
            args.extend(["-sbg".to_string(), sbg.clone()]);
        }
        if let Some(sfg) = &self.sfg {
            args.extend(["-sfg".to_string(), sfg.clone()]);
        }
        if let Some(tbg) = &self.tbg {
            args.extend(["-tbg".to_string(), tbg.clone()]);
        }
        if let Some(tfg) = &self.tfg {
            args.extend(["-tfg".to_string(), tfg.clone()]);
        }
        if let Some(f) = &self.f {
            args.extend(["-f".to_string(), f.clone()]);
        }
        if let Some(fs) = self.fs {
            args.extend(["-fs".to_string(), fs.to_string()]);
        }
        
        args
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

struct AppState {
    process_running: Mutex<bool>,
    commands: HashMap<String, AppCommand>,
    wlines_args: Vec<String>,
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

fn is_shortcut_pressed() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LWIN) & 0x8000u16 as i16 != 0) &&
        (GetAsyncKeyState(VK_SPACE) & 0x8000u16 as i16 != 0)
    }
}

fn load_config() -> Option<Config> {
    let config_path = PathBuf::from("config.toml");
    if !config_path.exists() {
        return None;
    }
    let config_content = fs::read_to_string(config_path).ok()?;
    toml::from_str(&config_content).ok()
}

fn initialize_app_state() -> AppState {
    let mut commands = HashMap::new();

    // Add Start menu commands
    for path in get_start_menu_paths() {
        if let Ok(lnk_files) = find_lnk_files(&path) {
            for (name, path) in lnk_files {
                commands.insert(name, AppCommand::Start(path));
            }
        }
    }

    // Load config and process commands/options
    let config = load_config();
    let wlines_args = config.as_ref().map_or(vec![], |c| c.options.to_args());
    
    if let Some(config) = config {
        for cmd in config.commands {
            let command = match cmd.command_type {
                CommandType::Args { args } => AppCommand::Configured(args),
                CommandType::Keys { keys } => AppCommand::KeyCombo(keys),
            };
            commands.insert(cmd.name, command);
        }
    }

    // Add built-in commands
    commands.insert("Toggle Caps Lock".to_string(), AppCommand::ToggleCapsLock);

    AppState {
        process_running: Mutex::new(false),
        commands,
        wlines_args,
    }
}

fn execute_wlines(state: Arc<AppState>) {
    // Check if already running
    {
        let running = state.process_running.lock().unwrap();
        if *running {
            return;
        }
    }

    // Prepare command list
    let joined = state.commands.keys()
        .fold(String::new(), |acc, s| acc + "\n" + s);

    // Check if daemon is already running
    if is_daemon_running() {
        println!("Daemon detected - attempting pipe communication...");
        if let Some(selected) = send_to_wlines_daemon(&joined) {
            // Execute the selected command
            match state.commands.get(&selected) {
                Some(AppCommand::Start(path)) => {
                    println!("Executing Start command: {}", selected);
                    if let Err(e) = launch_program(path) {
                        println!("Failed to launch program: {}", e);
                    }
                },
                Some(AppCommand::Configured(args)) => {
                    println!("Executing command: {}", selected);
                    if let Err(e) = launch_command(args) {
                        println!("Failed to execute command: {}", e);
                    }
                },
                Some(AppCommand::KeyCombo(keys)) => {
                    println!("Executing key combination: {}", selected);
                    if let Err(e) = send_key_combination(keys) {
                        println!("Failed to send key combination: {}", e);
                    }
                },
                Some(AppCommand::ToggleCapsLock) => {
                    println!("Toggling caps lock: {}", selected);
                    if let Err(e) = toggle_caps_lock() {
                        println!("Failed to toggle caps lock: {}", e);
                    }
                },
                None => {
                    println!("No command found for selection: '{}'", selected);
                }
            }
        } else {
            println!("Pipe communication failed!");
        }
        return;
    }

    // Set running flag
    *state.process_running.lock().unwrap() = true; 

    thread::spawn(move || {
        let output = Command::new("wlines.exe")
            .args(&state.wlines_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(); 

        if let Ok(mut child) = output {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(joined.as_bytes()).expect("Failed to write to stdin");
            }

            let output = child.wait_with_output().expect("Failed to read output");
            let selected = std::str::from_utf8(&output.stdout)
                .unwrap_or("")
                .trim();

            match state.commands.get(selected) {
                Some(AppCommand::Start(path)) => {
                    if let Err(e) = launch_program(path) {
                        println!("Failed to launch program: {}", e);
                    }
                },
                Some(AppCommand::Configured(args)) => {
                    if let Err(e) = launch_command(args) {
                        println!("Failed to execute command: {}", e);
                    }
                },
                Some(AppCommand::KeyCombo(keys)) => {
                    if let Err(e) = send_key_combination(keys) {
                        println!("Failed to send key combination: {}", e);
                    }
                },
                Some(AppCommand::ToggleCapsLock) => {
                    if let Err(e) = toggle_caps_lock() {
                        println!("Failed to toggle caps lock: {}", e);
                    }
                },
                None => {}
            }
        } 

        *state.process_running.lock().unwrap() = false;
    });
}

fn is_daemon_running() -> bool {
    // Use Windows API to check for the process without spawning console
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }

        let mut pe32 = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..std::mem::zeroed()
        };

        if Process32FirstW(snapshot, &mut pe32) != 0 {
            loop {
                let process_name = String::from_utf16_lossy(&pe32.szExeFile);
                
                if process_name.trim_end_matches('\0').eq_ignore_ascii_case("wlines-daemon.exe") {
                    CloseHandle(snapshot);
                    return true;
                }

                if Process32NextW(snapshot, &mut pe32) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        false
    }
}

fn send_to_wlines_daemon(data: &str) -> Option<String> {
    unsafe {
        // Convert pipe name to wide string
        let pipe_name_wide: Vec<u16> = OsStr::new(PIPE_NAME)
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
            return None;
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
            println!("Failed to write to pipe: error {}", GetLastError());
            CloseHandle(h_pipe);
            return None;
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
                return Some(response);
            }
        } else {
            println!("Failed to read from pipe: error {}", GetLastError());
        }
    }
    
    None
}

fn send_key_combination(keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("No keys provided".to_string());
    }

    // Parse virtual key codes from key names
    let vk_codes: Result<Vec<u16>, String> = keys.iter()
        .map(|key| match key.to_uppercase().as_str() {
            "ALT" => Ok(VK_MENU as u16),
            "CTRL" | "CONTROL" => Ok(0x11), // VK_CONTROL
            "SHIFT" => Ok(VK_SHIFT as u16),
            "WIN" | "WINDOWS" => Ok(0x5B), // VK_LWIN
            "TAB" => Ok(0x09), // VK_TAB
            "ESC" | "ESCAPE" => Ok(0x1B), // VK_ESCAPE
            "SPACE" => Ok(0x20), // VK_SPACE
            "ENTER" => Ok(0x0D), // VK_RETURN
            // Function keys
            "F1" => Ok(0x70), "F2" => Ok(0x71), "F3" => Ok(0x72), "F4" => Ok(0x73),
            "F5" => Ok(0x74), "F6" => Ok(0x75), "F7" => Ok(0x76), "F8" => Ok(0x77),
            "F9" => Ok(0x78), "F10" => Ok(0x79), "F11" => Ok(0x7A), "F12" => Ok(0x7B),
            // Arrow keys
            "LEFT" => Ok(0x25), "UP" => Ok(0x26), "RIGHT" => Ok(0x27), "DOWN" => Ok(0x28),
            // Number keys
            "0" => Ok(0x30), "1" => Ok(0x31), "2" => Ok(0x32), "3" => Ok(0x33), "4" => Ok(0x34),
            "5" => Ok(0x35), "6" => Ok(0x36), "7" => Ok(0x37), "8" => Ok(0x38), "9" => Ok(0x39),
            // Special punctuation keys
            "COMMA" | "," => Ok(0xBC), // VK_OEM_COMMA
            "PERIOD" | "." => Ok(0xBE), // VK_OEM_PERIOD
            "SEMICOLON" | ";" => Ok(0xBA), // VK_OEM_1
            "SLASH" | "/" => Ok(0xBF), // VK_OEM_2
            "BACKSLASH" | "\\" => Ok(0xDC), // VK_OEM_5
            "QUOTE" | "'" => Ok(0xDE), // VK_OEM_7
            "BACKTICK" | "`" => Ok(0xC0), // VK_OEM_3
            "MINUS" | "-" => Ok(0xBD), // VK_OEM_MINUS
            "EQUALS" | "PLUS" | "=" | "+" => Ok(0xBB), // VK_OEM_PLUS
            "LBRACKET" | "[" => Ok(0xDB), // VK_OEM_4
            "RBRACKET" | "]" => Ok(0xDD), // VK_OEM_6
            // Letter keys (A-Z)
            key if key.len() == 1 && key.chars().next().unwrap().is_ascii_alphabetic() => {
                Ok(key.chars().next().unwrap() as u16)
            },
            _ => Err(format!("Unknown key: {}", key)),
        })
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
            Err(format!("Failed to send key combination: error {}", GetLastError()))
        } else {
            Ok(())
        }
    }
}

fn toggle_caps_lock() -> Result<(), String> {
    unsafe {
        // Send a single VK_CAPITAL key press (down + up) to toggle caps lock
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
            return Err(format!("Failed to send caps lock down: {}", GetLastError()));
        }

        // Send key up
        input.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
        if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) == 0 {
            return Err(format!("Failed to send caps lock up: {}", GetLastError()));
        }

        println!("Caps lock toggled successfully");
        Ok(())
    }
}

fn main() {    
    let args: Vec<String> = env::args().collect();
    
    // Check if this is the detached background process
    if args.len() > 1 && args[1] == "--daemon" {
        // This is the background daemon process
        run_daemon();
        return;
    }
    
    // This is the initial process - spawn detached daemon and exit
    println!("Starting windmenu daemon...");
    
    let current_exe = env::current_exe().expect("Failed to get current executable path");
    
    let child = Command::new(&current_exe)
        .arg("--daemon")
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn daemon process");
    
    println!("Daemon started with PID: {}", child.id());
    println!("windmenu is now running in the background");
    println!("Press Win+Space to activate menu");
    println!("Use kill-windmenu.bat or kill-windmenu.ps1 to stop the daemon");
    
    // Exit the initial process, leaving the daemon running
}

fn run_daemon() {
    let state = Arc::new(initialize_app_state()); 
    
    loop {
        if is_shortcut_pressed() {
            execute_wlines(state.clone());
        } 
        thread::sleep(time::Duration::from_millis(50));
    }
}
