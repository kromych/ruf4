// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Actions, keybindings, and default binding table.
//!
//! Every user-visible operation is represented as an [`Action`]. Keys are mapped
//! to actions via a [`Binding`] table, which can be customized through settings.

use ruf4_tui::input::{InputKey, InputKeyMod, kbmod, vk};

use crate::panel::SortBy;

// ── Actions ────────────────────────────────────────────────────────────────

/// Every user-visible operation the app can perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Navigation
    CursorUp,
    CursorDown,
    PageUp,
    PageDown,
    CursorHome,
    CursorEnd,
    OpenOrEnter,
    ParentDir,
    SwitchPanel,

    // Selection
    ToggleSelect,
    SelectGroup,
    DeselectGroup,
    InvertSelection,
    SelectAll,
    DeselectAll,

    // File operations
    Copy,
    Move,
    Rename,
    Delete,
    MkDir,

    // View / sort
    ToggleQuickView,
    ToggleHidden,
    SortBy(SortBy),
    ChooseSort,

    // App
    Help,
    SaveSettings,
    Refresh,
    ChangeRoot,
    DirHistory,
    CmdHistory,
    FocusMenu,
    Quit,
}

impl Action {
    /// "Immediate" actions are dispatched from the keyboard handler in
    /// `state.rs`.  Everything else is dispatched via `consume_shortcut`
    /// in the draw pass, which ensures the menubar sees the key and
    /// avoids double-dispatch.
    pub const fn is_immediate(self) -> bool {
        matches!(
            self,
            Action::CursorUp
                | Action::CursorDown
                | Action::PageUp
                | Action::PageDown
                | Action::CursorHome
                | Action::CursorEnd
                | Action::OpenOrEnter
                | Action::ParentDir
                | Action::SwitchPanel
                | Action::ToggleSelect
        )
    }
}

// ── Bindings ───────────────────────────────────────────────────────────────

/// A single key -> action mapping.
#[derive(Clone, Copy)]
pub struct Binding {
    pub key: InputKey,
    pub action: Action,
}

/// The first matching key wins in `lookup`. Modifier
/// combinations are encoded into the key via `kbmod::*`.
pub fn default_bindings() -> Vec<Binding> {
    let mut bindings = vec![
        // Function keys (on macOS these need Fn; Ctrl alternatives below).
        Binding {
            key: vk::F1,
            action: Action::Help,
        },
        Binding {
            key: vk::F2,
            action: Action::SaveSettings,
        },
        Binding {
            key: vk::F3,
            action: Action::ToggleQuickView,
        },
        Binding {
            key: vk::F4,
            action: Action::Rename,
        },
        Binding {
            key: vk::F5,
            action: Action::Copy,
        },
        Binding {
            key: vk::F6,
            action: Action::Move,
        },
        Binding {
            key: vk::F7,
            action: Action::MkDir,
        },
        Binding {
            key: vk::F8,
            action: Action::Delete,
        },
        Binding {
            key: vk::F9,
            action: Action::FocusMenu,
        },
        Binding {
            key: vk::F10,
            action: Action::Quit,
        },
        // Ctrl combinations (cross-platform)
        Binding {
            key: kbmod::CTRL | vk::Q,
            action: Action::ToggleQuickView,
        },
        Binding {
            key: kbmod::CTRL | vk::G,
            action: Action::ChangeRoot,
        },
        Binding {
            key: kbmod::CTRL | vk::D,
            action: Action::DirHistory,
        },
        Binding {
            key: kbmod::CTRL | vk::E,
            action: Action::CmdHistory,
        },
        Binding {
            key: kbmod::CTRL | vk::R,
            action: Action::Refresh,
        },
        Binding {
            key: kbmod::CTRL | vk::H,
            action: Action::ToggleHidden,
        },
        Binding {
            key: kbmod::CTRL | vk::A,
            action: Action::SelectAll,
        },
        Binding {
            key: kbmod::CTRL | vk::F3,
            action: Action::SortBy(SortBy::Name),
        },
        Binding {
            key: kbmod::CTRL | vk::F4,
            action: Action::SortBy(SortBy::Extension),
        },
        Binding {
            key: kbmod::CTRL | vk::F5,
            action: Action::SortBy(SortBy::Modified),
        },
        Binding {
            key: kbmod::CTRL | vk::F6,
            action: Action::SortBy(SortBy::Size),
        },
        // Navigation
        Binding {
            key: vk::UP,
            action: Action::CursorUp,
        },
        Binding {
            key: vk::DOWN,
            action: Action::CursorDown,
        },
        Binding {
            key: vk::PRIOR,
            action: Action::PageUp,
        },
        Binding {
            key: vk::NEXT,
            action: Action::PageDown,
        },
        Binding {
            key: vk::HOME,
            action: Action::CursorHome,
        },
        Binding {
            key: vk::END,
            action: Action::CursorEnd,
        },
        Binding {
            key: vk::RETURN,
            action: Action::OpenOrEnter,
        },
        Binding {
            key: vk::BACK,
            action: Action::ParentDir,
        },
        Binding {
            key: vk::TAB,
            action: Action::SwitchPanel,
        },
        // Selection
        Binding {
            key: vk::INSERT,
            action: Action::ToggleSelect,
        },
        Binding {
            key: kbmod::SHIFT | vk::SPACE,
            action: Action::ToggleSelect,
        },
        Binding {
            key: vk::DELETE,
            action: Action::Delete,
        },
    ];

    // macOS: all F-keys are media/system keys by default (brightness,
    // Mission Control, Launchpad, media, volume). Add Ctrl+letter
    // alternatives so the core operations work without Fn.
    #[cfg(target_os = "macos")]
    bindings.extend([
        Binding {
            key: kbmod::CTRL | vk::S,
            action: Action::SaveSettings,
        },
        Binding {
            key: kbmod::CTRL | vk::P,
            action: Action::Rename,
        },
        Binding {
            key: kbmod::CTRL | vk::O,
            action: Action::Copy,
        },
        Binding {
            key: kbmod::CTRL | vk::K,
            action: Action::Move,
        },
        Binding {
            key: kbmod::CTRL | vk::N,
            action: Action::MkDir,
        },
        Binding {
            key: kbmod::CTRL | vk::X,
            action: Action::Delete,
        },
        Binding {
            key: kbmod::CTRL | vk::W,
            action: Action::Quit,
        },
    ]);

    bindings
}

/// Look up the first action matching `key` in the binding table.
pub fn lookup(bindings: &[Binding], key: InputKey) -> Option<Action> {
    bindings.iter().find(|b| b.key == key).map(|b| b.action)
}

/// Return the first key bound to `action`, or `vk::NULL` if none.
pub fn key_for(bindings: &[Binding], action: Action) -> InputKey {
    bindings
        .iter()
        .find(|b| b.action == action)
        .map(|b| b.key)
        .unwrap_or(vk::NULL)
}

// ── Help text generation ───────────────────────────────────────────────────

/// Format a key for display (e.g. "Ctrl+F3", "F5", "Tab").
pub fn key_display_name(key: InputKey) -> String {
    let base = key.key();
    let mods = key.modifiers();

    let mut prefix = String::new();
    if mods.contains(kbmod::CTRL) {
        prefix.push_str("Ctrl+");
    }
    if mods.contains(kbmod::ALT) {
        prefix.push_str("Alt+");
    }
    if mods.contains(kbmod::SHIFT) {
        prefix.push_str("Shift+");
    }

    let name: &str = match base {
        k if k == vk::F1 => "F1",
        k if k == vk::F2 => "F2",
        k if k == vk::F3 => "F3",
        k if k == vk::F4 => "F4",
        k if k == vk::F5 => "F5",
        k if k == vk::F6 => "F6",
        k if k == vk::F7 => "F7",
        k if k == vk::F8 => "F8",
        k if k == vk::F9 => "F9",
        k if k == vk::F10 => "F10",
        k if k == vk::UP => "Up",
        k if k == vk::DOWN => "Down",
        k if k == vk::LEFT => "Left",
        k if k == vk::RIGHT => "Right",
        k if k == vk::PRIOR => "PgUp",
        k if k == vk::NEXT => "PgDn",
        k if k == vk::HOME => "Home",
        k if k == vk::END => "End",
        k if k == vk::RETURN => "Enter",
        k if k == vk::ESCAPE => "Esc",
        k if k == vk::TAB => "Tab",
        k if k == vk::BACK => "Backspace",
        k if k == vk::INSERT => "Ins",
        k if k == vk::DELETE => "Delete",
        k if k == vk::SPACE => "Space",
        k if k == vk::A => "A",
        k if k == vk::B => "B",
        k if k == vk::C => "C",
        k if k == vk::D => "D",
        k if k == vk::E => "E",
        k if k == vk::F => "F",
        k if k == vk::G => "G",
        k if k == vk::H => "H",
        k if k == vk::I => "I",
        k if k == vk::J => "J",
        k if k == vk::K => "K",
        k if k == vk::L => "L",
        k if k == vk::M => "M",
        k if k == vk::N => "N",
        k if k == vk::O => "O",
        k if k == vk::P => "P",
        k if k == vk::Q => "Q",
        k if k == vk::R => "R",
        k if k == vk::S => "S",
        k if k == vk::T => "T",
        k if k == vk::U => "U",
        k if k == vk::V => "V",
        k if k == vk::W => "W",
        k if k == vk::X => "X",
        k if k == vk::Y => "Y",
        k if k == vk::Z => "Z",
        _ => return format!("{prefix}?"),
    };
    format!("{prefix}{name}")
}

/// Action display name for help text.
pub fn action_label(action: Action) -> &'static str {
    match action {
        Action::Help => "Help",
        Action::SaveSettings => "Save settings",
        Action::ToggleQuickView => "Toggle quick view",
        Action::Rename => "Rename",
        Action::Copy => "Copy",
        Action::Move => "Rename / Move",
        Action::MkDir => "Make directory",
        Action::Delete => "Delete",
        Action::FocusMenu => "Focus menubar",
        Action::Quit => "Quit",
        Action::CursorUp => "Navigate up",
        Action::CursorDown => "Navigate down",
        Action::PageUp => "Page up",
        Action::PageDown => "Page down",
        Action::CursorHome => "First entry",
        Action::CursorEnd => "Last entry",
        Action::OpenOrEnter => "Open / enter directory",
        Action::SwitchPanel => "Switch panel",
        Action::ParentDir => "Parent directory",
        Action::ToggleSelect => "Toggle selection",
        Action::SelectGroup => "Select group",
        Action::DeselectGroup => "Deselect group",
        Action::InvertSelection => "Invert selection",
        Action::SelectAll => "Select all",
        Action::DeselectAll => "Deselect all",
        Action::ToggleHidden => "Toggle hidden files",
        Action::SortBy(SortBy::Name) => "Sort by name",
        Action::SortBy(SortBy::Extension) => "Sort by extension",
        Action::SortBy(SortBy::Modified) => "Sort by date",
        Action::SortBy(SortBy::Size) => "Sort by size",
        Action::ChooseSort => "Choose sort mode",
        Action::Refresh => "Refresh panels",
        Action::ChangeRoot => "Change root",
        Action::DirHistory => "Directory history",
        Action::CmdHistory => "Command history",
    }
}

// ── Serialization helpers ─────────────────────────────────────────────────

/// Stable string name for an action, used in settings files.
pub fn action_str(action: Action) -> &'static str {
    match action {
        Action::CursorUp => "cursor_up",
        Action::CursorDown => "cursor_down",
        Action::PageUp => "page_up",
        Action::PageDown => "page_down",
        Action::CursorHome => "cursor_home",
        Action::CursorEnd => "cursor_end",
        Action::OpenOrEnter => "open_or_enter",
        Action::ParentDir => "parent_dir",
        Action::SwitchPanel => "switch_panel",
        Action::ToggleSelect => "toggle_select",
        Action::SelectGroup => "select_group",
        Action::DeselectGroup => "deselect_group",
        Action::InvertSelection => "invert_selection",
        Action::SelectAll => "select_all",
        Action::DeselectAll => "deselect_all",
        Action::Copy => "copy",
        Action::Move => "move",
        Action::Rename => "rename",
        Action::Delete => "delete",
        Action::MkDir => "mkdir",
        Action::ToggleQuickView => "toggle_quick_view",
        Action::ToggleHidden => "toggle_hidden",
        Action::SortBy(SortBy::Name) => "sort_by_name",
        Action::SortBy(SortBy::Extension) => "sort_by_extension",
        Action::SortBy(SortBy::Modified) => "sort_by_modified",
        Action::SortBy(SortBy::Size) => "sort_by_size",
        Action::ChooseSort => "choose_sort",
        Action::Help => "help",
        Action::SaveSettings => "save_settings",
        Action::Refresh => "refresh",
        Action::ChangeRoot => "change_root",
        Action::DirHistory => "dir_history",
        Action::CmdHistory => "cmd_history",
        Action::FocusMenu => "focus_menu",
        Action::Quit => "quit",
    }
}

/// Parse an action name from a settings file. Returns `None` for unknown names.
pub fn parse_action(s: &str) -> Option<Action> {
    Some(match s {
        "cursor_up" => Action::CursorUp,
        "cursor_down" => Action::CursorDown,
        "page_up" => Action::PageUp,
        "page_down" => Action::PageDown,
        "cursor_home" => Action::CursorHome,
        "cursor_end" => Action::CursorEnd,
        "open_or_enter" => Action::OpenOrEnter,
        "parent_dir" => Action::ParentDir,
        "switch_panel" => Action::SwitchPanel,
        "toggle_select" => Action::ToggleSelect,
        "select_group" => Action::SelectGroup,
        "deselect_group" => Action::DeselectGroup,
        "invert_selection" => Action::InvertSelection,
        "select_all" => Action::SelectAll,
        "deselect_all" => Action::DeselectAll,
        "copy" => Action::Copy,
        "move" => Action::Move,
        "rename" => Action::Rename,
        "delete" => Action::Delete,
        "mkdir" => Action::MkDir,
        "toggle_quick_view" => Action::ToggleQuickView,
        "toggle_hidden" => Action::ToggleHidden,
        "sort_by_name" => Action::SortBy(SortBy::Name),
        "sort_by_extension" => Action::SortBy(SortBy::Extension),
        "sort_by_modified" => Action::SortBy(SortBy::Modified),
        "sort_by_size" => Action::SortBy(SortBy::Size),
        "choose_sort" => Action::ChooseSort,
        "help" => Action::Help,
        "save_settings" => Action::SaveSettings,
        "refresh" => Action::Refresh,
        "change_root" => Action::ChangeRoot,
        "dir_history" => Action::DirHistory,
        "cmd_history" => Action::CmdHistory,
        "focus_menu" => Action::FocusMenu,
        "quit" => Action::Quit,
        _ => return None,
    })
}

/// Parse a human-readable key name (e.g. "Ctrl+F3", "F5", "Tab") into an InputKey.
/// This is the inverse of `key_display_name`.
pub fn parse_key_name(s: &str) -> Option<InputKey> {
    let s = s.trim();
    let mut remaining = s;
    let mut mods: InputKeyMod = kbmod::NONE;

    // Parse modifier prefixes.
    loop {
        if let Some(rest) = remaining.strip_prefix("Ctrl+") {
            mods |= kbmod::CTRL;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("Alt+") {
            mods |= kbmod::ALT;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("Shift+") {
            mods |= kbmod::SHIFT;
            remaining = rest;
        } else {
            break;
        }
    }

    let base = match remaining {
        "F1" => vk::F1,
        "F2" => vk::F2,
        "F3" => vk::F3,
        "F4" => vk::F4,
        "F5" => vk::F5,
        "F6" => vk::F6,
        "F7" => vk::F7,
        "F8" => vk::F8,
        "F9" => vk::F9,
        "F10" => vk::F10,
        "Up" => vk::UP,
        "Down" => vk::DOWN,
        "Left" => vk::LEFT,
        "Right" => vk::RIGHT,
        "PgUp" => vk::PRIOR,
        "PgDn" => vk::NEXT,
        "Home" => vk::HOME,
        "End" => vk::END,
        "Enter" => vk::RETURN,
        "Esc" => vk::ESCAPE,
        "Tab" => vk::TAB,
        "Backspace" => vk::BACK,
        "Ins" => vk::INSERT,
        "Delete" => vk::DELETE,
        "Space" => vk::SPACE,
        "A" => vk::A,
        "B" => vk::B,
        "C" => vk::C,
        "D" => vk::D,
        "E" => vk::E,
        "F" => vk::F,
        "G" => vk::G,
        "H" => vk::H,
        "I" => vk::I,
        "J" => vk::J,
        "K" => vk::K,
        "L" => vk::L,
        "M" => vk::M,
        "N" => vk::N,
        "O" => vk::O,
        "P" => vk::P,
        "Q" => vk::Q,
        "R" => vk::R,
        "S" => vk::S,
        "T" => vk::T,
        "U" => vk::U,
        "V" => vk::V,
        "W" => vk::W,
        "X" => vk::X,
        "Y" => vk::Y,
        "Z" => vk::Z,
        _ => return None,
    };

    Some(base | mods)
}

/// Build the help text table from the current binding table.
/// Groups bindings for the same action (e.g. "F3 / Ctrl+Q") and produces
/// the layout shown in the help dialog.
pub fn build_help_text(bindings: &[Binding]) -> Vec<(String, &'static str, Action)> {
    // Ordered list of actions shown in help, with group separators (None).
    let sections: &[Option<Action>] = &[
        Some(Action::Help),
        Some(Action::SaveSettings),
        Some(Action::ToggleQuickView),
        Some(Action::Rename),
        Some(Action::Copy),
        Some(Action::Move),
        Some(Action::MkDir),
        Some(Action::Delete),
        Some(Action::FocusMenu),
        Some(Action::Quit),
        None, // separator
        Some(Action::CursorUp),
        Some(Action::CursorDown),
        Some(Action::PageUp),
        Some(Action::PageDown),
        Some(Action::CursorHome),
        Some(Action::CursorEnd),
        Some(Action::OpenOrEnter),
        Some(Action::SwitchPanel),
        Some(Action::ParentDir),
        None,
        Some(Action::ToggleSelect),
        Some(Action::SelectGroup),
        Some(Action::DeselectGroup),
        Some(Action::InvertSelection),
        Some(Action::SelectAll),
        None,
        Some(Action::ChangeRoot),
        Some(Action::DirHistory),
        Some(Action::CmdHistory),
        Some(Action::Refresh),
        Some(Action::ToggleHidden),
        Some(Action::SortBy(SortBy::Name)),
        Some(Action::SortBy(SortBy::Extension)),
        Some(Action::SortBy(SortBy::Modified)),
        Some(Action::SortBy(SortBy::Size)),
        None,
    ];

    let mut result = Vec::new();
    for entry in sections {
        match entry {
            None => result.push((String::new(), "", Action::Help)), // separator
            Some(action) => {
                let mut keys: Vec<String> = bindings
                    .iter()
                    .filter(|b| b.action == *action)
                    .map(|b| key_display_name(b.key))
                    .collect();
                // Text-triggered actions that bypass the binding table.
                match action {
                    Action::SelectGroup => keys.push("+".to_string()),
                    Action::DeselectGroup => keys.push("-".to_string()),
                    Action::InvertSelection => keys.push("*".to_string()),
                    _ => {}
                }
                let key_str = if keys.is_empty() {
                    "(unbound)".to_string()
                } else {
                    keys.join(" / ")
                };
                result.push((key_str, action_label(*action), *action));
            }
        }
    }
    // Append Alt+letters note (not in binding table; handled specially).
    result.push((
        "Alt+letters".to_string(),
        "Quick search by name",
        Action::Help,
    ));
    result
}
