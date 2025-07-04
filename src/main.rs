use std::{thread, time};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use winapi::um::winuser::{GetAsyncKeyState, VK_LWIN, VK_SPACE};
use serde::Deserialize;
use toml;

#[derive(Debug)]
enum AppCommand {
    Start(PathBuf),            // For Start menu shortcuts
    Shutdown(Vec<String>),      // For hardcoded shutdown commands
    Configured(Vec<String>),  // For configured commands
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
    args: Vec<String>,
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

    // Add built-in custom commands
    commands.insert("shutdown (shutdown)".to_string(), AppCommand::Shutdown(vec!["shutdown.exe".to_string(), "/s".to_string()]));
    commands.insert("reboot (shutdown)".to_string(), AppCommand::Shutdown(vec!["shutdown.exe".to_string(), "/r".to_string()]));
    commands.insert("logoff (shutdown)".to_string(), AppCommand::Shutdown(vec!["shutdown.exe".to_string(), "/l".to_string()]));
    commands.insert("hybernate (shutdown)".to_string(), AppCommand::Shutdown(vec!["shutdown.exe".to_string(), "/h".to_string()]));

    // Load config and process commands/options
    let config = load_config();
    let wlines_args = config.as_ref().map_or(vec![], |c| c.options.to_args());
    
    if let Some(config) = config {
        for cmd in config.commands {
            commands.insert(cmd.name, AppCommand::Configured(cmd.args));
        }
    }

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

    // Set running flag
    *state.process_running.lock().unwrap() = true; 

    thread::spawn(move || {
        let output = Command::new("wlines.exe")
            .args(&state.wlines_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(); 

        let joined = state.commands.keys()
            .fold(String::new(), |acc, s| acc + "\n" + s);

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
                    Command::new("cmd")
                        .args(&["/C", "start", "", path.as_os_str().to_str().unwrap()])
                        .spawn()
                        .expect("Failed to start program");
                },
                Some(AppCommand::Shutdown(args)) | Some(AppCommand::Configured(args)) => {
                    Command::new(&args[0])
                        .args(&args[1..])
                        .spawn()
                        .expect("Failed to execute command");
                },
                None => {}
            }
        } 

        *state.process_running.lock().unwrap() = false;
    });
}

fn main() {    
    let state = Arc::new(initialize_app_state()); 
    loop {
        if is_shortcut_pressed() {
            execute_wlines(state.clone());
        }
        thread::sleep(time::Duration::from_millis(50));
    }
}
