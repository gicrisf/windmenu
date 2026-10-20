//! `windmenu doctor`: one-shot diagnostics of everything windmenu can see —
//! config resolution, binary location/PATH, daemon state, and auto-start
//! method. Consolidates the former `status` and `config path` commands.
//!
//! House style matches the other CLI commands: plain `println!`, no color,
//! "enabled"/"not set" and "yes"/"no", full paths via `.display()`.

use std::env;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use winapi::um::winbase::CREATE_NO_WINDOW;

// use crate::apps; // re-enable with the Windows Store app count at the end of run()
use crate::daemon::WindmenuDaemon;
use crate::menu;

/// Registry Run value written by `autostart.ps1 enable -Method Registry`
/// (`autostart/autostart.ps1`). Probed read-only.
const RUN_VALUE_NAME: &str = "WindmenuDaemon";
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Scheduled task written by `autostart.ps1 enable -Method Task`. The task is
/// per-user and lives in the `\Windmenu\` folder, or in the root folder when
/// the folder cannot be created. Probed read-only with `schtasks`.
const TASK_FOLDER: &str = "\\Windmenu\\";
const TASK_ROOT: &str = "\\";
const SUPERVISOR_MUTEX: &str = "windmenu-supervisor-mutex";

struct TaskInfo {
    path: String,
    name: String,
    command: String,
    arguments: String,
    delay: String,
}

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

    let daemon_running = daemon.is_running();
    println!("Daemon: {}", if daemon_running { "running" } else { "not running" });
    let supervised = supervisor_running();
    println!("Supervisor: {}", if supervised { "running" } else { "not running" });
    if supervised && !daemon_running {
        println!("  Warning: supervisor is running but the daemon is not (restarting or crashed)");
    }
    println!();

    println!("Auto-start");
    // Absolute exe path to bake into the paste-ready PowerShell below, so the
    // suggested commands work verbatim without the loose autostart.ps1.
    let exe = env::current_exe().ok();
    let task = probe_task();
    match &task {
        Some(info) => {
            println!(
                "  Task Scheduler: enabled  ({}{}{})",
                info.path,
                info.name,
                if info.delay.is_empty() {
                    String::new()
                } else {
                    format!(", delay {}", info.delay)
                }
            );
            if !info.command.is_empty() {
                println!("    -> {} {}", info.command, info.arguments);
            }
            if let Some(current) = exe.as_deref() {
                if !info.command.is_empty() && !same_path(&info.command, current) {
                    println!("    Warning: task points to a different executable; run enable to refresh it");
                }
            }
            if !info.arguments.contains("--start-supervisor") {
                println!("    Warning: task does not run --start-supervisor; run enable to refresh it");
            }
        }
        None => println!("  Task Scheduler: not set"),
    }
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
    print_autostart_hints(exe.as_deref(), task.as_ref(), startup_enabled, registry_enabled);

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

/// Print ready-to-paste PowerShell for toggling auto-start. When the shipped
/// helper sits next to the exe, point at it (it sets the exact auto-start
/// task); otherwise print the equivalent manual commands with the absolute exe
/// path baked in.
fn print_autostart_hints(
    exe: Option<&std::path::Path>,
    task: Option<&TaskInfo>,
    startup_enabled: bool,
    registry_enabled: bool,
) {
    let exe_path = match exe {
        Some(p) => p,
        // No exe path (env::current_exe failed) — the commands would be wrong.
        None => return,
    };
    let exe = exe_path.display().to_string();
    let run_key = format!("HKCU:\\{}", RUN_SUBKEY);
    // A PowerShell-quoted `$env:APPDATA\...\windmenu.lnk`; inside single quotes a
    // literal `"` needs no escaping, so the baked paths stay readable.
    let lnk = "\"$([Environment]::GetFolderPath('Startup'))\\windmenu.lnk\"";
    let active = task.is_some() || startup_enabled || registry_enabled;

    if let Some(helper) = exe_path
        .parent()
        .map(|p| p.join("autostart.ps1"))
        .filter(|p| p.exists())
    {
        println!();
        if active {
            println!("  Disable (paste into PowerShell):");
            println!(
                "    powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" disable",
                helper.display()
            );
        } else {
            println!("  Enable at login (paste into PowerShell) — Scheduled Task, no admin:");
            println!(
                "    powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" enable",
                helper.display()
            );
        }
        return;
    }

    println!();
    if active {
        println!("  Disable (paste into PowerShell):");
        if let Some(info) = task {
            println!(
                "    Unregister-ScheduledTask -TaskName '{}' -TaskPath '{}' -Confirm:$false",
                info.name, info.path
            );
        }
        if startup_enabled {
            println!("    Remove-Item {}", lnk);
        }
        if registry_enabled {
            println!("    Remove-ItemProperty -Path '{}' -Name '{}'", run_key, RUN_VALUE_NAME);
        }
    } else {
        println!("  Enable at login — Scheduled Task (recommended, from the release zip):");
        println!("    powershell -NoProfile -ExecutionPolicy Bypass -File <path-to-autostart.ps1> enable");
        println!("  Enable at login — Startup shortcut, no admin:");
        println!(
            "    $s=(New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath='{}'; $s.Arguments='--start-supervisor'; $s.Save()",
            lnk, exe
        );
        println!("  Enable at login — Registry Run key:");
        println!(
            "    Set-ItemProperty -Path '{}' -Name '{}' -Value '\"{}\" --start-supervisor'",
            run_key, RUN_VALUE_NAME, exe
        );
    }
}

/// Read-only probe of the supervisor mutex. The daemon never opens this; it is
/// only here so `doctor` can show whether the scheduled task's wrapper is up.
fn supervisor_running() -> bool {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::synchapi::OpenMutexW;
    use winapi::um::winnt::SYNCHRONIZE;

    let name: Vec<u16> = std::ffi::OsStr::new(SUPERVISOR_MUTEX)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = OpenMutexW(SYNCHRONIZE, 0, name.as_ptr());
        if handle.is_null() {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
}

fn task_name() -> Option<String> {
    env::var("USERNAME")
        .ok()
        .filter(|user| !user.is_empty())
        .map(|user| format!("Autorun for {}", user))
}

/// Read the task's XML through `schtasks` (read-only) and pull out the action
/// and trigger delay. Checks the `\Windmenu\` folder first, then the root
/// fallback used when that folder cannot be created.
fn probe_task() -> Option<TaskInfo> {
    let name = task_name()?;
    for path in [TASK_FOLDER, TASK_ROOT] {
        let target = format!("{}{}", path, name);
        let output = Command::new("schtasks")
            .args(["/query", "/tn", &target, "/xml"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let xml = decode_windows_output(&output.stdout);
        return Some(TaskInfo {
            path: path.to_string(),
            name,
            command: xml_tag(&xml, "Command")
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            arguments: xml_tag(&xml, "Arguments").unwrap_or_default(),
            delay: xml_tag(&xml, "Delay").unwrap_or_default(),
        });
    }
    None
}

/// `schtasks` output can be UTF-16LE (with or without a BOM) when redirected.
fn decode_windows_output(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16_le(rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.len() >= 2 && bytes[0] == b'<' && bytes[1] == 0 {
        return decode_utf16_le(bytes);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_utf16_le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml_unescape(xml[start..end].trim()))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn same_path(command: &str, current: &std::path::Path) -> bool {
    command
        .trim_matches('"')
        .eq_ignore_ascii_case(&current.to_string_lossy())
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\windmenu.lnk` —
/// the shortcut written by the Scoop manifest and `autostart.ps1 enable`
/// (Startup fallback method).
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

#[cfg(test)]
mod tests {
    use super::{decode_windows_output, xml_tag, xml_unescape};

    #[test]
    fn reads_action_from_task_xml() {
        let xml = r#"<Task><Actions Context="Author"><Exec>
            <Command>C:\Tools\windmenu.exe</Command>
            <Arguments>--start-supervisor</Arguments>
        </Exec></Actions><Triggers><LogonTrigger><Delay>PT3S</Delay></LogonTrigger></Triggers></Task>"#;
        assert_eq!(xml_tag(xml, "Command").as_deref(), Some(r"C:\Tools\windmenu.exe"));
        assert_eq!(xml_tag(xml, "Arguments").as_deref(), Some("--start-supervisor"));
        assert_eq!(xml_tag(xml, "Delay").as_deref(), Some("PT3S"));
    }

    #[test]
    fn unescapes_xml_entities() {
        assert_eq!(xml_unescape("a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos;"), "a & b <c> \"d\" 'e'");
    }

    #[test]
    fn decodes_utf16le_with_bom() {
        let text = "<Command>C:\\x.exe</Command>";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_windows_output(&bytes), text);
    }
}
