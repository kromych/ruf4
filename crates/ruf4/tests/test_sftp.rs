// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! SFTP client tests against an in-memory mock server that implements the
//! v3 wire format over `Read + Write`. Locks the packet layout and the
//! client's request/response handling without a network.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{self, Read, Write};

use ruf4::sftp::{Attrs, Client, MAX_DATA, OPEN_CREAT, OPEN_READ, OPEN_TRUNC, OPEN_WRITE};

// Packet types and status codes, spelled out independently of the client.
const FXP_INIT: u8 = 1;
const FXP_VERSION: u8 = 2;
const FXP_OPEN: u8 = 3;
const FXP_CLOSE: u8 = 4;
const FXP_READ: u8 = 5;
const FXP_WRITE: u8 = 6;
const FXP_LSTAT: u8 = 7;
const FXP_OPENDIR: u8 = 11;
const FXP_READDIR: u8 = 12;
const FXP_REMOVE: u8 = 13;
const FXP_MKDIR: u8 = 14;
const FXP_RMDIR: u8 = 15;
const FXP_REALPATH: u8 = 16;
const FXP_STAT: u8 = 17;
const FXP_RENAME: u8 = 18;
const FXP_READLINK: u8 = 19;
const FXP_SYMLINK: u8 = 20;
const FXP_STATUS: u8 = 101;
const FXP_HANDLE: u8 = 102;
const FXP_DATA: u8 = 103;
const FXP_NAME: u8 = 104;
const FXP_ATTRS: u8 = 105;
const FXP_EXTENDED: u8 = 200;
const FXP_EXTENDED_REPLY: u8 = 201;

const FX_OK: u32 = 0;
const FX_EOF: u32 = 1;
const FX_NO_SUCH_FILE: u32 = 2;
const FX_FAILURE: u32 = 4;

const ATTR_SIZE: u32 = 1;
const ATTR_PERMISSIONS: u32 = 4;
const ATTR_ACMODTIME: u32 = 8;

/// Entries per READDIR response, small to exercise the client's chunk loop.
const READDIR_CHUNK: usize = 2;

// ── Server-side wire helpers ────────────────────────────────────────────────

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_str(buf: &mut Vec<u8>, s: &[u8]) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s);
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn u32(&mut self) -> u32 {
        let v = u32::from_be_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        v
    }

    fn u64(&mut self) -> u64 {
        let v = u64::from_be_bytes(self.buf[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        v
    }

    fn bytes(&mut self) -> &'a [u8] {
        let len = self.u32() as usize;
        let s = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        s
    }

    fn string(&mut self) -> String {
        String::from_utf8(self.bytes().to_vec()).unwrap()
    }
}

fn attr_bytes(size: Option<u64>, perms: u32, mtime: u32) -> Vec<u8> {
    let mut b = Vec::new();
    let mut flags = ATTR_PERMISSIONS | ATTR_ACMODTIME;
    if size.is_some() {
        flags |= ATTR_SIZE;
    }
    put_u32(&mut b, flags);
    if let Some(size) = size {
        put_u64(&mut b, size);
    }
    put_u32(&mut b, perms);
    put_u32(&mut b, mtime); // atime
    put_u32(&mut b, mtime);
    b
}

// ── Mock server ─────────────────────────────────────────────────────────────

enum Handle {
    Dir(VecDeque<(String, Vec<u8>)>),
    File(String),
}

struct MockServer {
    dirs: BTreeSet<String>,
    files: BTreeMap<String, Vec<u8>>,
    symlinks: BTreeMap<String, String>,
    advertise_posix_rename: bool,
    handles: HashMap<u32, Handle>,
    next_handle: u32,
    inbox: Vec<u8>,
    outbox: VecDeque<u8>,
}

impl MockServer {
    fn new() -> Self {
        let mut s = Self {
            dirs: BTreeSet::new(),
            files: BTreeMap::new(),
            symlinks: BTreeMap::new(),
            advertise_posix_rename: true,
            handles: HashMap::new(),
            next_handle: 0,
            inbox: Vec::new(),
            outbox: VecDeque::new(),
        };
        s.dirs.insert("/".to_string());
        s.dirs.insert("/home".to_string());
        s.dirs.insert("/home/u".to_string());
        s.files
            .insert("/home/u/hello.txt".to_string(), b"hello world".to_vec());
        s.files.insert("/home/u/run.sh".to_string(), b"#!".to_vec());
        s.symlinks
            .insert("/home/u/link".to_string(), "/home/u/hello.txt".to_string());
        s
    }

    fn respond(&mut self, ptype: u8, body: &[u8]) {
        let mut pkt = Vec::with_capacity(body.len() + 5);
        put_u32(&mut pkt, body.len() as u32 + 1);
        pkt.push(ptype);
        pkt.extend_from_slice(body);
        self.outbox.extend(pkt);
    }

    fn status(&mut self, id: u32, code: u32, msg: &str) {
        let mut b = Vec::new();
        put_u32(&mut b, id);
        put_u32(&mut b, code);
        put_str(&mut b, msg.as_bytes());
        put_str(&mut b, b"");
        self.respond(FXP_STATUS, &b);
    }

    fn handle_reply(&mut self, id: u32, handle: Handle) {
        let hid = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(hid, handle);
        let mut b = Vec::new();
        put_u32(&mut b, id);
        put_str(&mut b, &hid.to_be_bytes());
        self.respond(FXP_HANDLE, &b);
    }

    fn attrs_of(&self, path: &str) -> Option<Vec<u8>> {
        if self.dirs.contains(path) {
            Some(attr_bytes(None, 0o040755, 1_700_000_000))
        } else if let Some(data) = self.files.get(path) {
            let perms = if path.ends_with(".sh") {
                0o100755
            } else {
                0o100644
            };
            Some(attr_bytes(Some(data.len() as u64), perms, 1_700_000_000))
        } else if self.symlinks.contains_key(path) {
            Some(attr_bytes(None, 0o120777, 1_700_000_000))
        } else {
            None
        }
    }

    fn resolve(&self, path: &str) -> String {
        match self.symlinks.get(path) {
            Some(target) => target.clone(),
            None => path.to_string(),
        }
    }

    fn listing(&self, dir: &str) -> VecDeque<(String, Vec<u8>)> {
        let prefix = if dir == "/" {
            "/".to_string()
        } else {
            format!("{dir}/")
        };
        let child_of = |p: &str| {
            p.strip_prefix(&prefix)
                .filter(|rest| !rest.is_empty() && !rest.contains('/'))
                .map(str::to_string)
        };
        let mut out = VecDeque::new();
        out.push_back((".".to_string(), attr_bytes(None, 0o040755, 0)));
        out.push_back(("..".to_string(), attr_bytes(None, 0o040755, 0)));
        for d in &self.dirs {
            if let Some(name) = child_of(d) {
                out.push_back((name, attr_bytes(None, 0o040755, 1_700_000_000)));
            }
        }
        for (f, data) in &self.files {
            if let Some(name) = child_of(f) {
                let perms = if f.ends_with(".sh") {
                    0o100755
                } else {
                    0o100644
                };
                out.push_back((
                    name,
                    attr_bytes(Some(data.len() as u64), perms, 1_700_000_000),
                ));
            }
        }
        for l in self.symlinks.keys() {
            if let Some(name) = child_of(l) {
                out.push_back((name, attr_bytes(None, 0o120777, 1_700_000_000)));
            }
        }
        out
    }

    fn exists(&self, path: &str) -> bool {
        self.dirs.contains(path)
            || self.files.contains_key(path)
            || self.symlinks.contains_key(path)
    }

    fn process(&mut self) {
        loop {
            if self.inbox.len() < 4 {
                return;
            }
            let len = u32::from_be_bytes(self.inbox[..4].try_into().unwrap()) as usize;
            if self.inbox.len() < 4 + len {
                return;
            }
            let packet: Vec<u8> = self.inbox.drain(..4 + len).collect();
            let ptype = packet[4];
            let body = &packet[5..];
            self.dispatch(ptype, body);
        }
    }

    fn dispatch(&mut self, ptype: u8, body: &[u8]) {
        let mut c = Cursor { buf: body, pos: 0 };

        if ptype == FXP_INIT {
            let _version = c.u32();
            let mut b = Vec::new();
            put_u32(&mut b, 3);
            if self.advertise_posix_rename {
                put_str(&mut b, b"posix-rename@openssh.com");
                put_str(&mut b, b"1");
            }
            put_str(&mut b, b"statvfs@openssh.com");
            put_str(&mut b, b"2");
            self.respond(FXP_VERSION, &b);
            return;
        }

        let id = c.u32();
        match ptype {
            FXP_REALPATH => {
                let path = c.string();
                let resolved = if path == "." {
                    "/home/u".to_string()
                } else {
                    path
                };
                let mut b = Vec::new();
                put_u32(&mut b, id);
                put_u32(&mut b, 1);
                put_str(&mut b, resolved.as_bytes());
                put_str(&mut b, b"");
                b.extend_from_slice(&attr_bytes(None, 0o040755, 0));
                self.respond(FXP_NAME, &b);
            }
            FXP_STAT | FXP_LSTAT => {
                let path = c.string();
                let lookup = if ptype == FXP_STAT {
                    self.resolve(&path)
                } else {
                    path
                };
                match self.attrs_of(&lookup) {
                    Some(attrs) => {
                        let mut b = Vec::new();
                        put_u32(&mut b, id);
                        b.extend_from_slice(&attrs);
                        self.respond(FXP_ATTRS, &b);
                    }
                    None => self.status(id, FX_NO_SUCH_FILE, "no such file"),
                }
            }
            FXP_OPENDIR => {
                let path = c.string();
                if self.dirs.contains(&path) {
                    let listing = self.listing(&path);
                    self.handle_reply(id, Handle::Dir(listing));
                } else {
                    self.status(id, FX_NO_SUCH_FILE, "no such directory");
                }
            }
            FXP_READDIR => {
                let hid = u32::from_be_bytes(c.bytes().try_into().unwrap());
                let chunk: Vec<(String, Vec<u8>)> = match self.handles.get_mut(&hid) {
                    Some(Handle::Dir(entries)) => {
                        let n = entries.len().min(READDIR_CHUNK);
                        entries.drain(..n).collect()
                    }
                    _ => {
                        self.status(id, FX_FAILURE, "bad handle");
                        return;
                    }
                };
                if chunk.is_empty() {
                    self.status(id, FX_EOF, "eof");
                    return;
                }
                let mut b = Vec::new();
                put_u32(&mut b, id);
                put_u32(&mut b, chunk.len() as u32);
                for (name, attrs) in chunk {
                    put_str(&mut b, name.as_bytes());
                    put_str(&mut b, b"");
                    b.extend_from_slice(&attrs);
                }
                self.respond(FXP_NAME, &b);
            }
            FXP_OPEN => {
                let path = c.string();
                let pflags = c.u32();
                if pflags & OPEN_CREAT != 0 {
                    let entry = self.files.entry(path.clone()).or_default();
                    if pflags & OPEN_TRUNC != 0 {
                        entry.clear();
                    }
                } else if !self.files.contains_key(&path) {
                    self.status(id, FX_NO_SUCH_FILE, "no such file");
                    return;
                }
                self.handle_reply(id, Handle::File(path));
            }
            FXP_READ => {
                let hid = u32::from_be_bytes(c.bytes().try_into().unwrap());
                let offset = c.u64() as usize;
                let len = c.u32() as usize;
                let data = match self.handles.get(&hid) {
                    Some(Handle::File(path)) => self.files.get(path).cloned().unwrap_or_default(),
                    _ => {
                        self.status(id, FX_FAILURE, "bad handle");
                        return;
                    }
                };
                if offset >= data.len() {
                    self.status(id, FX_EOF, "eof");
                    return;
                }
                let end = (offset + len).min(data.len());
                let mut b = Vec::new();
                put_u32(&mut b, id);
                put_str(&mut b, &data[offset..end]);
                self.respond(FXP_DATA, &b);
            }
            FXP_WRITE => {
                let hid = u32::from_be_bytes(c.bytes().try_into().unwrap());
                let offset = c.u64() as usize;
                let data = c.bytes().to_vec();
                match self.handles.get(&hid) {
                    Some(Handle::File(path)) => {
                        let path = path.clone();
                        let file = self.files.get_mut(&path).unwrap();
                        if file.len() < offset + data.len() {
                            file.resize(offset + data.len(), 0);
                        }
                        file[offset..offset + data.len()].copy_from_slice(&data);
                        self.status(id, FX_OK, "");
                    }
                    _ => self.status(id, FX_FAILURE, "bad handle"),
                }
            }
            FXP_CLOSE => {
                let hid = u32::from_be_bytes(c.bytes().try_into().unwrap());
                self.handles.remove(&hid);
                self.status(id, FX_OK, "");
            }
            FXP_MKDIR => {
                let path = c.string();
                if self.exists(&path) {
                    self.status(id, FX_FAILURE, "exists");
                } else {
                    self.dirs.insert(path);
                    self.status(id, FX_OK, "");
                }
            }
            FXP_RMDIR => {
                let path = c.string();
                let prefix = format!("{path}/");
                let has_children = self.dirs.iter().any(|d| d.starts_with(&prefix))
                    || self.files.keys().any(|f| f.starts_with(&prefix))
                    || self.symlinks.keys().any(|l| l.starts_with(&prefix));
                if has_children {
                    self.status(id, FX_FAILURE, "not empty");
                } else if self.dirs.remove(&path) {
                    self.status(id, FX_OK, "");
                } else {
                    self.status(id, FX_NO_SUCH_FILE, "no such directory");
                }
            }
            FXP_REMOVE => {
                let path = c.string();
                if self.files.remove(&path).is_some() || self.symlinks.remove(&path).is_some() {
                    self.status(id, FX_OK, "");
                } else {
                    self.status(id, FX_NO_SUCH_FILE, "no such file");
                }
            }
            FXP_RENAME => {
                let old = c.string();
                let new = c.string();
                // v3 semantics: fail when the target exists.
                if self.exists(&new) {
                    self.status(id, FX_FAILURE, "target exists");
                } else {
                    self.rename_entry(id, &old, &new);
                }
            }
            FXP_READLINK => {
                let path = c.string();
                match self.symlinks.get(&path) {
                    Some(target) => {
                        let mut b = Vec::new();
                        put_u32(&mut b, id);
                        put_u32(&mut b, 1);
                        put_str(&mut b, target.as_bytes());
                        put_str(&mut b, b"");
                        b.extend_from_slice(&attr_bytes(None, 0o120777, 0));
                        self.respond(FXP_NAME, &b);
                    }
                    None => self.status(id, FX_NO_SUCH_FILE, "not a symlink"),
                }
            }
            FXP_SYMLINK => {
                // OpenSSH argument order: target first, then link path.
                let target = c.string();
                let link = c.string();
                self.symlinks.insert(link, target);
                self.status(id, FX_OK, "");
            }
            FXP_EXTENDED => {
                let name = c.string();
                match name.as_str() {
                    "posix-rename@openssh.com" => {
                        let old = c.string();
                        let new = c.string();
                        self.files.remove(&new);
                        self.rename_entry(id, &old, &new);
                    }
                    "statvfs@openssh.com" => {
                        let _path = c.string();
                        let mut b = Vec::new();
                        put_u32(&mut b, id);
                        for v in [4096u64, 4096, 1000, 600, 500, 100, 90, 80, 7, 0, 255] {
                            put_u64(&mut b, v);
                        }
                        self.respond(FXP_EXTENDED_REPLY, &b);
                    }
                    _ => self.status(id, FX_FAILURE, "unsupported extension"),
                }
            }
            _ => self.status(id, FX_FAILURE, "unsupported request"),
        }
    }

    fn rename_entry(&mut self, id: u32, old: &str, new: &str) {
        if let Some(data) = self.files.remove(old) {
            self.files.insert(new.to_string(), data);
            self.status(id, FX_OK, "");
        } else if self.dirs.remove(old) {
            self.dirs.insert(new.to_string());
            self.status(id, FX_OK, "");
        } else if let Some(t) = self.symlinks.remove(old) {
            self.symlinks.insert(new.to_string(), t);
            self.status(id, FX_OK, "");
        } else {
            self.status(id, FX_NO_SUCH_FILE, "no such file");
        }
    }
}

impl Write for MockServer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inbox.extend_from_slice(buf);
        self.process();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for MockServer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.outbox.len());
        for b in buf.iter_mut().take(n) {
            *b = self.outbox.pop_front().unwrap();
        }
        Ok(n)
    }
}

fn client() -> Client<MockServer> {
    Client::new(MockServer::new()).unwrap()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn handshake_reads_extensions() {
    let c = client();
    assert!(c.has_extension("posix-rename@openssh.com"));
    assert!(c.has_extension("statvfs@openssh.com"));
    assert!(!c.has_extension("nope@example.com"));
}

#[test]
fn realpath_resolves_dot() {
    let mut c = client();
    assert_eq!(c.realpath(".").unwrap(), "/home/u");
}

#[test]
fn stat_follows_symlinks_lstat_does_not() {
    let mut c = client();
    let followed = c.stat("/home/u/link").unwrap();
    assert!(!followed.is_symlink());
    assert_eq!(followed.size, Some(11));
    let link = c.lstat("/home/u/link").unwrap();
    assert!(link.is_symlink());

    let dir = c.stat("/home/u").unwrap();
    assert!(dir.is_dir());
    assert_eq!(c.stat("/gone").unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn read_dir_collects_across_chunks() {
    let mut c = client();
    let entries = c.read_dir("/home/u").unwrap();
    // "." and ".." pass through; the caller filters.
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"."));
    assert!(names.contains(&".."));
    assert!(names.contains(&"hello.txt"));
    assert!(names.contains(&"run.sh"));
    assert!(names.contains(&"link"));
    // More entries than one READDIR chunk, so the loop ran several times.
    assert!(entries.len() > READDIR_CHUNK);

    let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
    assert_eq!(hello.attrs.size, Some(11));
    assert!(!hello.attrs.is_dir());
    let sh = entries.iter().find(|e| e.name == "run.sh").unwrap();
    assert_eq!(sh.attrs.permissions, Some(0o100755));
}

#[test]
fn file_read_write_roundtrip() {
    let mut c = client();

    // Write a payload larger than one chunk at explicit offsets.
    let payload: Vec<u8> = (0..(MAX_DATA + 100)).map(|i| (i % 251) as u8).collect();
    let handle = c
        .open("/home/u/new.bin", OPEN_WRITE | OPEN_CREAT | OPEN_TRUNC)
        .unwrap();
    let mut offset = 0usize;
    for chunk in payload.chunks(MAX_DATA) {
        c.write(&handle, offset as u64, chunk).unwrap();
        offset += chunk.len();
    }
    c.close(&handle).unwrap();

    // Read it back through the EOF-terminated read loop.
    let handle = c.open("/home/u/new.bin", OPEN_READ).unwrap();
    let mut back = Vec::new();
    while let Some(data) = c.read(&handle, back.len() as u64, MAX_DATA as u32).unwrap() {
        back.extend_from_slice(&data);
    }
    c.close(&handle).unwrap();
    assert_eq!(back, payload);

    assert_eq!(
        c.open("/missing", OPEN_READ).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn mkdir_rmdir_remove() {
    let mut c = client();
    c.mkdir("/home/u/newdir").unwrap();
    assert!(c.stat("/home/u/newdir").unwrap().is_dir());
    assert!(c.mkdir("/home/u/newdir").is_err());
    c.rmdir("/home/u/newdir").unwrap();
    assert!(c.stat("/home/u/newdir").is_err());

    c.remove("/home/u/hello.txt").unwrap();
    assert!(c.stat("/home/u/hello.txt").is_err());
    assert!(c.remove("/home/u/hello.txt").is_err());
}

#[test]
fn rename_uses_posix_extension_and_overwrites() {
    let mut c = client();
    // Target exists; the extension replaces it (plain v3 RENAME would fail).
    c.rename("/home/u/hello.txt", "/home/u/run.sh").unwrap();
    assert_eq!(c.stat("/home/u/run.sh").unwrap().size, Some(11));
    assert!(c.stat("/home/u/hello.txt").is_err());
}

#[test]
fn rename_without_extension_fails_on_existing_target() {
    let mut server = MockServer::new();
    server.advertise_posix_rename = false;
    let mut c = Client::new(server).unwrap();
    assert!(!c.has_extension("posix-rename@openssh.com"));
    assert!(c.rename("/home/u/hello.txt", "/home/u/run.sh").is_err());
    c.rename("/home/u/hello.txt", "/home/u/moved.txt").unwrap();
    assert_eq!(c.stat("/home/u/moved.txt").unwrap().size, Some(11));
}

#[test]
fn symlink_and_readlink_argument_order() {
    let mut c = client();
    c.symlink("/home/u/run.sh", "/home/u/newlink").unwrap();
    assert_eq!(c.read_link("/home/u/newlink").unwrap(), "/home/u/run.sh");
    assert!(c.lstat("/home/u/newlink").unwrap().is_symlink());
}

#[test]
fn statvfs_reports_available_bytes() {
    let mut c = client();
    // bavail(500) * frsize(4096)
    assert_eq!(c.statvfs_avail("/").unwrap(), Some(500 * 4096));
}

#[test]
fn attrs_type_predicates() {
    let dir = Attrs {
        permissions: Some(0o040755),
        ..Attrs::default()
    };
    let link = Attrs {
        permissions: Some(0o120777),
        ..Attrs::default()
    };
    let file = Attrs {
        permissions: Some(0o100644),
        ..Attrs::default()
    };
    let unknown = Attrs::default();
    assert!(dir.is_dir() && !dir.is_symlink());
    assert!(link.is_symlink() && !link.is_dir());
    assert!(!file.is_dir() && !file.is_symlink());
    assert!(!unknown.is_dir() && !unknown.is_symlink());
}

// ── Against the real OpenSSH sftp-server ────────────────────────────────────

struct ChildIo {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
}

impl Read for ChildIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl Write for ChildIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdin.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdin.flush()
    }
}

impl Drop for ChildIo {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_sftp_server() -> Option<ChildIo> {
    let path = [
        "/usr/libexec/sftp-server",
        "/usr/lib/openssh/sftp-server",
        "/usr/lib/ssh/sftp-server",
    ]
    .iter()
    .find(|p| std::path::Path::new(p).exists())?;
    let mut child = std::process::Command::new(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    Some(ChildIo {
        child,
        stdin,
        stdout,
    })
}

/// Full protocol pass against the OpenSSH sftp-server binary when one is
/// installed (macOS and Linux); a no-op elsewhere.
#[test]
fn against_real_openssh_sftp_server() {
    let Some(io) = spawn_sftp_server() else {
        return;
    };
    let mut c = Client::new(io).unwrap();

    let root = std::env::temp_dir().join(format!("ruf4_sftp_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // sftp-server resolves through symlinks (/var -> /private/var on macOS).
    let root = std::fs::canonicalize(&root).unwrap();
    let root_s = root.to_str().unwrap();

    // realpath and stat on a real directory.
    assert_eq!(c.realpath(root_s).unwrap(), root_s);
    assert!(c.stat(root_s).unwrap().is_dir());

    // mkdir, write, read back.
    let sub = format!("{root_s}/sub");
    c.mkdir(&sub).unwrap();
    let file = format!("{root_s}/data.bin");
    let payload: Vec<u8> = (0..(MAX_DATA * 2 + 17)).map(|i| (i % 256) as u8).collect();
    let handle = c.open(&file, OPEN_WRITE | OPEN_CREAT | OPEN_TRUNC).unwrap();
    let mut off = 0usize;
    for chunk in payload.chunks(MAX_DATA) {
        c.write(&handle, off as u64, chunk).unwrap();
        off += chunk.len();
    }
    c.close(&handle).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), payload);

    let attrs = c.stat(&file).unwrap();
    assert_eq!(attrs.size, Some(payload.len() as u64));
    assert!(!attrs.is_dir());

    let handle = c.open(&file, OPEN_READ).unwrap();
    let mut back = Vec::new();
    while let Some(d) = c.read(&handle, back.len() as u64, MAX_DATA as u32).unwrap() {
        back.extend_from_slice(&d);
    }
    c.close(&handle).unwrap();
    assert_eq!(back, payload);

    // Directory listing shows both entries with attributes.
    let entries = c.read_dir(root_s).unwrap();
    let find = |n: &str| entries.iter().find(|e| e.name == n);
    assert!(find("sub").unwrap().attrs.is_dir());
    assert_eq!(
        find("data.bin").unwrap().attrs.size,
        Some(payload.len() as u64)
    );

    // Symlinks (created target-first, OpenSSH order) and readlink.
    #[cfg(unix)]
    {
        let link = format!("{root_s}/link");
        c.symlink(&file, &link).unwrap();
        assert_eq!(c.read_link(&link).unwrap(), file);
        assert!(c.lstat(&link).unwrap().is_symlink());
        assert!(!c.stat(&link).unwrap().is_symlink());
        c.remove(&link).unwrap();
    }

    // Rename over an existing target goes through posix-rename.
    let file2 = format!("{root_s}/renamed.bin");
    std::fs::write(&file2, b"old").unwrap();
    c.rename(&file, &file2).unwrap();
    assert_eq!(std::fs::read(&file2).unwrap(), payload);
    assert!(c.stat(&file).is_err());

    // Free space via statvfs@openssh.com.
    assert!(c.statvfs_avail(root_s).unwrap().unwrap() > 0);

    // Cleanup through the protocol.
    c.remove(&file2).unwrap();
    c.rmdir(&sub).unwrap();
    c.rmdir(root_s).unwrap();
    assert!(!root.exists());
}
