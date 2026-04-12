// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! File panel - shows a directory listing with file details.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::platform;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Extension,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Ascending,
    Descending,
}

pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_hardlink: bool,
    pub is_executable: bool,
    pub is_readonly: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub selected: bool,
}

impl FileEntry {
    pub fn extension(&self) -> &str {
        if self.is_dir {
            return "";
        }
        Path::new(&self.name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    }

    pub fn is_link(&self) -> bool {
        self.is_symlink || self.is_hardlink
    }

    pub fn display_size(&self) -> String {
        if self.is_dir {
            if self.is_link() { "<LNK>" } else { "<DIR>" }.to_string()
        } else if self.is_link() {
            "<LNK>".to_string()
        } else if self.size >= 1_000_000_000 {
            format!("{}G", self.size / 1_000_000_000)
        } else if self.size >= 1_000_000 {
            format!("{}M", self.size / 1_000_000)
        } else if self.size >= 1_000 {
            format!("{}K", self.size / 1_000)
        } else {
            format!("{}", self.size)
        }
    }

    pub fn display_date(&self) -> String {
        match self.modified {
            Some(time) => {
                let lt = platform::epoch_to_local(time);
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    lt.year, lt.month, lt.day, lt.hour, lt.min
                )
            }
            None => String::new(),
        }
    }
}

pub struct Panel {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub sort_by: SortBy,
    pub sort_dir: SortDir,
    pub show_hidden: bool,
    pub last_refresh: String,
}

impl Panel {
    pub fn new(path: PathBuf) -> Self {
        let mut panel = Self {
            path,
            entries: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            sort_by: SortBy::Name,
            sort_dir: SortDir::Ascending,
            show_hidden: false,
            last_refresh: String::new(),
        };
        panel.refresh();
        panel
    }

    pub fn with_entries(path: PathBuf, entries: Vec<FileEntry>) -> Self {
        Self {
            path,
            entries,
            cursor: 0,
            scroll_offset: 0,
            sort_by: SortBy::Name,
            sort_dir: SortDir::Ascending,
            show_hidden: false,
            last_refresh: String::new(),
        }
    }

    pub fn refresh(&mut self) {
        self.last_refresh = platform::format_current_time();
        self.entries.clear();

        // ".." entry to go up.
        if self.path.parent().is_some() {
            self.entries.push(FileEntry {
                name: "..".to_string(),
                is_dir: true,
                is_symlink: false,
                is_hardlink: false,
                is_executable: false,
                is_readonly: false,
                size: 0,
                modified: None,
                selected: false,
            });
        }

        if let Ok(iter) = fs::read_dir(&self.path) {
            for entry in iter.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();

                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }

                let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
                // Follow symlinks: fs::metadata resolves the target,
                // while entry.metadata() returns the symlink itself.
                let metadata = if is_symlink {
                    fs::metadata(entry.path()).ok()
                } else {
                    entry.metadata().ok()
                };
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata.as_ref().and_then(|m| m.modified().ok());
                let is_readonly = metadata
                    .as_ref()
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(false);

                let (is_hardlink, is_executable) = platform::detect_hardlink_executable(
                    metadata.as_ref(),
                    is_dir,
                    is_symlink,
                    &name,
                );

                self.entries.push(FileEntry {
                    name,
                    is_dir,
                    is_symlink,
                    is_hardlink,
                    is_executable,
                    is_readonly,
                    size,
                    modified,
                    selected: false,
                });
            }
        }

        self.sort();
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    fn sort(&mut self) {
        let start = if self.entries.first().is_some_and(|e| e.name == "..") {
            1
        } else {
            0
        };

        let sort_by = self.sort_by;
        let ascending = self.sort_dir == SortDir::Ascending;

        self.entries[start..].sort_by(|a, b| {
            // Directories always come first.
            let dir_ord = b.is_dir.cmp(&a.is_dir);
            if dir_ord != Ordering::Equal {
                return dir_ord;
            }

            let ord = match sort_by {
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Extension => {
                    let ea = a.extension().to_lowercase();
                    let eb = b.extension().to_lowercase();
                    ea.cmp(&eb)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
                SortBy::Size => a.size.cmp(&b.size),
                SortBy::Modified => a.modified.cmp(&b.modified),
            };

            if ascending { ord } else { ord.reverse() }
        });
    }

    pub fn cursor_up(&mut self, n: usize) {
        self.cursor = self.cursor.saturating_sub(n);
    }

    pub fn cursor_down(&mut self, n: usize) {
        let max = self.entries.len().saturating_sub(1);
        self.cursor = (self.cursor + n).min(max);
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }

    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.cursor)
            && entry.name != ".."
        {
            entry.selected = !entry.selected;
        }
        self.cursor_down(1);
    }

    pub fn enter(&mut self) {
        if let Some(entry) = self.entries.get(self.cursor)
            && entry.is_dir
        {
            let new_path = if entry.name == ".." {
                self.path.parent().unwrap_or(&self.path).to_path_buf()
            } else {
                self.path.join(&entry.name)
            };

            if let Ok(canonical) = fs::canonicalize(&new_path) {
                let old_name = self
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                self.path = canonical;
                self.cursor = 0;
                self.scroll_offset = 0;
                self.refresh();

                if let Some(old) = old_name
                    && let Some(pos) = self.entries.iter().position(|e| e.name == old)
                {
                    self.cursor = pos;
                }
            }
        }
    }

    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if self.cursor >= self.scroll_offset + visible_height {
            self.scroll_offset = self.cursor - visible_height + 1;
        }
    }

    pub fn selection_info(&self) -> (usize, u64) {
        let mut count = 0;
        let mut total = 0u64;
        for e in &self.entries {
            if e.selected {
                count += 1;
                total += e.size;
            }
        }
        (count, total)
    }

    pub fn current_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.cursor)
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.current_entry().map(|e| self.path.join(&e.name))
    }

    pub fn selected_or_current(&self) -> Vec<PathBuf> {
        let selected: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| e.selected && e.name != "..")
            .map(|e| self.path.join(&e.name))
            .collect();

        if !selected.is_empty() {
            return selected;
        }

        if let Some(entry) = self.current_entry()
            && entry.name != ".."
        {
            return vec![self.path.join(&entry.name)];
        }
        Vec::new()
    }

    pub fn selected_or_current_names(&self) -> Vec<String> {
        let selected: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.selected && e.name != "..")
            .map(|e| e.name.clone())
            .collect();

        if !selected.is_empty() {
            return selected;
        }

        if let Some(entry) = self.current_entry()
            && entry.name != ".."
        {
            return vec![entry.name.clone()];
        }
        Vec::new()
    }

    pub fn clear_selection(&mut self) {
        for e in &mut self.entries {
            e.selected = false;
        }
    }

    pub fn select_all(&mut self) {
        for e in &mut self.entries {
            if e.name != ".." {
                e.selected = true;
            }
        }
    }

    pub fn invert_selection(&mut self) {
        for e in &mut self.entries {
            if e.name != ".." {
                e.selected = !e.selected;
            }
        }
    }

    pub fn select_by_pattern(&mut self, pattern: &str, select: bool) {
        for e in &mut self.entries {
            if e.name != ".." && glob_match(pattern, &e.name) {
                e.selected = select;
            }
        }
    }

    pub fn free_space(&self) -> Option<u64> {
        platform::disk_free(&self.path)
    }

    pub fn navigate_to_prefix(&mut self, prefix: &str) {
        let lower = prefix.to_lowercase();
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.name != ".." && e.name.to_lowercase().starts_with(&lower))
        {
            self.cursor = pos;
        }
    }

    pub fn set_sort(&mut self, sort_by: SortBy) {
        if self.sort_by == sort_by {
            self.sort_dir = match self.sort_dir {
                SortDir::Ascending => SortDir::Descending,
                SortDir::Descending => SortDir::Ascending,
            };
        } else {
            self.sort_by = sort_by;
            self.sort_dir = SortDir::Ascending;
        }
        self.sort();
    }
}

pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    glob_match_impl(&p, &n)
}

fn glob_match_impl(p: &[char], n: &[char]) -> bool {
    let (mut pi, mut ni) = (0, 0);
    let (mut star_p, mut star_n) = (usize::MAX, 0);

    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_p = pi;
            star_n = ni;
            pi += 1;
        } else if star_p != usize::MAX {
            pi = star_p + 1;
            star_n += 1;
            ni = star_n;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn make_entry(name: &str, is_dir: bool, size: u64) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        is_dir,
        is_symlink: false,
        is_hardlink: false,
        is_executable: false,
        is_readonly: false,
        size,
        modified: None,
        selected: false,
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000_000 {
        format!("{:.1}T", bytes as f64 / 1_000_000_000_000.0)
    } else if bytes >= 1_000_000_000 {
        format!("{:.1}G", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1}M", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{}K", bytes / 1_000)
    } else {
        format!("{bytes}")
    }
}
