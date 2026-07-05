// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! SMB share access through the operating system's native client.
//!
//! `smb://[user@]host[:port]/share/path` resolves to a local directory and
//! every subsequent filesystem operation is ordinary local I/O:
//!
//! * Windows: rewritten to a UNC path (`\\host\share\path`); the redirector
//!   handles the protocol, no mount step exists.
//! * macOS: mounted with `mount_smbfs` under `~/.ruf4/mnt/<host>/<share>`,
//!   prompting for credentials with the TUI suspended. A marker file beside
//!   the mountpoint records the URL so the share can be remounted when an
//!   unmounted path resurfaces (directory history, saved panels).
//! * Linux: mounted in user space with `gio mount` (GVFS); the share appears
//!   under the `gvfs` FUSE directory of the runtime dir.
//!
//! TODO: share enumeration for `smb://host` without a share name.
//! TODO: credential prompt for unauthenticated UNC access on Windows.

use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::platform;
use crate::vfs;

pub const SCHEME: &str = "smb://";

/// Name of the marker file recording a mountpoint's URL. It lives inside the
/// unmounted mountpoint directory; a live mount shadows it, which is fine
/// because remounting is only needed when the share is not mounted.
#[cfg(target_os = "macos")]
const URL_MARKER: &str = ".ruf4-url";

/// A parsed `smb://` location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmbPath {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    /// Share name; empty when the URL names only a host.
    pub share: String,
    /// Path below the share root, `/`-separated; empty or starting with `/`.
    pub path: String,
}

impl SmbPath {
    /// Canonical URL form, e.g. `smb://user@host/share/dir`.
    pub fn display(&self) -> String {
        let mut s = String::from(SCHEME);
        if let Some(user) = &self.user {
            s.push_str(user);
            s.push('@');
        }
        if self.host.contains(':') {
            s.push('[');
            s.push_str(&self.host);
            s.push(']');
        } else {
            s.push_str(&self.host);
        }
        if let Some(port) = self.port {
            s.push(':');
            s.push_str(&port.to_string());
        }
        if !self.share.is_empty() {
            s.push('/');
            s.push_str(&self.share);
            s.push_str(&self.path);
        }
        s
    }
}

/// Parse `smb://[user@]host[:port]/share/path`. `None` for other paths.
pub fn parse(path: &Path) -> Option<SmbPath> {
    let s = path.to_str()?;
    let rest = s.strip_prefix(SCHEME)?;
    let (authority, full_path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (user, host, port) = vfs::parse_authority(authority)?;
    let trimmed = full_path.trim_start_matches('/');
    let (share, sub) = match trimmed.find('/') {
        Some(i) => (&trimmed[..i], &trimmed[i..]),
        None => (trimmed, ""),
    };
    let sub = vfs::normalize_posix(sub);
    Some(SmbPath {
        user,
        host,
        port,
        share: share.to_string(),
        path: if sub == "/" { String::new() } else { sub },
    })
}

/// UNC form of an SMB location: `\\host\share\path`.
pub fn unc_path(smb: &SmbPath) -> PathBuf {
    let mut unc = format!(r"\\{}\{}", smb.host, smb.share);
    for comp in smb.path.split('/').filter(|c| !c.is_empty()) {
        unc.push('\\');
        unc.push_str(comp);
    }
    PathBuf::from(unc)
}

/// GVFS FUSE directory entry name for a share, e.g.
/// `smb-share:server=nas,share=media`.
pub fn gvfs_entry_name(smb: &SmbPath) -> String {
    let mut name = format!(
        "smb-share:server={},share={}",
        smb.host.to_lowercase(),
        smb.share.to_lowercase()
    );
    if let Some(port) = smb.port {
        name.push_str(&format!(",port={port}"));
    }
    if let Some(user) = &smb.user {
        name.push_str(&format!(",user={user}"));
    }
    name
}

/// Parse a GVFS `smb-share:` entry name back into server and share.
pub fn parse_gvfs_entry_name(name: &str) -> Option<SmbPath> {
    let rest = name.strip_prefix("smb-share:")?;
    let mut server = None;
    let mut share = None;
    let mut user = None;
    let mut port = None;
    for kv in rest.split(',') {
        let (key, value) = kv.split_once('=')?;
        match key {
            "server" => server = Some(value.to_string()),
            "share" => share = Some(value.to_string()),
            "user" => user = Some(value.to_string()),
            "port" => port = value.parse().ok(),
            _ => {}
        }
    }
    Some(SmbPath {
        user,
        host: server?,
        port,
        share: share?,
        path: String::new(),
    })
}

/// Replace characters unsuitable for a directory name.
#[cfg(target_os = "macos")]
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ── macOS: mount_smbfs under a private mount base ───────────────────────────

#[cfg(target_os = "macos")]
fn mount_base() -> PathBuf {
    platform::home_dir().join(".ruf4").join("mnt")
}

#[cfg(target_os = "macos")]
fn mountpoint_for(smb: &SmbPath) -> PathBuf {
    let host_dir = match smb.port {
        Some(port) => format!("{}_{port}", sanitize(&smb.host)),
        None => sanitize(&smb.host),
    };
    mount_base().join(host_dir).join(sanitize(&smb.share))
}

/// Whether `dir` currently is an smbfs mountpoint.
#[cfg(target_os = "macos")]
fn is_smbfs_mount(dir: &Path) -> bool {
    use std::ffi::{CStr, CString};
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return false;
    };
    let Ok(c_path) = CString::new(canonical.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
            return false;
        }
        let fstype = CStr::from_ptr(stat.f_fstypename.as_ptr());
        let mnt_on = CStr::from_ptr(stat.f_mntonname.as_ptr());
        fstype.to_string_lossy() == "smbfs" && Path::new(&*mnt_on.to_string_lossy()) == canonical
    }
}

/// Mount source for `mount_smbfs`: `//[user@]host[:port]/share`.
#[cfg(target_os = "macos")]
fn mount_source(smb: &SmbPath) -> String {
    let mut s = String::from("//");
    if let Some(user) = &smb.user {
        s.push_str(user);
        s.push('@');
    }
    s.push_str(&smb.host);
    if let Some(port) = smb.port {
        s.push(':');
        s.push_str(&port.to_string());
    }
    s.push('/');
    s.push_str(&smb.share);
    s
}

#[cfg(target_os = "macos")]
fn ensure_mounted(smb: &SmbPath) -> Result<PathBuf, String> {
    let mountpoint = mountpoint_for(smb);
    if is_smbfs_mount(&mountpoint) {
        return Ok(mountpoint);
    }

    std::fs::create_dir_all(&mountpoint)
        .map_err(|e| format!("Cannot create {}: {e}", mountpoint.display()))?;
    // Record the URL beside the mount so an unmounted path can be remounted.
    let _ = std::fs::write(mountpoint.join(URL_MARKER), smb.display());

    // Interactive: mount_smbfs prompts for the password on the terminal.
    let source = mount_source(smb);
    let mut cmd = std::process::Command::new("/sbin/mount_smbfs");
    cmd.arg(&source).arg(&mountpoint);
    let display = format!("mount_smbfs {source}");
    match platform::run_foreground(&mut cmd, &display, false) {
        Ok(st) if st.success() => Ok(mountpoint),
        Ok(_) => Err(format!("Cannot mount {}", smb.display())),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "macos")]
fn mounted_roots_impl() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Ok(hosts) = std::fs::read_dir(mount_base()) else {
        return roots;
    };
    for host in hosts.flatten() {
        let Ok(shares) = std::fs::read_dir(host.path()) else {
            continue;
        };
        for share in shares.flatten() {
            if is_smbfs_mount(&share.path()) {
                roots.push(share.path());
            }
        }
    }
    roots.sort();
    roots
}

/// Reconstruct the URL for a path under the mount base from its marker file.
/// `None` when the path is not managed here or the marker is unreadable.
#[cfg(target_os = "macos")]
fn url_for_local(path: &Path) -> Option<SmbPath> {
    let rel = path.strip_prefix(mount_base()).ok()?;
    let mut comps = rel.components();
    let host_dir = comps.next()?;
    let share_dir = comps.next()?;
    let mountpoint = mount_base().join(host_dir).join(share_dir);
    let url = std::fs::read_to_string(mountpoint.join(URL_MARKER)).ok()?;
    parse(Path::new(url.trim()))
}

// ── Linux: user-space mounts via GVFS ───────────────────────────────────────

#[cfg(target_os = "linux")]
fn gvfs_dir() -> PathBuf {
    if let Ok(d) = std::env::var("XDG_RUNTIME_DIR")
        && !d.is_empty()
    {
        return PathBuf::from(d).join("gvfs");
    }
    PathBuf::from(format!("/run/user/{}/gvfs", unsafe { libc::getuid() }))
}

/// Find the GVFS entry for a share, tolerating extra name fields (user, port).
#[cfg(target_os = "linux")]
fn find_gvfs_mount(smb: &SmbPath) -> Option<PathBuf> {
    let entries = std::fs::read_dir(gvfs_dir()).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(parsed) = parse_gvfs_entry_name(&name)
            && parsed.host.eq_ignore_ascii_case(&smb.host)
            && parsed.share.eq_ignore_ascii_case(&smb.share)
            && (smb.port.is_none() || parsed.port == smb.port)
        {
            return Some(entry.path());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn ensure_mounted(smb: &SmbPath) -> Result<PathBuf, String> {
    if let Some(dir) = find_gvfs_mount(smb) {
        return Ok(dir);
    }

    // Interactive: `gio mount` prompts for credentials on the terminal.
    let uri = smb.display();
    let mut cmd = std::process::Command::new("gio");
    cmd.arg("mount").arg(&uri);
    let display = format!("gio mount {uri}");
    match platform::run_foreground(&mut cmd, &display, false) {
        Ok(st) if st.success() => {}
        Ok(_) => return Err(format!("Cannot mount {uri}")),
        Err(_) => {
            return Err(
                "Mounting SMB shares needs `gio` (GVFS); install gvfs-fuse or mount manually"
                    .to_string(),
            );
        }
    }

    // The FUSE directory appears right after `gio mount` returns; poll briefly.
    for _ in 0..20 {
        if let Some(dir) = find_gvfs_mount(smb) {
            return Ok(dir);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!(
        "{uri} mounted, but no GVFS directory appeared under {}",
        gvfs_dir().display()
    ))
}

#[cfg(target_os = "linux")]
fn mounted_roots_impl() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(entries) = std::fs::read_dir(gvfs_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if parse_gvfs_entry_name(&name).is_some() {
                roots.push(entry.path());
            }
        }
    }
    roots.sort();
    roots
}

#[cfg(target_os = "linux")]
fn url_for_local(path: &Path) -> Option<SmbPath> {
    let rel = path.strip_prefix(gvfs_dir()).ok()?;
    let entry = rel.components().next()?;
    parse_gvfs_entry_name(&entry.as_os_str().to_string_lossy())
}

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Resolve an SMB location to a local directory, mounting the share first
/// when necessary (interactively, with the TUI suspended).
pub fn resolve_dir(smb: &SmbPath) -> Result<PathBuf, String> {
    if smb.share.is_empty() {
        // TODO: enumerate shares for a bare host.
        return Err(format!(
            "{}: share name required (smb://host/share)",
            smb.display()
        ));
    }

    #[cfg(windows)]
    {
        if smb.user.is_some() || smb.port.is_some() {
            // TODO: establish credentials/port with `net use` first.
            return Err("smb:// with user or port is not supported on Windows; \
                        authenticate to the share first"
                .to_string());
        }
        Ok(unc_path(smb))
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mountpoint = ensure_mounted(smb)?;
        let mut dir = mountpoint;
        for comp in smb.path.split('/').filter(|c| !c.is_empty()) {
            dir.push(comp);
        }
        Ok(dir)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err(format!(
            "{}: no SMB support on this platform",
            smb.display()
        ))
    }
}

/// Remount the share backing a currently-unmounted managed path. `None` when
/// the path is not SMB-managed; `Some(Err)` when remounting was attempted and
/// failed.
pub fn try_remount(path: &Path) -> Option<Result<(), String>> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let smb = url_for_local(path)?;
        Some(ensure_mounted(&smb).map(|_| ()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        None
    }
}

/// Local mountpoints of currently mounted SMB shares, for the roots dialog.
pub fn mounted_roots() -> Vec<PathBuf> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        mounted_roots_impl()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Vec::new()
    }
}
