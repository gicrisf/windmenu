use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
    PROCESSENTRY32W, TH32CS_SNAPPROCESS
};
use winapi::um::processthreadsapi::{TerminateProcess, OpenProcess};
use winapi::um::winnt::PROCESS_TERMINATE;
use winapi::um::handleapi::CloseHandle;
use winapi::shared::minwindef::FALSE;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub name: String,
    pub pid: u32,
    pub parent_pid: u32,
    pub thread_count: u32,
    pub base_priority: i32,
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PID {}: {} (parent: {}, threads: {}, priority: {})",
               self.pid, self.name, self.parent_pid, self.thread_count, self.base_priority)
    }
}

pub fn find_processes_with_name(target_name: &str) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return processes;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != FALSE {
            loop {
                let process_name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&x| x == 0).unwrap_or(entry.szExeFile.len())]
                );

                if process_name.eq_ignore_ascii_case(target_name) {
                    processes.push(ProcessInfo {
                        name: process_name,
                        pid: entry.th32ProcessID,
                        parent_pid: entry.th32ParentProcessID,
                        thread_count: entry.cntThreads,
                        base_priority: entry.pcPriClassBase,
                    });
                }

                if Process32NextW(snapshot, &mut entry) == FALSE {
                    break;
                }
            }
        }

        winapi::um::handleapi::CloseHandle(snapshot);
    }

    processes
}

pub fn find_first_process_with_name(target_name: &str) -> Option<ProcessInfo> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != FALSE {
            loop {
                let process_name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&x| x == 0).unwrap_or(entry.szExeFile.len())]
                );

                if process_name.eq_ignore_ascii_case(target_name) {
                    let result = ProcessInfo {
                        name: process_name,
                        pid: entry.th32ProcessID,
                        parent_pid: entry.th32ParentProcessID,
                        thread_count: entry.cntThreads,
                        base_priority: entry.pcPriClassBase,
                    };
                    winapi::um::handleapi::CloseHandle(snapshot);
                    return Some(result);
                }

                if Process32NextW(snapshot, &mut entry) == FALSE {
                    break;
                }
            }
        }

        winapi::um::handleapi::CloseHandle(snapshot);
        None
    }
}

pub fn terminate_process_by_pid(pid: u32) -> Result<(), String> {
    unsafe {
        let process_handle = OpenProcess(PROCESS_TERMINATE, FALSE, pid);
        if process_handle == std::ptr::null_mut() {
            return Err(format!("Failed to open process with PID {}", pid));
        }

        let result = TerminateProcess(process_handle, 0);
        CloseHandle(process_handle);

        if result == 0 {
            Err(format!("Failed to terminate process with PID {}", pid))
        } else {
            Ok(())
        }
    }
}