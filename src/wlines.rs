//! Native port of the wlines menu renderer (wlines.c by JerwuQu, daemon fork
//! by gicrisf). Renders a dmenu-style selection window via GDI in-process and
//! returns the user's selection, replacing the old wlines-daemon named-pipe
//! IPC and wlines.exe subprocess fallback.

use std::ffi::OsStr;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{COLORREF, HBITMAP, HDC, HFONT, HWND, POINT, RECT, HBRUSH, HMENU};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::shellscalingapi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use winapi::um::wingdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject,
    GetStockObject, Rectangle, SelectObject, SetBkColor, SetBkMode, SetDCBrushColor,
    SetDCPenColor, SetTextColor, DC_BRUSH, DC_PEN, FW_NORMAL, SRCCOPY, TRANSPARENT,
};
use winapi::um::winuser::{
    AttachThreadInput, BeginPaint, BringWindowToTop, CallWindowProcW, CreateWindowExW,
    DefWindowProcW, DestroyWindow, DispatchMessageW, DrawTextW, EndPaint, GetForegroundWindow,
    GetCursorPos, GetKeyState, GetMessageW, GetMonitorInfoW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, KillTimer, LoadCursorW,
    MonitorFromPoint, PostQuitMessage, RedrawWindow, RegisterClassExW, SendMessageW, SetFocus,
    SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowLongW, SetWindowTextW, ShowWindow, TranslateMessage,
    UpdateWindow, COLOR_WINDOW, DT_CALCRECT, DT_END_ELLIPSIS, DT_NOCLIP, DT_NOPREFIX, DT_SINGLELINE,
    EC_LEFTMARGIN, EC_RIGHTMARGIN, EM_GETSEL, EM_SETMARGINS, EM_SETSEL, ES_AUTOHSCROLL,
    ES_AUTOVSCROLL, ES_LEFT, GWLP_USERDATA, GWLP_WNDPROC, GWL_STYLE, IDC_ARROW,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, MSG,
    RDW_INVALIDATE, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, VK_CONTROL, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_UP, WM_CHAR,
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
    Fuzzy,
}

impl FilterMode {
    pub fn parse(s: &str) -> FilterMode {
        match s.to_ascii_lowercase().as_str() {
            "complete" | "0" => FilterMode::Complete,
            "keywords" | "1" => FilterMode::Keywords,
            "fuzzy" | "2" => FilterMode::Fuzzy,
            _ => FilterMode::Keywords,
        }
    }
}

/// A navigation keybinding: Ctrl/Shift modifiers plus a virtual-key code.
/// Alt/Win are intentionally unsupported — those keydowns arrive as WM_SYSKEYDOWN,
/// which the edit proc does not handle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub vk: u16,
}

impl KeyCombo {
    /// True if this combo is satisfied by the current key + modifier state.
    fn matches(&self, vk: u16, ctrl: bool, shift: bool) -> bool {
        self.vk == vk && self.ctrl == ctrl && self.shift == shift
    }

    /// The control character the EDIT control would insert for a Ctrl+<letter>
    /// combo, so WM_CHAR can swallow it (Ctrl+A=1 .. Ctrl+Z=26; e.g. Ctrl+J=0x0A,
    /// Ctrl+K=0x0B). None when this isn't a Ctrl+letter combo.
    pub(crate) fn char_to_swallow(&self) -> Option<u32> {
        if self.ctrl && (0x41..=0x5A).contains(&self.vk) {
            Some((self.vk & 0x1F) as u32)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub line_count: usize,
    pub horizontal: bool, // Single-row bar; entries flow left-to-right, line_count is ignored
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
    pub next: KeyCombo, // Move selection down (default Ctrl+J)
    pub prev: KeyCombo, // Move selection up (default Ctrl+K)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            line_count: 15,
            horizontal: false,
            prompt: None,
            filter_mode: FilterMode::Fuzzy,
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
            next: KeyCombo { ctrl: true, shift: false, vk: 0x4A }, // Ctrl+J
            prev: KeyCombo { ctrl: true, shift: false, vk: 0x4B }, // Ctrl+K
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
    width: i32, // text width in px; measured in create_window, horizontal mode only
}

struct State {
    settings: Settings,
    monitor_rect: RECT,
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
    marker_width: i32, // "<"/">" page-marker slot (horizontal mode, else 0)

    entries: Vec<Entry>,
    search_results: Vec<usize>, // indices into `entries`
    selected: Option<usize>,    // index into `search_results`

    // Double-buffer for WM_PAINT (owned per invocation, unlike the C static)
    buffer_dc: HDC,
    buffer_bitmap: HBITMAP,

    done: bool,
    result: Option<String>,
}

/// Greedily pack cell widths into pages of at most `avail` width, returning
/// the first index of each page. Every page holds at least one cell, so a
/// cell wider than `avail` gets a page of its own (drawn ellipsized).
fn pack_pages(widths: &[i32], avail: i32) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut used = 0;
    for (i, &w) in widths.iter().enumerate() {
        if starts.is_empty() || used + w > avail {
            starts.push(i);
            used = 0;
        }
        used += w;
    }
    starts
}

// Layout: all geometry questions ("what's visible, where does entry N go,
// what's under the cursor") are answered here, so the paint and mouse code
// stays layout-agnostic. The vertical/horizontal branch lives only in these
// helpers.
impl State {
    fn font_hmargin(&self) -> i32 {
        self.settings.font_size / 6
    }

    fn entries_top(&self) -> i32 {
        self.settings.font_size + self.settings.padding
    }

    /// Pixel width of the input box. Vertical mode gives it everything right
    /// of the prompt; the horizontal bar caps it at a quarter of the width so
    /// entries fit beside it.
    fn input_width(&self) -> i32 {
        let rest = self.width - self.settings.padding * 2 - self.prompt_width;
        if self.settings.horizontal {
            rest.min(self.width / 4)
        } else {
            rest
        }
    }

    /// X where the entry cells start (horizontal mode): right of the input box
    /// and the "<" page-marker slot.
    fn entries_left(&self) -> i32 {
        self.settings.padding + self.prompt_width + self.input_width() + self.marker_width
    }

    /// Width of entry cell `idx` (into `search_results`): its text plus the
    /// same side margins the vertical rows use.
    fn cell_width(&self, idx: usize) -> i32 {
        self.entries[self.search_results[idx]].width + self.font_hmargin() * 2
    }

    /// First result index of each page. Vertical pages are `line_count` fixed
    /// rows; horizontal pages hold as many cells as fit in the bar. Empty when
    /// nothing can be shown.
    fn page_starts(&self) -> Vec<usize> {
        let count = self.search_results.len();
        if count == 0 {
            return Vec::new();
        }
        if self.settings.horizontal {
            let widths: Vec<i32> = (0..count).map(|i| self.cell_width(i)).collect();
            // The ">" marker slot on the right mirrors the "<" inside entries_left
            let avail = self.width - self.settings.padding - self.marker_width - self.entries_left();
            pack_pages(&widths, avail)
        } else {
            if self.line_count == 0 {
                return Vec::new();
            }
            (0..count).step_by(self.line_count).collect()
        }
    }

    /// The page of results containing the selection.
    fn visible_range(&self) -> std::ops::Range<usize> {
        let starts = self.page_starts();
        if starts.is_empty() {
            return 0..0;
        }
        let sel = self.selected.unwrap_or(0);
        let page = starts.partition_point(|&s| s <= sel) - 1;
        let end = starts.get(page + 1).copied().unwrap_or(self.search_results.len());
        starts[page]..end
    }

    /// Selection box of entry `idx`, which must lie in `visible_range()`. Text
    /// is drawn inset by `font_hmargin` on each side.
    fn entry_rect(&self, idx: usize) -> RECT {
        let padding = self.settings.padding;
        let font_size = self.settings.font_size;
        if self.settings.horizontal {
            let mut left = self.entries_left();
            for i in self.visible_range().start..idx {
                left += self.cell_width(i);
            }
            RECT {
                left,
                top: padding,
                right: (left + self.cell_width(idx)).min(self.width - padding),
                bottom: padding + font_size,
            }
        } else {
            let row = (idx - self.visible_range().start) as i32;
            let top = self.entries_top() + row * font_size;
            RECT {
                left: padding,
                top,
                right: self.width - padding,
                bottom: top + font_size,
            }
        }
    }

    /// Result index under a client-area point, if any. Vertical mode keeps the
    /// C behavior of clamping clicks past the end to the last entry.
    fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        if self.search_results.is_empty() {
            return None;
        }
        if self.settings.horizontal {
            self.visible_range().find(|&idx| {
                let r = self.entry_rect(idx);
                x >= r.left && x < r.right && y >= r.top
            })
        } else {
            let entries_top = self.entries_top();
            if y < entries_top || self.settings.font_size <= 0 {
                return None;
            }
            let offset = ((y - entries_top) / self.settings.font_size).max(0) as usize;
            Some((self.visible_range().start + offset).min(self.search_results.len() - 1))
        }
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

/// fzf-style subsequence scorer. Returns None when `needle` is not a
/// subsequence of `haystack`, otherwise the score of the best-scoring
/// alignment (dynamic programming, not greedy first-occurrence), favoring
/// matches at word boundaries and camelCase humps, consecutive runs, and
/// short gaps. Leading and trailing gaps are free.
fn fuzzy_score(needle: &str, haystack: &str, case_sensitive: bool) -> Option<i32> {
    const SCORE_MATCH: i32 = 16;
    const BONUS_BOUNDARY: i32 = 16;
    const BONUS_CAMEL: i32 = 12;
    const BONUS_CONSECUTIVE: i32 = 8;
    const PENALTY_GAP_START: i32 = -3;
    const PENALTY_GAP_EXTEND: i32 = -1;
    const UNMATCHED: i32 = i32::MIN / 2; // headroom so additions can't overflow

    fn is_camel(prev: char, cur: char) -> bool {
        (prev.is_lowercase() && cur.is_uppercase())
            || (prev.is_alphabetic() && cur.is_numeric())
    }

    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().collect();

    // Positional bonus for a match at each haystack index
    let bonus: Vec<i32> = hay
        .iter()
        .enumerate()
        .map(|(j, &c)| match if j == 0 { None } else { Some(hay[j - 1]) } {
            None => BONUS_BOUNDARY,
            Some(p) if !p.is_alphanumeric() => BONUS_BOUNDARY,
            Some(p) if is_camel(p, c) => BONUS_CAMEL,
            _ => 0,
        })
        .collect();

    let matches = |nc: char, hc: char| {
        if case_sensitive {
            hc == nc
        } else {
            hc.to_lowercase().eq(nc.to_lowercase())
        }
    };

    // ending[j]: best score matching the needle prefix so far with its last
    // char matched exactly at haystack index j
    let mut ending = vec![UNMATCHED; hay.len()];
    let mut first_row = true;
    for nc in needle.chars() {
        let mut next = vec![UNMATCHED; hay.len()];
        // Best previous-row score ending strictly before j-1, with affine gap
        // penalties applied for the unmatched span up to j-1
        let mut gapped = UNMATCHED;
        for (j, &hc) in hay.iter().enumerate() {
            if matches(nc, hc) {
                if first_row {
                    next[j] = SCORE_MATCH + bonus[j];
                } else {
                    let diag = if j > 0 { ending[j - 1] } else { UNMATCHED };
                    let best = (diag + BONUS_CONSECUTIVE).max(gapped);
                    if best > UNMATCHED {
                        next[j] = best + SCORE_MATCH + bonus[j];
                    }
                }
            }
            if j > 0 {
                gapped = (gapped + PENALTY_GAP_EXTEND).max(ending[j - 1] + PENALTY_GAP_START);
            }
        }
        ending = next;
        first_row = false;
    }

    let best = ending.into_iter().max()?;
    if best > UNMATCHED / 2 { Some(best) } else { None }
}

fn filter_fuzzy(state: &mut State, needle: &str) {
    if needle.is_empty() {
        return;
    }
    let case_sensitive = state.settings.case_sensitive;
    let entries = &state.entries;
    let mut scored: Vec<(i32, usize)> = state
        .search_results
        .iter()
        .filter_map(|&i| fuzzy_score(needle, &entries[i].text, case_sensitive).map(|s| (s, i)))
        .collect();
    // Descending by score, original entry order as tie-break
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    state.search_results = scored.into_iter().map(|(_, i)| i).collect();
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
        FilterMode::Fuzzy => filter_fuzzy(state, &query),
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
    CallWindowProcW(mem::transmute::<isize, Option<unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>>(state.edit_proc), wnd, msg, wparam, lparam)
}

/// Current EDIT selection as (start, end) caret positions (UTF-16 units).
unsafe fn edit_caret(state: &State, wnd: HWND) -> (u32, u32) {
    let mut start: u32 = 0;
    let mut end: u32 = 0;
    call_orig_edit(state, wnd, EM_GETSEL as UINT,
            &mut start as *mut u32 as WPARAM, &mut end as *mut u32 as LPARAM);
    (start, end)
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
                    let (_, end_sel) = edit_caret(state, wnd);
                    call_orig_edit(state, wnd, WM_KEYDOWN, VK_LEFT as WPARAM, 0);
                    call_orig_edit(state, wnd, WM_KEYUP, VK_LEFT as WPARAM, 0);
                    let (start_sel, _) = edit_caret(state, wnd);
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
                // Swallow CR (handled in WM_KEYDOWN) plus the control char of any
                // Ctrl+<letter> navigation combo, so it isn't typed into the box.
                0x0D => return 0,
                c if Some(c as u32) == state.settings.next.char_to_swallow()
                    || Some(c as u32) == state.settings.prev.char_to_swallow() =>
                {
                    return 0
                }
                _ => {
                    result = call_orig_edit(state, wnd, msg, wparam, lparam);
                }
            }
            update_search_results(state);
            return result;
        }
        WM_KEYDOWN => {
            let ctrl_pressed = GetKeyState(VK_CONTROL) & 0x8000u16 as i16 != 0;
            let shift_pressed = GetKeyState(VK_SHIFT) & 0x8000u16 as i16 != 0;

            // Configurable next/prev navigation combos take precedence over the
            // hardcoded keys below (arrows/Home/End/PageUp/PageDown stay fixed).
            let vk = wparam as u16;
            if state.settings.next.matches(vk, ctrl_pressed, shift_pressed) {
                move_selection(state, 1);
                return 0;
            }
            if state.settings.prev.matches(vk, ctrl_pressed, shift_pressed) {
                move_selection(state, -1);
                return 0;
            }

            match wparam as i32 {
                VK_RETURN => {
                    // If no results or shift is held: return input, else: return selection
                    let result = if shift_pressed {
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
                // Edge-triggered navigation (dmenu's rule): an unmodified
                // Left/Right moves the selection once the caret can't travel
                // any further in that direction, and keeps editing text
                // otherwise. Ctrl/Shift+arrows stay pure text operations.
                VK_LEFT if !ctrl_pressed && !shift_pressed => {
                    if edit_caret(state, wnd) == (0, 0) {
                        move_selection(state, -1);
                        return 0;
                    }
                }
                VK_RIGHT if !ctrl_pressed && !shift_pressed => {
                    let len = GetWindowTextLengthW(state.edit_wnd) as u32;
                    if edit_caret(state, wnd) == (len, len) {
                        move_selection(state, 1);
                        return 0;
                    }
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
                    // Page Up - Start of the previous page
                    if let Some(sel) = state.selected {
                        let starts = state.page_starts();
                        if !starts.is_empty() {
                            let page = starts.partition_point(|&s| s <= sel) - 1;
                            set_selection(state, starts[page.saturating_sub(1)]);
                        }
                    }
                    return 0;
                }
                VK_NEXT => {
                    // Page Down - Start of the next page (or the last entry)
                    if let Some(sel) = state.selected {
                        let starts = state.page_starts();
                        if !starts.is_empty() {
                            let page = starts.partition_point(|&s| s <= sel) - 1;
                            let target = starts
                                .get(page + 1)
                                .copied()
                                .unwrap_or(state.search_results.len() - 1);
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
            SetTextColor(hdc, state.settings.fg);
            for idx in state.visible_range() {
                let rect = state.entry_rect(idx);
                let is_selected = state.selected == Some(idx);
                if is_selected {
                    SetDCPenColor(hdc, state.settings.bg_select);
                    SetDCBrushColor(hdc, state.settings.bg_select);
                    Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
                    SetTextColor(hdc, state.settings.fg_select);
                }

                let mut text_rect = RECT {
                    left: rect.left + hmargin,
                    top: rect.top,
                    right: rect.right - hmargin,
                    bottom: rect.bottom,
                };
                let entry = &state.entries[state.search_results[idx]];
                DrawTextW(hdc, entry.wide.as_ptr(), -1, &mut text_rect, DRAWTEXT_PARAMS);

                if is_selected {
                    SetTextColor(hdc, state.settings.fg);
                }
            }

            // Page markers (horizontal): "<"/">" in the reserved slots flanking
            // the entries, drawn only when more pages exist on that side
            if state.settings.horizontal {
                let visible = state.visible_range();
                SetTextColor(hdc, state.settings.fg);
                let draw_marker = |text: &str, left: i32| {
                    let wide = to_wide(text);
                    let mut rect = RECT {
                        left: left + hmargin,
                        top: padding,
                        right: left + state.marker_width - hmargin,
                        bottom: padding + font_size,
                    };
                    DrawTextW(hdc, wide.as_ptr(), -1, &mut rect, DRAWTEXT_PARAMS);
                };
                if visible.start > 0 {
                    draw_marker("<", state.entries_left() - state.marker_width);
                }
                if visible.end < state.search_results.len() {
                    draw_marker(">", state.width - padding - state.marker_width);
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
            let mx = (lparam & 0xffff) as i16 as i32; // GET_X_LPARAM
            let my = (lparam >> 16) as i16 as i32; // GET_Y_LPARAM
            if let Some(new_idx) = state.hit_test(mx, my) {
                if state.selected == Some(new_idx) {
                    // Second click on the same entry - select it
                    if let Some(text) = selected_entry_text(state) {
                        finish(state, Some(text));
                    }
                } else {
                    set_selection(state, new_idx);
                }
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

    // Window geometry (monitor under the cursor)
    let mon = state.monitor_rect;
    let display_width = mon.right - mon.left;
    let display_height = mon.bottom - mon.top;

    state.width = if state.settings.width > 0 { state.settings.width } else { display_width };
    state.height = if state.settings.horizontal {
        // Single-row bar: input and entries share one line
        state.settings.font_size + state.settings.padding * 2
    } else {
        state.settings.font_size * (state.line_count as i32 + 1)
                + state.settings.padding * 2
    };

    let (mut x, mut y) = (mon.left, mon.top);
    if state.settings.center_window {
        x = mon.left + (display_width - state.width) / 2;
        y = mon.top + (display_height - state.height) / 2;
    }

    let title = to_wide("wlines");
    state.main_wnd = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(), title.as_ptr(), WS_POPUP,
            x, y, state.width, state.height,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    if state.main_wnd.is_null() {
        return Err(format!("CreateWindowExW failed: error {}", GetLastError()));
    }

    // Measure the text the layout depends on: the prompt, and in horizontal
    // mode every entry (cells are sized to their text)
    if state.prompt_wide.is_some() || state.settings.horizontal {
        let tmp_hdc = CreateCompatibleDC(ptr::null_mut());
        SelectObject(tmp_hdc, state.font as _);
        if let Some(ref prompt_wide) = state.prompt_wide {
            let mut prompt_rect = RECT {
                left: 0,
                top: 0,
                right: state.width / 2 - state.settings.padding,
                bottom: state.settings.font_size * 2,
            };
            DrawTextW(tmp_hdc, prompt_wide.as_ptr(), -1,
                    &mut prompt_rect, DRAWTEXT_PARAMS | DT_CALCRECT);
            state.prompt_width = prompt_rect.right - prompt_rect.left + state.font_hmargin() * 2;
        }
        if state.settings.horizontal {
            for entry in &mut state.entries {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                DrawTextW(tmp_hdc, entry.wide.as_ptr(), -1, &mut rect,
                        DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT);
                entry.width = rect.right - rect.left;
            }
            // Fixed slots flanking the entries for the "<"/">" page markers,
            // reserved on both sides regardless of page so cells don't shift
            let mut marker = 0;
            for text in ["<", ">"] {
                let wide = to_wide(text);
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                DrawTextW(tmp_hdc, wide.as_ptr(), -1, &mut rect,
                        DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT);
                marker = marker.max(rect.right - rect.left);
            }
            state.marker_width = marker + state.font_hmargin() * 2;
        }
        DeleteDC(tmp_hdc);
    }

    // Create textbox
    let edit_class = to_wide("EDIT");
    let empty = to_wide("");
    let textbox_left = state.settings.padding + state.prompt_width;
    state.edit_wnd = CreateWindowExW(0, edit_class.as_ptr(), empty.as_ptr(),
        WS_VISIBLE | WS_CHILD | ES_LEFT | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        textbox_left, state.settings.padding,
        state.input_width(), state.settings.font_size,
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

/// Bounds and effective DPI of the monitor under the cursor. The menu opens
/// there, dmenu-style, instead of always on the primary display.
unsafe fn cursor_monitor() -> (RECT, u32) {
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);

    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    let rect = if GetMonitorInfoW(monitor, &mut info) != 0 {
        info.rcMonitor
    } else {
        RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    };

    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    let dpi = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) == 0 {
        dpi_x
    } else {
        96 // shcore unavailable (pre-8.1); sizes stay unscaled
    };
    let _ = dpi_y; // square pixels assumed, as everywhere on Windows

    (rect, dpi)
}

unsafe fn show_inner(settings: &Settings, entries: &[String]) -> Option<String> {
    let entries: Vec<Entry> = entries.iter().map(|text| Entry {
        lower: text.to_lowercase(),
        wide: to_wide(text),
        text: text.clone(),
        width: 0,
    }).collect();

    // Config sizes are 96-DPI logical pixels; scale for the target monitor
    let (monitor_rect, dpi) = cursor_monitor();
    let scale = |v: i32| (v * dpi as i32 + 48) / 96;
    let mut settings = settings.clone();
    settings.font_size = scale(settings.font_size);
    settings.padding = scale(settings.padding);
    settings.width = scale(settings.width);

    let font_name = to_wide(&settings.font_name);
    let font = CreateFontW(settings.font_size, 0, 0, 0,
        FW_NORMAL, 0, 0, 0, 0, 0, 0, 0x04, 0, font_name.as_ptr());
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
        settings,
        monitor_rect,
        font,
        main_wnd: ptr::null_mut(),
        edit_wnd: ptr::null_mut(),
        edit_proc: 0,
        width: 0,
        height: 0,
        line_count,
        had_foreground: false,
        prompt_width: 0,
        marker_width: 0,
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

#[cfg(test)]
mod tests {
    use super::{fuzzy_score, pack_pages};

    #[test]
    fn pack_empty() {
        assert_eq!(pack_pages(&[], 100), Vec::<usize>::new());
    }

    #[test]
    fn pack_fills_pages_greedily() {
        assert_eq!(pack_pages(&[10, 10, 10], 25), vec![0, 2]);
    }

    #[test]
    fn pack_exact_fit_stays_on_page() {
        assert_eq!(pack_pages(&[10, 10], 20), vec![0]);
    }

    #[test]
    fn pack_oversized_cell_gets_own_page() {
        // A cell wider than the bar still lands on a page of its own
        // (it gets ellipsized when drawn), and never stalls the packing.
        assert_eq!(pack_pages(&[50, 10], 30), vec![0, 1]);
        assert_eq!(pack_pages(&[10, 50, 10], 30), vec![0, 1, 2]);
    }

    #[test]
    fn non_subsequence_rejected() {
        assert_eq!(fuzzy_score("xyz", "Google Chrome", false), None);
        assert_eq!(fuzzy_score("chromee", "Chrome", false), None);
    }

    #[test]
    fn empty_needle_matches_everything() {
        assert_eq!(fuzzy_score("", "anything", false), Some(0));
    }

    #[test]
    fn case_insensitive_by_default() {
        assert!(fuzzy_score("chrome", "Google Chrome", false).is_some());
        assert_eq!(fuzzy_score("chrome", "Google Chrome", true), None);
        assert!(fuzzy_score("Chrome", "Google Chrome", true).is_some());
    }

    #[test]
    fn boundary_match_beats_mid_word_match() {
        let boundary = fuzzy_score("code", "Visual Studio Code", false).unwrap();
        let mid_word = fuzzy_score("code", "Unicodex", false).unwrap();
        assert!(boundary > mid_word);
    }

    #[test]
    fn consecutive_run_beats_scattered_match() {
        let run = fuzzy_score("term", "Terminal", false).unwrap();
        let scattered = fuzzy_score("term", "Text Formatter", false).unwrap();
        assert!(run > scattered);
    }

    #[test]
    fn camel_hump_scores_above_plain_mid_word() {
        let camel = fuzzy_score("pp", "PowerPoint", false).unwrap();
        let plain = fuzzy_score("pp", "clipper", false).unwrap();
        assert!(camel > plain);
    }

    #[test]
    fn acronym_style_matching_works() {
        // Classic fzf use case: initials of a multi-word entry beat a
        // scattered mid-word match
        let acronym = fuzzy_score("vsc", "Visual Studio Code", false).unwrap();
        let scattered = fuzzy_score("vsc", "vesicular", false).unwrap();
        assert!(acronym > scattered);
    }

    #[test]
    fn picks_best_alignment_not_first_occurrence() {
        // Greedy matching would take the 's' in "Visual" and miss the
        // word-boundary 'S' of "Studio"; the DP must find the better path
        let boundary = fuzzy_score("st", "Visual Studio", false).unwrap();
        let mid_word = fuzzy_score("st", "Restart", false).unwrap();
        assert!(boundary > mid_word);
    }

    #[test]
    fn shorter_gap_scores_higher() {
        let short_gap = fuzzy_score("ab", "acb", false).unwrap();
        let long_gap = fuzzy_score("ab", "acccccb", false).unwrap();
        assert!(short_gap > long_gap);
    }
}
