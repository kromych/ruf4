// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Application state for ruf4.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ruf4_tui::helpers::*;
use ruf4_tui::input::{Input, InputKey, InputMouseState, kbmod, vk};

use crate::action::{self, Action, Binding};
use crate::fileops;
use crate::panel::{Panel, SortBy};
use crate::platform;
use crate::preview::{self, Preview};
use crate::settings;
use crate::theme::Theme;

// ── Constants ──────────────────────────────────────────────────────────────

pub const MAX_HISTORY: usize = 20;
const DOUBLE_CLICK_MS: u128 = 400;
const PAGE_SCROLL: usize = 20;
const MOUSE_SCROLL: usize = 3;
/// Panel title row (menubar + top border).
const PANEL_TITLE_ROW: CoordType = 2;
/// Panel area: rows from top of screen to first file entry (menubar + border + title + header).
const PANEL_ENTRY_START: CoordType = 4;
/// Dialog content height overhead beyond entries (top spacer + prompt + gap + bottom spacer).
pub const LIST_DIALOG_CONTENT_PAD: CoordType = 4;
/// Dialog border lines (top + bottom) for outer height calculation.
const LIST_DIALOG_BORDER: CoordType = 2;
/// Rows from dialog top to first list entry (border + spacer + prompt).
const LIST_DIALOG_ENTRY_OFFSET: CoordType = 3;

pub const SORT_OPTIONS: &[(&str, SortBy)] = &[
    ("Name", SortBy::Name),
    ("Extension", SortBy::Extension),
    ("Date", SortBy::Modified),
    ("Size", SortBy::Size),
];

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivePanel {
    Left,
    Right,
}

/// What a list-selection dialog should do when the user picks an item.
#[derive(Clone)]
pub enum ListSelectKind {
    ChangeRoot { roots: Vec<PathBuf> },
    DirHistory { entries: Vec<PathBuf> },
    CmdHistory { entries: Vec<String> },
    ChooseSort,
}

pub enum Dialog {
    None,
    Help {
        scroll: usize,
    },
    Info {
        message: String,
    },
    Error {
        message: String,
    },
    MkDir {
        name: String,
    },
    Delete {
        files: Vec<String>,
    },
    Copy {
        files: Vec<String>,
        dest: String,
    },
    Move {
        files: Vec<String>,
        dest: String,
    },
    ShellOutput {
        command: String,
        output: String,
        scroll: usize,
    },
    ConfirmQuit {
        save_settings: bool,
    },
    SelectGroup {
        pattern: String,
        select: bool,
    },
    /// Unified list-selection dialog (roots, history, sort, etc.).
    ListSelect {
        title: String,
        prompt: String,
        labels: Vec<String>,
        cursor: usize,
        min_width: CoordType,
        kind: ListSelectKind,
    },
    ConfirmOverwrite {
        target_name: String,
        pending: Vec<(PathBuf, PathBuf)>,
        errors: Vec<String>,
        is_copy: bool,
    },
    Rename {
        name: String,
    },
}

pub struct State {
    pub left: Panel,
    pub right: Panel,
    pub active: ActivePanel,
    pub quit: bool,
    pub command_line: String,
    pub command_line_active: bool,
    pub cmd_cursor: usize,
    pub dialog: Dialog,
    pub menu_active: bool,
    pub want_menu_focus: bool,
    pub term_size: Size,
    pub quick_view: bool,
    pub preview: Preview,
    pub preview_scroll: usize,
    pub preview_path: Option<PathBuf>,
    pub alt_search: String,
    pub input_cursor: usize, // cursor position (char index) in text-input dialogs
    pub dir_history: Vec<PathBuf>,
    pub cmd_history: Vec<String>,
    pub bindings: Vec<Binding>,
    pub help_text: Vec<(String, &'static str, Action)>,
    pub theme: Theme,
    last_click: Option<(Instant, Point)>,
}

// ── Construction ────────────────────────────────────────────────────────────

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let bindings = action::default_bindings();
        let help_text = action::build_help_text(&bindings);
        let theme = Theme::far();
        let mut state = Self {
            left: Panel::new(cwd.clone()),
            right: Panel::new(cwd),
            active: ActivePanel::Left,
            quit: false,
            command_line: String::new(),
            command_line_active: false,
            cmd_cursor: 0,
            dialog: Dialog::None,
            menu_active: false,
            want_menu_focus: false,
            term_size: Size {
                width: 0,
                height: 0,
            },
            quick_view: false,
            preview: Preview::empty(),
            preview_scroll: 0,
            preview_path: None,
            alt_search: String::new(),
            input_cursor: 0,
            dir_history: Vec::new(),
            cmd_history: Vec::new(),
            bindings,
            help_text,
            theme,
            last_click: None,
        };

        if let Some(s) = settings::Settings::load() {
            settings::apply_to_panel(&mut state.left, &s.left);
            settings::apply_to_panel(&mut state.right, &s.right);
            state.active = s.active_panel;
            state.quick_view = s.quick_view;
            state.dir_history = s.dir_history;
            state.cmd_history = s.cmd_history;
            state.bindings = s.bindings;
            state.help_text = action::build_help_text(&state.bindings);
            state.theme = s.theme;
        }

        state
    }

    pub fn for_testing(left: Panel, right: Panel) -> Self {
        let bindings = action::default_bindings();
        let help_text = action::build_help_text(&bindings);
        let theme = Theme::far();
        Self {
            left,
            right,
            active: ActivePanel::Left,
            quit: false,
            command_line: String::new(),
            command_line_active: false,
            cmd_cursor: 0,
            dialog: Dialog::None,
            menu_active: false,
            want_menu_focus: false,
            term_size: Size {
                width: 80,
                height: 24,
            },
            quick_view: false,
            preview: Preview::empty(),
            preview_scroll: 0,
            preview_path: None,
            alt_search: String::new(),
            input_cursor: 0,
            dir_history: Vec::new(),
            cmd_history: Vec::new(),
            bindings,
            help_text,
            theme,
            last_click: None,
        }
    }

    // ── Panel accessors ─────────────────────────────────────────────────

    pub fn active_panel(&self) -> &Panel {
        match self.active {
            ActivePanel::Left => &self.left,
            ActivePanel::Right => &self.right,
        }
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        match self.active {
            ActivePanel::Left => &mut self.left,
            ActivePanel::Right => &mut self.right,
        }
    }

    pub fn inactive_panel(&self) -> &Panel {
        match self.active {
            ActivePanel::Left => &self.right,
            ActivePanel::Right => &self.left,
        }
    }

    pub fn inactive_panel_mut(&mut self) -> &mut Panel {
        match self.active {
            ActivePanel::Left => &mut self.right,
            ActivePanel::Right => &mut self.left,
        }
    }

    // ── Central action dispatch ─────────────────────────────────────────

    /// Execute a single action. This is the central dispatch point: keybindings,
    /// menu items, function-bar clicks, and help-row clicks all funnel here.
    pub fn execute_action(&mut self, action: Action) {
        match action {
            // Navigation
            Action::CursorUp => self.active_panel_mut().cursor_up(1),
            Action::CursorDown => self.active_panel_mut().cursor_down(1),
            Action::PageUp => self.active_panel_mut().cursor_up(PAGE_SCROLL),
            Action::PageDown => self.active_panel_mut().cursor_down(PAGE_SCROLL),
            Action::CursorHome => self.active_panel_mut().cursor_home(),
            Action::CursorEnd => self.active_panel_mut().cursor_end(),
            Action::OpenOrEnter => self.open_or_enter(),
            Action::ParentDir => {
                // Go up one directory (Backspace).
                if self.active_panel().path.parent().is_some() {
                    self.active_panel_mut().cursor = 0; // ".." is always first
                    self.open_or_enter();
                }
            }
            Action::SwitchPanel => {
                self.active = match self.active {
                    ActivePanel::Left => ActivePanel::Right,
                    ActivePanel::Right => ActivePanel::Left,
                };
            }

            // Selection
            Action::ToggleSelect => self.active_panel_mut().toggle_select(),
            Action::SelectGroup => {
                self.input_cursor = 1;
                self.dialog = Dialog::SelectGroup {
                    pattern: "*".to_string(),
                    select: true,
                };
            }
            Action::DeselectGroup => {
                self.input_cursor = 1;
                self.dialog = Dialog::SelectGroup {
                    pattern: "*".to_string(),
                    select: false,
                };
            }
            Action::InvertSelection => self.active_panel_mut().invert_selection(),
            Action::SelectAll => self.active_panel_mut().select_all(),
            Action::DeselectAll => self.active_panel_mut().clear_selection(),

            // File operations
            Action::Copy => self.open_copy_dialog(),
            Action::Move => self.open_move_dialog(),
            Action::Rename => self.open_rename_dialog(),
            Action::Delete => self.open_delete_dialog(),
            Action::MkDir => {
                self.input_cursor = 0;
                self.dialog = Dialog::MkDir {
                    name: String::new(),
                };
            }

            // View / sort
            Action::ToggleQuickView => {
                self.quick_view = !self.quick_view;
                if self.quick_view {
                    self.preview_path = None;
                }
            }
            Action::ToggleHidden => {
                let panel = self.active_panel_mut();
                panel.show_hidden = !panel.show_hidden;
                panel.refresh();
            }
            Action::SortBy(sort_by) => self.active_panel_mut().set_sort(sort_by),
            Action::ChooseSort => {
                let cursor = SORT_OPTIONS
                    .iter()
                    .position(|(_, s)| *s == self.active_panel().sort_by)
                    .unwrap_or(0);
                let labels = SORT_OPTIONS.iter().map(|(l, _)| l.to_string()).collect();
                self.dialog = Dialog::ListSelect {
                    title: "Sort By - Enter=OK  Esc=Cancel".to_string(),
                    prompt: "Sort mode:".to_string(),
                    labels,
                    cursor,
                    min_width: 25,
                    kind: ListSelectKind::ChooseSort,
                };
            }

            // App
            Action::Help => self.dialog = Dialog::Help { scroll: 0 },
            Action::SaveSettings => match self.save_settings() {
                Ok(()) => {
                    self.dialog = Dialog::Info {
                        message: "Settings saved.".to_string(),
                    };
                }
                Err(msg) => self.dialog = Dialog::Error { message: msg },
            },
            Action::Refresh => {
                self.left.refresh();
                self.right.refresh();
            }
            Action::ChangeRoot => self.open_choose_root(),
            Action::DirHistory => self.open_dir_history(),
            Action::CmdHistory => self.open_cmd_history(),
            Action::FocusMenu => self.want_menu_focus = true,
            Action::Quit => {
                self.dialog = Dialog::ConfirmQuit {
                    save_settings: true,
                };
            }
        }
    }

    // ── Actions ─────────────────────────────────────────────────────────

    pub fn save_settings(&self) -> Result<(), String> {
        settings::settings_from_state(self).save()
    }

    pub fn open_or_enter(&mut self) {
        if let Some(entry) = self.active_panel().current_entry() {
            if entry.is_dir {
                self.active_panel_mut().enter();
                let path = self.active_panel().path.clone();
                self.record_dir_change(&path);
            } else {
                let path = self.active_panel().path.join(&entry.name);
                if let Err(msg) = platform::open_file(&path) {
                    self.dialog = Dialog::Error { message: msg };
                }
            }
        }
    }

    pub fn record_dir_change(&mut self, path: &Path) {
        push_recent(&mut self.dir_history, path.to_path_buf());
    }

    pub fn record_command(&mut self, cmd: &str) {
        let cmd = cmd.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        push_recent(&mut self.cmd_history, cmd);
    }

    pub fn open_cmd_history(&mut self) {
        if self.cmd_history.is_empty() {
            self.dialog = Dialog::Info {
                message: "Command history is empty.".to_string(),
            };
            return;
        }
        let entries = self.cmd_history.clone();
        let labels = entries.clone();
        self.dialog = Dialog::ListSelect {
            title: "Command History - Enter=OK  Esc=Cancel".to_string(),
            prompt: "Recent commands:".to_string(),
            labels,
            cursor: 0,
            min_width: 40,
            kind: ListSelectKind::CmdHistory { entries },
        };
    }

    pub fn open_dir_history(&mut self) {
        if self.dir_history.is_empty() {
            self.dialog = Dialog::Info {
                message: "Directory history is empty.".to_string(),
            };
            return;
        }
        let entries = self.dir_history.clone();
        let labels = entries
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.dialog = Dialog::ListSelect {
            title: "Directory History - Enter=OK  Esc=Cancel".to_string(),
            prompt: "Recent directories:".to_string(),
            labels,
            cursor: 0,
            min_width: 40,
            kind: ListSelectKind::DirHistory { entries },
        };
    }

    pub fn open_choose_root(&mut self) {
        let roots = platform::discover_roots();
        if roots.is_empty() {
            return;
        }
        let current = &self.active_panel().path;
        let cursor = roots
            .iter()
            .position(|r| current.starts_with(r))
            .unwrap_or(0);
        let labels = roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.dialog = Dialog::ListSelect {
            title: "Change Root - Enter=OK  Esc=Cancel".to_string(),
            prompt: "Select root:".to_string(),
            labels,
            cursor,
            min_width: 30,
            kind: ListSelectKind::ChangeRoot { roots },
        };
    }

    pub fn open_copy_dialog(&mut self) {
        if let Some(files) = self.require_selection() {
            let dest = self.inactive_panel().path.to_string_lossy().into_owned();
            self.input_cursor = dest.chars().count();
            self.dialog = Dialog::Copy { files, dest };
        }
    }

    pub fn open_move_dialog(&mut self) {
        if let Some(files) = self.require_selection() {
            let dest = self.inactive_panel().path.to_string_lossy().into_owned();
            self.input_cursor = dest.chars().count();
            self.dialog = Dialog::Move { files, dest };
        }
    }

    pub fn open_rename_dialog(&mut self) {
        let panel = self.active_panel();
        if panel.entries.is_empty() {
            return;
        }
        let entry = &panel.entries[panel.cursor];
        if entry.name == ".." {
            self.dialog = Dialog::Error {
                message: "Cannot rename '..'".to_string(),
            };
            return;
        }
        let name = entry.name.clone();
        self.input_cursor = name.chars().count();
        self.dialog = Dialog::Rename { name };
    }

    pub fn open_delete_dialog(&mut self) {
        if let Some(files) = self.require_selection() {
            self.dialog = Dialog::Delete { files };
        }
    }

    fn require_selection(&mut self) -> Option<Vec<String>> {
        let files = self.active_panel().selected_or_current_names();
        if files.is_empty() {
            self.dialog = Dialog::Error {
                message: "No files selected".to_string(),
            };
            return None;
        }
        Some(files)
    }

    /// Navigate a ChooseRoot dialog to the selected root. Used by both
    /// keyboard Enter and mouse double-click to avoid duplication.
    fn choose_root(&mut self, path: PathBuf) {
        self.dialog = Dialog::None;
        self.record_dir_change(&path);
        let panel = self.active_panel_mut();
        panel.path = path;
        panel.cursor = 0;
        panel.scroll_offset = 0;
        panel.refresh();
    }

    // ── Layout feedback from draw pass ──────────────────────────────────

    pub fn apply_draw_result(&mut self, term_size: Size, menu_active: bool) {
        self.term_size = term_size;
        self.menu_active = menu_active;
        self.want_menu_focus = false;

        if self.term_size.width >= 10 && self.term_size.height >= 5 {
            let panel_height = self.term_size.height - 3;
            let visible =
                (panel_height - 1 /* title */ - 1 /* header */ - 1 /* footer */ - 2/* border */)
                    .max(1) as usize;
            self.left.adjust_scroll(visible);
            self.right.adjust_scroll(visible);

            if self.quick_view {
                let preview_visible = (panel_height - 2).max(1) as usize;
                let max_scroll = self.preview.lines.len().saturating_sub(preview_visible);
                self.preview_scroll = self.preview_scroll.min(max_scroll);
            }
        }
    }

    pub fn update_preview(&mut self) {
        if !self.quick_view {
            return;
        }
        let current = self.active_panel().current_path();
        if current == self.preview_path {
            return;
        }
        self.preview_path = current.clone();
        self.preview = match current {
            Some(path) => preview::generate(&path),
            None => Preview::empty(),
        };
    }

    // ── Query helpers ───────────────────────────────────────────────────

    pub fn dialog_is_none(&self) -> bool {
        matches!(self.dialog, Dialog::None)
    }

    pub fn dialog_is_error(&self) -> bool {
        matches!(self.dialog, Dialog::Error { .. })
    }

    // ── Input dispatch ──────────────────────────────────────────────────

    /// Top-level input handler. Returns true when the app should quit.
    pub fn handle_global_input(&mut self, ev: &Input) -> bool {
        // Resize must be handled regardless of dialog/command line state.
        if let Input::Resize(size) = ev {
            self.term_size = *size;
            return false;
        }
        if !matches!(self.dialog, Dialog::None) {
            self.handle_dialog_input(ev);
            return false;
        }
        if self.command_line_active && self.handle_command_line_input(ev) {
            return false;
        }
        if self.menu_active {
            return false;
        }

        match ev {
            Input::Mouse(mouse) => self.handle_mouse(mouse),
            Input::Text(text) => self.handle_text(text),
            Input::Keyboard(key) => self.handle_keyboard(*key),
            _ => {}
        }
        false
    }

    fn handle_text(&mut self, text: &str) {
        self.alt_search.clear();
        match text {
            "+" => self.execute_action(Action::SelectGroup),
            "-" => self.execute_action(Action::DeselectGroup),
            "*" => self.execute_action(Action::InvertSelection),
            _ => {
                self.command_line_active = true;
                self.command_line.clear();
                self.command_line.push_str(text);
                self.cmd_cursor = text.chars().count();
            }
        }
    }

    fn handle_keyboard(&mut self, key: InputKey) {
        // Alt+letter quick search (FAR-style).
        if let Some(ch) = alt_key_to_char(key) {
            self.alt_search.push(ch);
            let prefix = self.alt_search.clone();
            self.active_panel_mut().navigate_to_prefix(&prefix);
            return;
        }

        // Any non-Alt-letter key ends the quick search.
        self.alt_search.clear();

        // Escape is always dismiss / no-op at top level.
        if key == vk::ESCAPE {
            return;
        }

        // Look up the key in the binding table and dispatch the action.
        // The draw pass also calls consume_shortcut() for the same keys,
        // but only to mark the input as consumed by the TUI — it must NOT
        // call execute_action() again (see draw_menubar).
        if let Some(act) = action::lookup(&self.bindings, key) {
            self.execute_action(act);
        }
        // Unknown keys are silently ignored.
    }

    fn handle_mouse(&mut self, mouse: &ruf4_tui::input::InputMouse) {
        if mouse.state == InputMouseState::Left {
            self.handle_mouse_click(mouse.position);
        } else if mouse.state == InputMouseState::Scroll {
            let panel_width = self.term_size.width / 2;
            let panel = if mouse.position.x < panel_width {
                &mut self.left
            } else {
                &mut self.right
            };
            if mouse.scroll.y < 0 {
                panel.cursor_up(MOUSE_SCROLL);
            } else if mouse.scroll.y > 0 {
                panel.cursor_down(MOUSE_SCROLL);
            }
        }
    }

    fn handle_mouse_click(&mut self, pos: Point) {
        let w = self.term_size.width;
        let h = self.term_size.height;
        if w == 0 || h == 0 {
            return;
        }

        let is_double = detect_double_click(&mut self.last_click, pos);
        let y = pos.y;

        // Function key bar (last row).
        if y == h - 1 {
            self.handle_fn_bar_click(pos.x, w);
            return;
        }

        // Panel area.
        if y >= 1 && y < h - 2 {
            let panel_width = w / 2;
            let clicked_left = pos.x < panel_width;

            if clicked_left && self.active != ActivePanel::Left {
                self.active = ActivePanel::Left;
            } else if !clicked_left && self.active != ActivePanel::Right {
                self.active = ActivePanel::Right;
            }

            // Panel title row (path): click opens choose-root dialog.
            if y == PANEL_TITLE_ROW {
                self.execute_action(Action::ChangeRoot);
                return;
            }

            // Footer row: check for sort/hidden clicks.
            let footer_y = h - 4;
            if y == footer_y {
                self.handle_footer_click(pos.x, clicked_left);
                return;
            }

            if y >= PANEL_ENTRY_START && y < footer_y {
                let entry_row = (y - PANEL_ENTRY_START) as usize;
                let panel = self.active_panel_mut();
                let idx = panel.scroll_offset + entry_row;
                if idx < panel.entries.len() {
                    panel.cursor = idx;
                }
                if is_double {
                    self.open_or_enter();
                }
            }
        }
    }

    fn handle_footer_click(&mut self, x: CoordType, clicked_left: bool) {
        // Reconstruct the footer string to find hit regions by substring search.
        let panel = if clicked_left {
            &self.left
        } else {
            &self.right
        };
        let (sel_count, sel_size) = panel.selection_info();
        let sort_label = match panel.sort_by {
            SortBy::Name => "Name",
            SortBy::Extension => "Ext",
            SortBy::Size => "Size",
            SortBy::Modified => "Date",
        };
        let sort_arrow = match panel.sort_dir {
            crate::panel::SortDir::Ascending => "+",
            crate::panel::SortDir::Descending => "-",
        };
        let hidden_label = if panel.show_hidden { "[H]" } else { "[ ]" };
        let free = panel
            .free_space()
            .map(crate::panel::format_size)
            .unwrap_or_else(|| "N/A".to_string());
        let refreshed = &panel.last_refresh;
        let footer = if sel_count > 0 {
            format!(
                " {sel_count} sel, {sel_size} bytes | Sort:{sort_label}{sort_arrow} | {hidden_label} | {free} free | updated {refreshed}"
            )
        } else {
            let total = panel.entries.len();
            format!(
                " {total} items | Sort:{sort_label}{sort_arrow} | {hidden_label} | {free} free | updated {refreshed}"
            )
        };

        // Footer text starts after the panel's left border (1 cell in).
        let panel_width = self.term_size.width / 2;
        let panel_left = if clicked_left { 0 } else { panel_width };
        let text_origin = panel_left + 1;
        let lx = (x - text_origin) as usize;

        if let Some(sort_start) = footer.find("Sort:") {
            let sort_end = sort_start + 5 + sort_label.len() + 1; // "Sort:" + label + arrow
            if lx >= sort_start && lx < sort_end {
                self.execute_action(Action::ChooseSort);
                return;
            }
        }
        if let Some(hidden_start) = footer.find(hidden_label) {
            let hidden_end = hidden_start + hidden_label.len();
            if lx >= hidden_start && lx < hidden_end {
                let panel = if clicked_left {
                    &mut self.left
                } else {
                    &mut self.right
                };
                panel.show_hidden = !panel.show_hidden;
                panel.refresh();
            }
        }
    }

    fn handle_fn_bar_click(&mut self, x: CoordType, width: CoordType) {
        let slot_width = width / 10;
        if slot_width == 0 {
            return;
        }
        let slot = (x / slot_width).min(9);
        // F1..F10 map to slots 0..9; reuse the binding table.
        let fkeys = [
            vk::F1,
            vk::F2,
            vk::F3,
            vk::F4,
            vk::F5,
            vk::F6,
            vk::F7,
            vk::F8,
            vk::F9,
            vk::F10,
        ];
        if let Some(&fk) = fkeys.get(slot as usize)
            && let Some(act) = action::lookup(&self.bindings, fk)
        {
            self.execute_action(act);
        }
    }

    // ── Command line ────────────────────────────────────────────────────

    fn handle_command_line_input(&mut self, ev: &Input) -> bool {
        match ev {
            Input::Text(text) => {
                let cur = self.cmd_cursor;
                let byte_pos = char_to_byte(&self.command_line, cur);
                self.command_line.insert_str(byte_pos, text);
                self.cmd_cursor = cur + text.chars().count();
                true
            }
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::ESCAPE {
                    self.command_line_active = false;
                    self.command_line.clear();
                    self.cmd_cursor = 0;
                } else if key == vk::RETURN {
                    if !self.command_line.is_empty() {
                        fileops::execute_command(self);
                    }
                    self.command_line_active = false;
                    self.cmd_cursor = 0;
                } else if key == vk::BACK && self.cmd_cursor > 0 {
                    let cur = self.cmd_cursor;
                    let byte_pos = char_to_byte(&self.command_line, cur - 1);
                    let next = self.command_line[byte_pos..]
                        .char_indices()
                        .nth(1)
                        .map_or(self.command_line.len(), |(i, _)| byte_pos + i);
                    self.command_line.drain(byte_pos..next);
                    self.cmd_cursor = cur - 1;
                    if self.command_line.is_empty() {
                        self.command_line_active = false;
                    }
                } else if key == vk::DELETE {
                    let cur = self.cmd_cursor;
                    let len = self.command_line.chars().count();
                    if cur < len {
                        let byte_pos = char_to_byte(&self.command_line, cur);
                        let next = self.command_line[byte_pos..]
                            .char_indices()
                            .nth(1)
                            .map_or(self.command_line.len(), |(i, _)| byte_pos + i);
                        self.command_line.drain(byte_pos..next);
                        if self.command_line.is_empty() {
                            self.command_line_active = false;
                        }
                    }
                } else if key == vk::LEFT {
                    self.cmd_cursor = self.cmd_cursor.saturating_sub(1);
                } else if key == vk::RIGHT {
                    let len = self.command_line.chars().count();
                    if self.cmd_cursor < len {
                        self.cmd_cursor += 1;
                    }
                } else if key == vk::HOME {
                    self.cmd_cursor = 0;
                } else if key == vk::END {
                    self.cmd_cursor = self.command_line.chars().count();
                } else {
                    // Tab, Up, Down, function keys -- let them fall through.
                    return false;
                }
                true
            }
            _ => false,
        }
    }

    // ── Dialog input ────────────────────────────────────────────────────

    fn handle_dialog_input(&mut self, ev: &Input) {
        match &self.dialog {
            Dialog::None => {}
            Dialog::Help { .. } => self.handle_help_dialog(ev),
            Dialog::Info { .. } | Dialog::Error { .. } => {
                self.handle_dismiss_dialog(ev);
            }
            Dialog::MkDir { .. }
            | Dialog::Rename { .. }
            | Dialog::Copy { .. }
            | Dialog::Move { .. }
            | Dialog::SelectGroup { .. } => {
                self.handle_text_input_dialog(ev);
            }
            Dialog::Delete { .. } => self.handle_delete_dialog(ev),
            Dialog::ConfirmQuit { save_settings } => {
                let save = *save_settings;
                self.handle_quit_dialog(ev, save);
            }
            Dialog::ConfirmOverwrite { .. } => self.handle_overwrite_dialog(ev),
            Dialog::ShellOutput { .. } => self.handle_scrollable_dialog(ev),
            Dialog::ListSelect { .. } => self.handle_list_select_dialog(ev),
        }
    }

    fn handle_dismiss_dialog(&mut self, ev: &Input) {
        match ev {
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::ESCAPE || key == vk::RETURN || key == vk::SPACE {
                    self.dialog = Dialog::None;
                }
            }
            Input::Mouse(mouse) if mouse.state == InputMouseState::Left => {
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    /// Shared handler for dialogs with a text input field (MkDir, Copy, Move, SelectGroup).
    fn handle_text_input_dialog(&mut self, ev: &Input) {
        match ev {
            Input::Text(text) => {
                let cur = self.input_cursor;
                if let Some(field) = self.dialog_text_field() {
                    let byte_pos = char_to_byte(field, cur);
                    field.insert_str(byte_pos, text);
                }
                self.input_cursor = cur + text.chars().count();
            }
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::ESCAPE {
                    self.dialog = Dialog::None;
                } else if key == vk::RETURN {
                    self.commit_text_dialog();
                } else if key == vk::BACK && self.input_cursor > 0 {
                    let cur = self.input_cursor;
                    if let Some(field) = self.dialog_text_field() {
                        let byte_pos = char_to_byte(field, cur - 1);
                        let next = field[byte_pos..]
                            .char_indices()
                            .nth(1)
                            .map_or(field.len(), |(i, _)| byte_pos + i);
                        field.drain(byte_pos..next);
                    }
                    self.input_cursor = cur - 1;
                } else if key == vk::DELETE {
                    let cur = self.input_cursor;
                    let len = self.dialog_text_field().map_or(0, |f| f.chars().count());
                    if cur < len
                        && let Some(field) = self.dialog_text_field()
                    {
                        let byte_pos = char_to_byte(field, cur);
                        let next = field[byte_pos..]
                            .char_indices()
                            .nth(1)
                            .map_or(field.len(), |(i, _)| byte_pos + i);
                        field.drain(byte_pos..next);
                    }
                } else if key == vk::LEFT {
                    self.input_cursor = self.input_cursor.saturating_sub(1);
                } else if key == vk::RIGHT {
                    let cur = self.input_cursor;
                    let len = self.dialog_text_field().map_or(0, |f| f.chars().count());
                    if cur < len {
                        self.input_cursor = cur + 1;
                    }
                } else if key == vk::HOME {
                    self.input_cursor = 0;
                } else if key == vk::END {
                    let len = self.dialog_text_field().map_or(0, |f| f.chars().count());
                    self.input_cursor = len;
                }
            }
            _ => {}
        }
    }

    fn dialog_text_field(&mut self) -> Option<&mut String> {
        match &mut self.dialog {
            Dialog::MkDir { name } => Some(name),
            Dialog::Rename { name } => Some(name),
            Dialog::Copy { dest, .. } => Some(dest),
            Dialog::Move { dest, .. } => Some(dest),
            Dialog::SelectGroup { pattern, .. } => Some(pattern),
            _ => None,
        }
    }

    /// Commit the text-input dialog based on its variant.
    fn commit_text_dialog(&mut self) {
        match &self.dialog {
            Dialog::MkDir { name } => {
                let name = name.clone();
                fileops::do_mkdir(self, &name);
            }
            Dialog::Copy { dest, .. } => {
                let dest = dest.clone();
                fileops::do_copy(self, &dest);
            }
            Dialog::Move { dest, .. } => {
                let dest = dest.clone();
                fileops::do_move(self, &dest);
            }
            Dialog::Rename { name } => {
                let name = name.clone();
                fileops::do_rename(self, &name);
            }
            Dialog::SelectGroup { pattern, select } => {
                let pat = pattern.clone();
                let select = *select;
                self.active_panel_mut().select_by_pattern(&pat, select);
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn handle_delete_dialog(&mut self, ev: &Input) {
        match ev {
            Input::Text("y" | "Y") => fileops::do_delete(self),
            Input::Text("n" | "N") => self.dialog = Dialog::None,
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::RETURN {
                    fileops::do_delete(self);
                } else if key == vk::ESCAPE {
                    self.dialog = Dialog::None;
                }
            }
            _ => {}
        }
    }

    fn handle_quit_dialog(&mut self, ev: &Input, save: bool) {
        match ev {
            Input::Text("y" | "Y") => {
                if save {
                    let _ = self.save_settings();
                }
                self.quit = true;
            }
            Input::Text("n" | "N") => self.dialog = Dialog::None,
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::RETURN {
                    if save {
                        let _ = self.save_settings();
                    }
                    self.quit = true;
                } else if key == vk::ESCAPE {
                    self.dialog = Dialog::None;
                }
            }
            _ => {}
        }
    }

    fn handle_overwrite_dialog(&mut self, ev: &Input) {
        enum OverwriteAction {
            Yes,
            No,
            All,
            Cancel,
            None,
        }
        let action = match ev {
            Input::Text("y" | "Y") => OverwriteAction::Yes,
            Input::Text("n" | "N") => OverwriteAction::No,
            Input::Text("a" | "A") => OverwriteAction::All,
            Input::Keyboard(key) if *key == vk::RETURN => OverwriteAction::Yes,
            Input::Keyboard(key) if *key == vk::ESCAPE => OverwriteAction::Cancel,
            _ => OverwriteAction::None,
        };

        let Dialog::ConfirmOverwrite {
            pending,
            errors,
            is_copy,
            ..
        } = &mut self.dialog
        else {
            return;
        };

        match action {
            OverwriteAction::Yes => {
                let is_copy = *is_copy;
                let mut pending = std::mem::take(pending);
                let mut errors = std::mem::take(errors);
                if let Some((src, target)) = pending.first() {
                    fileops::execute_file_op(src, target, is_copy, &mut errors);
                }
                pending.remove(0);
                fileops::continue_copy_move(self, pending, errors, is_copy);
            }
            OverwriteAction::No => {
                let is_copy = *is_copy;
                let mut pending = std::mem::take(pending);
                let errors = std::mem::take(errors);
                pending.remove(0);
                fileops::continue_copy_move(self, pending, errors, is_copy);
            }
            OverwriteAction::All => {
                let is_copy = *is_copy;
                let pending = std::mem::take(pending);
                let mut errors = std::mem::take(errors);
                for (src, target) in &pending {
                    fileops::execute_file_op(src, target, is_copy, &mut errors);
                }
                fileops::finish_operation(self, errors, false);
            }
            OverwriteAction::Cancel => {
                let errors = std::mem::take(errors);
                fileops::finish_operation(self, errors, false);
            }
            OverwriteAction::None => {}
        }
    }

    fn handle_scrollable_dialog(&mut self, ev: &Input) {
        let Dialog::ShellOutput { scroll, .. } = &mut self.dialog else {
            return;
        };
        match ev {
            Input::Keyboard(key) => {
                let key = *key;
                if key == vk::ESCAPE || key == vk::RETURN || key == vk::SPACE {
                    *scroll = usize::MAX; // signal dismiss (finalize_dialog cleans up)
                } else if key == vk::UP {
                    *scroll = scroll.saturating_sub(1);
                } else if key == vk::DOWN {
                    *scroll += 1;
                } else if key == vk::PRIOR {
                    *scroll = scroll.saturating_sub(PAGE_SCROLL);
                } else if key == vk::NEXT {
                    *scroll += PAGE_SCROLL;
                } else if key == vk::HOME {
                    *scroll = 0;
                }
            }
            Input::Mouse(mouse) if mouse.state == InputMouseState::Left => {
                *scroll = usize::MAX;
            }
            _ => {}
        }
    }

    /// Unified list-selection dialog handler.
    fn handle_list_select_dialog(&mut self, ev: &Input) {
        // Dismiss / commit (needs full &mut self).
        if let Input::Keyboard(key) = ev {
            let key = *key;
            if key == vk::ESCAPE {
                self.dialog = Dialog::None;
                return;
            }
            if key == vk::RETURN {
                self.commit_list_select();
                return;
            }
        }

        // Mouse: navigate or double-click to commit.
        if let Input::Mouse(mouse) = ev
            && mouse.state == InputMouseState::Left
        {
            let Dialog::ListSelect { labels, cursor, .. } = &mut self.dialog else {
                return;
            };
            let is_double = detect_double_click(&mut self.last_click, mouse.position);
            if let Some(idx) =
                list_dialog_hit_index(mouse.position.y, labels.len(), self.term_size.height)
            {
                *cursor = idx;
                if is_double {
                    self.commit_list_select();
                }
            } else {
                self.dialog = Dialog::None;
            }
            return;
        }

        // Navigation keys within the list.
        let Dialog::ListSelect { labels, cursor, .. } = &mut self.dialog else {
            return;
        };
        if let Input::Keyboard(key) = ev {
            list_nav_key(*key, cursor, labels.len());
        }
    }

    /// Commit the list-selection dialog: extract the selected item and dispatch
    /// based on the dialog's `kind`.
    fn commit_list_select(&mut self) {
        let Dialog::ListSelect { cursor, kind, .. } =
            std::mem::replace(&mut self.dialog, Dialog::None)
        else {
            return;
        };

        match kind {
            ListSelectKind::ChangeRoot { roots } => {
                if let Some(path) = roots.into_iter().nth(cursor) {
                    self.choose_root(path);
                }
            }
            ListSelectKind::DirHistory { entries } => {
                if let Some(path) = entries.into_iter().nth(cursor) {
                    let panel = self.active_panel_mut();
                    panel.path = path;
                    panel.cursor = 0;
                    panel.scroll_offset = 0;
                    panel.refresh();
                }
            }
            ListSelectKind::CmdHistory { entries } => {
                if let Some(cmd) = entries.into_iter().nth(cursor) {
                    self.cmd_cursor = cmd.chars().count();
                    self.command_line = cmd;
                    self.command_line_active = true;
                }
            }
            ListSelectKind::ChooseSort => {
                if let Some((_, sort_by)) = SORT_OPTIONS.get(cursor) {
                    self.active_panel_mut().set_sort(*sort_by);
                }
            }
        }
    }

    fn handle_help_dialog(&mut self, ev: &Input) {
        // Dismiss/action first (needs full &mut self).
        if let Input::Keyboard(key) = ev {
            let key = *key;
            if key == vk::ESCAPE || key == vk::RETURN {
                self.dialog = Dialog::None;
                return;
            }
        }
        if let Input::Mouse(mouse) = ev
            && mouse.state == InputMouseState::Left
        {
            let Dialog::Help { scroll } = &self.dialog else {
                return;
            };
            let help_len = self.help_text.len();
            let idx = help_dialog_hit_index(mouse.position.y, *scroll, help_len, self.term_size);
            self.dialog = Dialog::None;
            if let Some(i) = idx {
                self.invoke_help_action(i);
            }
            return;
        }

        // Scroll navigation.
        let help_len = self.help_text.len();
        let Dialog::Help { scroll } = &mut self.dialog else {
            return;
        };
        let max_scroll = help_len.saturating_sub((self.term_size.height - 8).max(4) as usize);
        if let Input::Keyboard(key) = ev {
            let key = *key;
            if key == vk::UP {
                *scroll = scroll.saturating_sub(1);
            } else if key == vk::DOWN {
                *scroll = (*scroll + 1).min(max_scroll);
            } else if key == vk::PRIOR {
                *scroll = scroll.saturating_sub(PAGE_SCROLL);
            } else if key == vk::NEXT {
                *scroll = (*scroll + PAGE_SCROLL).min(max_scroll);
            } else if key == vk::HOME {
                *scroll = 0;
            } else if key == vk::END {
                *scroll = max_scroll;
            }
        } else if let Input::Mouse(mouse) = ev
            && mouse.state == InputMouseState::Scroll
        {
            if mouse.scroll.y < 0 {
                *scroll = scroll.saturating_sub(MOUSE_SCROLL);
            } else if mouse.scroll.y > 0 {
                *scroll = (*scroll + MOUSE_SCROLL).min(max_scroll);
            }
        }
    }

    fn invoke_help_action(&mut self, idx: usize) {
        if let Some((key_str, _, action)) = self.help_text.get(idx) {
            if key_str.is_empty() {
                return; // separator row
            }
            self.execute_action(*action);
        }
    }
}

// After handle_dialog_input, clean up sentinel values.
// ShellOutput uses usize::MAX as a "dismiss" signal because the handler
// receives &mut scroll but cannot call self.dialog = Dialog::None.
impl State {
    /// Call after handle_dialog_input to finalize deferred state changes.
    pub fn finalize_dialog(&mut self) {
        if let Dialog::ShellOutput { scroll, .. } = &self.dialog
            && *scroll == usize::MAX
        {
            self.dialog = Dialog::None;
        }
    }
}

// ── Shared helpers ─────────────────────────────────────────────────────────

/// Convert a char index to a byte index in a string.
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(i, _)| i)
}

fn push_recent<T: PartialEq>(list: &mut Vec<T>, item: T) {
    list.retain(|x| x != &item);
    list.insert(0, item);
    list.truncate(MAX_HISTORY);
}

fn detect_double_click(last_click: &mut Option<(Instant, Point)>, pos: Point) -> bool {
    let now = Instant::now();
    let is_double = last_click
        .is_some_and(|(t, p)| now.duration_since(t).as_millis() < DOUBLE_CLICK_MS && p == pos);
    *last_click = Some((now, pos));
    is_double
}

fn list_nav_key(key: InputKey, cursor: &mut usize, len: usize) {
    if key == vk::UP && *cursor > 0 {
        *cursor -= 1;
    } else if key == vk::DOWN && *cursor + 1 < len {
        *cursor += 1;
    } else if key == vk::HOME {
        *cursor = 0;
    } else if key == vk::END {
        *cursor = len.saturating_sub(1);
    }
}

pub fn list_dialog_hit_index(
    mouse_y: CoordType,
    entry_count: usize,
    term_height: CoordType,
) -> Option<usize> {
    let h = entry_count as CoordType + LIST_DIALOG_CONTENT_PAD + LIST_DIALOG_BORDER;
    let dialog_top = (term_height - h) / 2;
    let entry_start = dialog_top + LIST_DIALOG_ENTRY_OFFSET;
    if mouse_y >= entry_start && ((mouse_y - entry_start) as usize) < entry_count {
        Some((mouse_y - entry_start) as usize)
    } else {
        None
    }
}

/// Help dialog has no prompt line. Layout:
/// - inner height = content_h + 4 (top_spacer + content + bot_spacer + slack)
/// - outer height = inner + 2 (border)
/// - entries start at outer_top + 2 (border + top_spacer)
fn help_dialog_hit_index(
    mouse_y: CoordType,
    scroll: usize,
    help_len: usize,
    term_size: Size,
) -> Option<usize> {
    let total = help_len as CoordType;
    let max_visible = (term_size.height - 8).max(4);
    let content_h = total.min(max_visible);
    let inner_h = content_h + 4; // matches draw_help_dialog
    let outer_h = inner_h + 2; // border top + bottom
    let outer_top = (term_size.height - outer_h) / 2;
    let entry_start = outer_top + 2; // border + top_spacer (no prompt)
    if mouse_y >= entry_start && mouse_y < entry_start + content_h {
        let row = (mouse_y - entry_start) as usize;
        let idx = scroll + row;
        if idx < help_len {
            return Some(idx);
        }
    }
    None
}

// ── Alt+letter quick search ─────────────────────────────────────────────────

/// Extract the lowercase letter from an Alt+letter key, or None.
fn alt_key_to_char(key: InputKey) -> Option<char> {
    const LETTERS: [fn() -> InputKey; 26] = [
        || vk::A,
        || vk::B,
        || vk::C,
        || vk::D,
        || vk::E,
        || vk::F,
        || vk::G,
        || vk::H,
        || vk::I,
        || vk::J,
        || vk::K,
        || vk::L,
        || vk::M,
        || vk::N,
        || vk::O,
        || vk::P,
        || vk::Q,
        || vk::R,
        || vk::S,
        || vk::T,
        || vk::U,
        || vk::V,
        || vk::W,
        || vk::X,
        || vk::Y,
        || vk::Z,
    ];
    for (i, vk_fn) in LETTERS.iter().enumerate() {
        let vk_letter = vk_fn();
        if key == (kbmod::ALT | vk_letter) || key == (kbmod::ALT_SHIFT | vk_letter) {
            return Some((b'a' + i as u8) as char);
        }
    }
    None
}
