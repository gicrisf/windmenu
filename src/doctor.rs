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

/// Registry Run value written by `autostart.ps1 enable -Method Registry`
/// (`autostart/autostart.ps1`). Probed read-only.
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
    // Absolute exe path to bake into the paste-ready PowerShell below, so the
    // suggested commands work verbatim without the loose autostart.ps1.
    let exe = env::current_exe().ok();
    let startup_enabled = matches!(startup_shortcut_path(), Some(p) if p.exists());
    match startup_shortcut_path() {
        Some(p) if p.exists() => println!("  Startup folder: enabled  ({})", p.display()),
        Some(p) => println!("  Startup folder: not set  ({})", p.display()),
        None => println!("  Startup folder: unknown (APPDATA not set)"),
    }
    let registry_enabled = read_run_key().is_some();
    match read_run_key() {
        Some(value) => println!("  Registry Run key: enabled  (HKCU\\{}\\{} -> {})", RUN_SUBKEY, RUN_VALUE_NAME, value),
        None => println!("  Registry Run key: not set"),
    }
    print_autostart_hints(exe.as_deref(), startup_enabled, registry_enabled);

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

/// Print ready-to-paste PowerShell for toggling auto-start, with the absolute
/// exe path baked in — the same actions as `autostart/autostart.ps1` (identical
/// Run key/value and `.lnk` target + `start` args), so a bare `windmenu.exe`
/// carries its own instructions without the loose helper script. Contextual:
/// an *enable* line for each method that's off, a *disable* line for each on.
fn print_autostart_hints(exe: Option<&std::path::Path>, startup_enabled: bool, registry_enabled: bool) {
    let exe = match exe {
        Some(p) => p.display().to_string(),
        // No exe path (env::current_exe failed) — the commands would be wrong.
        None => return,
    };
    let run_key = format!("HKCU:\\{}", RUN_SUBKEY);
    // A PowerShell-quoted `$env:APPDATA\...\windmenu.lnk`; inside single quotes a
    // literal `"` needs no escaping, so the baked paths stay readable.
    let lnk = "\"$([Environment]::GetFolderPath('Startup'))\\windmenu.lnk\"";

    println!();
    if startup_enabled || registry_enabled {
        println!("  Disable (paste into PowerShell):");
        if startup_enabled {
            println!("    Remove-Item {}", lnk);
        }
        if registry_enabled {
            println!("    Remove-ItemProperty -Path '{}' -Name '{}'", run_key, RUN_VALUE_NAME);
        }
    } else {
        println!("  Enable at login (paste into PowerShell) — Startup shortcut, no admin:");
        println!(
            "    $s=(New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath='{}'; $s.Arguments='start'; $s.Save()",
            lnk, exe
        );
        println!("  Enable at login — Registry Run key:");
        println!(
            "    Set-ItemProperty -Path '{}' -Name '{}' -Value '\"{}\" start'",
            run_key, RUN_VALUE_NAME, exe
        );
    }
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\windmenu.lnk` —
/// the shortcut written by the Scoop manifest and `autostart.ps1 enable`
/// (default Startup method).
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
