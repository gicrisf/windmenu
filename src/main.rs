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
mod daemon;
mod menu;
mod theme;
mod wlines;

use daemon::{Daemon, DaemonError, WindmenuDaemon};
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
    /// Manage the windmenu.toml configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Test utilities
    Test {
        #[command(subcommand)]
        test_type: TestType,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Write a default windmenu.toml next to the executable
    Init {
        /// Overwrite an existing config file
        #[arg(long)]
        force: bool,
    },
    /// Show config resolution paths and the file in use
    Path,
    /// Open the config in an editor (creating it if needed)
    Edit,
    /// Manage bundled theme/command packs
    Pack {
        #[command(subcommand)]
        action: PackAction,
    },
}

#[derive(Subcommand)]
enum PackAction {
    /// List bundled packs
    List {
        /// Only theme packs
        #[arg(long)]
        themes: bool,
        /// Only command packs
        #[arg(long)]
        commands: bool,
    },
    /// Write a bundled pack next to windmenu.toml
    Install {
        /// Pack name (see `config pack list`)
        name: String,
        /// Overwrite an existing pack file
        #[arg(long)]
        force: bool,
    },
    /// Print a bundled pack to stdout
    Show {
        /// Pack name (see `config pack list`)
        name: String,
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
}

#[derive(Subcommand)]
enum TestType {
    /// Test and display reparse points
    #[command(name = "reparse-points")]
    ReparsePoints,
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
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null()
            && AttachConsole(ATTACH_PARENT_PROCESS) != 0
        {
            CONSOLE_ATTACHED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

static CONSOLE_ATTACHED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The shell prints its prompt without waiting for a GUI-subsystem exe, so our
/// output lands below the prompt and the cursor is left stranded — it looks
/// hung until the user presses Enter. Inject one Enter into the console input
/// buffer so the shell redraws its prompt after our output.
fn release_parent_console() {
    use std::io::Write;
    use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::wincon::{WriteConsoleInputW, INPUT_RECORD, KEY_EVENT};
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE};
    use winapi::um::winuser::VK_RETURN;

    if !CONSOLE_ATTACHED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    unsafe {
        let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        let conin = CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if conin == INVALID_HANDLE_VALUE {
            return;
        }
        let mut record: INPUT_RECORD = std::mem::zeroed();
        record.EventType = KEY_EVENT;
        {
            let key = record.Event.KeyEvent_mut();
            key.bKeyDown = 1;
            key.wRepeatCount = 1;
            key.wVirtualKeyCode = VK_RETURN as u16;
            *key.uChar.UnicodeChar_mut() = '\r' as u16;
        }
        let mut written = 0;
        WriteConsoleInputW(conin, &record, 1, &mut written);
        CloseHandle(conin);
    }
}

/// Exit a CLI code path, first waking the parent shell's prompt.
fn cli_exit(code: i32) -> ! {
    release_parent_console();
    std::process::exit(code);
}

/// Surface panics in a message box: with panic = "abort", a GUI subsystem and
/// the daemon's stderr detached, a panic on any thread would otherwise kill
/// the process with no trace — the hotkey just stops working.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic");
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let text = format!("windmenu crashed: {} ({})", message, location);
        eprintln!("{}", text);
        menu::error_box(&text);
    }));
}

/// Opt in to per-monitor DPI awareness so GDI text renders crisply on scaled
/// displays instead of being bitmap-stretched. Done via API rather than an
/// embedded manifest to keep the cross-compile free of a windres step.
fn enable_dpi_awareness() {
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::{SetProcessDPIAware, SetProcessDpiAwarenessContext};

    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            // Pre-1703 Windows 10: fall back to system-wide awareness
            SetProcessDPIAware();
        }
    }
}

fn main() {
    attach_parent_console();
    install_panic_hook();
    enable_dpi_awareness();

    let current_exe = env::current_exe()
        .expect("Failed to get current executable path");
    // try_parse instead of parse: clap's built-in exit on --help/--version
    // would skip the prompt-waking in cli_exit
    let cli = Cli::try_parse().unwrap_or_else(|e| {
        let _ = e.print();
        cli_exit(e.exit_code());
    });

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
        Some(Commands::Config { action }) => {
            handle_config_command(action);
        }
        Some(Commands::Test { test_type }) => {
            handle_test_command(test_type);
        }
        None => {
            // Default behavior - start windmenu daemon
            match windmenu_daemon.start() {
                Ok(()) => {
                    println!("windmenu is now running in the background");
                    println!("Press Ctrl+Alt+Space to activate menu");
                    println!("Use 'windmenu daemon stop' to stop the daemon");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("windmenu daemon is already running");
                }
                Err(err) => {
                    eprintln!("Failed to start windmenu daemon: {}", err);
                    cli_exit(1);
                }
            }
        }
    }

    release_parent_console();
}

fn handle_daemon_action<T: Daemon>(action: DaemonAction, daemon: &T) {
    match action {
        DaemonAction::Start => {
            match daemon.start() {
                Ok(()) => {
                    println!("windmenu is now running in the background");
                    println!("Press Ctrl+Alt+Space to activate menu");
                    println!("Use 'windmenu daemon stop' to stop the daemon");
                }
                Err(DaemonError::AlreadyRunning) => {
                    println!("windmenu daemon is already running");
                }
                Err(err) => {
                    eprintln!("Failed to start windmenu daemon: {}", err);
                    cli_exit(1);
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
                    println!("Press Ctrl+Alt+Space to activate menu");
                }
                Err(err) => {
                    eprintln!("Failed to restart windmenu daemon: {}", err);
                    cli_exit(1);
                }
            }
        }
        DaemonAction::Status => {
            let status = daemon.get_status();
            println!("windmenu daemon status:");
            print!("{}", status);
        }
    }
}

fn handle_config_command(action: ConfigAction) {
    let code = match action {
        ConfigAction::Init { force } => menu::config_init(force),
        ConfigAction::Path => {
            menu::config_path();
            0
        }
        ConfigAction::Edit => menu::config_edit(),
        ConfigAction::Pack { action } => match action {
            PackAction::List { themes, commands } => {
                menu::pack_list(themes, commands);
                0
            }
            PackAction::Install { name, force } => menu::pack_install(&name, force),
            PackAction::Show { name } => menu::pack_show(&name),
        },
    };
    if code != 0 {
        cli_exit(code);
    }
}

fn handle_test_command(test_type: TestType) {
    match test_type {
        TestType::ReparsePoints => {
            print_reparse_points_info();
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
