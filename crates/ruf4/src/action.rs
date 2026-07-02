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

/// Every [`Action`] value, used to derive [`parse_action`] from [`action_str`]
/// and to drive round-trip tests. Keep in step with the [`Action`] enum.
pub const ALL_ACTIONS: &[Action] = &[
    Action::CursorUp,
    Action::CursorDown,
    Action::PageUp,
    Action::PageDown,
    Action::CursorHome,
    Action::CursorEnd,
    Action::OpenOrEnter,
    Action::ParentDir,
    Action::SwitchPanel,
    Action::ToggleSelect,
    Action::SelectGroup,
    Action::DeselectGroup,
    Action::InvertSelection,
    Action::SelectAll,
    Action::DeselectAll,
    Action::Copy,
    Action::Move,
    Action::Rename,
    Action::Delete,
    Action::MkDir,
    Action::ToggleQuickView,
    Action::ToggleHidden,
    Action::SortBy(SortBy::Name),
    Action::SortBy(SortBy::Extension),
    Action::SortBy(SortBy::Modified),
    Action::SortBy(SortBy::Size),
    Action::ChooseSort,
    Action::Help,
    Action::SaveSettings,
    Action::Refresh,
    Action::ChangeRoot,
    Action::DirHistory,
    Action::CmdHistory,
    Action::FocusMenu,
    Action::Quit,
];

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
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
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
            key: kbmod::CTRL | vk::SPACE,
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

// ── Key name table ─────────────────────────────────────────────────────────

/// Base keys and their human-readable names. Single source of truth for both
/// [`key_display_name`] and [`parse_key_name`], so the two directions cannot
/// drift apart. (Modifier prefixes like `Ctrl+` are handled separately.)
const KEY_NAMES: &[(InputKey, &str)] = &[
    (vk::F1, "F1"),
    (vk::F2, "F2"),
    (vk::F3, "F3"),
    (vk::F4, "F4"),
    (vk::F5, "F5"),
    (vk::F6, "F6"),
    (vk::F7, "F7"),
    (vk::F8, "F8"),
    (vk::F9, "F9"),
    (vk::F10, "F10"),
    (vk::F11, "F11"),
    (vk::F12, "F12"),
    (vk::UP, "Up"),
    (vk::DOWN, "Down"),
    (vk::LEFT, "Left"),
    (vk::RIGHT, "Right"),
    (vk::PRIOR, "PgUp"),
    (vk::NEXT, "PgDn"),
    (vk::HOME, "Home"),
    (vk::END, "End"),
    (vk::RETURN, "Enter"),
    (vk::ESCAPE, "Esc"),
    (vk::TAB, "Tab"),
    (vk::BACK, "Backspace"),
    (vk::INSERT, "Ins"),
    (vk::DELETE, "Delete"),
    (vk::SPACE, "Space"),
    (vk::A, "A"),
    (vk::B, "B"),
    (vk::C, "C"),
    (vk::D, "D"),
    (vk::E, "E"),
    (vk::F, "F"),
    (vk::G, "G"),
    (vk::H, "H"),
    (vk::I, "I"),
    (vk::J, "J"),
    (vk::K, "K"),
    (vk::L, "L"),
    (vk::M, "M"),
    (vk::N, "N"),
    (vk::O, "O"),
    (vk::P, "P"),
    (vk::Q, "Q"),
    (vk::R, "R"),
    (vk::S, "S"),
    (vk::T, "T"),
    (vk::U, "U"),
    (vk::V, "V"),
    (vk::W, "W"),
    (vk::X, "X"),
    (vk::Y, "Y"),
    (vk::Z, "Z"),
];

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

    match KEY_NAMES.iter().find(|(k, _)| *k == base) {
        Some((_, name)) => format!("{prefix}{name}"),
        None => format!("{prefix}?"),
    }
}

/// The stable settings name and the human-readable label for an action. Single
/// exhaustive match so the compiler flags any new [`Action`] variant that is
/// missing here. [`action_str`] and [`action_label`] read from it.
fn action_meta(action: Action) -> (&'static str, &'static str) {
    match action {
        Action::CursorUp => ("cursor_up", "Navigate up"),
        Action::CursorDown => ("cursor_down", "Navigate down"),
        Action::PageUp => ("page_up", "Page up"),
        Action::PageDown => ("page_down", "Page down"),
        Action::CursorHome => ("cursor_home", "First entry"),
        Action::CursorEnd => ("cursor_end", "Last entry"),
        Action::OpenOrEnter => ("open_or_enter", "Open / enter directory"),
        Action::ParentDir => ("parent_dir", "Parent directory"),
        Action::SwitchPanel => ("switch_panel", "Switch panel"),
        Action::ToggleSelect => ("toggle_select", "Toggle selection"),
        Action::SelectGroup => ("select_group", "Select group"),
        Action::DeselectGroup => ("deselect_group", "Deselect group"),
        Action::InvertSelection => ("invert_selection", "Invert selection"),
        Action::SelectAll => ("select_all", "Select all"),
        Action::DeselectAll => ("deselect_all", "Deselect all"),
        Action::Copy => ("copy", "Copy"),
        Action::Move => ("move", "Rename / Move"),
        Action::Rename => ("rename", "Rename"),
        Action::Delete => ("delete", "Delete"),
        Action::MkDir => ("mkdir", "Make directory"),
        Action::ToggleQuickView => ("toggle_quick_view", "Toggle quick view"),
        Action::ToggleHidden => ("toggle_hidden", "Toggle hidden files"),
        Action::SortBy(SortBy::Name) => ("sort_by_name", "Sort by name"),
        Action::SortBy(SortBy::Extension) => ("sort_by_extension", "Sort by extension"),
        Action::SortBy(SortBy::Modified) => ("sort_by_modified", "Sort by date"),
        Action::SortBy(SortBy::Size) => ("sort_by_size", "Sort by size"),
        Action::ChooseSort => ("choose_sort", "Choose sort mode"),
        Action::Help => ("help", "Help"),
        Action::SaveSettings => ("save_settings", "Save settings"),
        Action::Refresh => ("refresh", "Refresh panels"),
        Action::ChangeRoot => ("change_root", "Change root"),
        Action::DirHistory => ("dir_history", "Directory history"),
        Action::CmdHistory => ("cmd_history", "Command history"),
        Action::FocusMenu => ("focus_menu", "Focus menubar"),
        Action::Quit => ("quit", "Quit"),
    }
}

/// Action display name for help text.
pub fn action_label(action: Action) -> &'static str {
    action_meta(action).1
}

// ── Serialization helpers ─────────────────────────────────────────────────

/// Stable string name for an action, used in settings files.
pub fn action_str(action: Action) -> &'static str {
    action_meta(action).0
}

/// Parse an action name from a settings file. Returns `None` for unknown names.
/// Inverse of [`action_str`], derived from it so the two cannot drift.
pub fn parse_action(s: &str) -> Option<Action> {
    ALL_ACTIONS.iter().copied().find(|&a| action_str(a) == s)
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

    let base = KEY_NAMES
        .iter()
        .find(|(_, name)| *name == remaining)
        .map(|(k, _)| *k)?;

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
