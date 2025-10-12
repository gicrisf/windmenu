use std::{env, fs, process::Command};

#[derive(Debug, Clone)]
pub enum TaskSchedulerError {
    CommandNotFound,
    AccessDenied,
    UnknownError(String),
}

#[derive(Debug, Clone)]
pub struct SchTaskExec {
    pub command: String,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SchTask {
    pub date: String,
    pub author: String,
    pub description: String,
    pub logon_trigger: bool,
    pub logon_delay: String, // e.g., "PT5S" (5 seconds), "PT10S" (10 seconds), "PT1M" (1 minute)
    pub privilege: String, // "LeastPrivilege" or "HighestAvailable"
    pub multiple_instance_policy: String, // "Parallel", "Queue", "IgnoreNew", "StopExisting"
    pub disallow_start_if_on_batteries: bool,
    pub allow_hard_terminate: bool,
    pub run_only_if_network_available: bool,
    pub stop_on_idle: bool,
    pub restart_on_idle: bool,
    pub allow_start_on_demand: bool,
    pub enabled: bool,
    pub hidden: bool,
    pub run_only_if_idle: bool,
    pub wake_to_run: bool,
    pub priority: usize, // 1-10, where 1 is highest priority
    pub actions: Vec<SchTaskExec>,
}

impl SchTask {
    pub fn as_xml(&self) -> String {
        let mut xml = String::new();

        xml.push_str(r#"<?xml version="1.0" encoding="UTF-16"?>"#);
        xml.push('\n');
        xml.push_str(r#"<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">"#);
        xml.push('\n');

        // Registration Info
        xml.push_str("  <RegistrationInfo>\n");
        xml.push_str(&format!("    <Date>{}</Date>\n", self.date));
        xml.push_str(&format!("    <Author>{}</Author>\n", self.author));
        xml.push_str(&format!("    <Description>{}</Description>\n", self.description));
        xml.push_str("  </RegistrationInfo>\n");

        // Triggers
        xml.push_str("  <Triggers>\n");
        if self.logon_trigger {
            xml.push_str("    <LogonTrigger>\n");
            xml.push_str("      <Enabled>true</Enabled>\n");
            xml.push_str(&format!("      <Delay>{}</Delay>\n", self.logon_delay));
            xml.push_str("    </LogonTrigger>\n");
        }
        xml.push_str("  </Triggers>\n");

        // Principals
        xml.push_str("  <Principals>\n");
        xml.push_str("    <Principal id=\"Author\">\n");
        xml.push_str("      <LogonType>InteractiveToken</LogonType>\n");
        xml.push_str(&format!("      <RunLevel>{}</RunLevel>\n", self.privilege));
        xml.push_str("    </Principal>\n");
        xml.push_str("  </Principals>\n");

        // Settings
        xml.push_str("  <Settings>\n");
        xml.push_str(&format!("    <MultipleInstancesPolicy>{}</MultipleInstancesPolicy>\n", self.multiple_instance_policy));
        xml.push_str(&format!("    <DisallowStartIfOnBatteries>{}</DisallowStartIfOnBatteries>\n", self.disallow_start_if_on_batteries));
        xml.push_str(&format!("    <StopIfGoingOnBatteries>{}</StopIfGoingOnBatteries>\n", !self.disallow_start_if_on_batteries));
        xml.push_str(&format!("    <AllowHardTerminate>{}</AllowHardTerminate>\n", self.allow_hard_terminate));
        xml.push_str("    <StartWhenAvailable>true</StartWhenAvailable>\n");
        xml.push_str(&format!("    <RunOnlyIfNetworkAvailable>{}</RunOnlyIfNetworkAvailable>\n", self.run_only_if_network_available));
        xml.push_str("    <IdleSettings>\n");
        xml.push_str(&format!("      <StopOnIdleEnd>{}</StopOnIdleEnd>\n", self.stop_on_idle));
        xml.push_str(&format!("      <RestartOnIdle>{}</RestartOnIdle>\n", self.restart_on_idle));
        xml.push_str("    </IdleSettings>\n");
        xml.push_str(&format!("    <AllowStartOnDemand>{}</AllowStartOnDemand>\n", self.allow_start_on_demand));
        xml.push_str(&format!("    <Enabled>{}</Enabled>\n", self.enabled));
        xml.push_str(&format!("    <Hidden>{}</Hidden>\n", self.hidden));
        xml.push_str(&format!("    <RunOnlyIfIdle>{}</RunOnlyIfIdle>\n", self.run_only_if_idle));
        xml.push_str(&format!("    <WakeToRun>{}</WakeToRun>\n", self.wake_to_run));
        xml.push_str("    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n");
        xml.push_str(&format!("    <Priority>{}</Priority>\n", self.priority));
        xml.push_str("  </Settings>\n");

        // Actions
        xml.push_str("  <Actions Context=\"Author\">\n");
        for action in &self.actions {
            xml.push_str("    <Exec>\n");
            xml.push_str(&format!("      <Command>{}</Command>\n", action.command));
            if let Some(ref wd) = action.working_directory {
                xml.push_str(&format!("      <WorkingDirectory>{}</WorkingDirectory>\n", wd));
            }
            xml.push_str("    </Exec>\n");
        }
        xml.push_str("  </Actions>\n");

        xml.push_str("</Task>");

        xml
    }

    pub fn write_to_disk(&self, task_name: &str) -> Result<(), String> {
        let temp_dir = env::temp_dir();
        let xml_path = temp_dir.join(format!("{}.xml", task_name));

        let xml_content = self.as_xml();

        fs::write(&xml_path, xml_content)
            .map_err(|_| format!("Failed to write {} XML file", task_name))?;

        let output = Command::new("schtasks")
            .args(&["/create", "/tn", task_name, "/xml", &xml_path.to_string_lossy(), "/f"])
            .output()
            .map_err(|_| "Failed to execute schtasks command".to_string())?;

        fs::remove_file(&xml_path).ok(); // Clean up temp file

        if output.status.success() {
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to create scheduled task '{}': {}", task_name, error))
        }
    }
}

pub fn check_scheduled_task(task_name: &str) -> Result<bool, TaskSchedulerError> {
    let output = Command::new("schtasks")
        .args(&["/query", "/tn", task_name])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TaskSchedulerError::CommandNotFound
            } else {
                TaskSchedulerError::UnknownError(format!("Failed to execute schtasks: {}", e))
            }
        })?;

    if output.status.success() {
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("access is denied") {
            Err(TaskSchedulerError::AccessDenied)
        } else if stderr.contains("the system cannot find") || stderr.contains("does not exist") {
            Ok(false)
        } else {
            Err(TaskSchedulerError::UnknownError(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    }
}

pub fn delete_task(task_name: &str) {
    let output = Command::new("schtasks")
        .args(&["/delete", "/tn", task_name, "/f"])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            // Task deleted successfully
        }
        _ => {
            // Task may not have existed, which is fine
        }
    }
}
