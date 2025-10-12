use std::path::PathBuf;
use std::process::Command;

const WLINES_DAEMON_DOWNLOAD_URL: &str = "https://github.com/gicrisf/wlines/releases/download/v0.1.0/wlines-daemon.exe";
const WLINES_DOWNLOAD_URL: &str = "https://github.com/gicrisf/wlines/releases/download/v0.1.0/wlines.exe";
const WLINES_DAEMON_FILENAME: &str = "wlines-daemon.exe";
const WLINES_FILENAME: &str = "wlines.exe";

#[derive(Debug)]
pub enum FetchError {
    IoError(std::io::Error),
    DownloadFailed(String),
}

impl From<std::io::Error> for FetchError {
    fn from(error: std::io::Error) -> Self {
        FetchError::IoError(error)
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::IoError(e) => write!(f, "IO error: {}", e),
            FetchError::DownloadFailed(e) => write!(f, "Download failed: {}", e),
        }
    }
}

impl std::error::Error for FetchError {}

fn download_binary(url: &str, filename: &str) -> Result<PathBuf, FetchError> {
    let file_path = PathBuf::from(filename);

    if file_path.exists() {
        return Ok(file_path);
    }

    println!("{} not found in root directory, downloading...", filename);
    println!("Downloading from: {}", url);

    // Use PowerShell to download
    let output = Command::new("powershell")
        .args([
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                url,
                file_path.display()
            ),
        ])
        .output()?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(FetchError::DownloadFailed(format!(
            "PowerShell download failed: {}",
            error_msg
        )));
    }

    if file_path.exists() {
        println!("Successfully downloaded {}", filename);
        Ok(file_path)
    } else {
        Err(FetchError::DownloadFailed(
            "File not found after download".to_string(),
        ))
    }
}

pub fn ensure_wlines_daemon_available() -> Result<PathBuf, FetchError> {
    download_binary(WLINES_DAEMON_DOWNLOAD_URL, WLINES_DAEMON_FILENAME)
}

pub fn ensure_wlines_available() -> Result<PathBuf, FetchError> {
    download_binary(WLINES_DOWNLOAD_URL, WLINES_FILENAME)
}
