// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Virtual filesystem dispatch.
//!
//! Panel paths are `PathBuf`s throughout the application. Local paths behave
//! as before; paths of the form `ssh://[user@]host[:port]/path` address a
//! remote filesystem reached through the SFTP client in [`crate::sftp`]. The
//! functions here mirror the `std::fs` calls the application needs and route
//! each one to the local filesystem or to a cached per-host SFTP connection.
//!
//! Transport: a spawned `ssh -s <destination> sftp` subsystem, so key
//! handling, agents, and `~/.ssh/config` all behave exactly like the user's
//! `ssh`. On Unix a connection master (`ControlMaster`) is bootstrapped
//! interactively with the TUI suspended, and every channel attaches to its
//! socket without re-authenticating. On Windows multiplexing is unavailable;
//! channels authenticate non-interactively (keys or agent).
//!
//! `smb://` locations resolve to local directories through [`crate::smb`]
//! (the operating system's native SMB client); after navigation they are
//! ordinary local paths.
//! TODO: interactive ssh authentication on Windows.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use crate::panel::{self, FileEntry};
use crate::platform;
use crate::sftp;

pub const SCHEME: &str = "ssh://";

// ── Remote path model ───────────────────────────────────────────────────────

/// A remote SSH destination.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HostSpec {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

impl HostSpec {
    /// URL prefix form, e.g. `ssh://user@host:2222`. IPv6 hosts are bracketed.
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
        s
    }

    /// Destination argument for `ssh`: `user@host` or `host`.
    fn ssh_dest(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }

    /// Options common to every `ssh` invocation for this destination.
    /// `RUF4_SSH_CONFIG` overrides the client configuration file (`ssh -F`).
    fn common_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Ok(config) = std::env::var("RUF4_SSH_CONFIG")
            && !config.is_empty()
        {
            args.push("-F".into());
            args.push(config);
        }
        args.extend([
            "-o".into(),
            "PermitLocalCommand=no".into(),
            "-o".into(),
            "ClearAllForwardings=yes".into(),
            "-o".into(),
            "ForwardX11=no".into(),
        ]);
        if let Some(port) = self.port {
            args.push("-p".into());
            args.push(port.to_string());
        }
        args
    }
}

/// A parsed `ssh://` path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePath {
    pub host: HostSpec,
    /// Absolute POSIX path on the remote side. Empty when the URL carries no
    /// path; the remote home directory is substituted on first use.
    pub path: String,
}

impl RemotePath {
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(format!("{}{}", self.host.display(), self.path))
    }
}

/// Parse a URL authority `[user@]host[:port]`; IPv6 hosts are bracketed.
/// `None` for an empty host or a malformed bracket form.
pub(crate) fn parse_authority(authority: &str) -> Option<(Option<String>, String, Option<u16>)> {
    let (user, host_port) = match authority.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() => (Some(u.to_string()), h),
        Some((_, h)) => (None, h),
        None => (None, authority),
    };
    let (host, port) = if let Some(bracketed) = host_port.strip_prefix('[') {
        // IPv6 literal: [addr] or [addr]:port.
        let (host, tail) = bracketed.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(p) => Some(p.parse().ok()?),
            None if tail.is_empty() => None,
            None => return None,
        };
        (host.to_string(), port)
    } else {
        match host_port.rsplit_once(':') {
            Some((h, p)) => match p.parse::<u16>() {
                Ok(port) => (h.to_string(), Some(port)),
                Err(_) => (host_port.to_string(), None),
            },
            None => (host_port.to_string(), None),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some((user, host, port))
}

/// Parse `ssh://[user@]host[:port]/path`. `None` for local paths and for
/// malformed remote ones.
pub fn parse_remote(path: &Path) -> Option<RemotePath> {
    let s = path.to_str()?;
    let rest = s.strip_prefix(SCHEME)?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (user, host, port) = parse_authority(authority)?;
    Some(RemotePath {
        host: HostSpec { user, host, port },
        path: path.to_string(),
    })
}

pub fn is_remote(path: &Path) -> bool {
    parse_remote(path).is_some()
}

/// Whether `s` is a location URL in one of the vfs schemes.
pub fn is_url(s: &str) -> bool {
    s.starts_with(SCHEME) || s.starts_with(crate::smb::SCHEME)
}

/// Collapse `//`, `.`, and `..` in a POSIX path, lexically.
pub fn normalize_posix(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    let mut out = String::from("/");
    out.push_str(&parts.join("/"));
    out
}

fn posix_join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Append `name` to a directory path, dispatching on scheme.
pub fn join(dir: &Path, name: &str) -> PathBuf {
    match parse_remote(dir) {
        Some(r) => {
            let joined = if name.starts_with('/') {
                name.to_string()
            } else {
                posix_join(&r.path, name)
            };
            RemotePath {
                host: r.host,
                path: normalize_posix(&joined),
            }
            .to_path_buf()
        }
        None => dir.join(name),
    }
}

/// Parent directory, dispatching on scheme. Remote roots (and `/`) have none.
pub fn parent(path: &Path) -> Option<PathBuf> {
    match parse_remote(path) {
        Some(r) => {
            let p = normalize_posix(&r.path);
            if p == "/" {
                return None;
            }
            let parent = match p.rfind('/') {
                Some(0) => "/".to_string(),
                Some(i) => p[..i].to_string(),
                None => return None,
            };
            Some(
                RemotePath {
                    host: r.host,
                    path: parent,
                }
                .to_path_buf(),
            )
        }
        None => path.parent().map(Path::to_path_buf),
    }
}

// ── Connections ─────────────────────────────────────────────────────────────

/// SFTP channel over the stdio of a spawned `ssh` subsystem process.
struct ChildTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Read for ChildTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl Write for ChildTransport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdin.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdin.flush()
    }
}

impl Drop for ChildTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

type Conn = sftp::Client<ChildTransport>;

static CONNECTIONS: LazyLock<Mutex<HashMap<HostSpec, Conn>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn connections() -> MutexGuard<'static, HashMap<HostSpec, Conn>> {
    CONNECTIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Multiplexing socket template; `%C` hashes host, port, and user. The
/// socket lives in the runtime directory when available, else `~/.ssh`
/// (created on demand), else `/tmp`. `TMPDIR` is not used: on macOS it is
/// deep enough to overflow the 104-byte socket-path limit once `ssh` appends
/// its temporary rendezvous suffix.
#[cfg(unix)]
fn control_path() -> String {
    let dir = 'dir: {
        if let Ok(d) = std::env::var("XDG_RUNTIME_DIR")
            && !d.is_empty()
        {
            break 'dir PathBuf::from(d);
        }
        let ssh_dir = platform::home_dir().join(".ssh");
        if ssh_dir.is_dir() {
            break 'dir ssh_dir;
        }
        if fs::create_dir_all(&ssh_dir).is_ok() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&ssh_dir, fs::Permissions::from_mode(0o700));
            break 'dir ssh_dir;
        }
        PathBuf::from("/tmp")
    };
    format!("{}/ruf4-%C", dir.display())
}

/// Ensure a connection master for `host` is running, prompting for
/// authentication through a suspended TUI when needed.
#[cfg(unix)]
fn ensure_master(host: &HostSpec) -> Result<(), String> {
    let control = format!("ControlPath={}", control_path());

    let check = Command::new("ssh")
        .args(host.common_args())
        .args(["-o", &control, "-O", "check"])
        .arg(host.ssh_dest())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if matches!(check, Ok(st) if st.success()) {
        return Ok(());
    }

    // Start a master in the foreground so host-key and password prompts reach
    // the user; `-f -N` backgrounds it once authenticated.
    let mut cmd = Command::new("ssh");
    cmd.args(host.common_args());
    cmd.args(["-o", &control]);
    cmd.args(["-o", "ControlMaster=auto", "-o", "ControlPersist=600"]);
    cmd.args(["-f", "-N"]);
    cmd.arg(host.ssh_dest());
    let display = format!("ssh {}", host.ssh_dest());
    match platform::run_foreground(&mut cmd, &display, false) {
        Ok(st) if st.success() => Ok(()),
        Ok(_) => Err(format!("Cannot connect to {}", host.display())),
        Err(e) => Err(e),
    }
}

/// Multiplexing is unavailable; channels authenticate on their own with keys
/// or an agent. TODO: interactive authentication on Windows.
#[cfg(not(unix))]
fn ensure_master(_host: &HostSpec) -> Result<(), String> {
    Ok(())
}

/// Spawn one SFTP channel. Never prompts: with a live master the channel
/// attaches to its socket; otherwise `BatchMode` restricts authentication to
/// keys and agents so a broken setup fails fast instead of hanging.
fn spawn_channel(host: &HostSpec) -> io::Result<ChildTransport> {
    let mut cmd = Command::new("ssh");
    cmd.args(host.common_args());
    cmd.args(["-o", "BatchMode=yes"]);
    #[cfg(unix)]
    {
        cmd.arg("-o").arg(format!("ControlPath={}", control_path()));
        cmd.args(["-o", "ControlMaster=no"]);
    }
    cmd.arg("-s").arg(host.ssh_dest()).arg("sftp");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    Ok(ChildTransport {
        child,
        stdin,
        stdout,
    })
}

fn is_transport_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::WriteZero
    )
}

/// Run `f` against the cached connection for `host`, opening a channel first
/// when none exists. A transport-level failure drops the connection and
/// retries once on a fresh channel, so an expired channel heals transparently.
fn with_conn<R>(host: &HostSpec, mut f: impl FnMut(&mut Conn) -> io::Result<R>) -> io::Result<R> {
    let mut map = connections();
    let mut retried = false;
    loop {
        if !map.contains_key(host) {
            let transport = spawn_channel(host)?;
            let client = sftp::Client::new(transport).map_err(|e| {
                io::Error::new(e.kind(), format!("sftp channel to {}: {e}", host.display()))
            })?;
            map.insert(host.clone(), client);
        }
        let conn = map.get_mut(host).expect("inserted above");
        match f(conn) {
            Err(e) if !retried && is_transport_error(&e) => {
                map.remove(host);
                retried = true;
            }
            result => return result,
        }
    }
}

/// Close a remote handle on the cached connection, if it still exists.
fn close_handle(host: &HostSpec, handle: &[u8]) {
    let mut map = connections();
    if let Some(conn) = map.get_mut(host) {
        let _ = conn.close(handle);
    }
}

/// Bootstrap connectivity for `path`'s host when it is remote, prompting for
/// authentication through a suspended TUI as needed. No-op for local paths.
/// Workers must not authenticate interactively, so jobs touching a remote
/// destination call this on the UI thread before spawning.
pub fn ensure_host(path: &Path) -> Result<(), String> {
    match parse_remote(path) {
        Some(r) => ensure_master(&r.host),
        None => Ok(()),
    }
}

// ── Navigation ──────────────────────────────────────────────────────────────

/// Resolve `path` to a directory the panel can show. Local paths are
/// canonicalized and verified; `smb://` locations resolve to a local
/// mountpoint (mounting the share first when needed); `ssh://` paths
/// bootstrap the connection (interactively when authentication is required),
/// substitute the remote home for an empty path, and are verified to be
/// directories.
pub fn prepare_dir(path: &Path) -> Result<PathBuf, String> {
    if let Some(smb) = crate::smb::parse(path) {
        let dir = crate::smb::resolve_dir(&smb)?;
        return if dir.is_dir() {
            Ok(dir)
        } else {
            Err(format!("Not a directory: {}", dir.display()))
        };
    }
    match parse_remote(path) {
        Some(r) => {
            ensure_master(&r.host)?;
            let host = r.host.clone();
            let resolved = with_conn(&host, |c| {
                let p = if r.path.is_empty() {
                    c.realpath(".")?
                } else {
                    normalize_posix(&r.path)
                };
                let attrs = c.stat(&p)?;
                if !attrs.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "not a directory",
                    ));
                }
                Ok(p)
            })
            .map_err(|e| format!("{}: {e}", r.host.display()))?;
            Ok(RemotePath {
                host,
                path: normalize_posix(&resolved),
            }
            .to_path_buf())
        }
        None => {
            let dest = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if dest.is_dir() {
                return Ok(dest);
            }
            // A managed SMB mountpoint may simply be unmounted; remount it.
            match crate::smb::try_remount(&dest) {
                Some(Ok(())) if dest.is_dir() => Ok(dest),
                Some(Err(message)) => Err(message),
                _ => Err(format!("Not a directory: {}", dest.display())),
            }
        }
    }
}

/// SMB mountpoints for the roots dialog.
pub fn smb_roots() -> Vec<PathBuf> {
    crate::smb::mounted_roots()
}

// ── Directory listing ───────────────────────────────────────────────────────

/// List a directory as panel entries, dispatching on scheme. A failed remote
/// listing degrades to `..` so the panel stays navigable.
pub fn scan_dir(path: &Path, show_hidden: bool) -> Vec<FileEntry> {
    match parse_remote(path) {
        Some(r) => scan_dir_remote(&r, show_hidden).unwrap_or_else(|_| {
            let mut entries = Vec::new();
            if normalize_posix(&r.path) != "/" {
                entries.push(panel::make_entry("..", true, 0));
            }
            entries
        }),
        None => panel::scan_dir(path, show_hidden),
    }
}

fn scan_dir_remote(r: &RemotePath, show_hidden: bool) -> io::Result<Vec<FileEntry>> {
    with_conn(&r.host, |c| {
        let mut entries = Vec::new();
        if normalize_posix(&r.path) != "/" {
            entries.push(panel::make_entry("..", true, 0));
        }
        for de in c.read_dir(&r.path)? {
            if de.name == "." || de.name == ".." {
                continue;
            }
            if !show_hidden && de.name.starts_with('.') {
                continue;
            }
            let is_symlink = de.attrs.is_symlink();
            let mut attrs = de.attrs;
            if is_symlink {
                // Follow the link for display metadata, as the local scan does.
                if let Ok(followed) = c.stat(&posix_join(&r.path, &de.name)) {
                    attrs = followed;
                }
            }
            entries.push(remote_entry(de.name, is_symlink, &attrs));
        }
        Ok(entries)
    })
}

/// Panel entry from remote attributes. When the entry is a symlink, `attrs`
/// describes the link target where resolution succeeded.
pub fn remote_entry(name: String, is_symlink: bool, attrs: &sftp::Attrs) -> FileEntry {
    let is_dir = attrs.is_dir();
    let perms = attrs.permissions.unwrap_or(0);
    FileEntry {
        is_hidden: name.starts_with('.'),
        name,
        is_dir,
        is_symlink,
        is_hardlink: false,
        is_executable: !is_dir && perms & 0o111 != 0,
        is_readonly: attrs.permissions.is_some_and(|p| p & 0o200 == 0),
        size: attrs.size.unwrap_or(0),
        modified: attrs
            .times
            .map(|(_, mtime)| SystemTime::UNIX_EPOCH + Duration::from_secs(mtime as u64)),
        selected: false,
    }
}

/// Child names with a directory marker, for the quick-view listing.
pub fn list_names(path: &Path) -> io::Result<Vec<(String, bool)>> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| {
            Ok(c.read_dir(&r.path)?
                .into_iter()
                .filter(|e| e.name != "." && e.name != "..")
                .map(|e| (e.name, e.attrs.is_dir()))
                .collect())
        }),
        None => {
            let mut out = Vec::new();
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                out.push((entry.file_name().to_string_lossy().into_owned(), is_dir));
            }
            Ok(out)
        }
    }
}

// ── Metadata ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
}

#[derive(Clone, Copy, Debug)]
pub struct Meta {
    pub kind: Kind,
    pub size: u64,
}

fn meta_from_attrs(attrs: &sftp::Attrs) -> Meta {
    let kind = if attrs.is_symlink() {
        Kind::Symlink
    } else if attrs.is_dir() {
        Kind::Dir
    } else {
        Kind::File
    };
    Meta {
        kind,
        size: attrs.size.unwrap_or(0),
    }
}

/// lstat-style metadata (symlinks are not followed).
pub fn symlink_meta(path: &Path) -> io::Result<Meta> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| Ok(meta_from_attrs(&c.lstat(&r.path)?))),
        None => {
            let meta = fs::symlink_metadata(path)?;
            let ft = meta.file_type();
            let kind = if ft.is_symlink() {
                Kind::Symlink
            } else if ft.is_dir() {
                Kind::Dir
            } else {
                Kind::File
            };
            Ok(Meta {
                kind,
                size: meta.len(),
            })
        }
    }
}

/// Whether `path` exists, following symlinks.
pub fn exists(path: &Path) -> bool {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| c.stat(&r.path)).is_ok(),
        None => path.exists(),
    }
}

/// Whether `path` is a directory, following symlinks.
pub fn is_dir(path: &Path) -> bool {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| c.stat(&r.path))
            .map(|a| a.is_dir())
            .unwrap_or(false),
        None => path.is_dir(),
    }
}

/// Whether the two paths refer to the same file.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (parse_remote(a), parse_remote(b)) {
        (None, None) => match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        },
        (Some(x), Some(y)) => {
            x.host == y.host && normalize_posix(&x.path) == normalize_posix(&y.path)
        }
        _ => false,
    }
}

/// Whether a rename between the two paths can possibly succeed (local↔local
/// or the same remote host). Local renames may still fail across devices;
/// callers keep that fallback.
pub fn same_domain(a: &Path, b: &Path) -> bool {
    match (parse_remote(a), parse_remote(b)) {
        (None, None) => true,
        (Some(x), Some(y)) => x.host == y.host,
        _ => false,
    }
}

/// Whether the directory `outer` contains (or equals) `inner`, even when
/// `inner` does not exist yet. Guards copies of a directory into itself.
pub fn dir_contains(outer: &Path, inner: &Path) -> bool {
    match (parse_remote(outer), parse_remote(inner)) {
        (None, None) => match (
            fs::canonicalize(outer),
            crate::fileops::normalize_against_existing(inner),
        ) {
            (Ok(co), Ok(ci)) => ci.starts_with(&co),
            _ => false,
        },
        (Some(a), Some(b)) if a.host == b.host => {
            let outer = normalize_posix(&a.path);
            let inner = normalize_posix(&b.path);
            inner == outer || outer == "/" || inner.starts_with(&format!("{outer}/"))
        }
        _ => false,
    }
}

/// Available bytes on the filesystem holding `path`, if known.
pub fn free_space(path: &Path) -> Option<u64> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| c.statvfs_avail(&r.path))
            .ok()
            .flatten(),
        None => platform::disk_free(path),
    }
}

/// Count files and total bytes under `path`; the path itself counts as one
/// entry when it is a file or symlink. Used for progress totals.
pub fn scan_tree(path: &Path) -> (u64, u64) {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| Ok(scan_tree_conn(c, &r.path))).unwrap_or((1, 0)),
        None => scan_tree_local(path),
    }
}

fn scan_tree_conn(c: &mut Conn, path: &str) -> (u64, u64) {
    let Ok(attrs) = c.lstat(path) else {
        return (1, 0);
    };
    if attrs.is_symlink() {
        return (1, 0);
    }
    if attrs.is_dir() {
        let mut files = 0;
        let mut bytes = 0;
        if let Ok(entries) = c.read_dir(path) {
            for e in entries {
                if e.name == "." || e.name == ".." {
                    continue;
                }
                if e.attrs.is_dir() && !e.attrs.is_symlink() {
                    let (f, b) = scan_tree_conn(c, &posix_join(path, &e.name));
                    files += f;
                    bytes += b;
                } else if e.attrs.is_symlink() {
                    files += 1;
                } else {
                    files += 1;
                    bytes += e.attrs.size.unwrap_or(0);
                }
            }
        }
        (files, bytes)
    } else {
        (1, attrs.size.unwrap_or(0))
    }
}

fn scan_tree_local(path: &Path) -> (u64, u64) {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (1, 0),
    };
    if meta.file_type().is_symlink() {
        return (1, 0);
    }
    if meta.is_dir() {
        let mut files = 0;
        let mut bytes = 0;
        if let Ok(rd) = fs::read_dir(path) {
            for entry in rd.flatten() {
                let (f, b) = scan_tree_local(&entry.path());
                files += f;
                bytes += b;
            }
        }
        (files, bytes)
    } else {
        (1, meta.len())
    }
}

/// Directory children as (name, lstat metadata), one listing per directory.
pub fn read_dir_meta(path: &Path) -> io::Result<Vec<(String, Meta)>> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| {
            Ok(c.read_dir(&r.path)?
                .into_iter()
                .filter(|e| e.name != "." && e.name != "..")
                .map(|e| (e.name.clone(), meta_from_attrs(&e.attrs)))
                .collect())
        }),
        None => {
            let mut out = Vec::new();
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let ft = entry.file_type()?;
                let kind = if ft.is_symlink() {
                    Kind::Symlink
                } else if ft.is_dir() {
                    Kind::Dir
                } else {
                    Kind::File
                };
                let size = if kind == Kind::File {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                } else {
                    0
                };
                out.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    Meta { kind, size },
                ));
            }
            Ok(out)
        }
    }
}

// ── Mutations ───────────────────────────────────────────────────────────────

/// Create a directory and any missing ancestors.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| {
            let normalized = normalize_posix(&r.path);
            let mut cur = String::new();
            for comp in normalized.split('/').filter(|c| !c.is_empty()) {
                cur.push('/');
                cur.push_str(comp);
                match c.stat(&cur) {
                    Ok(attrs) if attrs.is_dir() => {}
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!("{cur}: exists and is not a directory"),
                        ));
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => c.mkdir(&cur)?,
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }),
        None => fs::create_dir_all(path),
    }
}

/// Rename within one filesystem domain (see [`same_domain`]).
pub fn rename(src: &Path, dst: &Path) -> io::Result<()> {
    match (parse_remote(src), parse_remote(dst)) {
        (None, None) => fs::rename(src, dst),
        (Some(a), Some(b)) if a.host == b.host => {
            with_conn(&a.host, |c| c.rename(&a.path, &b.path))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "rename across filesystem domains",
        )),
    }
}

/// Remove a file, symlink, or directory tree.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| remove_tree_conn(c, &r.path)),
        None => {
            let is_symlink = fs::symlink_metadata(path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            // Symlinks are removed as links, never following the target.
            if is_symlink {
                platform::remove_symlink(path)
            } else if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            }
        }
    }
}

fn remove_tree_conn(c: &mut Conn, path: &str) -> io::Result<()> {
    let attrs = c.lstat(path)?;
    if attrs.is_dir() && !attrs.is_symlink() {
        for e in c.read_dir(path)? {
            if e.name == "." || e.name == ".." {
                continue;
            }
            remove_tree_conn(c, &posix_join(path, &e.name))?;
        }
        c.rmdir(path)
    } else {
        c.remove(path)
    }
}

/// Symlink target as a string, without following.
pub fn read_link(path: &Path) -> io::Result<String> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| c.read_link(&r.path)),
        None => Ok(fs::read_link(path)?.to_string_lossy().into_owned()),
    }
}

/// Create a symlink at `link` pointing to `target`.
pub fn symlink(link: &Path, target: &str) -> io::Result<()> {
    match parse_remote(link) {
        Some(r) => with_conn(&r.host, |c| c.symlink(target, &r.path)),
        None => {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(target, link)
            }
            #[cfg(windows)]
            {
                // The target's type is unknown here. TODO: directory symlinks
                // on Windows for cross-domain copies.
                std::os::windows::fs::symlink_file(target, link)
            }
            #[cfg(not(any(unix, windows)))]
            {
                std::os::unix::fs::symlink(target, link)
            }
        }
    }
}

/// Recreate the symlink `src` at `dst` with the same target.
pub fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    match (parse_remote(src), parse_remote(dst)) {
        (None, None) => platform::copy_symlink(src, dst),
        _ => {
            let target = read_link(src)?;
            symlink(dst, &target)
        }
    }
}

// ── File I/O ────────────────────────────────────────────────────────────────

struct RemoteReader {
    host: HostSpec,
    handle: Vec<u8>,
    offset: u64,
    eof: bool,
}

impl Read for RemoteReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.eof || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min(sftp::MAX_DATA) as u32;
        let data = with_conn(&self.host, |c| c.read(&self.handle, self.offset, want))?;
        match data {
            None => {
                self.eof = true;
                Ok(0)
            }
            Some(d) => {
                if d.len() > buf.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "sftp: server returned more data than requested",
                    ));
                }
                if d.is_empty() {
                    self.eof = true;
                    return Ok(0);
                }
                buf[..d.len()].copy_from_slice(&d);
                self.offset += d.len() as u64;
                Ok(d.len())
            }
        }
    }
}

impl Drop for RemoteReader {
    fn drop(&mut self) {
        close_handle(&self.host, &self.handle);
    }
}

struct RemoteWriter {
    host: HostSpec,
    handle: Vec<u8>,
    offset: u64,
}

impl Write for RemoteWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = buf.len().min(sftp::MAX_DATA);
        with_conn(&self.host, |c| {
            c.write(&self.handle, self.offset, &buf[..n])
        })?;
        self.offset += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RemoteWriter {
    fn drop(&mut self) {
        close_handle(&self.host, &self.handle);
    }
}

/// Open `path` for sequential reading.
pub fn open_read(path: &Path) -> io::Result<Box<dyn Read>> {
    match parse_remote(path) {
        Some(r) => {
            let handle = with_conn(&r.host, |c| c.open(&r.path, sftp::OPEN_READ))?;
            Ok(Box::new(RemoteReader {
                host: r.host,
                handle,
                offset: 0,
                eof: false,
            }))
        }
        None => Ok(Box::new(fs::File::open(path)?)),
    }
}

/// Create or truncate `path` for sequential writing.
pub fn open_write(path: &Path) -> io::Result<Box<dyn Write>> {
    match parse_remote(path) {
        Some(r) => {
            let flags = sftp::OPEN_WRITE | sftp::OPEN_CREAT | sftp::OPEN_TRUNC;
            let handle = with_conn(&r.host, |c| c.open(&r.path, flags))?;
            Ok(Box::new(RemoteWriter {
                host: r.host,
                handle,
                offset: 0,
            }))
        }
        None => Ok(Box::new(fs::File::create(path)?)),
    }
}

/// Read up to `max` bytes from the start of the file, plus its total size.
pub fn read_prefix(path: &Path, max: usize) -> io::Result<(Vec<u8>, u64)> {
    match parse_remote(path) {
        Some(r) => with_conn(&r.host, |c| {
            let attrs = c.stat(&r.path)?;
            let size = attrs.size.unwrap_or(0);
            let handle = c.open(&r.path, sftp::OPEN_READ)?;
            let mut buf = Vec::with_capacity(max.min(size as usize));
            let result = loop {
                if buf.len() >= max {
                    break Ok(());
                }
                let want = (max - buf.len()).min(sftp::MAX_DATA) as u32;
                match c.read(&handle, buf.len() as u64, want) {
                    Ok(Some(d)) if d.is_empty() => break Ok(()),
                    Ok(Some(d)) => buf.extend_from_slice(&d),
                    Ok(None) => break Ok(()),
                    Err(e) => break Err(e),
                }
            };
            let _ = c.close(&handle);
            result.map(|()| (buf, size))
        }),
        None => {
            let size = fs::metadata(path)?.len();
            let file = fs::File::open(path)?;
            let mut buf = Vec::new();
            file.take(max as u64).read_to_end(&mut buf)?;
            Ok((buf, size))
        }
    }
}

// ── Remote shell commands ───────────────────────────────────────────────────

/// Quote `s` for a POSIX shell.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Run `cmd` on the remote host in the foreground with the TUI suspended,
/// with the panel path as the remote working directory.
pub fn run_remote_interactive(remote: &RemotePath, cmd: &str) -> Result<(), String> {
    let mut command = Command::new("ssh");
    command.args(remote.host.common_args());
    #[cfg(unix)]
    {
        command
            .arg("-o")
            .arg(format!("ControlPath={}", control_path()));
        command.args(["-o", "ControlMaster=auto", "-o", "ControlPersist=600"]);
    }
    command.arg("-t");
    command.arg(remote.host.ssh_dest());
    let remote_cmd = if remote.path.is_empty() {
        cmd.to_string()
    } else {
        format!("cd {} && {}", shell_quote(&remote.path), cmd)
    };
    command.arg(remote_cmd);
    platform::run_foreground(&mut command, cmd, true).map(|_| ())
}

// ── SSH roots discovery ─────────────────────────────────────────────────────

/// Concrete `Host` aliases from OpenSSH client configuration text. Wildcard
/// and negated patterns are skipped. TODO: honor Include directives.
pub fn ssh_config_hosts(text: &str) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Keyword and arguments separate on whitespace or a single '='.
        let (keyword, rest) = match line.split_once(['=', ' ', '\t']) {
            Some((k, r)) => (k.trim(), r.trim()),
            None => continue,
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for pattern in rest.split_whitespace() {
            let pattern = pattern.trim_matches('"');
            if pattern.is_empty() || pattern.contains(['*', '?']) || pattern.starts_with('!') {
                continue;
            }
            if !hosts.iter().any(|h| h == pattern) {
                hosts.push(pattern.to_string());
            }
        }
    }
    hosts
}

/// SSH destinations from the client configuration (`~/.ssh/config`, or
/// `RUF4_SSH_CONFIG` when set) as `ssh://host` roots.
pub fn ssh_roots() -> Vec<PathBuf> {
    let config = match std::env::var("RUF4_SSH_CONFIG") {
        Ok(c) if !c.is_empty() => PathBuf::from(c),
        _ => platform::home_dir().join(".ssh").join("config"),
    };
    let Ok(text) = fs::read_to_string(&config) else {
        return Vec::new();
    };
    ssh_config_hosts(&text)
        .into_iter()
        .map(|h| PathBuf::from(format!("{SCHEME}{h}")))
        .collect()
}
