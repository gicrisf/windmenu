//! `windmenu doctor`: one-shot diagnostics of everything windmenu can see —
//! config resolution, binary location/PATH, daemon state, and auto-start
//! method. Consolidates the former `status` and `config path` commands.
//!
//! House style matches the other CLI commands: plain `println!`, no color,
//! "enabled"/"not set" and "yes"/"no", full paths via `.display()`.

use std::env;
use std::path::PathBuf;

// use crate::apps; // re-enable with the Windows Store app count at the end of run()
use crate::daemon::WindmenuDaemon;
use crate::menu;

/// Registry Run value that the NSIS installer's "Registry Run (Basic)" method
/// writes (`installer.nsi`). Probed read-only.
const RUN_VALUE_NAME: &str = "WindmenuDaemon";
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn run(daemon: &WindmenuDaemon) {
    println!("Config");
    menu::config_diagnostics();
    println!();

    println!("Binary");
    match env::current_exe() {
        Ok(p) => println!("  Path: {}", p.display()),
        Err(e) => println!("  Path: unknown ({})", e),
    }
    match crate::find_on_path("windmenu.exe") {
        Some(p) => println!("  On PATH: yes  ({})", p.display()),
        None => println!("  On PATH: no"),
    }
    println!();

    println!("Daemon: {}", if daemon.is_running() { "running" } else { "not running" });
    println!();

    println!("Auto-start");
    match startup_shortcut_path() {
        Some(p) if p.exists() => println!("  Startup folder: enabled  ({})", p.display()),
        Some(p) => println!("  Startup folder: not set  ({})", p.display()),
        None => println!("  Startup folder: unknown (APPDATA not set)"),
    }
    match read_run_key() {
        Some(value) => println!("  Registry Run key: enabled  (HKCU\\{}\\{} -> {})", RUN_SUBKEY, RUN_VALUE_NAME, value),
        None => println!("  Registry Run key: not set"),
    }

    // A bare Windows Store app count isn't an actionable health signal, and the
    // detail already lives in `windmenu test reparse-points`. Kept commented in
    // case a summary line proves useful; re-enable the `apps` import above too.
    // println!();
    // match apps::get_windows_apps_path() {
    //     Some(dir) => match apps::find_reparse_points(&dir) {
    //         Ok(points) => println!("Windows Store apps: {} detected", points.len()),
    //         Err(e) => println!("Windows Store apps: error scanning ({})", e),
    //     },
    //     None => println!("Windows Store apps: unknown (LOCALAPPDATA not set)"),
    // }
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\windmenu.lnk` —
/// the shortcut written by the Scoop manifest and the installer's Startup-folder
/// method.
fn startup_shortcut_path() -> Option<PathBuf> {
    env::var("APPDATA").ok().map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft\\Windows\\Start Menu\\Programs\\Startup\\windmenu.lnk")
    })
}

/// Read-only probe of `HKCU\...\Run\WindmenuDaemon`. Returns the value's string
/// data if the key/value exists, `None` otherwise. `KEY_READ` only — no writes,
/// so this does not trip AV heuristics.
fn read_run_key() -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::winnt::{KEY_READ, REG_SZ, WCHAR};
    use winapi::um::winreg::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER};

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let subkey = wide(RUN_SUBKEY);
    let value_name = wide(RUN_VALUE_NAME);

    unsafe {
        let mut hkey = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) as u32
            != ERROR_SUCCESS
        {
            return None;
        }

        // First query the size and type of the value.
        let mut value_type = 0u32;
        let mut data_len = 0u32;
        let status = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            std::ptr::null_mut(),
            &mut data_len,
        );
        if status as u32 != ERROR_SUCCESS || value_type != REG_SZ || data_len == 0 {
            RegCloseKey(hkey);
            return None;
        }

        // data_len is in bytes; read into a WCHAR buffer.
        let wchar_len = (data_len as usize).div_ceil(std::mem::size_of::<WCHAR>());
        let mut buf: Vec<WCHAR> = vec![0; wchar_len];
        let status = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut u8,
            &mut data_len,
        );
        RegCloseKey(hkey);
        if status as u32 != ERROR_SUCCESS {
            return None;
        }

        // Trim the trailing NUL(s) REG_SZ includes.
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}
