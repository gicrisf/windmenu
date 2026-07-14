// GUI subsystem: no console window is allocated when launched from the shell,
// a Startup shortcut, or the registry Run key. CLI output still works via
// attach_parent_console() below.
#![windows_subsystem = "windows"]

use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use std::thread;
use clap::{Parser, Subcommand};

mod apps;
mod reg;
mod daemon;
mod menu;
mod theme;
mod wlines;

use daemon::{Daemon, DaemonError, StartupMethod, WindmenuDaemon};
use apps::print_reparse_points_info;
use menu::Menu;

#[derive(Parser)]
#[command(name = "windmenu")]
#[command(version)] // taken from Cargo.toml
#[command(about = "WINdows DMENU-like launcher")]
struct Cli {
    /// Run as background daemon process (internal)
    #[arg(long, hide = true)]
    start_daemon_self_detached: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Daemon management commands
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Test utilities
    Test {
        #[command(subcommand)]
        test_type: TestType,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Restart the daemon
    Restart,
    /// Check daemon status
    Status,
    /// Enable startup method
    Enable {
        /// Startup method to enable
        #[arg(value_enum)]
        method: StartupMethod,
    },
    /// Disable startup method (all methods if none given)
    Disable {
        /// Startup method to disable; omit to disable all
        #[arg(value_enum)]
        method: Option<StartupMethod>,
    },
}

#[derive(Subcommand)]
enum TestType {
    /// Test and display reparse points
    #[command(name = "reparse-points")]
    ReparsePoints,
    /// Show config resolution paths
    #[command(name = "config")]
    Config,
}

/// Find an executable on PATH using where.exe.
/// Returns the first match, which for Scoop installs will be the stable shim path.
fn find_on_path(exe_name: &str) -> Option<PathBuf> {
    std::process::Command::new("where.exe")
        .arg(exe_name)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.lines().next().map(|line| PathBuf::from(line.trim()))
            } else {
                None
            }
        })
}

/// Attach to the parent process console so println!/eprintln! reach the
/// terminal despite the GUI subsystem. Skipped when stdout is already valid
/// (redirected to a file/pipe); a no-op when there is no parent console
/// (launched from Explorer or a Startup shortcut).
fn attach_parent_console() {
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
    use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};

    unsafe {
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn main() {
    attach_parent_console();

    let current_exe = env::current_exe()
        .expect("Failed to get current executable path");
    let cli = Cli::parse();

    // Resolve stable windmenu path: prefer PATH (Scoop shim) over resolved current_exe
    let windmenu_path = find_on_path("windmenu.exe")
        .unwrap_or_else(|| current_exe.clone());

    let windmenu_daemon = WindmenuDaemon::new(&windmenu_path);

    if cli.start_daemon_self_detached {
        // This is the background daemon process
        start_daemon_self_detached();
        return;
    }

    match cli.command {
        Some(Commands::Daemon { action }) => {
            handle_daemon_action(action, &windmenu_daemon);
        }
        Some(Commands::Test { test_type }) => {
            handle_test_command(test_type);
        }
        None => {
            // Default behavior - start windmenu daemon
            match windmenu_daemon.start() {
                Ok(()) => {
                    println!("windmenu is now running in the background");
                    println!("Press Win+Space to activate menu");
                    println!("Use 'windmenu daemon stop' to stop the daemon");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("windmenu daemon is already running");
                }
                Err(err) => {
                    eprintln!("Failed to start windmenu daemon: {}", err);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn handle_daemon_action<T: Daemon>(action: DaemonAction, daemon: &T) {
    match action {
        DaemonAction::Start => {
            match daemon.start() {
                Ok(()) => {
                    println!("windmenu is now running in the background");
                    println!("Press Win+Space to activate menu");
                    println!("Use 'windmenu daemon stop' to stop the daemon");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("windmenu daemon is already running");
                }
                Err(err) => {
                    eprintln!("Failed to start windmenu daemon: {}", err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Stop => {
            match daemon.stop() {
                Ok(()) => {
                    println!("windmenu daemon stopped successfully");
                }
                Err(DaemonError::NotRunning) => {
                    println!("No windmenu daemon was running");
                }
                Err(err) => {
                    eprintln!("Failed to stop windmenu daemon: {}", err);
                }
            }
        }
        DaemonAction::Restart => {
            match daemon.restart() {
                Ok(()) => {
                    println!("windmenu daemon restarted successfully");
                    println!("Press Win+Space to activate menu");
                }
                Err(err) => {
                    eprintln!("Failed to restart windmenu daemon: {}", err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Status => {
            let status = daemon.get_status();
            println!("windmenu daemon status:");
            print!("{}", status);
        }
        DaemonAction::Enable { method } => {
            match daemon.enable_startup(&method) {
                Ok(()) => {
                    println!("windmenu daemon startup method '{}' enabled successfully", method);
                }
                Err(err) => {
                    eprintln!("Failed to enable windmenu daemon startup method '{}': {}", method, err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Disable { method } => {
            let methods: Vec<StartupMethod> = match method {
                Some(m) => vec![m],
                None => vec![StartupMethod::Registry, StartupMethod::UserFolder],
            };

            let mut failed = false;
            for m in methods {
                match daemon.disable_startup(&m) {
                    Ok(()) => {
                        println!("windmenu daemon startup method '{}' disabled successfully", m);
                    }
                    Err(err) => {
                        eprintln!("Failed to disable windmenu daemon startup method '{}': {}", m, err);
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1);
            }
        }
    }
}

fn handle_test_command(test_type: TestType) {
    match test_type {
        TestType::ReparsePoints => {
            print_reparse_points_info();
        }
        TestType::Config => {
            menu::print_config_debug();
        }
    }
}

fn start_daemon_self_detached() {
    use winapi::um::synchapi::CreateMutexW;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::shared::winerror::ERROR_ALREADY_EXISTS;

    unsafe {
        let name: Vec<u16> = "windmenu-daemon-mutex\0".encode_utf16().collect();
        let mutex = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if !mutex.is_null() {
                CloseHandle(mutex);
            }
            std::process::exit(0);
        }
    }

    let menu = Arc::new(Menu::new());

    let entries_bg = menu.entries.clone();
    thread::spawn(move || {
        entries_bg.write().unwrap().rescan_dynamic();
    });

    menu.hotkey.listen(|| {
        if let Err(e) = menu.clone().show() {
            eprintln!("Menu show error: {}", e);
        }
    });
}
