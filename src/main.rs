use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use clap::{Parser, Subcommand};

mod apps;
mod reg;
mod task;
mod daemon;
mod proc;
mod menu;
mod theme;
mod wlan;
mod wlines;

use daemon::{Daemon, DaemonError, StartupMethod, WindmenuDaemon};
use apps::print_reparse_points_info;
use wlan::{print_wlan_interfaces_info, test_wlan_scan};
use menu::Menu;

#[derive(Parser)]
#[command(name = "windmenu")]
#[command(version = "0.6.0")]
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
        daemon_type: DaemonType,
    },
    /// Test utilities
    Test {
        #[command(subcommand)]
        test_type: TestType,
    },
}

#[derive(Subcommand)]
enum DaemonType {
    /// Windmenu daemon operations
    #[command(name = "self")]
    Self_ {
        #[command(subcommand)]
        action: DaemonAction,
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
    /// Disable startup method
    Disable {
        /// Startup method to disable
        #[arg(value_enum)]
        method: StartupMethod,
    },
}

#[derive(Subcommand)]
enum TestType {
    /// Test and display reparse points
    #[command(name = "reparse-points")]
    ReparsePoints,
    /// Test and display WLAN interfaces
    #[command(name = "wlan-interfaces")]
    WlanInterfaces,
    /// Trigger WLAN scan on all interfaces
    #[command(name = "wlan-scan")]
    WlanScan,
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

fn main() {
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
        Some(Commands::Daemon { daemon_type }) => {
            handle_daemon_command(daemon_type, &windmenu_daemon);
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
                    println!("Use 'windmenu daemon self stop' to stop the daemon");
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

fn handle_daemon_command(daemon_type: DaemonType, windmenu_daemon: &WindmenuDaemon) {
    match daemon_type {
        DaemonType::Self_ { action } => {
            handle_daemon_action(action, windmenu_daemon, "windmenu");
        }
    }
}

fn handle_daemon_action<T: Daemon>(action: DaemonAction, daemon: &T, daemon_name: &str) {
    match action {
        DaemonAction::Start => {
            match daemon.start() {
                Ok(()) => {
                    if daemon_name == "windmenu" {
                        println!("windmenu is now running in the background");
                        println!("Press Win+Space to activate menu");
                        println!("Use 'windmenu daemon self stop' to stop the daemon");
                    } else {
                        println!("{} daemon started successfully", daemon_name);
                    }
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("{} daemon is already running", daemon_name);
                }
                Err(err) => {
                    eprintln!("Failed to start {} daemon: {}", daemon_name, err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Stop => {
            match daemon.stop() {
                Ok(()) => {
                    println!("{} daemon stopped successfully", daemon_name);
                }
                Err(DaemonError::NotRunning) => {
                    println!("No {} daemon was running", daemon_name);
                }
                Err(err) => {
                    eprintln!("Failed to stop {} daemon: {}", daemon_name, err);
                }
            }
        }
        DaemonAction::Restart => {
            match daemon.restart() {
                Ok(()) => {
                    if daemon_name == "windmenu" {
                        println!("windmenu daemon restarted successfully");
                        println!("Press Win+Space to activate menu");
                    } else {
                        println!("{} daemon restarted successfully", daemon_name);
                    }
                }
                Err(err) => {
                    eprintln!("Failed to restart {} daemon: {}", daemon_name, err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Status => {
            let status = daemon.get_status();
            println!("{} daemon status:", daemon_name);
            print!("{}", status);
        }
        DaemonAction::Enable { method } => {
            match daemon.enable_startup(&method) {
                Ok(()) => {
                    println!("{} daemon startup method '{}' enabled successfully", daemon_name, method);
                }
                Err(err) => {
                    eprintln!("Failed to enable {} daemon startup method '{}': {}", daemon_name, method, err);
                    std::process::exit(1);
                }
            }
        }
        DaemonAction::Disable { method } => {
            match daemon.disable_startup(&method) {
                Ok(()) => {
                    println!("{} daemon startup method '{}' disabled successfully", daemon_name, method);
                }
                Err(err) => {
                    eprintln!("Failed to disable {} daemon startup method '{}': {}", daemon_name, method, err);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn handle_test_command(test_type: TestType) {
    match test_type {
        TestType::ReparsePoints => {
            print_reparse_points_info();
        }
        TestType::WlanInterfaces => {
            print_wlan_interfaces_info();
        }
        TestType::WlanScan => {
            test_wlan_scan();
        }
        TestType::Config => {
            menu::print_config_debug();
        }
    }
}

fn start_daemon_self_detached() {
    let menu = Arc::new(Menu::new());

    menu.hotkey.listen(|| {
        if let Err(e) = menu.clone().show() {
            eprintln!("Menu show error: {}", e);
        }
    });
}
