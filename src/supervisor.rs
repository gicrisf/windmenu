//! External lifecycle supervisor for the daemon.
//!
//! The scheduled task starts this process at logon. It spawns the ordinary
//! daemon (`--start-daemon-self-detached`) and respawns it if it exits with a
//! failure. It owns no daemon state and never opens the daemon mutex or
//! shutdown event: the daemon keeps its single-process behavior untouched.
//!
//! Contract:
//! - child exits 0 -> the supervisor exits (normal `windmenu stop`, or another
//!   daemon already held the mutex)
//! - child exits nonzero -> restart after a short, interruptible backoff
//! - `windmenu-supervisor-shutdown-event` signaled -> stop the child (the
//!   daemon's own event is signaled by `windmenu stop`) and exit

use std::env;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use winapi::shared::ntdef::HANDLE;
use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::synchapi::{
    CreateEventW, CreateMutexW, ResetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use winapi::um::winbase::{
    CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS, INFINITE, WAIT_FAILED, WAIT_OBJECT_0,
};

const MUTEX_NAME: &str = "windmenu-supervisor-mutex";
const SHUTDOWN_EVENT_NAME: &str = "windmenu-supervisor-shutdown-event";
const DAEMON_ARG: &str = "--start-daemon-self-detached";
const MIN_BACKOFF_MS: u32 = 1_000;
const MAX_BACKOFF_MS: u32 = 5_000;
const STOP_WAIT_MS: u32 = 2_000;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn create_singleton() -> Option<HANDLE> {
    let name = wide(MUTEX_NAME);
    unsafe {
        let mutex = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
        if mutex.is_null() {
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            return None;
        }
        Some(mutex)
    }
}

fn create_shutdown_event() -> HANDLE {
    let name = wide(SHUTDOWN_EVENT_NAME);
    unsafe {
        let event = CreateEventW(std::ptr::null_mut(), 1, 0, name.as_ptr());
        if !event.is_null() {
            // A previous supervisor may have left it signaled; this process is
            // the new owner, so start clean.
            ResetEvent(event);
        }
        event
    }
}

fn event_signaled(event: HANDLE, timeout_ms: u32) -> bool {
    unsafe { WaitForSingleObject(event, timeout_ms) == WAIT_OBJECT_0 }
}

fn spawn_daemon() -> Result<std::process::Child, String> {
    let exe = env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {}", e))?;
    let mut cmd = Command::new(&exe);
    cmd.arg(DAEMON_ARG)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn()
        .map_err(|e| format!("Failed to spawn daemon at '{}': {}", exe.display(), e))
}

/// Run until a stop is requested or the daemon exits cleanly.
pub fn run() {
    let mutex = match create_singleton() {
        Some(handle) => handle,
        None => return,
    };

    let event = create_shutdown_event();
    if event.is_null() {
        eprintln!("windmenu supervisor: failed to create shutdown event");
        return;
    }

    let mut backoff = MIN_BACKOFF_MS;

    loop {
        if event_signaled(event, 0) {
            break;
        }

        let mut child = match spawn_daemon() {
            Ok(child) => child,
            Err(e) => {
                eprintln!("windmenu supervisor: {}", e);
                if event_signaled(event, backoff) {
                    break;
                }
                backoff = (backoff * 2).min(MAX_BACKOFF_MS);
                continue;
            }
        };

        let started = Instant::now();
        let handles = [child.as_raw_handle() as HANDLE, event];
        let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };

        if wait == WAIT_OBJECT_0 + 1 {
            // Stop requested. `windmenu stop` also signals the daemon event,
            // so give the child a moment to leave on its own.
            unsafe {
                WaitForSingleObject(child.as_raw_handle() as HANDLE, STOP_WAIT_MS);
            }
            break;
        }

        if wait == WAIT_FAILED {
            eprintln!("windmenu supervisor: WaitForMultipleObjects failed");
        }

        let status = child.wait();
        if event_signaled(event, 0) {
            break;
        }
        match status {
            Ok(status) if status.success() => break,
            Ok(status) => {
                eprintln!("windmenu supervisor: daemon exited with {}", status);
            }
            Err(e) => {
                eprintln!("windmenu supervisor: failed to wait for daemon: {}", e);
            }
        }

        if started.elapsed() >= Duration::from_secs(10) {
            backoff = MIN_BACKOFF_MS;
        }
        if event_signaled(event, backoff) {
            break;
        }
        backoff = (backoff * 2).min(MAX_BACKOFF_MS);
    }

    unsafe {
        CloseHandle(event);
        CloseHandle(mutex);
    }
}
