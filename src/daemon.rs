use std::{thread, time};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs, fmt};
use clap::ValueEnum;

use winapi::um::winbase::{DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP};
use std::os::windows::process::CommandExt;

use mslnk::ShellLink;

use winapi::um::synchapi::{OpenMutexW, OpenEventW, SetEvent};
use winapi::um::handleapi::CloseHandle;
use winapi::um::winnt::{SYNCHRONIZE, EVENT_MODIFY_STATE};

use crate::reg::{check_registry_entry, add_registry_entry, remove_registry_entry, RegistryError};

#[derive(Debug, Clone, ValueEnum)]
pub enum StartupMethod {
    #[value(name = "registry")]
    Registry,
    #[value(name = "user-folder")]
    UserFolder,
}

impl fmt::Display for StartupMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
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

pub trait Daemon {
    fn name(&self) -> &'static str;
    fn registry_name(&self) -> &'static str;
    fn shortcut_name(&self) -> &'static str;
    fn path(&self) -> &Path;

    fn path_str(&self) -> String {
        self.path().to_string_lossy().to_string()
    }

    fn working_directory(&self) -> Option<PathBuf> {
        self.path()
            .parent()
            .map(|p| p.to_path_buf())
    }

    fn is_running(&self) -> bool;

    fn start(&self) -> Result<(), DaemonError> {
        if self.is_running() {
            return Err(DaemonError::AlreadyRunning);
        }

        let mut cmd = Command::new(self.path());
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
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

    fn stop(&self) -> Result<(), DaemonError>;

    fn restart(&self) -> Result<(), DaemonError> {
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

    fn enable_startup(&self, method: &StartupMethod) -> Result<(), String> {
        match method {
            StartupMethod::Registry => self.enable_registry_startup(),
            StartupMethod::UserFolder => self.enable_user_folder_startup(),
        }
    }

    fn disable_startup(&self, method: &StartupMethod) -> Result<(), String> {
        match method {
            StartupMethod::Registry => self.disable_registry_startup(),
            StartupMethod::UserFolder => self.disable_user_folder_startup(),
        }
    }

    fn get_status(&self) -> DaemonStatus {
        let is_running = self.is_running();

        let registry_status = self.get_registry_startup_status();
        let user_folder_status = self.get_user_folder_startup_status();

        DaemonStatus {
            is_running,
            registry_status,
            user_folder_status,
        }
    }


    // Registry startup methods
    fn enable_registry_startup(&self) -> Result<(), String> {
        add_registry_entry(self.registry_name(), &self.path_str())
            .map_err(|e| match e {
                RegistryError::AccessDenied => format!("Access denied when setting {} registry entry", self.name()),
                RegistryError::KeyNotFound => "Registry key not found".to_string(),
                RegistryError::UnknownError(code) => format!("Failed to set {} registry value: error code {}", self.name(), code),
            })?;

        println!("Registry startup enabled for {} daemon", self.name());
        Ok(())
    }

    fn disable_registry_startup(&self) -> Result<(), String> {
        remove_registry_entry(self.registry_name())
            .map_err(|e| match e {
                RegistryError::AccessDenied => format!("Access denied when removing {} registry entry", self.name()),
                RegistryError::KeyNotFound => "Registry key not found".to_string(),
                RegistryError::UnknownError(code) => format!("Failed to remove {} registry value: error code {}", self.name(), code),
            })?;

        println!("Registry startup disabled for {} daemon", self.name());
        Ok(())
    }

    // User folder startup methods: a plain .lnk shortcut in the per-user
    // Startup folder. No admin, no script host, nothing for AV to flag.
    fn user_startup_shortcut_path(&self) -> Result<PathBuf, String> {
        let startup_folder = env::var("APPDATA")
            .map_err(|_| "Could not determine user startup folder".to_string())?;

        Ok(Path::new(&startup_folder)
            .join("Microsoft\\Windows\\Start Menu\\Programs\\Startup")
            .join(self.shortcut_name()))
    }

    fn enable_user_folder_startup(&self) -> Result<(), String> {
        let shortcut_path = self.user_startup_shortcut_path()?;

        let mut link = ShellLink::new(self.path())
            .map_err(|e| format!("Failed to create {} startup shortcut: {}", self.name(), e))?;
        link.set_working_dir(self.working_directory().map(|p| p.to_string_lossy().to_string()));
        link.create_lnk(&shortcut_path)
            .map_err(|e| format!("Failed to write {} startup shortcut: {}", self.name(), e))?;

        println!("User folder startup enabled for {} daemon", self.name());
        Ok(())
    }

    fn disable_user_folder_startup(&self) -> Result<(), String> {
        let shortcut_path = self.user_startup_shortcut_path()?;

        if shortcut_path.exists() {
            fs::remove_file(&shortcut_path)
                .map_err(|_| format!("Failed to remove {} startup shortcut", self.name()))?;
        }

        println!("User folder startup disabled for {} daemon", self.name());
        Ok(())
    }

    fn get_registry_startup_status(&self) -> bool {
        match check_registry_entry(self.registry_name()) {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                eprintln!("Warning: Failed to check registry startup for {}: {:?}", self.name(), e);
                false
            }
        }
    }

    fn get_user_folder_startup_status(&self) -> bool {
        self.user_startup_shortcut_path()
            .map(|p| p.exists())
            .unwrap_or(false)
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
}

impl Daemon for WindmenuDaemon {
    fn path(&self) -> &Path {
        &self.path
    }

    fn name(&self) -> &'static str {
        "windmenu"
    }

    fn registry_name(&self) -> &'static str {
        "WindmenuDaemon"
    }

    fn shortcut_name(&self) -> &'static str {
        "windmenu.lnk"
    }

    fn is_running(&self) -> bool {
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

    fn start(&self) -> Result<(), DaemonError> {
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

    fn stop(&self) -> Result<(), DaemonError> {
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
}

#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub is_running: bool,
    pub registry_status: bool,
    pub user_folder_status: bool,
}

impl fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Running: {}", if self.is_running { "Yes" } else { "No" })?;

        writeln!(f, "  Startup configuration:")?;
        if self.registry_status {
            writeln!(f, "    Registry: Enabled")?;
        }
        if self.user_folder_status {
            writeln!(f, "    User Folder: Enabled")?;
        }

        Ok(())
    }
}
