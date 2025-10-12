use winapi::um::winreg::{
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, RegDeleteValueW, RegCloseKey,
    HKEY_CURRENT_USER
};
use winapi::um::winnt::{KEY_READ, KEY_WRITE, REG_SZ};
use winapi::shared::winerror::{ERROR_SUCCESS, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND};
use winapi::shared::minwindef::DWORD;

const REGISTRY_RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[derive(Debug, Clone)]
pub enum RegistryError {
    AccessDenied,
    KeyNotFound,
    UnknownError(i32),
}

pub fn check_registry_entry(entry_name: &str) -> Result<Option<String>, RegistryError> {
    unsafe {
        let run_key_name: Vec<u16> = format!("{}\0", REGISTRY_RUN_KEY)
            .encode_utf16().collect();

        let mut hkey = std::ptr::null_mut();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            run_key_name.as_ptr(),
            0,
            KEY_READ,
            &mut hkey
        );

        if result != ERROR_SUCCESS as i32 {
            return Err(match result as u32 {
                ERROR_ACCESS_DENIED => RegistryError::AccessDenied,
                ERROR_FILE_NOT_FOUND => RegistryError::KeyNotFound,
                _ => RegistryError::UnknownError(result),
            });
        }

        let value_name: Vec<u16> = format!("{}\0", entry_name).encode_utf16().collect();
        let mut buffer = vec![0u16; 1024];
        let mut buffer_size = (buffer.len() * 2) as DWORD;

        let query_result = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut u8,
            &mut buffer_size
        );

        RegCloseKey(hkey);

        match query_result as u32 {
            ERROR_SUCCESS => {
                let value = String::from_utf16_lossy(&buffer[..buffer_size as usize / 2 - 1]);
                Ok(Some(value))
            },
            ERROR_FILE_NOT_FOUND => Ok(None),
            ERROR_ACCESS_DENIED => Err(RegistryError::AccessDenied),
            _ => Err(RegistryError::UnknownError(query_result)),
        }
    }
}

pub fn add_registry_entry(entry_name: &str, entry_value: &str) -> Result<(), RegistryError> {
    unsafe {
        let run_key_name: Vec<u16> = format!("{}\0", REGISTRY_RUN_KEY)
            .encode_utf16().collect();

        let mut hkey = std::ptr::null_mut();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            run_key_name.as_ptr(),
            0,
            KEY_WRITE,
            &mut hkey
        );

        if result != ERROR_SUCCESS as i32 {
            return Err(match result as u32 {
                ERROR_ACCESS_DENIED => RegistryError::AccessDenied,
                ERROR_FILE_NOT_FOUND => RegistryError::KeyNotFound,
                _ => RegistryError::UnknownError(result),
            });
        }

        let name: Vec<u16> = format!("{}\0", entry_name).encode_utf16().collect();
        let value: Vec<u16> = format!("{}\0", entry_value).encode_utf16().collect();

        let set_result = RegSetValueExW(
            hkey,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * 2) as DWORD
        );

        RegCloseKey(hkey);

        match set_result as u32 {
            ERROR_SUCCESS => Ok(()),
            ERROR_ACCESS_DENIED => Err(RegistryError::AccessDenied),
            _ => Err(RegistryError::UnknownError(set_result)),
        }
    }
}

pub fn remove_registry_entry(entry_name: &str) -> Result<(), RegistryError> {
    unsafe {
        let run_key_name: Vec<u16> = format!("{}\0", REGISTRY_RUN_KEY)
            .encode_utf16().collect();

        let mut hkey = std::ptr::null_mut();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            run_key_name.as_ptr(),
            0,
            KEY_WRITE,
            &mut hkey
        );

        if result != ERROR_SUCCESS as i32 {
            return Err(match result as u32 {
                ERROR_ACCESS_DENIED => RegistryError::AccessDenied,
                ERROR_FILE_NOT_FOUND => RegistryError::KeyNotFound,
                _ => RegistryError::UnknownError(result),
            });
        }

        let name: Vec<u16> = format!("{}\0", entry_name).encode_utf16().collect();
        RegDeleteValueW(hkey, name.as_ptr());

        RegCloseKey(hkey);

        Ok(())
    }
}