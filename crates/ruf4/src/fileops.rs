// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! File operations: copy, move, delete, mkdir, shell commands.
//!
//! Pure operations (`ops_*`) take paths and return errors. State wrappers
//! (`do_*`) read from `State`, call through, and refresh panels.

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use crate::platform;
use crate::state::{Dialog, State};
use crate::vfs;

/// The final path component as a lossy string, or empty if there is none.
pub fn base_name(path: &Path) -> Cow<'_, str> {
    path.file_name().unwrap_or_default().to_string_lossy()
}

/// The "copy/move onto itself" error message for `name`.
pub fn same_file_error(name: &str, is_copy: bool) -> String {
    if is_copy {
        format!("{name}: cannot copy file to itself")
    } else {
        format!("{name}: source and destination are the same")
    }
}

pub fn ops_mkdir(path: &Path) -> Result<(), String> {
    vfs::create_dir_all(path).map_err(|e| format!("Cannot create \"{}\": {e}", path.display()))
}

pub fn ops_delete(paths: &[PathBuf]) -> Vec<String> {
    let mut errors = Vec::new();
    for path in paths {
        if let Err(e) = vfs::remove_tree(path) {
            errors.push(format!(
                "{}: {e}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    errors
}

pub fn ops_build_pairs(sources: &[PathBuf], dest: &str) -> Vec<(PathBuf, PathBuf)> {
    let dest_path = PathBuf::from(dest);
    let dest_is_dir = vfs::is_dir(&dest_path);
    let mut pairs = Vec::new();

    for src in sources {
        let file_name = src.file_name().unwrap_or_default().to_string_lossy();
        let target = if dest_is_dir || sources.len() > 1 {
            vfs::join(&dest_path, &file_name)
        } else {
            dest_path.clone()
        };
        pairs.push((src.clone(), target));
    }
    pairs
}

pub fn ops_execute_one(src: &Path, target: &Path, is_copy: bool, errors: &mut Vec<String>) {
    let name = src.file_name().unwrap_or_default().to_string_lossy();
    let result = if is_copy {
        let is_symlink = fs::symlink_metadata(src)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_symlink {
            platform::copy_symlink(src, target)
        } else if src.is_dir() {
            copy_dir_recursive(src, target)
        } else {
            fs::copy(src, target).map(|_| ())
        }
    } else {
        // fs::rename fails across filesystems/devices; fall back to copy + remove
        // only for that specific error.
        fs::rename(src, target).or_else(|e| {
            if is_cross_device_error(&e) {
                move_across_filesystems(src, target)
            } else {
                Err(e)
            }
        })
    };
    if let Err(e) = result {
        errors.push(format!("{name}: {e}"));
    }
}

fn is_cross_device_error(e: &std::io::Error) -> bool {
    // Unix: EXDEV (18), Windows: ERROR_NOT_SAME_DEVICE (17)
    #[cfg(unix)]
    const CROSS_DEVICE: i32 = libc::EXDEV;
    #[cfg(windows)]
    const CROSS_DEVICE: i32 = 17; // ERROR_NOT_SAME_DEVICE
    #[cfg(not(any(unix, windows)))]
    const CROSS_DEVICE: i32 = -1;
    e.raw_os_error() == Some(CROSS_DEVICE)
}

/// Move by copying then removing the source. Used when `fs::rename` fails
/// (e.g. across filesystem boundaries).
pub fn move_across_filesystems(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dst)?;
        fs::remove_dir_all(src)
    } else {
        fs::copy(src, dst)?;
        fs::remove_file(src)
    }
}

/// Execute all pairs, skipping same-file copies. No overwrite prompts.
pub fn ops_execute_all(pairs: &[(PathBuf, PathBuf)], is_copy: bool) -> Vec<String> {
    let mut errors = Vec::new();
    for (src, target) in pairs {
        if same_file(src, target) {
            let name = src.file_name().unwrap_or_default().to_string_lossy();
            let msg = if is_copy {
                format!("{name}: cannot copy file to itself")
            } else {
                format!("{name}: source and destination are the same")
            };
            errors.push(msg);
            continue;
        }
        ops_execute_one(src, target, is_copy, &mut errors);
    }
    errors
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    vfs::same_file(a, b)
}

/// Resolve `path` to an absolute form even if it doesn't exist yet:
/// canonicalize the longest existing prefix, then append the rest.
pub(crate) fn normalize_against_existing(path: &Path) -> std::io::Result<PathBuf> {
    if let Ok(p) = fs::canonicalize(path) {
        return Ok(p);
    }
    // Walk up until we find an ancestor that exists.
    let mut tail = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(base) = fs::canonicalize(&cur) {
            let mut result = base;
            for comp in tail.into_iter().rev() {
                result.push(comp);
            }
            return Ok(result);
        }
        match cur.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                cur.pop();
            }
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "cannot resolve path",
                ));
            }
        }
    }
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    // Detect copy-into-self even when dst doesn't exist yet: canonicalize
    // dst's nearest existing ancestor and append the remaining components.
    if let Ok(cs) = fs::canonicalize(src) {
        let cd = normalize_against_existing(dst)?;
        if cd.starts_with(&cs) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot copy directory into itself",
            ));
        }
    }

    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ft.is_symlink() {
            platform::copy_symlink(&entry.path(), &target)?;
        } else if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

// State wrappers

pub fn do_mkdir(state: &mut State, name: &str) {
    if name.is_empty() {
        state.dialog = Dialog::None;
        return;
    }
    let path = vfs::join(&state.active_panel().path, name);
    match ops_mkdir(&path) {
        Ok(()) => {
            state.dialog = Dialog::None;
            state.active_panel_mut().refresh();
        }
        Err(msg) => {
            state.dialog = Dialog::Error { message: msg };
        }
    }
}

pub fn do_delete(state: &mut State) {
    let files = state.active_panel().selected_or_current();
    if files.is_empty() {
        state.dialog = Dialog::None;
        return;
    }
    state.start_job(crate::job::spawn_delete(files));
}

pub fn do_copy(state: &mut State, dest: &str) {
    do_copy_move(state, dest, true);
}

pub fn do_move(state: &mut State, dest: &str) {
    do_copy_move(state, dest, false);
}

fn do_copy_move(state: &mut State, dest: &str, is_copy: bool) {
    // Connect a remote destination now: the worker cannot prompt for
    // authentication.
    if let Err(message) = vfs::ensure_host(Path::new(dest)) {
        state.dialog = Dialog::Error { message };
        return;
    }
    let sources = state.active_panel().selected_or_current();
    let pairs = ops_build_pairs(&sources, dest);
    if pairs.is_empty() {
        state.dialog = Dialog::None;
        return;
    }
    state.start_job(crate::job::spawn_copy_move(pairs, is_copy));
}

pub fn do_rename(state: &mut State, new_name: &str) {
    if new_name.is_empty() {
        state.dialog = Dialog::None;
        return;
    }
    let panel = state.active_panel();
    let old_path = vfs::join(&panel.path, &panel.entries[panel.cursor].name);
    let new_path = vfs::join(&panel.path, new_name);
    match vfs::rename(&old_path, &new_path) {
        Ok(()) => {
            state.dialog = Dialog::None;
            state.active_panel_mut().refresh();
        }
        Err(e) => {
            state.dialog = Dialog::Error {
                message: format!("Rename failed: {e}"),
            };
        }
    }
}

pub fn execute_command(state: &mut State) {
    let cmd = state.command_line.clone();
    state.record_command(&cmd);
    let cwd = state.active_panel().path.clone();

    // Intercept "cd" to change the active panel's directory.
    if let Some(target) = parse_cd_command(&cmd) {
        state.command_line.clear();
        let raw = resolve_cd_target(&cwd, &target);
        state.navigate_panel(raw);
        return;
    }

    state.command_line.clear();

    // External commands run in the foreground with the terminal handed back to
    // them, so interactive programs work. The TUI is suspended and restored for
    // the duration; the screen must be fully repainted afterwards. With a
    // remote working directory the command runs on that host over ssh.
    let result = match vfs::parse_remote(&cwd) {
        Some(remote) => vfs::run_remote_interactive(&remote, &cmd),
        None => platform::run_interactive(&cmd, &cwd),
    };
    if let Err(msg) = result {
        state.dialog = Dialog::Error { message: msg };
    }
    state.request_repaint();
    state.left.refresh();
    state.right.refresh();
}

/// Resolve a `cd` argument against the panel's working directory. An empty
/// target and `~` go to the home directory (the remote login home when the
/// panel is on a remote host); `ssh://` targets switch hosts.
pub fn resolve_cd_target(cwd: &Path, target: &str) -> PathBuf {
    if vfs::is_url(target) {
        return PathBuf::from(target);
    }
    match vfs::parse_remote(cwd) {
        Some(remote) => {
            if target.is_empty() || target == "~" {
                // No path component resolves to the remote home directory.
                PathBuf::from(remote.host.display())
            } else {
                vfs::join(cwd, target)
            }
        }
        None => {
            if target.is_empty() || target == "~" {
                platform::home_dir()
            } else {
                let p = PathBuf::from(target);
                if p.is_absolute() { p } else { cwd.join(p) }
            }
        }
    }
}

fn parse_cd_command(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();
    if trimmed == "cd" {
        Some(String::new())
    } else {
        trimmed
            .strip_prefix("cd ")
            .map(|rest| rest.trim().to_string())
    }
}

pub fn finish_operation(state: &mut State, errors: Vec<String>, active_only: bool) {
    if errors.is_empty() {
        state.dialog = Dialog::None;
    } else {
        state.dialog = Dialog::Error {
            message: errors.join("\n"),
        };
    }
    state.active_panel_mut().clear_selection();
    state.active_panel_mut().refresh();
    if !active_only {
        state.inactive_panel_mut().refresh();
    }
}
