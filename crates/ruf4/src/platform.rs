// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Platform-specific abstractions.
//!
//! All `#[cfg]`-gated code lives here so the rest of the crate stays
//! platform-agnostic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

// ── Local time ─────────────────────────────────────────────────────────────

/// Local time components.
#[allow(dead_code)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

/// Current local time.
#[cfg(unix)]
pub fn local_time_now() -> LocalTime {
    unsafe {
        let epoch = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&epoch, &mut tm);
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
        }
    }
}

#[cfg(windows)]
pub fn local_time_now() -> LocalTime {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    unsafe {
        let mut lt: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut lt);
        LocalTime {
            year: lt.wYear as i32,
            month: lt.wMonth as u32,
            day: lt.wDay as u32,
            hour: lt.wHour as u32,
            min: lt.wMinute as u32,
            sec: lt.wSecond as u32,
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn local_time_now() -> LocalTime {
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let (y, m, d) = days_to_ymd(days);
    LocalTime {
        year: y as i32,
        month: m as u32,
        day: d as u32,
        hour: ((secs % 86400) / 3600) as u32,
        min: ((secs % 3600) / 60) as u32,
        sec: (secs % 60) as u32,
    }
}

/// Convert a `SystemTime` to local time components.
#[cfg(unix)]
pub fn epoch_to_local(time: SystemTime) -> LocalTime {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
        }
    }
}

#[cfg(windows)]
pub fn epoch_to_local(time: SystemTime) -> LocalTime {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    let dur = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // Windows FILETIME: 100-ns intervals since 1601-01-01.
    let intervals =
        dur.as_secs() * 10_000_000 + dur.subsec_nanos() as u64 / 100 + 116_444_736_000_000_000;
    let ft = FILETIME {
        dwLowDateTime: intervals as u32,
        dwHighDateTime: (intervals >> 32) as u32,
    };
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        FileTimeToSystemTime(&ft, &mut st);
        SystemTimeToTzSpecificLocalTime(std::ptr::null(), &st, &mut local);
        LocalTime {
            year: local.wYear as i32,
            month: local.wMonth as u32,
            day: local.wDay as u32,
            hour: local.wHour as u32,
            min: local.wMinute as u32,
            sec: local.wSecond as u32,
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn epoch_to_local(time: SystemTime) -> LocalTime {
    let dur = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let (y, m, d) = days_to_ymd(days);
    LocalTime {
        year: y as i32,
        month: m as u32,
        day: d as u32,
        hour: ((secs % 86400) / 3600) as u32,
        min: ((secs % 3600) / 60) as u32,
        sec: (secs % 60) as u32,
    }
}

/// Format current local time as `HH:MM:SS`.
pub fn format_current_time() -> String {
    let lt = local_time_now();
    format!("{:02}:{:02}:{:02}", lt.hour, lt.min, lt.sec)
}

// Howard Hinnant's civil_from_days.
#[cfg_attr(any(unix, windows), allow(dead_code))]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

// ── Disk free space ────────────────────────────────────────────────────────

#[cfg(unix)]
pub fn disk_free(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) == 0 {
            #[allow(clippy::unnecessary_cast)] // types differ across platforms
            Some(stat.f_bavail as u64 * stat.f_bsize as u64)
        } else {
            None
        }
    }
}

#[cfg(windows)]
pub fn disk_free(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_bytes: u64 = 0;
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut free_bytes,
        )
    };
    if ok != 0 { Some(free_bytes) } else { None }
}

#[cfg(not(any(unix, windows)))]
pub fn disk_free(_path: &Path) -> Option<u64> {
    None
}

// ── File properties ────────────────────────────────────────────────────────

/// Detect whether a file is a hard link and/or executable.
/// Returns `(is_hardlink, is_executable)`.
#[cfg(unix)]
pub fn detect_hardlink_executable(
    metadata: Option<&std::fs::Metadata>,
    is_dir: bool,
    is_symlink: bool,
    _name: &str,
) -> (bool, bool) {
    use std::os::unix::fs::MetadataExt;
    let nlinks = metadata.map(|m| m.nlink()).unwrap_or(1);
    let mode = metadata.map(|m| m.mode()).unwrap_or(0);
    let hardlink = !is_dir && !is_symlink && nlinks > 1;
    let executable = !is_dir && (mode & 0o111) != 0;
    (hardlink, executable)
}

#[cfg(windows)]
pub fn detect_hardlink_executable(
    _metadata: Option<&std::fs::Metadata>,
    is_dir: bool,
    _is_symlink: bool,
    name: &str,
) -> (bool, bool) {
    let executable = !is_dir && has_executable_extension(name);
    (false, executable)
}

#[cfg(not(any(unix, windows)))]
pub fn detect_hardlink_executable(
    _metadata: Option<&std::fs::Metadata>,
    _is_dir: bool,
    _is_symlink: bool,
    _name: &str,
) -> (bool, bool) {
    (false, false)
}

#[cfg(windows)]
fn has_executable_extension(name: &str) -> bool {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "exe" | "cmd" | "bat" | "com" | "ps1" | "msi"
    )
}

// ── Root / volume discovery ────────────────────────────────────────────────

#[cfg(windows)]
pub fn discover_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let path = PathBuf::from(&drive);
        if path.exists() {
            roots.push(path);
        }
    }
    roots
}

#[cfg(target_os = "macos")]
pub fn discover_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    if let Ok(entries) = fs::read_dir("/Volumes") {
        for e in entries.flatten() {
            roots.push(e.path());
        }
    }
    roots.sort();
    roots
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn discover_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let mount = parts[1];
                if mount.starts_with("/home")
                    || mount.starts_with("/mnt")
                    || mount.starts_with("/media")
                    || mount.starts_with("/run/media")
                {
                    let p = PathBuf::from(mount);
                    if !roots.contains(&p) {
                        roots.push(p);
                    }
                }
            }
        }
    }
    roots.sort();
    roots
}

// ── Privilege elevation ────────────────────────────────────────────────────

pub fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(windows)]
    {
        is_windows_admin()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

pub fn prompt_symbol() -> &'static str {
    if is_elevated() {
        "#"
    } else {
        #[cfg(windows)]
        {
            ">"
        }
        #[cfg(not(windows))]
        {
            "$"
        }
    }
}

#[cfg(windows)]
fn is_windows_admin() -> bool {
    use std::mem;
    use std::ptr;

    unsafe extern "system" {
        fn OpenProcessToken(
            process: *mut std::ffi::c_void,
            access: u32,
            token: *mut *mut std::ffi::c_void,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetTokenInformation(
            token: *mut std::ffi::c_void,
            class: u32,
            info: *mut std::ffi::c_void,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;

    unsafe {
        let mut token: *mut std::ffi::c_void = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation: u32 = 0;
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION,
            &mut elevation as *mut u32 as *mut std::ffi::c_void,
            mem::size_of::<u32>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation != 0
    }
}

// ── Settings path ──────────────────────────────────────────────────────────

pub fn settings_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(xdg).join("ruf4").join("settings.ini"));
        }
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("ruf4")
                    .join("settings.ini"),
            );
        }
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("ruf4").join("settings.ini"));
        }
    }
    None
}

// ── Shell commands ─────────────────────────────────────────────────────────

pub fn run_command(cmd: &str, cwd: &Path) -> Result<(String, i32), String> {
    #[cfg(unix)]
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .output();

    #[cfg(windows)]
    let output = Command::new("cmd.exe")
        .arg("/C")
        .arg(cmd)
        .current_dir(cwd)
        .output();

    #[cfg(not(any(unix, windows)))]
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&stderr);
            }
            if text.is_empty() {
                text = format!("(exit code: {})", out.status);
            }
            let code = out.status.code().unwrap_or(-1);
            Ok((text, code))
        }
        Err(e) => Err(format!("Failed to execute \"{cmd}\": {e}")),
    }
}

/// Open a file with the system-associated application.
pub fn open_file(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();

    #[cfg(target_os = "windows")]
    let result = Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = Command::new("xdg-open").arg(path).spawn();

    result
        .map(|_| ())
        .map_err(|e| format!("Cannot open \"{}\": {e}", path.display()))
}

/// Home directory.
pub fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile);
        }
    }
    PathBuf::from("/")
}

// ── Symlink operations ─────────────────────────────────────────────────────

/// Remove a symlink without following it.
pub fn remove_symlink(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::remove_file(path)
    }
    #[cfg(windows)]
    {
        if path.is_dir() {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::remove_file(path)
    }
}

/// Recreate a symlink at `dst` pointing to the same target as `src`.
/// Copy text to the system clipboard via OSC 52 escape sequence.
/// This works in most modern terminals (iTerm2, kitty, alacritty, Windows Terminal, etc.).
pub fn copy_to_clipboard(text: &str) {
    use ruf4_tui::sys;
    use std::io::Write;

    let mut buf = Vec::new();
    let encoded = base64_encode(text.as_bytes());
    write!(buf, "\x1b]52;c;{encoded}\x1b\\").ok();
    if let Ok(s) = std::str::from_utf8(&buf) {
        sys::write_stdout(s);
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn copy_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    let link_target = fs::read_link(src)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_target, dst)
    }
    #[cfg(windows)]
    {
        if link_target.is_dir() {
            std::os::windows::fs::symlink_dir(&link_target, dst)
        } else {
            std::os::windows::fs::symlink_file(&link_target, dst)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::os::unix::fs::symlink(&link_target, dst)
    }
}
