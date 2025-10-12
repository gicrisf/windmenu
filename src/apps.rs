use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::os::windows::ffi::OsStrExt;
use winapi::um::fileapi::{GetFileAttributesW, INVALID_FILE_ATTRIBUTES};
use winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT;

#[derive(Debug)]
pub struct ReparsePoint {
    pub name: String,
    pub full_path: PathBuf,
    pub length: u64,
    pub attributes: u32,
}

/// Get the Windows Apps directory path from LOCALAPPDATA
pub fn get_windows_apps_path() -> Option<PathBuf> {
    env::var("LOCALAPPDATA")
        .ok()
        .map(|appdata| PathBuf::from(appdata).join("Microsoft\\WindowsApps"))
}

/// Find all reparse points in the given directory
pub fn find_reparse_points(dir: &Path) -> std::io::Result<Vec<ReparsePoint>> {
    let mut reparse_points = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        // Check if it's a reparse point using Windows API
        let attributes = get_file_attributes(&path);
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                reparse_points.push(ReparsePoint {
                    name: name.to_string(),
                    full_path: path,
                    length: metadata.len(),
                    attributes,
                });
            }
        }
    }

    // Sort by name, similar to PowerShell command
    reparse_points.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(reparse_points)
}


/// Get file attributes for a given path
pub fn get_file_attributes(path: &Path) -> u32 {
    unsafe {
        let path_wide: Vec<u16> = path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let attributes = GetFileAttributesW(path_wide.as_ptr());

        if attributes == INVALID_FILE_ATTRIBUTES {
            0
        } else {
            attributes
        }
    }
}

/// Format file attributes as a human-readable string
pub fn format_file_attributes(attributes: u32) -> String {
    let mut attr_strings = Vec::new();

    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        attr_strings.push("ReparsePoint");
    }
    if attributes & 0x1 != 0 { // FILE_ATTRIBUTE_READONLY
        attr_strings.push("ReadOnly");
    }
    if attributes & 0x2 != 0 { // FILE_ATTRIBUTE_HIDDEN
        attr_strings.push("Hidden");
    }
    if attributes & 0x4 != 0 { // FILE_ATTRIBUTE_SYSTEM
        attr_strings.push("System");
    }
    if attributes & 0x10 != 0 { // FILE_ATTRIBUTE_DIRECTORY
        attr_strings.push("Directory");
    }
    if attributes & 0x20 != 0 { // FILE_ATTRIBUTE_ARCHIVE
        attr_strings.push("Archive");
    }

    if attr_strings.is_empty() {
        "Normal".to_string()
    } else {
        attr_strings.join(", ")
    }
}

/// Print detailed information about reparse points in Windows Apps directory
pub fn print_reparse_points_info() {
    if let Some(windows_apps_path) = get_windows_apps_path() {
        println!("Scanning Windows Apps directory: {:?}", windows_apps_path);

        match find_reparse_points(&windows_apps_path) {
            Ok(reparse_points) => {
                if reparse_points.is_empty() {
                    println!("No reparse points found in Windows Apps directory");
                } else {
                    println!("Found {} reparse points:", reparse_points.len());
                    println!("{:<30} {:<10} {:<30} {}", "Name", "Length", "Attributes", "FullName");
                    println!("{}", "-".repeat(100));

                    for rp in reparse_points {
                        println!("{:<30} {:<10} {:<30} {}",
                            rp.name,
                            rp.length,
                            format_file_attributes(rp.attributes),
                            rp.full_path.display()
                        );
                    }
                }
            }
            Err(e) => {
                println!("Error scanning Windows Apps directory: {}", e);
            }
        }
    } else {
        println!("Could not determine Windows Apps directory path");
    }
}