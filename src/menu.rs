use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::{Command, Stdio};
use std::thread;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::fmt;
use serde::Deserialize;
use toml;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::shellapi::ShellExecuteW;
use winapi::um::winuser::{
    DispatchMessageW, MessageBoxW, MsgWaitForMultipleObjects, PeekMessageW, RegisterHotKey,
    SendInput, TranslateMessage, INPUT, INPUT_KEYBOARD, MB_ICONERROR, MB_OK, MOD_ALT,
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, MSG, PM_REMOVE, QS_ALLINPUT,
    SW_RESTORE, WM_HOTKEY, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_MENU, VK_SHIFT, VK_CAPITAL, VK_CONTROL, VK_TAB, VK_ESCAPE, VK_LWIN, VK_SPACE,
    VK_RETURN, VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN, VK_F1, VK_F2, VK_F3, VK_F4, VK_F5,
    VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_OEM_COMMA, VK_OEM_PERIOD,
    VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7,
    VK_OEM_MINUS, VK_OEM_PLUS
};
use winapi::um::winbase::{CREATE_NEW_PROCESS_GROUP, INFINITE, WAIT_OBJECT_0};
use winapi::um::handleapi::CloseHandle;
use winapi::um::synchapi::CreateEventW;

use crate::apps::{find_reparse_points, get_windows_apps_path};
use crate::theme::{self, Palette};
use crate::wlines;

#[derive(Debug)]
pub enum MenuError {
    ConfigLoad(String),
    CommandExecution(String),
    KeyParsing(String),
    KeyInput(u32),
    ProcessSpawn(std::io::Error),
    InvalidArguments(String),
    WindowsApi(u32),
    MenuAlreadyRunning,
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MenuError::ConfigLoad(msg) => write!(f, "Failed to load configuration: {}", msg),
            MenuError::CommandExecution(msg) => write!(f, "Failed to execute command: {}", msg),
            MenuError::KeyParsing(key) => write!(f, "Unknown key: {}", key),
            MenuError::KeyInput(error) => write!(f, "Failed to send key input: error {}", error),
            MenuError::ProcessSpawn(error) => write!(f, "Failed to spawn process: {}", error),
            MenuError::InvalidArguments(msg) => write!(f, "Invalid arguments: {}", msg),
            MenuError::WindowsApi(error) => write!(f, "Windows API error: {}", error),
            MenuError::MenuAlreadyRunning => write!(f, "Menu is already running"),
        }
    }
}

impl std::error::Error for MenuError {}

#[derive(Debug, Clone)]
pub enum MenuCommand {
    Start(PathBuf),            // For Start menu shortcuts
    Configured(Vec<String>),  // For configured commands
    KeyCombo(Vec<String>),    // For key combinations like ALT+X
    ToggleCapsLock,           // For caps lock toggle
    RefreshApps,              // Rescan Start Menu and Windows Store apps
    ReloadConfig,             // Reload commands from windmenu.toml
}

pub(crate) struct EntryStore {
    builtins: HashMap<String, MenuCommand>,
    config: HashMap<String, MenuCommand>,
    dynamic: HashMap<String, MenuCommand>,
}

impl EntryStore {
    fn empty() -> Self {
        let mut builtins = HashMap::new();
        builtins.insert("Toggle Caps Lock".to_string(), MenuCommand::ToggleCapsLock);
        builtins.insert("Refresh Apps".to_string(), MenuCommand::RefreshApps);
        builtins.insert("Reload Config".to_string(), MenuCommand::ReloadConfig);
        Self { builtins, config: HashMap::new(), dynamic: HashMap::new() }
    }

    fn apply_config_commands(&mut self, cmds: Vec<CommandConfig>) {
        let mut config = HashMap::new();
        for cmd in cmds {
            let (key, command) = match cmd.command_type {
                CommandType::Args { args } => (cmd.name, MenuCommand::Configured(args)),
                CommandType::Keys { keys } => {
                    let key_sequence = keys.join(", ");
                    let formatted_key = format!("{} [{}]", cmd.name, key_sequence);
                    (formatted_key, MenuCommand::KeyCombo(keys))
                }
            };
            config.insert(key, command);
        }
        self.config = config;
    }

    pub(crate)     fn reload_config(&mut self) {
        if let Ok((cfg, _dir, warnings)) = load_with_imports() {
            for warning in warnings {
                eprintln!("Warning: {}", warning);
            }
            if let Some(cmds) = cfg.commands {
                self.apply_config_commands(cmds);
            } else {
                self.config.clear();
            }
        }
    }

    pub(crate) fn rescan_dynamic(&mut self) {
        let mut dynamic = HashMap::new();

        for path in get_start_menu_paths() {
            if let Ok(lnk_files) = find_lnk_files(&path) {
                for (name, path) in lnk_files {
                    dynamic.insert(name, MenuCommand::Start(path));
                }
            }
        }

        if let Some(windows_apps_path) = get_windows_apps_path() {
            if let Ok(reparse_points) = find_reparse_points(&windows_apps_path) {
                for rp in reparse_points {
                    let command_name = if let Some(stem) = rp.full_path.file_stem().and_then(|s| s.to_str()) {
                        stem.to_string()
                    } else {
                        rp.name.clone()
                    };
                    dynamic.insert(command_name, MenuCommand::Start(rp.full_path));
                }
            }
        }

        self.dynamic = dynamic;
    }

    fn all_entries(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.builtins.keys()
            .chain(self.config.keys())
            .chain(self.dynamic.keys())
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn get(&self, name: &str) -> Option<&MenuCommand> {
        self.builtins.get(name)
            .or_else(|| self.config.get(name))
            .or_else(|| self.dynamic.get(name))
    }
}

#[derive(Debug)]
pub struct Hotkey {
    keys: Vec<String>,
}

impl Hotkey {
    const HOTKEY_ID: i32 = 1;

    /// Map the configured keys to a RegisterHotKey (modifiers, vk) pair.
    /// Valid combos are any number of modifiers (WIN/CTRL/ALT/SHIFT) plus
    /// exactly one other key.
    fn to_registration(&self) -> Result<(u32, u32), MenuError> {
        let mut modifiers = MOD_NOREPEAT;
        let mut vk: Option<u32> = None;

        for key in &self.keys {
            match key.to_uppercase().as_str() {
                "WIN" | "WINDOWS" => modifiers |= MOD_WIN,
                "CTRL" | "CONTROL" => modifiers |= MOD_CONTROL,
                "ALT" => modifiers |= MOD_ALT,
                "SHIFT" => modifiers |= MOD_SHIFT,
                _ => {
                    let code = Menu::parse_key_name_to_vk_code(key)?;
                    if vk.is_some() {
                        return Err(MenuError::InvalidArguments(format!(
                            "hotkey {:?} has more than one non-modifier key; \
                             use any number of WIN/CTRL/ALT/SHIFT plus exactly one other key",
                            self.keys
                        )));
                    }
                    vk = Some(code as u32);
                }
            }
        }

        match vk {
            Some(vk) => Ok((modifiers as u32, vk)),
            None => Err(MenuError::InvalidArguments(format!(
                "hotkey {:?} has no non-modifier key; \
                 use any number of WIN/CTRL/ALT/SHIFT plus exactly one other key",
                self.keys
            ))),
        }
    }

    /// Wait for hotkey activations via RegisterHotKey, invoking `callback`
    /// on each WM_HOTKEY. Event-driven: the thread sleeps in GetMessageW,
    /// so idle CPU cost is zero and presses can't be missed or repeated
    /// (MOD_NOREPEAT). Fatal errors are shown in a message box because the
    /// detached daemon has no visible stderr.
    pub fn listen<F>(&self, mut callback: F)
    where
        F: FnMut(),
    {
        let (modifiers, vk) = self.to_registration().unwrap_or_else(|e| {
            Self::fatal(&format!("Invalid hotkey configuration: {}", e));
        });

        unsafe {
            if RegisterHotKey(std::ptr::null_mut(), Self::HOTKEY_ID, modifiers, vk) == 0 {
                Self::fatal(&format!(
                    "Failed to register hotkey {} (error {}). \
                     Another application may already use this combo; \
                     change 'hotkey' in windmenu.toml.",
                    self.keys.join("+"),
                    GetLastError()
                ));
            }

            let event_name: Vec<u16> = "windmenu-shutdown-event\0".encode_utf16().collect();
            let shutdown_event = CreateEventW(
                std::ptr::null_mut(),
                1, // manual-reset
                0, // initially non-signaled
                event_name.as_ptr(),
            );
            if shutdown_event.is_null() {
                Self::fatal("Failed to create shutdown event");
            }

            println!("Hotkey registered ({})", self.keys.join("+"));
            let mut msg: MSG = std::mem::zeroed();
            loop {
                let result = MsgWaitForMultipleObjects(
                    1,
                    &shutdown_event,
                    0,
                    INFINITE,
                    QS_ALLINPUT,
                );

                if result == WAIT_OBJECT_0 {
                    break;
                }

                if result == WAIT_OBJECT_0 + 1 {
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_HOTKEY && msg.wParam == Self::HOTKEY_ID as usize {
                            callback();
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                } else {
                    break;
                }
            }

            CloseHandle(shutdown_event);
        }
    }

    /// Report a fatal daemon error and exit. Uses a message box since the
    /// detached daemon process has stdout/stderr redirected to null.
    fn fatal(message: &str) -> ! {
        eprintln!("{}", message);
        error_box(message);
        std::process::exit(1);
    }
}

/// Show an error message box. Used for errors that must reach the user even
/// when the detached daemon has stdout/stderr redirected to null.
pub fn error_box(message: &str) {
    unsafe {
        let text: Vec<u16> = OsStr::new(message).encode_wide().chain(std::iter::once(0)).collect();
        let caption: Vec<u16> = OsStr::new("windmenu").encode_wide().chain(std::iter::once(0)).collect();
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

/// The whole config, flat. Search behavior, window geometry, and color
/// overrides all sit at the top level; named themes live under `[themes.<name>]`
/// and are selected by `theme = "<name>"`. Unknown/legacy keys are ignored
/// (no `deny_unknown_fields`), so old sectioned configs degrade to defaults.
#[derive(Debug, Deserialize)]
struct MenuConfig {
    hotkey: Option<Vec<String>>, // Custom hotkey keys (e.g., ["WIN", "SPACE"])

    // Search behavior (rofi's -matching / -case-sensitive).
    matching: Option<String>,     // "complete" / "keywords" / "fuzzy"
    case_sensitive: Option<bool>, // Match case exactly (default: false)

    // Window geometry and font.
    lines: Option<usize>,   // Lines to show
    width: Option<usize>,   // Window width (centers the window)
    padding: Option<usize>, // Window padding
    font: Option<String>,   // Font as "Family Size", e.g. "Consolas 18"
    prompt: Option<String>, // Text shown in the input box

    // Color scheme: pick a named preset, then override individual keys.
    theme: Option<String>,               // Selects [themes.<name>]
    #[serde(flatten)]
    colors: Palette,                     // Top-level color overrides
    themes: Option<HashMap<String, Palette>>,

    commands: Option<Vec<CommandConfig>>,

    // Extra theme/command packs pulled in from other TOML files (paths relative
    // to this config's directory). Non-recursive: packs cannot import further.
    import: Option<Vec<String>>,
}

/// An imported pack: a TOML file that contributes only `[themes.*]` and/or
/// `[[commands]]`. It has no `import` field, so nested imports are ignored
/// (non-recursive); with no `deny_unknown_fields`, any stray settings a pack
/// carries are ignored rather than silently overriding the root config.
#[derive(Debug, Deserialize, Default)]
struct Pack {
    themes: Option<HashMap<String, Palette>>,
    commands: Option<Vec<CommandConfig>>,
}

/// Fold imported packs into the root config. Root config wins over imports, and
/// among imports the later one wins — for both themes (merged by name) and
/// commands. Command name-dedupe happens for free in `apply_config_commands`
/// (its `HashMap` is last-write-wins), so we only order the vec: imports first,
/// root last.
fn merge_packs(cfg: &mut MenuConfig, packs: Vec<Pack>) {
    let mut themes: HashMap<String, Palette> = HashMap::new();
    let mut commands: Vec<CommandConfig> = Vec::new();
    for pack in packs {
        if let Some(t) = pack.themes {
            themes.extend(t); // later import overwrites earlier
        }
        if let Some(c) = pack.commands {
            commands.extend(c);
        }
    }
    if let Some(root_themes) = cfg.themes.take() {
        themes.extend(root_themes); // root wins over every import
    }
    if !themes.is_empty() {
        cfg.themes = Some(themes);
    }
    if let Some(root_commands) = cfg.commands.take() {
        commands.extend(root_commands); // root applied last -> wins in the HashMap
    }
    if !commands.is_empty() {
        cfg.commands = Some(commands);
    }
}

/// Read and parse each imported pack (path relative to `base_dir`). A missing or
/// unparseable file warns and is skipped — imports never abort startup.
fn read_packs(imports: &[String], base_dir: &Path) -> (Vec<Pack>, Vec<String>) {
    let mut packs = Vec::new();
    let mut warnings = Vec::new();
    for rel in imports {
        let path = base_dir.join(rel);
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Pack>(&text) {
                Ok(pack) => packs.push(pack),
                Err(e) => warnings.push(format!("import '{}' failed to parse: {}", rel, e)),
            },
            Err(_) => warnings.push(format!("import '{}' not found", rel)),
        }
    }
    (packs, warnings)
}

/// Load the root config and merge any `import`ed packs into it. Returns the
/// merged config, its directory, and any non-fatal import warnings.
fn load_with_imports() -> Result<(MenuConfig, PathBuf, Vec<String>), MenuError> {
    let (mut cfg, config_dir) = MenuConfig::load()?;
    let mut warnings = Vec::new();
    if let Some(imports) = cfg.import.take() {
        let (packs, w) = read_packs(&imports, &config_dir);
        warnings = w;
        merge_packs(&mut cfg, packs);
    }
    Ok((cfg, config_dir, warnings))
}

/// Resolve a loaded config into renderer settings, starting from the built-in
/// defaults. Returns any non-fatal warnings (e.g. an unknown theme name) so
/// callers can surface them. A missing theme is not fatal: the launcher keeps
/// the default palette and runs.
fn resolve_settings(cfg: &MenuConfig) -> (wlines::Settings, Vec<String>) {
    let mut settings = theme::default_settings();
    let mut warnings = Vec::new();

    // 1. Named theme preset (if any), then 2. per-key overrides win over it.
    // "default" is a reserved name for the built-in palette (already applied by
    // default_settings above), so it always resolves silently even without a
    // [themes.default] table; a user-defined [themes.default] still wins.
    if let Some(ref name) = cfg.theme {
        match cfg.themes.as_ref().and_then(|t| t.get(name)) {
            Some(palette) => palette.apply(&mut settings),
            None if name == "default" => {}
            None => warnings.push(format!(
                "theme '{}' not found in [themes.*] — using defaults",
                name
            )),
        }
    }
    cfg.colors.apply(&mut settings);

    // 3. Window geometry and font.
    if let Some(lines) = cfg.lines {
        settings.line_count = lines;
    }
    if let Some(width) = cfg.width {
        settings.width = width as i32;
        settings.center_window = true;
    }
    if let Some(padding) = cfg.padding {
        settings.padding = padding as i32;
    }
    if let Some(ref font) = cfg.font {
        theme::apply_font(&mut settings, font);
    }
    if let Some(ref prompt) = cfg.prompt {
        settings.prompt = Some(prompt.clone());
    }

    // 4. Search behavior.
    if let Some(ref matching) = cfg.matching {
        settings.filter_mode = wlines::FilterMode::parse(matching);
    }
    if let Some(case_sensitive) = cfg.case_sensitive {
        settings.case_sensitive = case_sensitive;
    }

    (settings, warnings)
}

/// The commented default config, embedded at compile time. This is the same
/// file that ships in the repo, so `config init` produces a byte-identical,
/// fully documented windmenu.toml — the binary carries its own config template.
const DEFAULT_CONFIG: &str = include_str!("../windmenu.toml");

impl MenuConfig {
    const DEFAULT_CONFIG_PATH: &'static str = "windmenu.toml";

    fn load_from_file(config_path: &Path) -> Result<MenuConfig, MenuError> {
        let config_content = fs::read_to_string(config_path)
            .map_err(|e| MenuError::ConfigLoad(format!("Failed to read config file: {}", e)))?;
        let config: MenuConfig = toml::from_str(&config_content)
            .map_err(|e| MenuError::ConfigLoad(format!("Failed to parse TOML: {}", e)))?;
        Ok(config)
    }

    /// Resolve the config file to use: CWD first (portable installs), then the
    /// executable's directory (Scoop installs). None if neither exists.
    fn resolve_path() -> Option<PathBuf> {
        let cwd_path = Path::new(Self::DEFAULT_CONFIG_PATH);
        if cwd_path.exists() {
            let dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            return Some(dir.join(Self::DEFAULT_CONFIG_PATH));
        }
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let exe_config = exe_dir.join(Self::DEFAULT_CONFIG_PATH);
                if exe_config.exists() {
                    return Some(exe_config);
                }
            }
        }
        None
    }

    fn load() -> Result<(MenuConfig, PathBuf), MenuError> {
        match Self::resolve_path() {
            Some(path) => {
                let config = Self::load_from_file(&path)?;
                let config_dir = path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                Ok((config, config_dir))
            }
            None => {
                // Nothing found — return the CWD read error for backward-compatible messaging
                let cwd_path = Path::new(Self::DEFAULT_CONFIG_PATH);
                let config = Self::load_from_file(cwd_path)?;
                Ok((config, env::current_dir().unwrap_or_else(|_| PathBuf::from("."))))
            }
        }
    }
}

/// `config path` / `test config`: report where windmenu looks for its config
/// and which file (if any) is currently in effect.
pub fn config_path() {
    let exe_path = env::current_exe().ok();
    println!("Exe path: {}", exe_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unknown".into()));
    println!("CWD: {}", env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "unknown".into()));

    let cwd_config = Path::new(MenuConfig::DEFAULT_CONFIG_PATH);
    println!("CWD config ({}): {}", cwd_config.display(), if cwd_config.exists() { "found" } else { "not found" });

    if let Some(exe_dir) = exe_path.as_ref().and_then(|p| p.parent()) {
        let exe_config = exe_dir.join(MenuConfig::DEFAULT_CONFIG_PATH);
        println!("Exe config ({}): {}", exe_config.display(), if exe_config.exists() { "found" } else { "not found" });
    }

    match MenuConfig::resolve_path() {
        Some(path) => match MenuConfig::load_from_file(&path) {
            Ok(mut cfg) => {
                println!("Result: using {}", path.display());
                let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
                if let Some(imports) = cfg.import.take() {
                    for rel in &imports {
                        let ip = base_dir.join(rel);
                        println!("Import ({}): {}", ip.display(), if ip.exists() { "found" } else { "not found" });
                    }
                    let (packs, import_warnings) = read_packs(&imports, base_dir);
                    for warning in import_warnings {
                        println!("Warning: {}", warning);
                    }
                    merge_packs(&mut cfg, packs);
                }
                let (_, warnings) = resolve_settings(&cfg);
                for warning in warnings {
                    println!("Warning: {}", warning);
                }
            }
            Err(e) => println!("Result: {} ({})", e, path.display()),
        },
        None => println!("Result: no config file found — using built-in defaults"),
    }
}

/// The default location `config init` writes to: next to the executable. This
/// matches the portable-binary story and Scoop installs, and is independent of
/// the shell's current directory.
fn init_target() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|e| format!("cannot locate executable: {}", e))?;
    let dir = exe.parent().ok_or_else(|| "executable has no parent directory".to_string())?;
    Ok(dir.join(MenuConfig::DEFAULT_CONFIG_PATH))
}

const RESTART_REMINDER: &str =
    "Restart the daemon ('windmenu daemon restart') or run 'Reload Config' from the menu to apply changes.";

/// `config init`: write the embedded default config next to the executable.
/// Refuses to overwrite an existing file unless `force` is set. Returns a
/// process exit code.
pub fn config_init(force: bool) -> i32 {
    let target = match init_target() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("config init: {}", e);
            return 1;
        }
    };

    if target.exists() && !force {
        eprintln!("config init: {} already exists; use --force to overwrite", target.display());
        return 1;
    }

    if let Err(e) = fs::write(&target, DEFAULT_CONFIG) {
        eprintln!("config init: failed to write {}: {}", target.display(), e);
        return 1;
    }

    println!("Wrote default config to {}", target.display());
    println!("{}", RESTART_REMINDER);
    0
}

/// `config edit`: open the resolved config in an editor, creating it first if
/// none exists. Uses %EDITOR% when set, otherwise notepad. Returns an exit code.
pub fn config_edit() -> i32 {
    let path = match MenuConfig::resolve_path() {
        Some(p) => p,
        None => {
            // No config anywhere yet: create one next to the exe, then edit it.
            let code = config_init(false);
            if code != 0 {
                return code;
            }
            match MenuConfig::resolve_path() {
                Some(p) => p,
                None => {
                    eprintln!("config edit: config file not found after init");
                    return 1;
                }
            }
        }
    };

    let editor = env::var("EDITOR").unwrap_or_else(|_| "notepad.exe".to_string());
    let mut parts = split_command(&editor);
    if parts.is_empty() {
        eprintln!("config edit: EDITOR is empty");
        return 1;
    }
    let program = parts.remove(0);

    println!("Opening {} in {}", path.display(), editor);
    match Command::new(&program).args(&parts).arg(&path).spawn() {
        Ok(mut child) => {
            let _ = child.wait();
        }
        Err(e) => {
            eprintln!("config edit: failed to launch {}: {}", program, e);
            return 1;
        }
    }

    println!("{}", RESTART_REMINDER);
    0
}

/// Split an editor command into program + arguments. Double quotes group a
/// token so paths with spaces survive (e.g. `"C:\Program Files\x.exe" --wait`),
/// while an unquoted, space-free command still splits on whitespace (`code
/// --wait`). A bare unquoted path with spaces is treated as a single token,
/// since that's the more common Windows case than an unquoted program + flags.
fn split_command(command: &str) -> Vec<String> {
    // If nothing looks like a flag/separator, treat the whole string as one
    // path — this preserves unquoted "C:\Program Files\..." editor paths.
    if !command.contains('"') && !command.contains(" -") {
        let trimmed = command.trim();
        return if trimmed.is_empty() { Vec::new() } else { vec![trimmed.to_string()] };
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;

    for ch in command.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        tokens.push(current);
    }
    tokens
}


#[derive(Debug, Deserialize)]
struct CommandConfig {
    name: String,
    #[serde(flatten)]
    command_type: CommandType,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandType {
    Args { args: Vec<String> },
    Keys { keys: Vec<String> },
}

pub struct Menu {
    pub process_running: AtomicBool,
    pub entries: Arc<RwLock<EntryStore>>,
    pub settings: wlines::Settings,
    pub hotkey: Hotkey,
}

fn get_start_menu_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(appdata) = env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu"));
    }

    if let Ok(program_data) = env::var("ProgramData") {
        paths.push(PathBuf::from(program_data).join("Microsoft\\Windows\\Start Menu"));
    }

    paths
}

fn find_lnk_files(dir: &Path) -> std::io::Result<HashMap<String, PathBuf>> {
    let mut lnk_files = HashMap::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            lnk_files.extend(find_lnk_files(&path)?);
        } else if path.extension().map_or(false, |ext| ext == "lnk") {
            if let Some(file_name) = path.file_stem().and_then(|n| n.to_str()) {
                lnk_files.insert(file_name.to_lowercase().to_string(), path);
            }
        }
    }

    Ok(lnk_files)
}


impl Menu {
    pub fn new() -> Menu {
        let process_running = AtomicBool::new(false);

        // Ctrl+Alt+Space
        let mut hotkey = Hotkey {
            keys: vec!["CTRL".to_string(), "ALT".to_string(), "SPACE".to_string()],
        };
        let mut settings = theme::default_settings();
        let entries = Arc::new(RwLock::new(EntryStore::empty()));

        if let Ok((cfg, _config_dir, import_warnings)) = load_with_imports() {
            let (resolved, warnings) = resolve_settings(&cfg);
            settings = resolved;
            for warning in import_warnings.iter().chain(warnings.iter()) {
                eprintln!("Warning: {}", warning);
            }
            if let Some(ref keys) = &cfg.hotkey {
                hotkey.keys = keys.clone();
            }
            if let Some(cmds) = cfg.commands {
                entries.write().unwrap().apply_config_commands(cmds);
            }
        }

        Menu {
            process_running,
            entries,
            settings,
            hotkey,
        }
    }

    pub fn show(self: Arc<Self>) -> Result<(), MenuError> {
        if self.process_running.swap(true, Ordering::SeqCst) {
            return Err(MenuError::MenuAlreadyRunning);
        }

        let entries = self.prepare_entries();

        // Run the menu window and its message loop on a dedicated thread
        thread::spawn(move || {
            let result = match wlines::show(&self.settings, &entries) {
                Some(selected) => self.execute_command(&selected),
                None => Ok(()), // User cancelled
            };

            self.process_running.store(false, Ordering::SeqCst);

            // Log any errors that occurred
            if let Err(e) = result {
                eprintln!("Menu error: {}", e);
            }
        });

        Ok(())
    }

    fn prepare_entries(&self) -> Vec<String> {
        self.entries.read().unwrap().all_entries()
    }

    fn execute_command(&self, selected: &str) -> Result<(), MenuError> {
        let cmd = {
            self.entries.read().unwrap().get(selected).cloned()
        };

        match cmd {
            Some(MenuCommand::Start(path)) => {
                println!("Executing Start command: {}", selected);
                Self::launch_program(&path)
            },
            Some(MenuCommand::Configured(args)) => {
                println!("Executing command: {}", selected);
                Self::launch_command(&args)
            },
            Some(MenuCommand::KeyCombo(keys)) => {
                println!("Executing key combination: {}", selected);
                Self::send_key_combination(&keys)
            },
            Some(MenuCommand::ToggleCapsLock) => {
                println!("Toggling caps lock: {}", selected);
                Self::toggle_caps_lock()
            },
            Some(MenuCommand::RefreshApps) => {
                let entries = self.entries.clone();
                thread::spawn(move || {
                    entries.write().unwrap().rescan_dynamic();
                });
                Ok(())
            },
            Some(MenuCommand::ReloadConfig) => {
                let entries = self.entries.clone();
                thread::spawn(move || {
                    entries.write().unwrap().reload_config();
                });
                Ok(())
            },
            None => {
                if !selected.is_empty() {
                    Err(MenuError::CommandExecution(format!("No command found for selection: '{}'", selected)))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn launch_program(path: &Path) -> Result<(), MenuError> {
        unsafe {
            // Convert path to wide string
            let path_wide: Vec<u16> = path.as_os_str()
                                          .encode_wide()
                                          .chain(std::iter::once(0))
                                          .collect();

            // Use ShellExecuteW to launch the program - SW_RESTORE brings window to foreground
            let result = ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_RESTORE,  // SW_RESTORE is more likely to bring window to foreground than SW_SHOWNORMAL
            );

            if result as usize <= 32 {
                Err(MenuError::WindowsApi(result as u32))
            } else {
                Ok(())
            }
        }
    }

    fn launch_command(args: &[String]) -> Result<(), MenuError> {
        if args.is_empty() {
            return Err(MenuError::InvalidArguments("No command provided".to_string()));
        }

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        // Hide console window for these commands
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(MenuError::ProcessSpawn(e)),
        }
    }

    fn send_key_combination(keys: &[String]) -> Result<(), MenuError> {
        if keys.is_empty() {
            return Err(MenuError::InvalidArguments("No keys provided".to_string()));
        }

        // Parse virtual key codes from key names
        let vk_codes: Result<Vec<u16>, MenuError> = keys.iter()
                                                     .map(|key| Self::parse_key_name_to_vk_code(key))
                                                     .collect();

        let vk_codes = vk_codes?;

        unsafe {
            let mut inputs = Vec::new();

            // Press all keys down
            for &vk_code in &vk_codes {
                let mut input: INPUT = std::mem::zeroed();
                input.type_ = INPUT_KEYBOARD;
                *input.u.ki_mut() = KEYBDINPUT {
                    wVk: vk_code,
                    wScan: 0,
                    dwFlags: 0, // Key down
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            // Release all keys up (in reverse order)
            for &vk_code in vk_codes.iter().rev() {
                let mut input: INPUT = std::mem::zeroed();
                input.type_ = INPUT_KEYBOARD;
                *input.u.ki_mut() = KEYBDINPUT {
                    wVk: vk_code,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                };
                inputs.push(input);
            }

            // Send all inputs
            let sent = SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );

            if sent == 0 {
                Err(MenuError::KeyInput(GetLastError()))
            } else {
                Ok(())
            }
        }
    }

    fn toggle_caps_lock() -> Result<(), MenuError> {
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.type_ = INPUT_KEYBOARD;
            *input.u.ki_mut() = KEYBDINPUT {
                wVk: VK_CAPITAL as u16,
                wScan: 0,
                dwFlags: 0, // Key down
                time: 0,
                dwExtraInfo: 0,
            };

            // Send key down
            if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) == 0 {
                return Err(MenuError::KeyInput(GetLastError()));
            }

            // Send key up
            input.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;
            if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) == 0 {
                return Err(MenuError::KeyInput(GetLastError()));
            }

            println!("Caps lock toggled successfully");
            Ok(())
        }
    }

    fn parse_key_name_to_vk_code(key: &str) -> Result<u16, MenuError> {
        match key.to_uppercase().as_str() {
            "ALT" => Ok(VK_MENU as u16),
            "CTRL" | "CONTROL" => Ok(VK_CONTROL as u16),
            "SHIFT" => Ok(VK_SHIFT as u16),
            "WIN" | "WINDOWS" => Ok(VK_LWIN as u16),
            "TAB" => Ok(VK_TAB as u16),
            "ESC" | "ESCAPE" => Ok(VK_ESCAPE as u16),
            "SPACE" => Ok(VK_SPACE as u16),
            "ENTER" => Ok(VK_RETURN as u16),
            "CAPS" => Ok(VK_CAPITAL as u16),
            // Function keys
            "F1" => Ok(VK_F1 as u16), "F2" => Ok(VK_F2 as u16), "F3" => Ok(VK_F3 as u16), "F4" => Ok(VK_F4 as u16),
            "F5" => Ok(VK_F5 as u16), "F6" => Ok(VK_F6 as u16), "F7" => Ok(VK_F7 as u16), "F8" => Ok(VK_F8 as u16),
            "F9" => Ok(VK_F9 as u16), "F10" => Ok(VK_F10 as u16), "F11" => Ok(VK_F11 as u16), "F12" => Ok(VK_F12 as u16),
            // Arrow keys
            "LEFT" => Ok(VK_LEFT as u16), "UP" => Ok(VK_UP as u16), "RIGHT" => Ok(VK_RIGHT as u16), "DOWN" => Ok(VK_DOWN as u16),
            // Number keys
            "0" => Ok(0x30), "1" => Ok(0x31), "2" => Ok(0x32), "3" => Ok(0x33), "4" => Ok(0x34),
            "5" => Ok(0x35), "6" => Ok(0x36), "7" => Ok(0x37), "8" => Ok(0x38), "9" => Ok(0x39),
            // Special punctuation keys
            "COMMA" | "," => Ok(VK_OEM_COMMA as u16),
            "PERIOD" | "." => Ok(VK_OEM_PERIOD as u16),
            "SEMICOLON" | ";" => Ok(VK_OEM_1 as u16),
            "SLASH" | "/" => Ok(VK_OEM_2 as u16),
            "BACKSLASH" | "\\" => Ok(VK_OEM_5 as u16),
            "QUOTE" | "'" => Ok(VK_OEM_7 as u16),
            "BACKTICK" | "`" => Ok(VK_OEM_3 as u16),
            "MINUS" | "-" => Ok(VK_OEM_MINUS as u16),
            "EQUALS" | "PLUS" | "=" | "+" => Ok(VK_OEM_PLUS as u16),
            "LBRACKET" | "[" => Ok(VK_OEM_4 as u16),
            "RBRACKET" | "]" => Ok(VK_OEM_6 as u16),
            // Letter keys (A-Z)
            key if key.len() == 1 && key.chars().next().unwrap().is_ascii_alphabetic() => {
                Ok(key.chars().next().unwrap() as u16)
            },
            _ => Err(MenuError::KeyParsing(key.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_command;

    #[test]
    fn bare_program_stays_single_token() {
        assert_eq!(split_command("notepad.exe"), vec!["notepad.exe"]);
    }

    #[test]
    fn unquoted_path_with_spaces_stays_single_token() {
        // The common Windows case: an editor path under "Program Files" with no
        // quoting and no flags must not be split on its internal spaces.
        assert_eq!(
            split_command(r"C:\Program Files\Editor\ed.exe"),
            vec![r"C:\Program Files\Editor\ed.exe"],
        );
    }

    #[test]
    fn program_with_dash_flag_splits() {
        assert_eq!(split_command("code --wait"), vec!["code", "--wait"]);
    }

    #[test]
    fn quoted_program_then_flag_splits_but_keeps_path() {
        assert_eq!(
            split_command(r#""C:\Program Files\Editor\ed.exe" --wait"#),
            vec![r"C:\Program Files\Editor\ed.exe", "--wait"],
        );
    }

    #[test]
    fn quoted_path_without_flags_is_one_token() {
        assert_eq!(
            split_command(r#""C:\Program Files\Editor\ed.exe""#),
            vec![r"C:\Program Files\Editor\ed.exe"],
        );
    }

    #[test]
    fn empty_and_whitespace_yield_no_tokens() {
        assert!(split_command("").is_empty());
        assert!(split_command("   ").is_empty());
    }

    #[test]
    fn surrounding_whitespace_trimmed_on_single_token() {
        assert_eq!(split_command("  notepad.exe  "), vec!["notepad.exe"]);
    }

    #[test]
    fn multiple_flags_split() {
        assert_eq!(
            split_command("gvim -f --nofork"),
            vec!["gvim", "-f", "--nofork"],
        );
    }

    use super::{merge_packs, read_packs, resolve_settings, MenuConfig, Pack, DEFAULT_CONFIG};
    use crate::theme::default_settings;
    use crate::wlines::parse_color;

    fn parse_config(s: &str) -> MenuConfig {
        toml::from_str(s).expect("config should parse")
    }

    fn parse_pack(s: &str) -> Pack {
        toml::from_str(s).expect("pack should parse")
    }

    #[test]
    fn theme_preset_is_applied() {
        let cfg = parse_config(
            r##"
            theme = "nord"
            [themes.nord]
            bg_select = "#5e81ac"
        "##,
        );
        let (settings, warnings) = resolve_settings(&cfg);
        assert!(warnings.is_empty());
        assert_eq!(settings.bg_select, parse_color("#5e81ac").unwrap());
    }

    #[test]
    fn override_beats_preset() {
        let cfg = parse_config(
            r##"
            theme = "nord"
            bg_select = "#ffffff"
            [themes.nord]
            bg_select = "#5e81ac"
        "##,
        );
        let (settings, _) = resolve_settings(&cfg);
        assert_eq!(settings.bg_select, parse_color("#ffffff").unwrap());
    }

    #[test]
    fn reserved_default_theme_never_warns() {
        // "default" resolves to the built-in palette even with no [themes.default].
        let cfg = parse_config(r#"theme = "default""#);
        let (settings, warnings) = resolve_settings(&cfg);
        assert!(warnings.is_empty());
        assert_eq!(settings.bg, default_settings().bg);
    }

    #[test]
    fn missing_theme_warns_and_keeps_default() {
        let cfg = parse_config(r#"theme = "nope""#);
        let (settings, warnings) = resolve_settings(&cfg);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("nope"));
        // The default palette is left intact; the launcher still resolves.
        let def = default_settings();
        assert_eq!(settings.bg, def.bg);
        assert_eq!(settings.bg_select, def.bg_select);
    }

    #[test]
    fn flat_fields_parse_and_apply() {
        let cfg = parse_config(
            r#"
            hotkey = ["ALT", "F2"]
            matching = "keywords"
            case_sensitive = true
            lines = 20
            width = 640
            padding = 4
            font = "Cascadia Code 14"
            prompt = "Run:"
        "#,
        );
        let (settings, _) = resolve_settings(&cfg);
        assert_eq!(settings.line_count, 20);
        assert_eq!(settings.width, 640);
        assert_eq!(settings.padding, 4);
        assert_eq!(settings.font_name, "Cascadia Code");
        assert_eq!(settings.font_size, 14);
        assert_eq!(settings.prompt.as_deref(), Some("Run:"));
        assert!(settings.case_sensitive);
        assert_eq!(cfg.hotkey, Some(vec!["ALT".to_string(), "F2".to_string()]));
    }

    #[test]
    fn shipped_config_matches_builtin_defaults() {
        // `config init` writes DEFAULT_CONFIG; resolving it must reproduce the
        // no-config appearance exactly (colors + geometry + font).
        let cfg: MenuConfig = toml::from_str(DEFAULT_CONFIG).expect("shipped config parses");
        let (settings, warnings) = resolve_settings(&cfg);
        assert!(warnings.is_empty(), "shipped config warned: {:?}", warnings);
        let def = default_settings();
        assert_eq!(settings.bg, def.bg);
        assert_eq!(settings.fg, def.fg);
        assert_eq!(settings.bg_select, def.bg_select);
        assert_eq!(settings.fg_select, def.fg_select);
        assert_eq!(settings.bg_edit, def.bg_edit);
        assert_eq!(settings.fg_edit, def.fg_edit);
        assert_eq!(settings.line_count, def.line_count);
        assert_eq!(settings.width, def.width);
        assert_eq!(settings.padding, def.padding);
        assert_eq!(settings.font_name, def.font_name);
        assert_eq!(settings.font_size, def.font_size);
    }

    #[test]
    fn merge_packs_adds_pack_theme() {
        let mut cfg = parse_config(r#"theme = "nord""#);
        let pack = parse_pack("[themes.nord]\nbg = \"#2e3440\"\n");
        merge_packs(&mut cfg, vec![pack]);
        let (settings, warnings) = resolve_settings(&cfg);
        assert!(warnings.is_empty());
        assert_eq!(settings.bg, parse_color("#2e3440").unwrap());
    }

    #[test]
    fn merge_packs_root_theme_wins_over_pack() {
        let mut cfg = parse_config("[themes.nord]\nbg = \"#111111\"\n");
        let pack = parse_pack("[themes.nord]\nbg = \"#2e3440\"\n");
        merge_packs(&mut cfg, vec![pack]);
        let bg = cfg.themes.as_ref().unwrap().get("nord").unwrap().bg.clone();
        assert_eq!(bg.as_deref(), Some("#111111"));
    }

    #[test]
    fn merge_packs_later_import_wins() {
        let mut cfg = parse_config("");
        let a = parse_pack("[themes.x]\nbg = \"#aaaaaa\"\n");
        let b = parse_pack("[themes.x]\nbg = \"#bbbbbb\"\n");
        merge_packs(&mut cfg, vec![a, b]);
        let bg = cfg.themes.as_ref().unwrap().get("x").unwrap().bg.clone();
        assert_eq!(bg.as_deref(), Some("#bbbbbb"));
    }

    #[test]
    fn merge_packs_commands_imports_before_root() {
        let mut cfg = parse_config("[[commands]]\nname = \"Root\"\nargs = [\"r\"]\n");
        let pack = parse_pack("[[commands]]\nname = \"Pack\"\nargs = [\"p\"]\n");
        merge_packs(&mut cfg, vec![pack]);
        let names: Vec<&str> = cfg.commands.as_ref().unwrap().iter().map(|c| c.name.as_str()).collect();
        // Imports first, root last — so root wins a name clash in apply_config_commands.
        assert_eq!(names, vec!["Pack", "Root"]);
    }

    #[test]
    fn pack_ignores_nested_import_and_stray_keys() {
        // A pack carries only themes/commands; hotkey/bg/import are silently ignored.
        let pack = parse_pack(
            "hotkey = [\"WIN\", \"SPACE\"]\nbg = \"#123456\"\nimport = [\"other.toml\"]\n[themes.z]\nfg = \"#ffffff\"\n",
        );
        assert!(pack.themes.as_ref().unwrap().contains_key("z"));
        assert!(pack.commands.is_none());
    }

    #[test]
    fn read_packs_missing_file_warns_and_skips() {
        let (packs, warnings) = read_packs(&["does-not-exist.toml".to_string()], std::path::Path::new("."));
        assert!(packs.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not found"));
    }

    #[test]
    fn read_packs_reads_real_file() {
        let dir = std::env::temp_dir();
        let name = format!("windmenu-pack-test-{}.toml", std::process::id());
        let path = dir.join(&name);
        std::fs::write(&path, "[themes.temp]\nbg = \"#010203\"\n").unwrap();
        let (packs, warnings) = read_packs(&[name], &dir);
        let _ = std::fs::remove_file(&path);
        assert!(warnings.is_empty());
        assert_eq!(packs.len(), 1);
        assert!(packs[0].themes.as_ref().unwrap().contains_key("temp"));
    }
}
