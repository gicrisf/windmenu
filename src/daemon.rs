use std::{thread, time};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs, fmt};
use clap::ValueEnum;

use winapi::um::winbase::{DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP};
use std::os::windows::process::CommandExt;

use crate::reg::{check_registry_entry, add_registry_entry, remove_registry_entry, RegistryError};
use crate::task::{check_scheduled_task, delete_task, SchTask, SchTaskExec, TaskSchedulerError};
use crate::proc::{find_processes_with_name, find_first_process_with_name, terminate_process_by_pid, ProcessInfo};

#[derive(Debug, Clone, ValueEnum)]
pub enum StartupMethod {
    #[value(name = "registry")]
    Registry,
    #[value(name = "task")]
    TaskScheduler,
    #[value(name = "user-folder")]
    UserFolder,
    #[value(name = "all-users-folder")]
    AllUsersFolder,
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
    ProcessError(String),
    StartupFailed(String),
    ShutdownFailed(String),
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::AlreadyRunning => write!(f, "Daemon is already running"),
            DaemonError::NotRunning => write!(f, "Daemon is not running"),
            DaemonError::ProcessError(msg) => write!(f, "Process error: {}", msg),
            DaemonError::StartupFailed(msg) => write!(f, "Startup failed: {}", msg),
            DaemonError::ShutdownFailed(msg) => write!(f, "Shutdown failed: {}", msg),
        }
    }
}

pub trait Daemon {
    fn name(&self) -> &'static str;
    fn process_name(&self) -> &'static str;
    fn registry_name(&self) -> &'static str;
    fn task_name(&self) -> &'static str;
    fn user_script_name(&self) -> &'static str;
    fn all_users_script_name(&self) -> &'static str;
    fn path(&self) -> &Path;

    fn path_str(&self) -> String {
        self.path().to_string_lossy().to_string()
    }

    fn working_directory(&self) -> Option<PathBuf> {
        self.path()
            .parent()
            .map(|p| p.to_path_buf())
    }

    fn is_running(&self) -> bool {
        // Just check if any instance exists
        find_first_process_with_name(self.process_name()).is_some()
    }

    fn start(&self) -> Result<(), DaemonError> {
        if self.is_running() {
            return Err(DaemonError::AlreadyRunning);
        }

        let child = Command::new(self.path())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
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
        // Find all instances and terminate them
        let processes = find_processes_with_name(self.process_name());

        if processes.is_empty() {
            return Err(DaemonError::ProcessError("No daemon processes found".to_string()));
        }

        for process in processes {
            terminate_process_by_pid(process.pid)
                .map_err(|err| DaemonError::ShutdownFailed(format!("Failed to terminate {} daemon PID {}: {}", self.name(), process.pid, err)))?;
            println!("Terminated {} process: {}", self.name(), process);
        }

        Ok(())
    }

    fn restart(&self) -> Result<(), DaemonError> {
        // Stop the daemon if it's running (ignore NotRunning error)
        match self.stop() {
            Ok(()) => {},
            Err(DaemonError::NotRunning) => {},
            Err(DaemonError::ProcessError(_)) => {},
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
            StartupMethod::TaskScheduler => self.enable_task_startup(),
            StartupMethod::UserFolder => self.enable_user_folder_startup(),
            StartupMethod::AllUsersFolder => self.enable_all_users_folder_startup(),
        }
    }

    fn disable_startup(&self, method: &StartupMethod) -> Result<(), String> {
        match method {
            StartupMethod::Registry => self.disable_registry_startup(),
            StartupMethod::TaskScheduler => self.disable_task_startup(),
            StartupMethod::UserFolder => self.disable_user_folder_startup(),
            StartupMethod::AllUsersFolder => self.disable_all_users_folder_startup(),
        }
    }

    fn get_status(&self) -> DaemonStatus {
        let is_running = self.is_running();
        let processes = find_processes_with_name(self.process_name());

        let registry_status = self.get_registry_startup_status();
        let task_scheduler_status = self.get_task_scheduler_startup_status();
        let user_folder_status = self.get_user_folder_startup_status();
        let all_users_folder_status = self.get_all_users_folder_startup_status();

        DaemonStatus {
            is_running,
            processes,
            registry_status,
            task_scheduler_status,
            user_folder_status,
            all_users_folder_status,
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

    // Task scheduler startup methods
    // TODO impl special for each case
    fn enable_task_startup(&self) -> Result<(), String> {
        let task = SchTask {
            date: "2025-07-11T00:00:00.0000000".to_string(),
            author: env!("CARGO_PKG_AUTHORS").to_string(),
            description: format!("{} daemon", self.name()),
            logon_trigger: true,
            logon_delay: if self.name() == "wlines" { "PT3S".to_string() } else { "PT5S".to_string() },
            privilege: "LeastPrivilege".to_string(),
            multiple_instance_policy: "IgnoreNew".to_string(),
            disallow_start_if_on_batteries: false,
            allow_hard_terminate: true,
            run_only_if_network_available: false,
            stop_on_idle: false,
            restart_on_idle: false,
            allow_start_on_demand: true,
            enabled: true,
            hidden: false,
            run_only_if_idle: false,
            wake_to_run: false,
            priority: 7,
            actions: vec![SchTaskExec {
                command: self.path_str(),
                working_directory: self.working_directory().map(|p| p.to_string_lossy().to_string()),
            }],
        };

        task.write_to_disk(self.task_name())?;
        println!("Task scheduler startup enabled for {} daemon", self.name());
        Ok(())
    }

    fn disable_task_startup(&self) -> Result<(), String> {
        delete_task(self.task_name());
        println!("Task scheduler startup disabled for {} daemon", self.name());
        Ok(())
    }

    // User folder startup methods
    fn enable_user_folder_startup(&self) -> Result<(), String> {
        let startup_folder = env::var("APPDATA")
            .map_err(|_| "Could not determine user startup folder".to_string())?;

        let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        let script_path = startup_dir.join(self.user_script_name());
        let vbs_content = format!(r#"Set WshShell = CreateObject("WScript.Shell")
WshShell.Run """{}""", 0, False"#, self.path_str());

        fs::write(&script_path, vbs_content)
            .map_err(|_| format!("Failed to create {} VBS startup script", self.name()))?;

        println!("User folder startup enabled for {} daemon", self.name());
        Ok(())
    }

    fn disable_user_folder_startup(&self) -> Result<(), String> {
        let startup_folder = env::var("APPDATA")
            .map_err(|_| "Could not determine user startup folder".to_string())?;

        let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        let script_path = startup_dir.join(self.user_script_name());

        if script_path.exists() {
            fs::remove_file(&script_path)
                .map_err(|_| format!("Failed to remove {} VBS startup script", self.name()))?;
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

    fn get_task_scheduler_startup_status(&self) -> bool {
        match check_scheduled_task(self.task_name()) {
            Ok(exists) => exists,
            Err(TaskSchedulerError::UnknownError(msg)) => {
                eprintln!("Warning: Failed to check task scheduler startup for {}: {}", self.name(), msg);
                false
            }
            Err(e) => {
                eprintln!("Warning: Failed to check task scheduler startup for {}: {:?}", self.name(), e);
                false
            }
        }
    }

    fn get_user_folder_startup_status(&self) -> bool {
        if let Ok(startup_folder) = env::var("APPDATA") {
            let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
            let script_path = startup_dir.join(self.user_script_name());
            script_path.exists()
        } else {
            false
        }
    }

    // All users folder startup methods
    fn enable_all_users_folder_startup(&self) -> Result<(), String> {
        let startup_folder = env::var("ALLUSERSPROFILE")
            .map_err(|_| "Could not determine all users startup folder".to_string())?;

        let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        let script_path = startup_dir.join(self.all_users_script_name());
        let vbs_content = format!(r#"Set WshShell = CreateObject("WScript.Shell")
WshShell.Run """{}""", 0, False"#, self.path_str());

        fs::write(&script_path, vbs_content)
            .map_err(|_| format!("Failed to create {} VBS startup script (admin privileges may be required)", self.name()))?;

        println!("All users folder startup enabled for {} daemon", self.name());
        Ok(())
    }

    fn disable_all_users_folder_startup(&self) -> Result<(), String> {
        let startup_folder = env::var("ALLUSERSPROFILE")
            .map_err(|_| "Could not determine all users startup folder".to_string())?;

        let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        let script_path = startup_dir.join(self.all_users_script_name());

        if script_path.exists() {
            fs::remove_file(&script_path)
                .map_err(|_| format!("Failed to remove {} VBS startup script (admin privileges may be required)", self.name()))?;
        }

        println!("All users folder startup disabled for {} daemon", self.name());
        Ok(())
    }

    fn get_all_users_folder_startup_status(&self) -> bool {
        if let Ok(startup_folder) = env::var("ALLUSERSPROFILE") {
            let startup_dir = Path::new(&startup_folder).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
            let script_path = startup_dir.join(self.all_users_script_name());
            script_path.exists()
        } else {
            false
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
}

impl Daemon for WindmenuDaemon {
    fn path(&self) -> &Path {
        &self.path
    }

    fn name(&self) -> &'static str {
        "windmenu"
    }

    fn process_name(&self) -> &'static str {
        "windmenu.exe"
    }

    fn registry_name(&self) -> &'static str {
        "WindmenuDaemon"
    }

    fn task_name(&self) -> &'static str {
        "windmenu-daemon"
    }

    fn user_script_name(&self) -> &'static str {
        "start-windmenu-daemon-user.vbs"
    }

    fn all_users_script_name(&self) -> &'static str {
        "start-windmenu-daemon-all.vbs"
    }

    fn is_running(&self) -> bool {
        // For windmenu, check if any instance other than current is running
        let current_pid = std::process::id();
        let processes = find_processes_with_name(self.process_name());

        // Get parent PID to exclude shims
        let parent_pid = processes.iter()
            .find(|p| p.pid == current_pid)
            .map(|p| p.parent_pid);

        // Exclude current process and parent process
        processes.iter().any(|proc| {
            proc.pid != current_pid && Some(proc.pid) != parent_pid
        })
    }

    fn start(&self) -> Result<(), DaemonError> {
        if self.is_running() {
            return Err(DaemonError::AlreadyRunning);
        }

        // let current_exe = env::current_exe()
        //     .map_err(|e| DaemonError::StartupFailed(format!("Failed to get current executable path: {}", e)))?;
        // I used current exe before:
        // let child = Command::new(&current_exe)
        let child = Command::new(self.path())
            .arg("--start-daemon-self-detached")  // <-- main reason for this
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
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

        // For windmenu, find all instances except current process and terminate them
        let processes = find_processes_with_name(self.process_name());
        let current_pid = std::process::id();

        let daemon_processes: Vec<_> = processes.iter()
                                                .filter(|process| process.pid != current_pid)
                                                .collect();

        if daemon_processes.is_empty() {
            return Err(DaemonError::ProcessError("No daemon processes found".to_string()));
        }

        for process in daemon_processes {
            terminate_process_by_pid(process.pid)
                .map_err(|err| DaemonError::ShutdownFailed(format!("Failed to terminate {} daemon PID {}: {}", self.name(), process.pid, err)))?;
            println!("Terminated {} process: {}", self.name(), process);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WlinesDaemon {
    pub path: PathBuf,
}

impl WlinesDaemon {
    // Named pipe name for communicating with wlines daemon
    pub const PIPE_NAME: &'static str = r"\\.\pipe\wlines_pipe";

    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self { path: path.as_ref().to_path_buf() }
    }
}

impl Daemon for WlinesDaemon {
    fn path(&self) -> &Path {
        &self.path
    }

    fn name(&self) -> &'static str {
        "wlines"
    }

    fn process_name(&self) -> &'static str {
        "wlines-daemon.exe"
    }

    fn registry_name(&self) -> &'static str {
        "WlinesDaemon"
    }

    fn task_name(&self) -> &'static str {
        "wlines-daemon"
    }

    fn user_script_name(&self) -> &'static str {
        "start-wlines-daemon-user.vbs"
    }

    fn all_users_script_name(&self) -> &'static str {
        "start-wlines-daemon-all.vbs"
    }
}

#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub is_running: bool,
    pub processes: Vec<ProcessInfo>,
    pub registry_status: bool,
    pub task_scheduler_status: bool,
    pub user_folder_status: bool,
    pub all_users_folder_status: bool,
}

impl fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Running: {}", if self.is_running { "Yes" } else { "No" })?;

        if self.is_running && !self.processes.is_empty() {
            for process in &self.processes {
                writeln!(f, "    {}", process)?;
            }
        }

        writeln!(f, "  Startup configuration:")?;
        if self.registry_status {
            writeln!(f, "    Registry: Enabled")?;
        }
        if self.task_scheduler_status {
            writeln!(f, "    Task Scheduler: Enabled")?;
        }
        if self.user_folder_status {
            writeln!(f, "    User Folder: Enabled")?;
        }
        if self.all_users_folder_status {
            writeln!(f, "    All Users Folder: Enabled")?;
        }

        Ok(())
    }
}
