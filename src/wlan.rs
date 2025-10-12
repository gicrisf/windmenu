use std::ptr;
use std::sync::{Arc, Mutex, Condvar};
use std::time::Duration;
use std::fmt;
use winapi::shared::minwindef::{DWORD, LPVOID};
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::winnt::HANDLE;
use winapi::um::wlanapi::{
    WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetAvailableNetworkList,
    WlanOpenHandle, WlanScan, WlanRegisterNotification, WLAN_AVAILABLE_NETWORK,
    WLAN_AVAILABLE_NETWORK_LIST, WLAN_INTERFACE_INFO_LIST, WLAN_NOTIFICATION_SOURCE_ACM,
    WLAN_NOTIFICATION_DATA,
};

#[derive(Debug)]
pub enum WlanError {
    ApiError { code: DWORD, operation: String },
    Timeout(Duration),
    ScanFailed,
    NoInterfaces,
    InvalidGuid(String),
    RegistrationFailed(DWORD),
}

impl fmt::Display for WlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WlanError::ApiError { code, operation } => {
                write!(f, "WLAN API error during {}: code {}", operation, code)
            }
            WlanError::Timeout(duration) => {
                write!(f, "WLAN operation timed out after {:?}", duration)
            }
            WlanError::ScanFailed => write!(f, "WLAN scan failed"),
            WlanError::NoInterfaces => write!(f, "No WLAN interfaces found"),
            WlanError::InvalidGuid(guid) => write!(f, "Invalid GUID: {}", guid),
            WlanError::RegistrationFailed(code) => {
                write!(f, "WLAN notification registration failed: code {}", code)
            }
        }
    }
}

impl std::error::Error for WlanError {}

#[derive(Debug, Clone, PartialEq)]
pub enum AcmNotificationCode {
    AutoconfEnabled = 1,
    AutoconfDisabled = 2,
    BackgroundScanEnabled = 3,
    BackgroundScanDisabled = 4,
    BssTypeChange = 5,
    PowerSettingChange = 6,
    ScanComplete = 7,
    ScanFail = 8,
    ConnectionStart = 9,
    ConnectionComplete = 10,
    ConnectionAttemptFail = 11,
    FilterListChange = 12,
    InterfaceArrival = 13,
    InterfaceRemoval = 14,
    ProfileChange = 15,
    ProfileNameChange = 16,
    ProfilesExhausted = 17,
    NetworkNotAvailable = 18,
    NetworkAvailable = 19,
    Disconnecting = 20,
    Disconnected = 21,
    AdhocNetworkStateChange = 22,
    ProfileUnblocked = 23,
    ScreenPowerChange = 24,
    ProfileBlocked = 25,
    ScanListRefresh = 26,
    OperationalStateChange = 27,
    Unknown,
}

impl From<u32> for AcmNotificationCode {
    fn from(code: u32) -> Self {
        match code {
            1 => AcmNotificationCode::AutoconfEnabled,
            2 => AcmNotificationCode::AutoconfDisabled,
            3 => AcmNotificationCode::BackgroundScanEnabled,
            4 => AcmNotificationCode::BackgroundScanDisabled,
            5 => AcmNotificationCode::BssTypeChange,
            6 => AcmNotificationCode::PowerSettingChange,
            7 => AcmNotificationCode::ScanComplete,
            8 => AcmNotificationCode::ScanFail,
            9 => AcmNotificationCode::ConnectionStart,
            10 => AcmNotificationCode::ConnectionComplete,
            11 => AcmNotificationCode::ConnectionAttemptFail,
            12 => AcmNotificationCode::FilterListChange,
            13 => AcmNotificationCode::InterfaceArrival,
            14 => AcmNotificationCode::InterfaceRemoval,
            15 => AcmNotificationCode::ProfileChange,
            16 => AcmNotificationCode::ProfileNameChange,
            17 => AcmNotificationCode::ProfilesExhausted,
            18 => AcmNotificationCode::NetworkNotAvailable,
            19 => AcmNotificationCode::NetworkAvailable,
            20 => AcmNotificationCode::Disconnecting,
            21 => AcmNotificationCode::Disconnected,
            22 => AcmNotificationCode::AdhocNetworkStateChange,
            23 => AcmNotificationCode::ProfileUnblocked,
            24 => AcmNotificationCode::ScreenPowerChange,
            25 => AcmNotificationCode::ProfileBlocked,
            26 => AcmNotificationCode::ScanListRefresh,
            27 => AcmNotificationCode::OperationalStateChange,
            _ => AcmNotificationCode::Unknown,
        }
    }
}

impl std::fmt::Display for AcmNotificationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AcmNotificationCode::AutoconfEnabled => write!(f, "Autoconf Enabled"),
            AcmNotificationCode::AutoconfDisabled => write!(f, "Autoconf Disabled"),
            AcmNotificationCode::BackgroundScanEnabled => write!(f, "Background Scan Enabled"),
            AcmNotificationCode::BackgroundScanDisabled => write!(f, "Background Scan Disabled"),
            AcmNotificationCode::BssTypeChange => write!(f, "BSS Type Change"),
            AcmNotificationCode::PowerSettingChange => write!(f, "Power Setting Change"),
            AcmNotificationCode::ScanComplete => write!(f, "Scan Complete"),
            AcmNotificationCode::ScanFail => write!(f, "Scan Fail"),
            AcmNotificationCode::ConnectionStart => write!(f, "Connection Start"),
            AcmNotificationCode::ConnectionComplete => write!(f, "Connection Complete"),
            AcmNotificationCode::ConnectionAttemptFail => write!(f, "Connection Attempt Fail"),
            AcmNotificationCode::FilterListChange => write!(f, "Filter List Change"),
            AcmNotificationCode::InterfaceArrival => write!(f, "Interface Arrival"),
            AcmNotificationCode::InterfaceRemoval => write!(f, "Interface Removal"),
            AcmNotificationCode::ProfileChange => write!(f, "Profile Change"),
            AcmNotificationCode::ProfileNameChange => write!(f, "Profile Name Change"),
            AcmNotificationCode::ProfilesExhausted => write!(f, "Profiles Exhausted"),
            AcmNotificationCode::NetworkNotAvailable => write!(f, "Network Not Available"),
            AcmNotificationCode::NetworkAvailable => write!(f, "Network Available"),
            AcmNotificationCode::Disconnecting => write!(f, "Disconnecting"),
            AcmNotificationCode::Disconnected => write!(f, "Disconnected"),
            AcmNotificationCode::AdhocNetworkStateChange => write!(f, "Ad Hoc Network State Change"),
            AcmNotificationCode::ProfileUnblocked => write!(f, "Profile Unblocked"),
            AcmNotificationCode::ScreenPowerChange => write!(f, "Screen Power Change"),
            AcmNotificationCode::ProfileBlocked => write!(f, "Profile Blocked"),
            AcmNotificationCode::ScanListRefresh => write!(f, "Scan List Refresh"),
            AcmNotificationCode::OperationalStateChange => write!(f, "Operational State Change"),
            AcmNotificationCode::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceState {
    NotReady = 0,
    Connected = 1,
    AdHocNetworkFormed = 2,
    Disconnecting = 3,
    Disconnected = 4,
    Associating = 5,
    Discovering = 6,
    Authenticating = 7,
    Unknown,
}

impl From<u32> for InterfaceState {
    fn from(state: u32) -> Self {
        match state {
            0 => InterfaceState::NotReady,
            1 => InterfaceState::Connected,
            2 => InterfaceState::AdHocNetworkFormed,
            3 => InterfaceState::Disconnecting,
            4 => InterfaceState::Disconnected,
            5 => InterfaceState::Associating,
            6 => InterfaceState::Discovering,
            7 => InterfaceState::Authenticating,
            _ => InterfaceState::Unknown,
        }
    }
}

impl std::fmt::Display for InterfaceState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            InterfaceState::NotReady => write!(f, "Not Ready"),
            InterfaceState::Connected => write!(f, "Connected"),
            InterfaceState::AdHocNetworkFormed => write!(f, "Ad Hoc Network Formed"),
            InterfaceState::Disconnecting => write!(f, "Disconnecting"),
            InterfaceState::Disconnected => write!(f, "Disconnected"),
            InterfaceState::Associating => write!(f, "Associating"),
            InterfaceState::Discovering => write!(f, "Discovering"),
            InterfaceState::Authenticating => write!(f, "Authenticating"),
            InterfaceState::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub guid: String,
    pub description: String,
    pub state: InterfaceState,
}

impl InterfaceInfo {
    fn from_raw(raw: &winapi::um::wlanapi::WLAN_INTERFACE_INFO) -> Self {
        let guid = &raw.InterfaceGuid;
        // format guid
        let guid = format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            guid.Data1,
            guid.Data2,
            guid.Data3,
            guid.Data4[0],
            guid.Data4[1],
            guid.Data4[2],
            guid.Data4[3],
            guid.Data4[4],
            guid.Data4[5],
            guid.Data4[6],
            guid.Data4[7]
        );
        // wide string to string
        let description = &raw.strInterfaceDescription;
        let description = String::from_utf16_lossy(
            &description
                .iter()
                .take_while(|&&c| c != 0)
                .copied()
                .collect::<Vec<u16>>(),
        );

        let state = InterfaceState::from(raw.isState);

        Self {
            guid,
            description,
            state,
        }
    }

    pub fn guid_raw(&self) -> Result<winapi::shared::guiddef::GUID, WlanError> {
        let cleaned = self.guid.trim_matches(|c| c == '{' || c == '}');
        let parts: Vec<&str> = cleaned.split('-').collect();

        if parts.len() != 5 {
            return Err(WlanError::InvalidGuid(self.guid.clone()));
        }

        let data1 = u32::from_str_radix(parts[0], 16)
            .map_err(|_| WlanError::InvalidGuid(self.guid.clone()))?;
        let data2 = u16::from_str_radix(parts[1], 16)
            .map_err(|_| WlanError::InvalidGuid(self.guid.clone()))?;
        let data3 = u16::from_str_radix(parts[2], 16)
            .map_err(|_| WlanError::InvalidGuid(self.guid.clone()))?;

        let mut data4 = [0u8; 8];
        let part3_bytes = u16::from_str_radix(parts[3], 16)
            .map_err(|_| WlanError::InvalidGuid(self.guid.clone()))?
            .to_be_bytes();
        data4[0] = part3_bytes[0];
        data4[1] = part3_bytes[1];

        let part4_str = parts[4];
        if part4_str.len() != 12 {
            return Err(WlanError::InvalidGuid(self.guid.clone()));
        }

        for i in 0..6 {
            let start = i * 2;
            let end = start + 2;
            data4[i + 2] = u8::from_str_radix(&part4_str[start..end], 16)
                .map_err(|_| WlanError::InvalidGuid(self.guid.clone()))?;
        }

        Ok(winapi::shared::guiddef::GUID {
            Data1: data1,
            Data2: data2,
            Data3: data3,
            Data4: data4,
        })
    }
}

impl std::fmt::Display for InterfaceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "  GUID: {}\n  Description: {}\n  State: {}",
            self.guid, self.description, self.state
        )
    }
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub ssid: String,
    pub signal_quality: u32,
    pub security_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum NotificationState {
    ScanWaiting,
    ScanComplete,
    ScanFailed,
}

struct NotificationContext {
    state: Mutex<NotificationState>,
    condvar: Condvar,
}

pub struct WlanClient {
    handle: HANDLE,
    notification_context: Arc<NotificationContext>,
    context_ptr: *const NotificationContext, // Keep ptr to clean up properly
}

impl WlanClient {
    pub unsafe fn new() -> Result<Self, WlanError> {
        let mut handle: HANDLE = ptr::null_mut();
        let mut negotiated_version: DWORD = 0;

        let result = WlanOpenHandle(
            2, // Client version for Windows Vista and later
            ptr::null_mut(),
            &mut negotiated_version,
            &mut handle,
        );

        if result != ERROR_SUCCESS {
            return Err(WlanError::ApiError {
                code: result,
                operation: "WlanOpenHandle".to_string(),
            });
        }

        let notification_context = Arc::new(NotificationContext {
            state: Mutex::new(NotificationState::ScanWaiting),
            condvar: Condvar::new(),
        });

        let context_ptr = Arc::into_raw(notification_context.clone());

        let result = WlanRegisterNotification(
            handle,
            WLAN_NOTIFICATION_SOURCE_ACM,
            true as i32,
            Some(wlan_notification_callback),
            context_ptr as LPVOID,
            ptr::null_mut(),
            ptr::null_mut(),
        );

        if result != ERROR_SUCCESS {
            // Clean up the Arc if registration failed
            let _ = Arc::from_raw(context_ptr);
            WlanCloseHandle(handle, ptr::null_mut());
            return Err(WlanError::RegistrationFailed(result));
        }

        Ok(Self {
            handle,
            notification_context,
            context_ptr,
        })
    }


    pub fn get_interfaces(&self) -> Result<Vec<InterfaceInfo>, WlanError> {
        unsafe {
            let mut interface_list: *mut WLAN_INTERFACE_INFO_LIST = ptr::null_mut();

            let result = WlanEnumInterfaces(self.handle, ptr::null_mut(), &mut interface_list);

            if result != ERROR_SUCCESS {
                return Err(WlanError::ApiError {
                    code: result,
                    operation: "WlanEnumInterfaces".to_string(),
                });
            }

            if interface_list.is_null() {
                return Err(WlanError::NoInterfaces);
            }

            let num_interfaces = (*interface_list).dwNumberOfItems;

            if num_interfaces == 0 {
                WlanFreeMemory(interface_list as LPVOID);
                return Err(WlanError::NoInterfaces);
            }

            let interfaces = &(*interface_list).InterfaceInfo[..num_interfaces as usize];

            let result = interfaces
                .iter()
                .map(|raw| InterfaceInfo::from_raw(raw))
                .collect();

            WlanFreeMemory(interface_list as LPVOID);

            Ok(result)
        }
    }

    pub fn scan_networks(
        &self,
        interface_guid: &winapi::shared::guiddef::GUID,
        timeout: Duration,
    ) -> Result<(), WlanError> {
        // Reset notification state
        {
            let mut state = self.notification_context.state.lock().unwrap();
            *state = NotificationState::ScanWaiting;
        }

        // Trigger scan
        unsafe {
            let result = WlanScan(
                self.handle,
                interface_guid,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );

            if result != ERROR_SUCCESS {
                return Err(WlanError::ApiError {
                    code: result,
                    operation: "WlanScan".to_string(),
                });
            }
        }

        // Wait for scan completion using condvar
        let state = self.notification_context.state.lock().unwrap();
        let result = self.notification_context.condvar
            .wait_timeout_while(state, timeout, |s| *s == NotificationState::ScanWaiting)
            .unwrap();

        match *result.0 {
            NotificationState::ScanComplete => Ok(()),
            NotificationState::ScanFailed => Err(WlanError::ScanFailed),
            NotificationState::ScanWaiting => {
                if result.1.timed_out() {
                    Err(WlanError::Timeout(timeout))
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn get_available_networks(
        &self,
        interface_guid: &winapi::shared::guiddef::GUID,
    ) -> Result<Vec<ScanResult>, WlanError> {
        unsafe {
            let mut network_list: *mut WLAN_AVAILABLE_NETWORK_LIST = ptr::null_mut();

            let result = WlanGetAvailableNetworkList(
                self.handle,
                interface_guid,
                0,
                ptr::null_mut(),
                &mut network_list,
            );

            if result != ERROR_SUCCESS {
                return Err(WlanError::ApiError {
                    code: result,
                    operation: "WlanGetAvailableNetworkList".to_string(),
                });
            }

            if network_list.is_null() {
                return Ok(Vec::new());
            }

            let num_networks = (*network_list).dwNumberOfItems as usize;
            let network_ptr = (*network_list).Network.as_ptr();

            let mut results = Vec::new();
            for i in 0..num_networks {
                let network = &*network_ptr.add(i);
                results.push(ScanResult::from_raw(network));
            }

            WlanFreeMemory(network_list as LPVOID);

            Ok(results)
        }
    }

    pub fn scan_and_get_networks(
        &self,
        interface_guid: &winapi::shared::guiddef::GUID,
        timeout: Duration,
    ) -> Result<Vec<ScanResult>, WlanError> {
        self.scan_networks(interface_guid, timeout)?;
        self.get_available_networks(interface_guid)
    }
}

impl Drop for WlanClient {
    fn drop(&mut self) {
        unsafe {
            // Unregister notification first
            WlanRegisterNotification(
                self.handle,
                WLAN_NOTIFICATION_SOURCE_ACM,
                false as i32,
                None,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );

            // Now it's safe to reclaim and drop the Arc
            if !self.context_ptr.is_null() {
                let _ = Arc::from_raw(self.context_ptr);
            }

            WlanCloseHandle(self.handle, ptr::null_mut());
        }
    }
}

impl ScanResult {
    unsafe fn from_raw(network: &WLAN_AVAILABLE_NETWORK) -> Self {
        let ssid_len = network.dot11Ssid.uSSIDLength as usize;
        let ssid_bytes = &network.dot11Ssid.ucSSID[..ssid_len.min(32)];
        let ssid = String::from_utf8_lossy(ssid_bytes).to_string();

        Self {
            ssid,
            signal_quality: network.wlanSignalQuality,
            security_enabled: network.bSecurityEnabled != 0,
        }
    }
}

unsafe extern "system" fn wlan_notification_callback(
    data: *mut WLAN_NOTIFICATION_DATA,
    context: LPVOID,
) {
    if data.is_null() || context.is_null() {
        return;
    }

    let context_ptr = context as *const NotificationContext;
    let context_arc = Arc::from_raw(context_ptr);

    if let Ok(mut state) = context_arc.state.lock() {
        let notification_code = (*data).NotificationCode;

        match notification_code {
            7 => {
                *state = NotificationState::ScanComplete;
                context_arc.condvar.notify_one();
            }
            8 => {
                *state = NotificationState::ScanFailed;
                context_arc.condvar.notify_one();
            }
            _ => {} // Ignore other notifications
        }
    }

    // Don't drop the Arc - convert it back to raw to keep it alive
    let _ = Arc::into_raw(context_arc);
}

/// Convenience function to enumerate all WLAN interfaces
pub fn enumerate_wlan_interfaces() -> Result<Vec<InterfaceInfo>, WlanError> {
    unsafe {
        let client = WlanClient::new()?;
        client.get_interfaces()
    }
}

/// Scan and get available networks for all interfaces
pub fn scan_all_wlan_interfaces(
    timeout: Duration,
) -> Result<Vec<(InterfaceInfo, Vec<ScanResult>)>, WlanError> {
    unsafe {
        let client = WlanClient::new()?;
        let interfaces = client.get_interfaces()?;

        let mut results = Vec::new();
        for interface in interfaces {
            let guid = interface.guid_raw()?;
            match client.scan_and_get_networks(&guid, timeout) {
                Ok(networks) => results.push((interface, networks)),
                Err(_) => continue,
            }
        }

        Ok(results)
    }
}

/// Print detailed information about all WLAN interfaces
pub fn print_wlan_interfaces_info() {
    match enumerate_wlan_interfaces() {
        Ok(interfaces) => {
            if interfaces.is_empty() {
                println!("No WLAN interfaces found.");
            } else {
                println!("Found {} WLAN interface(s):\n", interfaces.len());
                for (i, interface) in interfaces.iter().enumerate() {
                    println!("Interface {}:\n{}\n", i + 1, interface);
                }
            }
        }
        Err(err) => {
            println!("Error enumerating WLAN interfaces: {}", err);
        }
    }
}

/// Test function to trigger WLAN scan on all interfaces and display results
pub fn test_wlan_scan() {
    println!("Scanning WLAN networks on all interfaces...\n");

    let timeout = Duration::from_secs(10);

    match scan_all_wlan_interfaces(timeout) {
        Ok(results) => {
            if results.is_empty() {
                println!("No WLAN interfaces found or no networks detected.");
            } else {
                for (interface, networks) in results {
                    println!("Interface: {}", interface.description);
                    println!("  GUID: {}", interface.guid);
                    println!("  State: {}", interface.state);
                    println!("  Found {} network(s):", networks.len());

                    if networks.is_empty() {
                        println!("    (no networks found)");
                    } else {
                        for (i, network) in networks.iter().enumerate() {
                            println!(
                                "    {}. {} - Signal: {}%, Security: {}",
                                i + 1,
                                network.ssid,
                                network.signal_quality,
                                if network.security_enabled {
                                    "Enabled"
                                } else {
                                    "Open"
                                }
                            );
                        }
                    }
                    println!();
                }
            }
        }
        Err(err) => {
            println!("Error scanning WLAN interfaces: {}", err);
        }
    }
}
