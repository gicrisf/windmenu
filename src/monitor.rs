#![windows_subsystem = "windows"]

use std::ptr;
use std::mem;
use std::sync::Mutex;
use winapi::um::winuser::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, LoadIconW,
    PostQuitMessage, RegisterClassExW, ShowWindow, UpdateWindow, BeginPaint, EndPaint,
    InvalidateRect, SetTimer, KillTimer, DrawTextW, FillRect, FrameRect,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, IDI_APPLICATION, MSG, PAINTSTRUCT,
    SW_SHOW, WM_DESTROY, WM_PAINT, WM_TIMER, WM_CLOSE, WM_LBUTTONDOWN, WM_LBUTTONUP, WNDCLASSEXW,
    WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_MINIMIZEBOX,
    DT_CENTER, DT_VCENTER, DT_SINGLELINE, DT_LEFT, COLOR_BTNFACE,
};
use winapi::um::wingdi::{CreateSolidBrush, DeleteObject, RGB, SetBkColor, SetTextColor};
use winapi::um::processthreadsapi::{TerminateProcess, OpenProcess};
use winapi::um::winnt::{PROCESS_TERMINATE};
use std::process::Command;
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
    PROCESSENTRY32W, TH32CS_SNAPPROCESS
};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::shared::windef::{HWND, RECT, HBRUSH};
use winapi::shared::minwindef::{UINT, WPARAM, LPARAM, LRESULT};

const TIMER_ID: usize = 1;
const TIMER_INTERVAL: u32 = 2000; // 2 seconds
const BUTTON_RELEASE_TIMER_ID: usize = 2;
const BUTTON_RELEASE_DELAY: u32 = 150; // 150ms for button feedback
const STATUS_CLEAR_TIMER_ID: usize = 3;
const STATUS_CLEAR_DELAY: u32 = 5000; // 5 seconds to clear status messages

#[derive(Clone)]
struct ButtonState {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    is_pressed: bool,
}

// Global state for button press tracking
static BUTTON_STATES: Mutex<Vec<ButtonState>> = Mutex::new(Vec::new());
// Global window handle for timer operations
static mut WINDOW_HANDLE: HWND = ptr::null_mut();
// Global status message
static STATUS_MESSAGE: Mutex<String> = Mutex::new(String::new());

#[derive(Clone)]
struct ProcessInfo {
    pid: u32,
}

#[derive(Clone)]
struct DaemonStatus {
    windmenu_processes: Vec<ProcessInfo>,
    wlines_processes: Vec<ProcessInfo>,
}

// Global state for the window - using Mutex for thread safety
static DAEMON_STATUS: Mutex<DaemonStatus> = Mutex::new(DaemonStatus {
    windmenu_processes: Vec::new(),
    wlines_processes: Vec::new(),
});

fn main() {
    unsafe {
        let hinstance = winapi::um::libloaderapi::GetModuleHandleW(ptr::null());
        
        // Convert class name to wide string
        let class_name = "WindmenuMonitor\0".encode_utf16().collect::<Vec<u16>>();
        let window_title = "Windmenu Monitor\0".encode_utf16().collect::<Vec<u16>>();
        
        // Register window class
        let wnd_class = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
        };
        
        RegisterClassExW(&wnd_class);
        
        // Create window
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, // Fixed size window
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            320, // Fixed width
            290, // Fixed height
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        );
        
        if hwnd.is_null() {
            panic!("Failed to create window");
        }
        
        // Store window handle for later use
        WINDOW_HANDLE = hwnd;
        
        // Show window and start timer
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL, None);
        
        // Initial status check
        update_daemon_status();
        InvalidateRect(hwnd, ptr::null(), 1);
        
        // Message loop
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) != 0 {
            DispatchMessageW(&msg);
        }
        
        KillTimer(hwnd, TIMER_ID);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            
            // Get window dimensions
            let mut rect: RECT = mem::zeroed();
            winapi::um::winuser::GetClientRect(hwnd, &mut rect);
            
            // Set background
            let bg_brush = CreateSolidBrush(RGB(240, 240, 240));
            FillRect(hdc, &rect, bg_brush);
            DeleteObject(bg_brush as *mut _);
            
            // Draw daemon status
            draw_daemon_status(hdc, &rect);
            
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            if wparam == TIMER_ID {
                update_daemon_status();
                InvalidateRect(hwnd, ptr::null(), 1);
            } else if wparam == BUTTON_RELEASE_TIMER_ID {
                // Auto-release all pressed buttons after delay
                {
                    let mut button_states = BUTTON_STATES.lock().unwrap();
                    for button in button_states.iter_mut() {
                        button.is_pressed = false;
                    }
                }
                InvalidateRect(hwnd, ptr::null(), 1);
                KillTimer(hwnd, BUTTON_RELEASE_TIMER_ID);
            } else if wparam == STATUS_CLEAR_TIMER_ID {
                // Clear status message after delay
                {
                    let mut status = STATUS_MESSAGE.lock().unwrap();
                    status.clear();
                }
                InvalidateRect(hwnd, ptr::null(), 1);
                KillTimer(hwnd, STATUS_CLEAR_TIMER_ID);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            handle_button_press(x, y);
            InvalidateRect(hwnd, ptr::null(), 1); // Redraw to show pressed state
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            handle_button_release(x, y);
            InvalidateRect(hwnd, ptr::null(), 1); // Redraw to show released state
            0
        }
        WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_daemon_status(hdc: winapi::shared::windef::HDC, rect: &RECT) {
    let status = DAEMON_STATUS.lock().unwrap().clone();
    
    // Set text properties
    SetBkColor(hdc, RGB(240, 240, 240));
    
    // Windmenu status (moved up to start at top + 20)
    let windmenu_active = !status.windmenu_processes.is_empty();
    let windmenu_text = if windmenu_active {
        if status.windmenu_processes.len() == 1 {
            format!("● Windmenu: Active (PID: {})", status.windmenu_processes[0].pid)
        } else {
            let pids: Vec<String> = status.windmenu_processes.iter().map(|p| p.pid.to_string()).collect();
            format!("● Windmenu: Active ({} instances: {})", status.windmenu_processes.len(), pids.join(", "))
        }
    } else {
        "● Windmenu: Inactive".to_string()
    };
    let windmenu_status = format!("{}\0", windmenu_text).encode_utf16().collect::<Vec<u16>>();
    
    let mut windmenu_rect = RECT {
        left: rect.left + 20,
        top: rect.top + 20,
        right: rect.right - 20,
        bottom: rect.top + 50,
    };
    
    // Set color based on status
    if windmenu_active {
        SetTextColor(hdc, RGB(0, 128, 0)); // Green
    } else {
        SetTextColor(hdc, RGB(192, 0, 0)); // Red
    }
    
    DrawTextW(
        hdc,
        windmenu_status.as_ptr(),
        windmenu_status.len() as i32 - 1,
        &mut windmenu_rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    
    // Wlines status
    let wlines_active = !status.wlines_processes.is_empty();
    let wlines_text = if wlines_active {
        if status.wlines_processes.len() == 1 {
            format!("● Wlines: Active (PID: {})", status.wlines_processes[0].pid)
        } else {
            let pids: Vec<String> = status.wlines_processes.iter().map(|p| p.pid.to_string()).collect();
            format!("● Wlines: Active ({} instances: {})", status.wlines_processes.len(), pids.join(", "))
        }
    } else {
        "● Wlines: Inactive".to_string()
    };
    let wlines_status = format!("{}\0", wlines_text).encode_utf16().collect::<Vec<u16>>();
    
    let mut wlines_rect = RECT {
        left: rect.left + 20,
        top: rect.top + 120,
        right: rect.right - 20,
        bottom: rect.top + 150,
    };
    
    // Set color based on status
    if wlines_active {
        SetTextColor(hdc, RGB(0, 128, 0)); // Green
    } else {
        SetTextColor(hdc, RGB(192, 0, 0)); // Red
    }
    
    DrawTextW(
        hdc,
        wlines_status.as_ptr(),
        wlines_status.len() as i32 - 1,
        &mut wlines_rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    
    // Draw buttons
    draw_buttons(hdc, rect, &status);
    
    // Draw status bar
    draw_status_bar(hdc, rect);
}

unsafe fn draw_buttons(hdc: winapi::shared::windef::HDC, rect: &RECT, status: &DaemonStatus) {
    let windmenu_active = !status.windmenu_processes.is_empty();
    let wlines_active = !status.wlines_processes.is_empty();
    
    // Button layout
    let button_width = 80;
    let button_height = 25;
    let button_spacing = 10;
    
    // Position buttons under their respective status labels
    let windmenu_buttons_y = rect.top + 60; // Right under windmenu status (which ends at top + 50)
    let wlines_buttons_y = rect.top + 160; // Right under wlines status (which ends at top + 150)
    
    // Initialize button states if empty
    {
        let mut button_states = BUTTON_STATES.lock().unwrap();
        if button_states.is_empty() {
            // Windmenu buttons
            button_states.push(ButtonState {
                x: 20,
                y: windmenu_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
            button_states.push(ButtonState {
                x: 20 + button_width + button_spacing,
                y: windmenu_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
            button_states.push(ButtonState {
                x: 20 + 2 * (button_width + button_spacing),
                y: windmenu_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
            
            // Wlines buttons
            button_states.push(ButtonState {
                x: 20,
                y: wlines_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
            button_states.push(ButtonState {
                x: 20 + button_width + button_spacing,
                y: wlines_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
            button_states.push(ButtonState {
                x: 20 + 2 * (button_width + button_spacing),
                y: wlines_buttons_y,
                width: button_width,
                height: button_height,
                is_pressed: false,
            });
        }
    }
    
    // Get current button states
    let button_states = BUTTON_STATES.lock().unwrap();
    
    // Windmenu buttons
    draw_button(hdc, 20, windmenu_buttons_y, button_width, button_height, "Start", !windmenu_active, button_states.get(0).map_or(false, |b| b.is_pressed));
    draw_button(hdc, 20 + button_width + button_spacing, windmenu_buttons_y, button_width, button_height, "Restart", windmenu_active, button_states.get(1).map_or(false, |b| b.is_pressed));
    draw_button(hdc, 20 + 2 * (button_width + button_spacing), windmenu_buttons_y, button_width, button_height, "Kill", windmenu_active, button_states.get(2).map_or(false, |b| b.is_pressed));
    
    // Wlines buttons
    draw_button(hdc, 20, wlines_buttons_y, button_width, button_height, "Start", !wlines_active, button_states.get(3).map_or(false, |b| b.is_pressed));
    draw_button(hdc, 20 + button_width + button_spacing, wlines_buttons_y, button_width, button_height, "Restart", wlines_active, button_states.get(4).map_or(false, |b| b.is_pressed));
    draw_button(hdc, 20 + 2 * (button_width + button_spacing), wlines_buttons_y, button_width, button_height, "Kill", wlines_active, button_states.get(5).map_or(false, |b| b.is_pressed));
}

unsafe fn draw_status_bar(hdc: winapi::shared::windef::HDC, rect: &RECT) {
    let status_message = STATUS_MESSAGE.lock().unwrap().clone();
    
    // Use default message if no active status message
    let display_message = if status_message.is_empty() {
        "Updating every 2 seconds...".to_string()
    } else {
        status_message
    };
    
    // Status bar background
    let status_rect = RECT {
        left: rect.left,
        top: rect.bottom - 25,
        right: rect.right,
        bottom: rect.bottom,
    };
    
    let status_bg_brush = CreateSolidBrush(RGB(220, 220, 220));
    FillRect(hdc, &status_rect, status_bg_brush);
    DeleteObject(status_bg_brush as *mut _);
    
    // Status bar border
    let border_brush = CreateSolidBrush(RGB(180, 180, 180));
    let border_rect = RECT {
        left: rect.left,
        top: rect.bottom - 25,
        right: rect.right,
        bottom: rect.bottom - 24,
    };
    FillRect(hdc, &border_rect, border_brush);
    DeleteObject(border_brush as *mut _);
    
    // Status message text
    SetTextColor(hdc, RGB(64, 64, 64));
    SetBkColor(hdc, RGB(220, 220, 220));
    
    let status_text = format!("{}\0", display_message).encode_utf16().collect::<Vec<u16>>();
    let mut text_rect = RECT {
        left: rect.left + 10,
        top: rect.bottom - 23,
        right: rect.right - 10,
        bottom: rect.bottom - 2,
    };
    
    DrawTextW(
        hdc,
        status_text.as_ptr(),
        status_text.len() as i32 - 1,
        &mut text_rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe fn draw_button(hdc: winapi::shared::windef::HDC, x: i32, y: i32, width: i32, height: i32, text: &str, enabled: bool, pressed: bool) {
    // Button background - darker when pressed
    let button_color = if !enabled {
        RGB(200, 200, 200) // Disabled
    } else if pressed {
        RGB(180, 180, 180) // Pressed (darker)
    } else {
        RGB(225, 225, 225) // Normal
    };
    let brush = CreateSolidBrush(button_color);
    
    let mut button_rect = RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };
    
    // Offset the content when pressed to simulate depth
    if pressed && enabled {
        button_rect.left += 1;
        button_rect.top += 1;
        button_rect.right += 1;
        button_rect.bottom += 1;
    }
    
    FillRect(hdc, &button_rect, brush);
    DeleteObject(brush as *mut _);
    
    // Button border - different style when pressed
    let border_color = if !enabled {
        RGB(180, 180, 180)
    } else if pressed {
        RGB(120, 120, 120) // Darker border when pressed
    } else {
        RGB(160, 160, 160)
    };

    let border_brush = CreateSolidBrush(border_color);
    FrameRect(hdc, &button_rect, border_brush);
    DeleteObject(border_brush as *mut _);
    
    // Additional pressed effect - draw a subtle inner shadow
    if pressed && enabled {
        let shadow_brush = CreateSolidBrush(RGB(150, 150, 150));
        let shadow_rect = RECT {
            left: button_rect.left,
            top: button_rect.top,
            right: button_rect.left + 2,
            bottom: button_rect.bottom,
        };
        FillRect(hdc, &shadow_rect, shadow_brush);
        
        let shadow_rect_top = RECT {
            left: button_rect.left,
            top: button_rect.top,
            right: button_rect.right,
            bottom: button_rect.top + 2,
        };
        FillRect(hdc, &shadow_rect_top, shadow_brush);
        DeleteObject(shadow_brush as *mut _);
    }
    
    // Button text
    let text_color = if enabled { RGB(0, 0, 0) } else { RGB(128, 128, 128) };
    SetTextColor(hdc, text_color);
    SetBkColor(hdc, button_color);
    
    let button_text = format!("{}\0", text).encode_utf16().collect::<Vec<u16>>();
    let mut text_rect = button_rect;
    DrawTextW(
        hdc,
        button_text.as_ptr(),
        button_text.len() as i32 - 1,
        &mut text_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

fn update_daemon_status() {
    let mut status = DAEMON_STATUS.lock().unwrap();
    status.windmenu_processes = find_processes("windmenu.exe");
    
    // Check for both wlines-daemon.exe and wlines.exe
    let mut wlines_processes = find_processes("wlines-daemon.exe");
    wlines_processes.extend(find_processes("wlines.exe"));
    status.wlines_processes = wlines_processes;
}

fn find_processes(process_name: &str) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();
    
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return processes;
        }

        let mut pe32 = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..mem::zeroed()
        };

        if Process32FirstW(snapshot, &mut pe32) != 0 {
            loop {
                let current_process_name = String::from_utf16_lossy(&pe32.szExeFile);
                let current_process_name = current_process_name.trim_end_matches('\0');
                
                if current_process_name.eq_ignore_ascii_case(process_name) {
                    processes.push(ProcessInfo {
                        pid: pe32.th32ProcessID,
                    });
                }

                if Process32NextW(snapshot, &mut pe32) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }
    
    processes
}

fn find_button_at_position(x: i32, y: i32) -> Option<usize> {
    let button_states = BUTTON_STATES.lock().unwrap();
    for (index, button) in button_states.iter().enumerate() {
        if x >= button.x && x <= button.x + button.width &&
           y >= button.y && y <= button.y + button.height {
            return Some(index);
        }
    }
    None
}

fn update_status_message(message: String) {
    {
        let mut status = STATUS_MESSAGE.lock().unwrap();
        *status = message;
    }
    
    // Trigger a redraw to show the new status message
    unsafe {
        if !WINDOW_HANDLE.is_null() {
            InvalidateRect(WINDOW_HANDLE, ptr::null(), 1);
            // Start timer to clear the status message after a delay
            SetTimer(WINDOW_HANDLE, STATUS_CLEAR_TIMER_ID, STATUS_CLEAR_DELAY, None);
        }
    }
}

fn handle_button_press(x: i32, y: i32) {
    if let Some(index) = find_button_at_position(x, y) {
        let mut button_states = BUTTON_STATES.lock().unwrap();
        if let Some(button) = button_states.get_mut(index) {
            button.is_pressed = true;
        }
    }
}

fn handle_button_release(x: i32, y: i32) {
    // Execute the button action if released on the same button
    if find_button_at_position(x, y).is_some() {
        handle_button_click(x, y);
    }
    
    // Start timer to auto-release button after a short delay for visual feedback
    unsafe {
        if !WINDOW_HANDLE.is_null() {
            SetTimer(WINDOW_HANDLE, BUTTON_RELEASE_TIMER_ID, BUTTON_RELEASE_DELAY, None);
        }
    }
}

fn handle_button_click(x: i32, y: i32) {
    if let Some(button_index) = find_button_at_position(x, y) {
        match button_index {
            0 => {
                // Windmenu Start button
                start_windmenu();
            }
            1 => {
                // Windmenu Restart button
                restart_windmenu();
            }
            2 => {
                // Windmenu Kill button
                kill_windmenu();
            }
            3 => {
                // Wlines Start button
                start_wlines();
            }
            4 => {
                // Wlines Restart button
                restart_wlines();
            }
            5 => {
                // Wlines Kill button
                kill_wlines();
            }
            _ => {
                // Unknown button index
                update_status_message(format!("Unknown button clicked: {}", button_index));
            }
        }
    }
}

fn start_windmenu() {
    update_status_message("Starting Windmenu daemon...".to_string());
    
    // Try different possible locations for windmenu.exe
    let possible_paths = [
        "windmenu.exe",                           // Same directory (installed)
        ".\\windmenu.exe",                        // Same directory with explicit path
        ".\\target\\release\\windmenu.exe",       // Development build
        "target\\release\\windmenu.exe",          // Development build alternative
    ];
    
    let mut started = false;
    for path in &possible_paths {
        match Command::new(path).spawn() {
            Ok(_) => {
                update_status_message("Windmenu started successfully".to_string());
                started = true;
                break;
            },
            Err(_) => continue,
        }
    }
    
    if !started {
        update_status_message("Failed to start Windmenu: executable not found".to_string());
    }
}

fn restart_windmenu() {
    update_status_message("Restarting Windmenu daemon...".to_string());
    kill_windmenu();
    std::thread::sleep(std::time::Duration::from_millis(500)); // Wait a bit
    start_windmenu();
}

fn kill_windmenu() {
    update_status_message("Killing Windmenu processes...".to_string());
    let processes = find_processes("windmenu.exe");
    for process in processes {
        kill_process_by_pid(process.pid);
    }
}

fn kill_wlines() {
    update_status_message("Killing Wlines processes...".to_string());
    let mut processes = find_processes("wlines-daemon.exe");
    processes.extend(find_processes("wlines.exe"));
    for process in processes {
        kill_process_by_pid(process.pid);
    }
}

fn start_wlines() {
    update_status_message("Starting Wlines daemon...".to_string());
    
    // Check if daemon-config.txt exists to determine if we should use the batch script
    let config_paths = [
        "daemon-config.txt",                      // Same directory (installed)
        ".\\daemon-config.txt",                   // Same directory with explicit path
        "assets\\daemon-config.txt",              // Development build
        ".\\assets\\daemon-config.txt",           // Development build alternative
    ];
    
    let mut config_found = false;
    for config_path in &config_paths {
        if std::path::Path::new(config_path).exists() {
            config_found = true;
            break;
        }
    }
    
    let mut started = false;
    
    if config_found {
        // If config file exists, prefer using the batch script to load configuration
        let batch_paths = [
            "start-wlines-daemon.bat",                // Same directory (installed)
            ".\\start-wlines-daemon.bat",             // Same directory with explicit path
            "assets\\start-wlines-daemon.bat",        // Development build
            ".\\assets\\start-wlines-daemon.bat",     // Development build alternative
        ];
        
        for path in &batch_paths {
            match Command::new("cmd").args(["/C", path]).spawn() {
                Ok(_) => {
                    update_status_message("Wlines daemon started successfully with config".to_string());
                    started = true;
                    break;
                },
                Err(_) => continue,
            }
        }
    }
    
    // Fallback to direct executable if batch script didn't work or config not found
    if !started {
        let exe_paths = [
            "wlines-daemon.exe",                      // Same directory (installed)
            ".\\wlines-daemon.exe",                   // Same directory with explicit path
            "assets\\wlines-daemon.exe",              // Development build
            ".\\assets\\wlines-daemon.exe",           // Development build alternative
        ];
        
        for path in &exe_paths {
            match Command::new(path).spawn() {
                Ok(_) => {
                    let message = if config_found {
                        "Wlines daemon started (fallback to direct exe)".to_string()
                    } else {
                        "Wlines daemon started successfully".to_string()
                    };
                    update_status_message(message);
                    started = true;
                    break;
                },
                Err(_) => continue,
            }
        }
    }
    
    if !started {
        update_status_message("Failed to start Wlines daemon: executable not found".to_string());
    }
}

fn restart_wlines() {
    update_status_message("Restarting Wlines daemon...".to_string());
    kill_wlines();
    std::thread::sleep(std::time::Duration::from_millis(500)); // Wait a bit
    start_wlines();
}

fn kill_process_by_pid(pid: u32) {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            let result = TerminateProcess(handle, 1);
            CloseHandle(handle);
            if result != 0 {
                update_status_message(format!("Successfully terminated process PID: {}", pid));
            } else {
                update_status_message(format!("Failed to terminate process PID: {}", pid));
            }
        } else {
            update_status_message(format!("Failed to open process PID: {}", pid));
        }
    }
}
