//! Native port of the wlines menu renderer (wlines.c by JerwuQu, daemon fork
//! by gicrisf). Renders a dmenu-style selection window via GDI in-process and
//! returns the user's selection, replacing the old wlines-daemon named-pipe
//! IPC and wlines.exe subprocess fallback.

use std::ffi::OsStr;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{COLORREF, HBITMAP, HDC, HFONT, HWND, RECT, HBRUSH, HMENU};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::wingdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject,
    GetStockObject, Rectangle, SelectObject, SetBkColor, SetBkMode, SetDCBrushColor,
    SetDCPenColor, SetTextColor, DC_BRUSH, DC_PEN, FW_NORMAL, SRCCOPY, TRANSPARENT,
};
use winapi::um::winuser::{
    AttachThreadInput, BeginPaint, BringWindowToTop, CallWindowProcW, CreateWindowExW,
    DefWindowProcW, DestroyWindow, DispatchMessageW, DrawTextW, EndPaint, GetForegroundWindow,
    GetKeyState, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, KillTimer, LoadCursorW,
    PostQuitMessage, RedrawWindow, RegisterClassExW, SendMessageW, SetFocus, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowLongW, SetWindowTextW, ShowWindow, TranslateMessage,
    UpdateWindow, COLOR_WINDOW, DT_CALCRECT, DT_END_ELLIPSIS, DT_NOCLIP, DT_NOPREFIX,
    EC_LEFTMARGIN, EC_RIGHTMARGIN, EM_GETSEL, EM_SETMARGINS, EM_SETSEL, ES_AUTOHSCROLL,
    ES_AUTOVSCROLL, ES_LEFT, GWLP_USERDATA, GWLP_WNDPROC, GWL_STYLE, IDC_ARROW, MSG,
    RDW_INVALIDATE, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, VK_CONTROL, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_SHIFT, VK_UP, WM_CHAR,
    WM_CLOSE, WM_CTLCOLOREDIT, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE, PAINTSTRUCT,
};

const WND_CLASS: &str = "windmenu_wlines_window";
const FOREGROUND_TIMER_ID: usize = 1;
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
const DRAWTEXT_PARAMS: UINT = DT_NOCLIP | DT_NOPREFIX | DT_END_ELLIPSIS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Complete,
    Keywords,
}

impl FilterMode {
    pub fn parse(s: &str) -> FilterMode {
        match s.to_ascii_lowercase().as_str() {
            "keywords" | "1" => FilterMode::Keywords,
            _ => FilterMode::Complete,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub line_count: usize,
    pub prompt: Option<String>,
    pub filter_mode: FilterMode,
    pub initial_index: usize,
    pub padding: i32,
    pub width: i32, // 0 = full screen width
    pub center_window: bool,
    pub case_sensitive: bool,
    pub bg: COLORREF,
    pub fg: COLORREF,
    pub bg_select: COLORREF,
    pub fg_select: COLORREF,
    pub bg_edit: COLORREF,
    pub fg_edit: COLORREF,
    pub font_name: String,
    pub font_size: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            line_count: 15,
            prompt: None,
            filter_mode: FilterMode::Complete,
            initial_index: 0,
            padding: 4,
            width: 0,
            center_window: false,
            case_sensitive: false,
            bg: parse_color("#000000").unwrap(),
            fg: parse_color("#ffffff").unwrap(),
            bg_select: parse_color("#ffffff").unwrap(),
            fg_select: parse_color("#000000").unwrap(),
            bg_edit: parse_color("#111111").unwrap(),
            fg_edit: parse_color("#ffffff").unwrap(),
            font_name: "Courier New".to_string(),
            font_size: 24,
        }
    }
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color into a Windows COLORREF (BGR).
pub fn parse_color(s: &str) -> Option<COLORREF> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return None;
    }
    let r = u32::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u32::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u32::from_str_radix(&hex[4..6], 16).ok()?;
    Some((b << 16) | (g << 8) | r)
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

struct Entry {
    text: String,
    lower: String,
    wide: Vec<u16>,
}

struct State {
    settings: Settings,
    font: HFONT,
    main_wnd: HWND,
    edit_wnd: HWND,
    edit_proc: isize, // original EDIT wndproc
    width: i32,
    height: i32,
    line_count: usize,
    had_foreground: bool,
    prompt_width: i32,
    prompt_wide: Option<Vec<u16>>,

    entries: Vec<Entry>,
    search_results: Vec<usize>, // indices into `entries`
    selected: Option<usize>,    // index into `search_results`

    // Double-buffer for WM_PAINT (owned per invocation, unlike the C static)
    buffer_dc: HDC,
    buffer_bitmap: HBITMAP,

    done: bool,
    result: Option<String>,
}

impl State {
    fn font_hmargin(&self) -> i32 {
        self.settings.font_size / 6
    }

    fn entries_top(&self) -> i32 {
        self.settings.font_size + self.settings.padding
    }

    fn page_start(&self) -> usize {
        if self.line_count == 0 {
            return 0;
        }
        (self.selected.unwrap_or(0) / self.line_count) * self.line_count
    }
}

/// Show the menu and block until the user selects an entry, submits custom
/// text (Shift+Enter), or cancels (Escape / focus loss / close). Must be
/// called from a thread that can own a window and pump messages.
pub fn show(settings: &Settings, entries: &[String]) -> Option<String> {
    unsafe { show_inner(settings, entries) }
}

unsafe fn finish(state: &mut State, result: Option<String>) {
    if !state.done {
        state.done = true;
        state.result = result;
        ShowWindow(state.main_wnd, SW_HIDE);
        PostQuitMessage(0);
    }
}

unsafe fn get_edit_text(state: &State) -> String {
    let len = GetWindowTextLengthW(state.edit_wnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let read = GetWindowTextW(state.edit_wnd, buf.as_mut_ptr(), len + 1);
    String::from_utf16_lossy(&buf[..read.max(0) as usize])
}

fn filter_reduce(state: &mut State, needle: &str) {
    if needle.is_empty() {
        return;
    }
    if state.settings.case_sensitive {
        let entries = &state.entries;
        state.search_results.retain(|&i| entries[i].text.contains(needle));
    } else {
        let needle = needle.to_lowercase();
        let entries = &state.entries;
        state.search_results.retain(|&i| entries[i].lower.contains(&needle));
    }
}

unsafe fn update_search_results(state: &mut State) {
    state.search_results = (0..state.entries.len()).collect();

    let query = get_edit_text(state);
    match state.settings.filter_mode {
        FilterMode::Complete => filter_reduce(state, &query),
        FilterMode::Keywords => {
            for word in query.split(' ') {
                filter_reduce(state, word);
            }
        }
    }

    state.selected = if state.search_results.is_empty() { None } else { Some(0) };
    RedrawWindow(state.main_wnd, ptr::null(), ptr::null_mut(), RDW_INVALIDATE);
}

/// Move the selection by `delta` (wrapping), matching the C modulo behavior.
unsafe fn move_selection(state: &mut State, delta: isize) {
    let count = state.search_results.len();
    if count == 0 {
        return;
    }
    let cur = state.selected.unwrap_or(0) as isize;
    let next = (cur + delta).rem_euclid(count as isize) as usize;
    state.selected = Some(next);
    RedrawWindow(state.main_wnd, ptr::null(), ptr::null_mut(), RDW_INVALIDATE);
}

unsafe fn set_selection(state: &mut State, index: usize) {
    let count = state.search_results.len();
    if count == 0 {
        return;
    }
    state.selected = Some(index.min(count - 1));
    RedrawWindow(state.main_wnd, ptr::null(), ptr::null_mut(), RDW_INVALIDATE);
}

unsafe fn selected_entry_text(state: &State) -> Option<String> {
    let sel = state.selected?;
    Some(state.entries[state.search_results[sel]].text.clone())
}

/// Use trick from https://stackoverflow.com/a/59659421
unsafe fn force_foreground(hwnd: HWND) {
    let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), ptr::null_mut());
    let current_thread = winapi::um::processthreadsapi::GetCurrentThreadId();
    AttachThreadInput(foreground_thread, current_thread, 1);
    BringWindowToTop(hwnd);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    AttachThreadInput(foreground_thread, current_thread, 0);
}

unsafe fn state_from_wnd<'a>(wnd: HWND) -> Option<&'a mut State> {
    let ptr = GetWindowLongPtrW(wnd, GWLP_USERDATA) as *mut State;
    ptr.as_mut()
}

unsafe fn call_orig_edit(state: &State, wnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    CallWindowProcW(mem::transmute(state.edit_proc), wnd, msg, wparam, lparam)
}

unsafe extern "system" fn edit_wnd_proc(wnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let state = match state_from_wnd(wnd) {
        Some(s) => s,
        None => return DefWindowProcW(wnd, msg, wparam, lparam),
    };

    match msg {
        WM_KILLFOCUS => {
            // Losing focus cancels the menu to avoid lingering windows
            finish(state, None);
        }
        WM_CHAR => {
            let mut result: LRESULT = 0;
            match wparam {
                0x01 => {
                    // Ctrl+A - Select everything
                    call_orig_edit(state, wnd, EM_SETSEL as UINT, 0, -1);
                    return 0;
                }
                0x7f => {
                    // Ctrl+Backspace - Simulate traditional word-delete behavior
                    let mut end_sel: u32 = 0;
                    let mut start_sel: u32 = 0;
                    call_orig_edit(state, wnd, EM_GETSEL as UINT, 0, &mut end_sel as *mut u32 as LPARAM);
                    call_orig_edit(state, wnd, WM_KEYDOWN, VK_LEFT as WPARAM, 0);
                    call_orig_edit(state, wnd, WM_KEYUP, VK_LEFT as WPARAM, 0);
                    call_orig_edit(state, wnd, EM_GETSEL as UINT, &mut start_sel as *mut u32 as WPARAM, 0);
                    call_orig_edit(state, wnd, EM_SETSEL as UINT, start_sel as WPARAM, end_sel as LPARAM);
                    call_orig_edit(state, wnd, WM_CHAR, 0x08, 0); // Backspace
                }
                0x09 => {
                    // Tab - Autocomplete with the selected entry
                    if let Some(sel) = state.selected {
                        let entry = &state.entries[state.search_results[sel]];
                        let len = entry.text.encode_utf16().count();
                        SetWindowTextW(wnd, entry.wide.as_ptr());
                        call_orig_edit(state, wnd, EM_SETSEL as UINT, len, len as LPARAM);
                    }
                }
                // Swallow CR (handled in WM_KEYDOWN), Ctrl+J (LF), Ctrl+K (VT)
                0x0A | 0x0B | 0x0D => return 0,
                _ => {
                    result = call_orig_edit(state, wnd, msg, wparam, lparam);
                }
            }
            update_search_results(state);
            return result;
        }
        WM_KEYDOWN => {
            let ctrl_pressed = GetKeyState(VK_CONTROL) & 0x8000u16 as i16 != 0;

            match wparam as i32 {
                0x4A if ctrl_pressed => {
                    // Ctrl+J - Down
                    move_selection(state, 1);
                    return 1;
                }
                0x4B if ctrl_pressed => {
                    // Ctrl+K - Up
                    move_selection(state, -1);
                    return 0;
                }
                VK_RETURN => {
                    // If no results or shift is held: return input, else: return selection
                    let shift = GetKeyState(VK_SHIFT) & 0x8000u16 as i16 != 0;
                    let result = if shift {
                        get_edit_text(state)
                    } else {
                        match selected_entry_text(state) {
                            Some(text) => text,
                            None => get_edit_text(state),
                        }
                    };
                    finish(state, Some(result));
                    return 0;
                }
                VK_ESCAPE => {
                    finish(state, None);
                    return 0;
                }
                VK_UP => {
                    move_selection(state, -1);
                    return 0;
                }
                VK_DOWN => {
                    move_selection(state, 1);
                    return 0;
                }
                VK_HOME => {
                    set_selection(state, 0);
                    return 0;
                }
                VK_END => {
                    let count = state.search_results.len();
                    if count > 0 {
                        set_selection(state, count - 1);
                    }
                    return 0;
                }
                VK_PRIOR => {
                    // Page Up - Previous page
                    if state.line_count > 0 {
                        if let Some(sel) = state.selected {
                            let page = sel / state.line_count;
                            let target = page.saturating_sub(1) * state.line_count;
                            set_selection(state, target);
                        }
                    }
                    return 0;
                }
                VK_NEXT => {
                    // Page Down - Next page
                    if state.line_count > 0 {
                        if let Some(sel) = state.selected {
                            let target = (sel / state.line_count + 1) * state.line_count;
                            set_selection(state, target);
                        }
                    }
                    return 0;
                }
                _ => {}
            }
        }
        _ => {}
    }

    call_orig_edit(state, wnd, msg, wparam, lparam)
}

unsafe extern "system" fn main_wnd_proc(wnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let state = match state_from_wnd(wnd) {
        Some(s) => s,
        None => return DefWindowProcW(wnd, msg, wparam, lparam),
    };

    match msg {
        WM_TIMER => {
            // Repeating timer to make sure we're the foreground window
            if wparam == FOREGROUND_TIMER_ID {
                if GetForegroundWindow() == wnd {
                    state.had_foreground = true;
                } else if state.had_foreground {
                    // Focus lost after being in front: cancel
                    finish(state, None);
                } else {
                    force_foreground(state.main_wnd);
                }
            }
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let real_hdc = BeginPaint(wnd, &mut ps);

            // Lazily create the draw buffer
            if state.buffer_dc.is_null() {
                state.buffer_dc = CreateCompatibleDC(real_hdc);
                state.buffer_bitmap = CreateCompatibleBitmap(real_hdc, state.width, state.height);
                SelectObject(state.buffer_dc, state.buffer_bitmap as _);
                SelectObject(state.buffer_dc, state.font as _);
                SelectObject(state.buffer_dc, GetStockObject(DC_PEN as i32));
                SelectObject(state.buffer_dc, GetStockObject(DC_BRUSH as i32));
                SetBkMode(state.buffer_dc, TRANSPARENT as i32);
            }
            let hdc = state.buffer_dc;

            // Clear window
            SetDCPenColor(hdc, state.settings.bg);
            SetDCBrushColor(hdc, state.settings.bg);
            Rectangle(hdc, 0, 0, state.width, state.height);

            let padding = state.settings.padding;
            let font_size = state.settings.font_size;
            let hmargin = state.font_hmargin();

            // Draw prompt
            if let Some(ref prompt_wide) = state.prompt_wide {
                let mut prompt_rect = RECT {
                    left: padding + hmargin,
                    top: padding,
                    right: state.width / 2 - hmargin,
                    bottom: padding + font_size * 2,
                };

                SetDCPenColor(hdc, state.settings.bg_select);
                SetDCBrushColor(hdc, state.settings.bg_select);
                Rectangle(hdc, padding, prompt_rect.top,
                        padding + state.prompt_width,
                        prompt_rect.top + font_size);

                SetTextColor(hdc, state.settings.fg_select);
                DrawTextW(hdc, prompt_wide.as_ptr(), -1, &mut prompt_rect, DRAWTEXT_PARAMS);
            }

            // Draw entries
            let entries_top = state.entries_top();
            let page_start = state.page_start();
            let mut text_rect = RECT {
                left: padding + hmargin,
                top: entries_top,
                right: state.width - padding - hmargin,
                bottom: state.height,
            };
            SetTextColor(hdc, state.settings.fg);
            let count = state.line_count.min(state.search_results.len().saturating_sub(page_start));
            for idx in page_start..page_start + count {
                let is_selected = state.selected == Some(idx);
                if is_selected {
                    SetDCPenColor(hdc, state.settings.bg_select);
                    SetDCBrushColor(hdc, state.settings.bg_select);
                    Rectangle(hdc, padding, text_rect.top,
                            state.width - padding,
                            text_rect.top + font_size);
                    SetTextColor(hdc, state.settings.fg_select);
                }

                let entry = &state.entries[state.search_results[idx]];
                DrawTextW(hdc, entry.wide.as_ptr(), -1, &mut text_rect, DRAWTEXT_PARAMS);
                text_rect.top += font_size;

                if is_selected {
                    SetTextColor(hdc, state.settings.fg);
                }
            }

            // Blit
            BitBlt(real_hdc, 0, 0, state.width, state.height, hdc, 0, 0, SRCCOPY);

            EndPaint(wnd, &ps);
            return 0;
        }
        WM_CTLCOLOREDIT => {
            let hdc = wparam as HDC;
            SetTextColor(hdc, state.settings.fg_edit);
            SetBkColor(hdc, state.settings.bg_edit);
            SetDCBrushColor(hdc, state.settings.bg_edit);
            return GetStockObject(DC_BRUSH as i32) as LRESULT;
        }
        WM_CLOSE => {
            finish(state, None);
            return 0;
        }
        WM_LBUTTONDOWN => {
            let my = (lparam >> 16) as i16 as i32; // GET_Y_LPARAM
            let entries_top = state.entries_top();
            if my < entries_top || state.search_results.is_empty() || state.settings.font_size <= 0 {
                return 0;
            }
            let page_start = state.page_start();
            let offset = ((my - entries_top) / state.settings.font_size).max(0) as usize;
            let new_idx = (page_start + offset).min(state.search_results.len() - 1);
            if state.selected == Some(new_idx) {
                // Second click on the same entry - select it
                if let Some(text) = selected_entry_text(state) {
                    finish(state, Some(text));
                }
            } else {
                set_selection(state, new_idx);
            }
            return 0;
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16 as isize) / 120; // GET_WHEEL_DELTA_WPARAM
            move_selection(state, -delta);
            return 0;
        }
        _ => {}
    }

    DefWindowProcW(wnd, msg, wparam, lparam)
}

unsafe fn create_window(state: &mut State) -> Result<(), String> {
    let class_name = to_wide(WND_CLASS);

    // Register window class (ignore "already exists" from earlier invocations)
    let mut wc: WNDCLASSEXW = mem::zeroed();
    wc.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(main_wnd_proc);
    wc.lpszClassName = class_name.as_ptr();
    wc.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
    wc.hbrBackground = (COLOR_WINDOW + 1) as usize as HBRUSH;
    if RegisterClassExW(&wc) == 0 {
        let err = GetLastError();
        if err != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("RegisterClassExW failed: error {}", err));
        }
    }

    // Window geometry
    let display_width = GetSystemMetrics(SM_CXSCREEN);
    let display_height = GetSystemMetrics(SM_CYSCREEN);

    state.width = if state.settings.width > 0 { state.settings.width } else { display_width };
    state.height = state.settings.font_size * (state.line_count as i32 + 1)
            + state.settings.padding * 2;

    let (mut x, mut y) = (0, 0);
    if state.settings.center_window {
        x = (display_width - state.width) / 2;
        y = (display_height - state.height) / 2;
    }

    let title = to_wide("wlines");
    state.main_wnd = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(), title.as_ptr(), WS_POPUP,
            x, y, state.width, state.height,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    if state.main_wnd.is_null() {
        return Err(format!("CreateWindowExW failed: error {}", GetLastError()));
    }

    // Calculate prompt width
    if let Some(ref prompt_wide) = state.prompt_wide {
        let mut prompt_rect = RECT {
            left: 0,
            top: 0,
            right: state.width / 2 - state.settings.padding,
            bottom: state.settings.font_size * 2,
        };
        let tmp_hdc = CreateCompatibleDC(ptr::null_mut());
        SelectObject(tmp_hdc, state.font as _);
        DrawTextW(tmp_hdc, prompt_wide.as_ptr(), -1,
                &mut prompt_rect, DRAWTEXT_PARAMS | DT_CALCRECT);
        DeleteDC(tmp_hdc);
        state.prompt_width = prompt_rect.right - prompt_rect.left + state.font_hmargin() * 2;
    }

    // Create textbox
    let edit_class = to_wide("EDIT");
    let empty = to_wide("");
    let textbox_left = state.settings.padding + state.prompt_width;
    state.edit_wnd = CreateWindowExW(0, edit_class.as_ptr(), empty.as_ptr(),
        WS_VISIBLE | WS_CHILD | ES_LEFT | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        textbox_left, state.settings.padding,
        state.width - textbox_left - state.settings.padding, state.settings.font_size,
        state.main_wnd, 101 as HMENU, ptr::null_mut(), ptr::null_mut());
    if state.edit_wnd.is_null() {
        return Err(format!("CreateWindowExW (edit) failed: error {}", GetLastError()));
    }

    SendMessageW(state.edit_wnd, WM_SETFONT, state.font as WPARAM, 1);
    let hmargin = state.font_hmargin() as usize;
    SendMessageW(state.edit_wnd, EM_SETMARGINS as UINT,
            (EC_LEFTMARGIN | EC_RIGHTMARGIN) as WPARAM,
            (hmargin | (hmargin << 16)) as LPARAM);
    state.edit_proc = SetWindowLongPtrW(state.edit_wnd, GWLP_WNDPROC, edit_wnd_proc as *const () as isize);

    // Add state pointer
    let state_ptr = state as *mut State;
    SetWindowLongPtrW(state.main_wnd, GWLP_USERDATA, state_ptr as isize);
    SetWindowLongPtrW(state.edit_wnd, GWLP_USERDATA, state_ptr as isize);

    // Remove default window styling
    let style = GetWindowLongW(state.main_wnd, GWL_STYLE);
    SetWindowLongW(state.main_wnd, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW as i32));

    // Show and attempt to focus window
    UpdateWindow(state.main_wnd);
    force_foreground(state.main_wnd);
    SetFocus(state.edit_wnd);

    // Start foreground timer
    SetTimer(state.main_wnd, FOREGROUND_TIMER_ID, 50, None);

    Ok(())
}

unsafe fn show_inner(settings: &Settings, entries: &[String]) -> Option<String> {
    let entries: Vec<Entry> = entries.iter().map(|text| Entry {
        lower: text.to_lowercase(),
        wide: to_wide(text),
        text: text.clone(),
    }).collect();

    let font_name = to_wide(&settings.font_name);
    let font = CreateFontW(settings.font_size, 0, 0, 0,
        FW_NORMAL as i32, 0, 0, 0, 0, 0, 0, 0x04, 0, font_name.as_ptr());
    if font.is_null() {
        eprintln!("wlines: CreateFontW failed: error {}", GetLastError());
        return None;
    }

    let line_count = settings.line_count.min(entries.len());
    let search_results: Vec<usize> = (0..entries.len()).collect();
    let selected = if search_results.is_empty() {
        None
    } else {
        Some(settings.initial_index.min(search_results.len() - 1))
    };

    let mut state = Box::new(State {
        prompt_wide: settings.prompt.as_deref().map(to_wide),
        settings: settings.clone(),
        font,
        main_wnd: ptr::null_mut(),
        edit_wnd: ptr::null_mut(),
        edit_proc: 0,
        width: 0,
        height: 0,
        line_count,
        had_foreground: false,
        prompt_width: 0,
        entries,
        search_results,
        selected,
        buffer_dc: ptr::null_mut(),
        buffer_bitmap: ptr::null_mut(),
        done: false,
        result: None,
    });

    if let Err(e) = create_window(&mut state) {
        eprintln!("wlines: {}", e);
        DeleteObject(state.font as _);
        return None;
    }

    // Message loop - runs until finish() posts WM_QUIT
    let mut msg: MSG = mem::zeroed();
    while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    // Cleanup
    KillTimer(state.main_wnd, FOREGROUND_TIMER_ID);
    // Detach state and restore the original edit proc before destroying,
    // so destruction-time messages don't touch freed state
    SetWindowLongPtrW(state.edit_wnd, GWLP_WNDPROC, state.edit_proc);
    SetWindowLongPtrW(state.main_wnd, GWLP_USERDATA, 0);
    SetWindowLongPtrW(state.edit_wnd, GWLP_USERDATA, 0);
    DestroyWindow(state.main_wnd); // destroys the edit child too
    if !state.buffer_dc.is_null() {
        DeleteDC(state.buffer_dc);
    }
    if !state.buffer_bitmap.is_null() {
        DeleteObject(state.buffer_bitmap as _);
    }
    DeleteObject(state.font as _);

    state.result.take()
}
