use std::{thread, time};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fmt};

use winapi::um::winbase::{DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP};
use std::os::windows::process::CommandExt;

use winapi::um::synchapi::{OpenMutexW, OpenEventW, SetEvent};
use winapi::um::handleapi::CloseHandle;
use winapi::um::winnt::{SYNCHRONIZE, EVENT_MODIFY_STATE};

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
        if !self.is_running() {
            return Err(DaemonError::NotRunning);
        }

        let name: Vec<u16> = "windmenu-shutdown-event\0".encode_utf16().collect();
        unsafe {
            let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
            if event.is_null() {
                return Err(DaemonError::ShutdownFailed(
                    "Daemon is running but its shutdown event was not found. Try again.".to_string()
                ));
            }
            SetEvent(event);
            CloseHandle(event);
        }

        // Poll for exit instead of a single fixed sleep: the daemon may be
        // mid-callback when the event fires
        for _ in 0..20 {
            thread::sleep(time::Duration::from_millis(100));
            if !self.is_running() {
                return Ok(());
            }
        }

        Err(DaemonError::ShutdownFailed(
            "Daemon did not shut down within timeout".to_string()
        ))
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

    pub fn get_status(&self) -> DaemonStatus {
        DaemonStatus {
            is_running: self.is_running(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub is_running: bool,
}

impl fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Running: {}", if self.is_running { "Yes" } else { "No" })
    }
}
