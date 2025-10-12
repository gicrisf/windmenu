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
mod fetch;
mod theme;
mod wlan;

use daemon::{Daemon, DaemonError, StartupMethod, WlinesDaemon, WindmenuDaemon};
use apps::print_reparse_points_info;
use wlan::{print_wlan_interfaces_info, test_wlan_scan};
use menu::Menu;

#[derive(Parser)]
#[command(name = "windmenu")]
#[command(version = "0.5.0")]
#[command(about = "WINdows DMENU-like launcher wrapper")]
struct Cli {
    /// Run as background daemon process (internal)
    #[arg(long, hide = true)]
    start_daemon_self_detached: bool,

    /// Path to wlines-daemon.exe (optional, defaults to same directory as windmenu.exe)
    #[arg(long, value_name = "PATH")]
    wlines_daemon_path: Option<PathBuf>,

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
    /// Download dependencies
    Fetch {
        #[command(subcommand)]
        fetch_type: FetchType,
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
    /// Wlines daemon operations
    Wlines {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Apply action to all daemons (windmenu and wlines)
    All {
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
}

#[derive(Subcommand)]
enum FetchType {
    /// Download wlines-daemon.exe
    #[command(name = "wlines-daemon")]
    WlinesDaemon,
    /// Download wlines.exe
    #[command(name = "wlines-cli")]
    WlinesCli,
}

fn main() {
    // Create daemon instances early - we'll use them throughout
    let current_exe = env::current_exe()
        .expect("Failed to get current executable path");
    // Cli parsing
    let cli = Cli::parse();

    let wlines_daemon_path = if let Some(custom_path) = &cli.wlines_daemon_path {
        custom_path.clone()
    } else {
        let install_dir = current_exe
            .parent()
            .expect("Failed to get installation directory");

        install_dir.join("wlines-daemon.exe")
    };

    let windmenu_daemon = WindmenuDaemon::new(&current_exe);
    let wlines_daemon = WlinesDaemon::new(&wlines_daemon_path);

    if cli.start_daemon_self_detached {
        // This is the background daemon process
        start_daemon_self_detached(&wlines_daemon);
        return;
    }

    match cli.command {
        Some(Commands::Daemon { daemon_type }) => {
            handle_daemon_command(daemon_type, &windmenu_daemon, &wlines_daemon);
        }
        Some(Commands::Test { test_type }) => {
            handle_test_command(test_type);
        }
        Some(Commands::Fetch { fetch_type }) => {
            handle_fetch_command(fetch_type);
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

fn handle_daemon_command(daemon_type: DaemonType, windmenu_daemon: &WindmenuDaemon, wlines_daemon: &WlinesDaemon) {
    match daemon_type {
        DaemonType::Self_ { action } => {
            handle_daemon_action(action, windmenu_daemon, "windmenu");
        }
        DaemonType::Wlines { action } => {
            handle_daemon_action(action, wlines_daemon, "wlines");
        }
        DaemonType::All { action } => {
            handle_all_daemon_action(action, windmenu_daemon, wlines_daemon);
        }
    }
}

fn handle_all_daemon_action(action: DaemonAction, windmenu_daemon: &WindmenuDaemon, wlines_daemon: &WlinesDaemon) {
    match action {
        DaemonAction::Start => {
            println!("Starting all daemons...");

            // Start windmenu first
            match windmenu_daemon.start() {
                Ok(()) => {
                    println!("✓ windmenu daemon started successfully");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("✓ windmenu daemon is already running");
                }
                Err(err) => {
                    eprintln!("✗ Failed to start windmenu daemon: {}", err);
                }
            }

            // Then start wlines
            match wlines_daemon.start() {
                Ok(()) => {
                    println!("✓ wlines daemon started successfully");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("✓ wlines daemon is already running");
                }
                Err(err) => {
                    eprintln!("✗ Failed to start wlines daemon: {}", err);
                }
            }

            println!("Press Win+Space to activate menu");
        }
        DaemonAction::Stop => {
            println!("Stopping all daemons...");

            // Stop windmenu first
            match windmenu_daemon.stop() {
                Ok(()) => {
                    println!("✓ windmenu daemon stopped successfully");
                }
                Err(DaemonError::NotRunning) => {
                    println!("✓ windmenu daemon was not running");
                }
                Err(err) => {
                    eprintln!("✗ Failed to stop windmenu daemon: {}", err);
                }
            }

            // Then stop wlines
            match wlines_daemon.stop() {
                Ok(()) => {
                    println!("✓ wlines daemon stopped successfully");
                }
                Err(DaemonError::NotRunning) => {
                    println!("✓ wlines daemon was not running");
                }
                Err(err) => {
                    eprintln!("✗ Failed to stop wlines daemon: {}", err);
                }
            }
        }
        DaemonAction::Restart => {
            println!("Restarting all daemons...");

            // Restart windmenu first
            match windmenu_daemon.restart() {
                Ok(()) => {
                    println!("✓ windmenu daemon restarted successfully");
                }
                Err(err) => {
                    eprintln!("✗ Failed to restart windmenu daemon: {}", err);
                }
            }

            // Then restart wlines
            match wlines_daemon.restart() {
                Ok(()) => {
                    println!("✓ wlines daemon restarted successfully");
                }
                Err(err) => {
                    eprintln!("✗ Failed to restart wlines daemon: {}", err);
                }
            }

            println!("Press Win+Space to activate menu");
        }
        DaemonAction::Status => {
            println!("Status of all daemons:");
            println!();

            println!("windmenu daemon:");
            let windmenu_status = windmenu_daemon.get_status();
            print!("{}", windmenu_status);
            println!();

            println!("wlines daemon:");
            let wlines_status = wlines_daemon.get_status();
            print!("{}", wlines_status);
        }
        DaemonAction::Enable { method } => {
            println!("Enabling startup method '{}' for all daemons...", method);

            // Enable for windmenu
            match windmenu_daemon.enable_startup(&method) {
                Ok(()) => {
                    println!("✓ windmenu daemon startup method '{}' enabled successfully", method);
                }
                Err(err) => {
                    eprintln!("✗ Failed to enable windmenu daemon startup method '{}': {}", method, err);
                }
            }

            // Enable for wlines
            match wlines_daemon.enable_startup(&method) {
                Ok(()) => {
                    println!("✓ wlines daemon startup method '{}' enabled successfully", method);
                }
                Err(err) => {
                    eprintln!("✗ Failed to enable wlines daemon startup method '{}': {}", method, err);
                }
            }
        }
        DaemonAction::Disable { method } => {
            println!("Disabling startup method '{}' for all daemons...", method);

            // Disable for windmenu
            match windmenu_daemon.disable_startup(&method) {
                Ok(()) => {
                    println!("✓ windmenu daemon startup method '{}' disabled successfully", method);
                }
                Err(err) => {
                    eprintln!("✗ Failed to disable windmenu daemon startup method '{}': {}", method, err);
                }
            }

            // Disable for wlines
            match wlines_daemon.disable_startup(&method) {
                Ok(()) => {
                    println!("✓ wlines daemon startup method '{}' disabled successfully", method);
                }
                Err(err) => {
                    eprintln!("✗ Failed to disable wlines daemon startup method '{}': {}", method, err);
                }
            }
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
    }
}

fn handle_fetch_command(fetch_type: FetchType) {
    match fetch_type {
        FetchType::WlinesDaemon => {
            match fetch::ensure_wlines_daemon_available() {
                Ok(path) => {
                    println!("wlines-daemon.exe is available at: {}", path.display());
                }
                Err(e) => {
                    eprintln!("Failed to fetch wlines-daemon.exe: {}", e);
                    std::process::exit(1);
                }
            }
        }
        FetchType::WlinesCli => {
            match fetch::ensure_wlines_available() {
                Ok(path) => {
                    println!("wlines.exe is available at: {}", path.display());
                }
                Err(e) => {
                    eprintln!("Failed to fetch wlines.exe: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn start_daemon_self_detached(wlines_daemon: &WlinesDaemon) {
    let menu = Arc::new(Menu::new());

    menu.hotkey.poll(|| {
        if let Err(e) = menu.clone().show(wlines_daemon) {
            eprintln!("Menu show error: {}", e);
        }
    });
}
