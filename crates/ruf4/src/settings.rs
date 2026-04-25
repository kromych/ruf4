// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Persistent user settings (panel paths, sort mode, view options).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::action::{self, Binding};
use crate::panel::{SortBy, SortDir};
use crate::state::{ActivePanel, MAX_HISTORY};
use crate::theme::{self, THEME_FIELDS, Theme};

const MAX_SETTINGS_SIZE: u64 = 1_048_576; // 1 MiB

pub struct PanelSettings {
    pub path: PathBuf,
    pub sort_by: SortBy,
    pub sort_dir: SortDir,
    pub show_hidden: bool,
}

pub struct Settings {
    pub left: PanelSettings,
    pub right: PanelSettings,
    pub active_panel: ActivePanel,
    pub quick_view: bool,
    pub dir_history: Vec<PathBuf>,
    pub cmd_history: Vec<String>,
    pub bindings: Vec<Binding>,
    pub theme: Theme,
}

fn sort_by_str(s: SortBy) -> &'static str {
    match s {
        SortBy::Name => "name",
        SortBy::Extension => "extension",
        SortBy::Size => "size",
        SortBy::Modified => "modified",
    }
}

fn parse_sort_by(s: &str) -> SortBy {
    match s {
        "extension" => SortBy::Extension,
        "size" => SortBy::Size,
        "modified" => SortBy::Modified,
        _ => SortBy::Name,
    }
}

fn sort_dir_str(d: SortDir) -> &'static str {
    match d {
        SortDir::Ascending => "asc",
        SortDir::Descending => "desc",
    }
}

fn parse_sort_dir(s: &str) -> SortDir {
    match s {
        "desc" => SortDir::Descending,
        _ => SortDir::Ascending,
    }
}

impl Settings {
    pub fn load() -> Option<Self> {
        let path = crate::platform::settings_path()?;
        let meta = fs::metadata(&path).ok()?;
        if meta.len() > MAX_SETTINGS_SIZE {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        let map = parse_ini(&text);

        let left = PanelSettings {
            path: map
                .get("left.path")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/".into()),
            sort_by: map
                .get("left.sort_by")
                .map(|s| parse_sort_by(s))
                .unwrap_or(SortBy::Name),
            sort_dir: map
                .get("left.sort_dir")
                .map(|s| parse_sort_dir(s))
                .unwrap_or(SortDir::Ascending),
            show_hidden: map.get("left.show_hidden").is_some_and(|s| *s == "true"),
        };

        let right = PanelSettings {
            path: map
                .get("right.path")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/".into()),
            sort_by: map
                .get("right.sort_by")
                .map(|s| parse_sort_by(s))
                .unwrap_or(SortBy::Name),
            sort_dir: map
                .get("right.sort_dir")
                .map(|s| parse_sort_dir(s))
                .unwrap_or(SortDir::Ascending),
            show_hidden: map.get("right.show_hidden").is_some_and(|s| *s == "true"),
        };

        let active_panel = match map.get("active_panel") {
            Some(&"right") => ActivePanel::Right,
            _ => ActivePanel::Left,
        };
        let quick_view = map.get("quick_view").is_some_and(|s| *s == "true");

        let mut dir_history = Vec::new();
        for i in 0..MAX_HISTORY {
            let key = format!("dir_history.{i}");
            if let Some(val) = map.get(key.as_str())
                && !val.is_empty()
            {
                dir_history.push(PathBuf::from(val));
            }
        }

        let mut cmd_history = Vec::new();
        for i in 0..MAX_HISTORY {
            let key = format!("cmd_history.{i}");
            if let Some(val) = map.get(key.as_str())
                && !val.is_empty()
            {
                cmd_history.push(val.to_string());
            }
        }

        // Keybindings: start from defaults, apply saved overrides.
        // Comma-separated format (`bind.<action>=Key1,Key2,...`) replaces all default
        // bindings for that action.  A single key (no comma) is treated as a legacy
        // entry and added alongside the defaults to avoid losing existing bindings.
        let mut bindings = action::default_bindings();
        for (key, value) in &map {
            if let Some(action_name) = key.strip_prefix("bind.")
                && let Some(action) = action::parse_action(action_name) {
                    if value.contains(',') {
                        // New format: full replacement.
                        let parsed_keys: Vec<_> = value
                            .split(',')
                            .filter_map(|s| action::parse_key_name(s.trim()))
                            .collect();
                        if !parsed_keys.is_empty() {
                            bindings.retain(|b| b.action != action);
                            for input_key in parsed_keys {
                                bindings.push(Binding {
                                    key: input_key,
                                    action,
                                });
                            }
                        }
                    } else if let Some(input_key) = action::parse_key_name(value) {
                        // Legacy format: add if not already present.
                        if !bindings
                            .iter()
                            .any(|b| b.action == action && b.key == input_key)
                        {
                            bindings.push(Binding {
                                key: input_key,
                                action,
                            });
                        }
                    }
                }
        }

        // Theme: start from defaults, override with saved colors.
        let mut theme_val = Theme::far();
        for (key, value) in &map {
            if let Some(field_name) = key.strip_prefix("theme.")
                && let Some(color) = theme::parse_color(value) {
                    theme_val.set_field(field_name, color);
                }
        }

        Some(Self {
            left,
            right,
            active_panel,
            quick_view,
            dir_history,
            cmd_history,
            bindings,
            theme: theme_val,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let path = crate::platform::settings_path().ok_or("cannot determine config directory")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create config directory: {e}"))?;
        }

        let mut content = format!(
            "\
left.path={}\n\
left.sort_by={}\n\
left.sort_dir={}\n\
left.show_hidden={}\n\
right.path={}\n\
right.sort_by={}\n\
right.sort_dir={}\n\
right.show_hidden={}\n\
active_panel={}\n\
quick_view={}\n",
            self.left.path.display(),
            sort_by_str(self.left.sort_by),
            sort_dir_str(self.left.sort_dir),
            self.left.show_hidden,
            self.right.path.display(),
            sort_by_str(self.right.sort_by),
            sort_dir_str(self.right.sort_dir),
            self.right.show_hidden,
            match self.active_panel {
                ActivePanel::Left => "left",
                ActivePanel::Right => "right",
            },
            self.quick_view,
        );

        for (i, path) in self.dir_history.iter().enumerate() {
            content.push_str(&format!("dir_history.{i}={}\n", path.display()));
        }
        for (i, cmd) in self.cmd_history.iter().enumerate() {
            content.push_str(&format!("cmd_history.{i}={cmd}\n"));
        }

        // Keybindings: for each action whose key set differs from defaults,
        // save all keys (comma-separated).
        let defaults = action::default_bindings();
        let mut saved_actions = Vec::new();
        for binding in &self.bindings {
            if saved_actions.contains(&action::action_str(binding.action)) {
                continue;
            }
            let action_name = action::action_str(binding.action);
            let current_keys: Vec<_> = self
                .bindings
                .iter()
                .filter(|b| b.action == binding.action)
                .map(|b| b.key)
                .collect();
            let default_keys: Vec<_> = defaults
                .iter()
                .filter(|b| b.action == binding.action)
                .map(|b| b.key)
                .collect();
            if current_keys != default_keys {
                let key_strs: Vec<_> = current_keys
                    .iter()
                    .map(|k| action::key_display_name(*k))
                    .collect();
                content.push_str(&format!("bind.{}={}\n", action_name, key_strs.join(",")));
                saved_actions.push(action_name);
            }
        }

        // Theme: save only colors that differ from the default theme.
        let default_theme = Theme::far();
        for &field in THEME_FIELDS {
            if let (Some(current), Some(default)) =
                (self.theme.get_field(field), default_theme.get_field(field))
                && theme::color_str(current) != theme::color_str(default) {
                    content.push_str(&format!("theme.{field}={}\n", theme::color_str(current),));
                }
        }

        fs::write(&path, content).map_err(|e| format!("cannot write settings: {e}"))
    }
}

fn parse_ini(text: &str) -> HashMap<&str, &str> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim(), value.trim());
        }
    }
    map
}

pub fn settings_from_state(state: &crate::state::State) -> Settings {
    Settings {
        left: PanelSettings {
            path: state.left.path.clone(),
            sort_by: state.left.sort_by,
            sort_dir: state.left.sort_dir,
            show_hidden: state.left.show_hidden,
        },
        right: PanelSettings {
            path: state.right.path.clone(),
            sort_by: state.right.sort_by,
            sort_dir: state.right.sort_dir,
            show_hidden: state.right.show_hidden,
        },
        active_panel: state.active,
        quick_view: state.quick_view,
        dir_history: state.dir_history.clone(),
        cmd_history: state.cmd_history.clone(),
        bindings: state.bindings.clone(),
        theme: state.theme.clone(),
    }
}

pub fn apply_to_panel(panel: &mut crate::panel::Panel, ps: &PanelSettings) {
    if ps.path.is_dir() {
        panel.path = ps.path.clone();
    }
    panel.sort_by = ps.sort_by;
    panel.sort_dir = ps.sort_dir;
    panel.show_hidden = ps.show_hidden;
    panel.refresh();
}
