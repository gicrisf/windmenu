use std::{thread, time};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{ Path, PathBuf };
use winapi::um::winuser::{ GetAsyncKeyState, VK_LWIN, VK_SPACE };

struct AppState {
    process_running: Mutex<bool>,
    start_menu_map: HashMap<String, PathBuf>,
}

fn get_start_menu_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    
    // User Start Menu
    if let Ok(appdata) = env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu"));
    }
    
    // All Users Start Menu
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
            (GetAsyncKeyState(VK_LWIN) & 0x8000u16 as i16 != 0)
            && (GetAsyncKeyState(VK_SPACE) & 0x8000u16 as i16 != 0)
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

    // Spawn thread to run and monitor the process
    thread::spawn(move || {
        let output = Command::new("wlines.exe")
            .args(&[
                "-sbg", "#285577",
                "-sfg", "#ffffff",
                "-fs", "16",
                "-p", "oi mbare, run something"
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(); 

        // Join link names for stdin 
        let joined = state
            .start_menu_map
            .keys()
            .fold(String::new(), |acc, s| acc + "\n" + s);

        if let Ok(mut child) = output {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(joined.as_bytes()).expect("Failed to write to stdin");
            }
            // Then you can wait for the output
            let output = child.wait_with_output().expect("Failed to read output");

            // Convert UTF8 vec of bytes
            let s = match std::str::from_utf8(&output.stdout) {
                Ok(v) => v,
                Err(e) => panic!("invalid UTF8!: {}", e),
            };

            // Get the path from the selected
            if let Some(selected_path) = state.start_menu_map.get(s.trim()) {
                Command::new("cmd")
                    .args(&["/C", "start", "", selected_path
                            .as_os_str()
                            .to_str()
                            .expect("Failed to convert path")])
                    .spawn()
                    .expect("Failed to start program");
            }  
        } 

        // Reset flag when done
        *state.process_running.lock().unwrap() = false;
    });
}

fn main() {    
    let start_menu_paths = get_start_menu_paths();

    let mut start_menu_map = HashMap::new();
    for path in start_menu_paths {
        start_menu_map.extend(find_lnk_files(&path).unwrap());
    }

    let state = Arc::new(AppState {
        process_running: Mutex::new(false),
        start_menu_map,
    });

    loop {
        if is_shortcut_pressed() {
            execute_wlines(state.clone());
        }
        thread::sleep(time::Duration::from_millis(50));
    }
}
