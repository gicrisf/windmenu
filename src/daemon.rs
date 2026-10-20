use std::{thread, time};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fmt};

use winapi::um::winbase::{DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP};
use std::os::windows::process::CommandExt;

use winapi::shared::ntdef::HANDLE;
use winapi::um::synchapi::{OpenMutexW, OpenEventW, SetEvent};
use winapi::um::handleapi::CloseHandle;
use winapi::um::winnt::{SYNCHRONIZE, EVENT_MODIFY_STATE};

const SUPERVISOR_SHUTDOWN_EVENT: &str = "windmenu-supervisor-shutdown-event";
const DAEMON_SHUTDOWN_EVENT: &str = "windmenu-shutdown-event";

fn open_event(name: &str) -> HANDLE {
    let name: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
    unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) }
}

#[derive(Debug, Clone)]
pub enum DaemonError {
    AlreadyRunning,
    NotRunning,
    StartupFailed(String),
    ShutdownFailed(String),
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::AlreadyRunning => write!(f, "Daemon is already running"),
            DaemonError::NotRunning => write!(f, "Daemon is not running"),
            DaemonError::StartupFailed(msg) => write!(f, "Startup failed: {}", msg),
            DaemonError::ShutdownFailed(msg) => write!(f, "Shutdown failed: {}", msg),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindmenuDaemon {
    pub path: PathBuf,
}

impl WindmenuDaemon {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }

    fn name(&self) -> &'static str {
        "windmenu"
    }

    fn path_str(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    fn working_directory(&self) -> Option<PathBuf> {
        self.path.parent().map(|p| p.to_path_buf())
    }

    pub fn is_running(&self) -> bool {
        let name: Vec<u16> = "windmenu-daemon-mutex\0".encode_utf16().collect();
        unsafe {
            let h = OpenMutexW(SYNCHRONIZE, 0, name.as_ptr());
            if h.is_null() {
                false
            } else {
                CloseHandle(h);
                true
            }
        }
    }

    pub fn start(&self) -> Result<(), DaemonError> {
        if self.is_running() {
            return Err(DaemonError::AlreadyRunning);
        }

        // Always spawn ourselves via current_exe() to avoid going through
        // package manager shims, which may allocate a visible console window.
        let current_exe = env::current_exe()
            .map_err(|e| DaemonError::StartupFailed(format!("Failed to get current executable path: {}", e)))?;
        let mut cmd = Command::new(&current_exe);
        cmd.arg("--start-daemon-self-detached")  // <-- main reason for this
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let Some(dir) = self.working_directory() {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()
            .map_err(|e| DaemonError::StartupFailed(
                format!("Failed to start {} at '{}': {}",
                        self.name(),
                        self.path_str(), e)))?;

        println!("{} started with PID: {} (path: {})",
                 self.name(),
                 child.id(),
                 self.path_str());

        // Give it a moment to initialize
        thread::sleep(time::Duration::from_millis(500));
        Ok(())
    }

    pub fn stop(&self) -> Result<(), DaemonError> {
        let supervisor = open_event(SUPERVISOR_SHUTDOWN_EVENT);
        let mut daemon_event = open_event(DAEMON_SHUTDOWN_EVENT);
        let had_supervisor = !supervisor.is_null();
        let mut running = self.is_running();

        if !had_supervisor && daemon_event.is_null() && !running {
            return Err(DaemonError::NotRunning);
        }

        // Signal the supervisor first so it cannot respawn the daemon while
        // this stop is in flight.
        unsafe {
            if had_supervisor {
                SetEvent(supervisor);
            }
            if !daemon_event.is_null() {
                SetEvent(daemon_event);
            }
        }

        // A supervisor may be about to spawn the daemon. Give it a moment to
        // appear so the stop covers that child too, then wait for exit.
        let mut grace = if had_supervisor && !running { 10 } else { 0 };
        let mut ticks = 0;

        let result = loop {
            running = self.is_running();
            if running {
                if grace > 0 {
                    grace = 0;
                    ticks = 0;
                }
                if daemon_event.is_null() {
                    daemon_event = open_event(DAEMON_SHUTDOWN_EVENT);
                    if !daemon_event.is_null() {
                        unsafe { SetEvent(daemon_event); }
                    }
                }
            } else if grace > 0 {
                grace -= 1;
            } else {
                break Ok(());
            }

            if ticks >= 20 {
                break Err(DaemonError::ShutdownFailed(
                    "Daemon did not shut down within timeout".to_string(),
                ));
            }
            ticks += 1;
            thread::sleep(time::Duration::from_millis(100));
        };

        unsafe {
            if !supervisor.is_null() {
                CloseHandle(supervisor);
            }
            if !daemon_event.is_null() {
                CloseHandle(daemon_event);
            }
        }

        result
    }

    pub fn restart(&self) -> Result<(), DaemonError> {
        match self.stop() {
            Ok(()) => {},
            Err(DaemonError::NotRunning) => {},
            Err(e) => return Err(e),
        }

        // Wait a moment for cleanup
        thread::sleep(time::Duration::from_millis(1000));

        // Start the daemon
        self.start()
    }
}
