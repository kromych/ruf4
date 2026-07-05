// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! SFTP version 3 client (draft-ietf-secsh-filexfer-02, as implemented by
//! OpenSSH).
//!
//! The client speaks the binary SFTP protocol over any `Read + Write`
//! transport; in production that is the stdio of a spawned
//! `ssh -s <destination> sftp` subsystem, in tests an in-memory mock.
//! Requests are synchronous with a single outstanding id, which matches how
//! the rest of the application consumes it.

use std::io::{self, Read, Write};

// Packet types.
const SSH_FXP_INIT: u8 = 1;
const SSH_FXP_VERSION: u8 = 2;
const SSH_FXP_OPEN: u8 = 3;
const SSH_FXP_CLOSE: u8 = 4;
const SSH_FXP_READ: u8 = 5;
const SSH_FXP_WRITE: u8 = 6;
const SSH_FXP_LSTAT: u8 = 7;
const SSH_FXP_OPENDIR: u8 = 11;
const SSH_FXP_READDIR: u8 = 12;
const SSH_FXP_REMOVE: u8 = 13;
const SSH_FXP_MKDIR: u8 = 14;
const SSH_FXP_RMDIR: u8 = 15;
const SSH_FXP_REALPATH: u8 = 16;
const SSH_FXP_STAT: u8 = 17;
const SSH_FXP_RENAME: u8 = 18;
const SSH_FXP_READLINK: u8 = 19;
const SSH_FXP_SYMLINK: u8 = 20;
const SSH_FXP_STATUS: u8 = 101;
const SSH_FXP_HANDLE: u8 = 102;
const SSH_FXP_DATA: u8 = 103;
const SSH_FXP_NAME: u8 = 104;
const SSH_FXP_ATTRS: u8 = 105;
const SSH_FXP_EXTENDED: u8 = 200;
const SSH_FXP_EXTENDED_REPLY: u8 = 201;

// Status codes.
const SSH_FX_OK: u32 = 0;
const SSH_FX_EOF: u32 = 1;
const SSH_FX_NO_SUCH_FILE: u32 = 2;
const SSH_FX_PERMISSION_DENIED: u32 = 3;
const SSH_FX_OP_UNSUPPORTED: u32 = 8;

// Attribute presence flags.
const ATTR_SIZE: u32 = 0x0000_0001;
const ATTR_UIDGID: u32 = 0x0000_0002;
const ATTR_PERMISSIONS: u32 = 0x0000_0004;
const ATTR_ACMODTIME: u32 = 0x0000_0008;
const ATTR_EXTENDED: u32 = 0x8000_0000;

// `SSH_FXP_OPEN` pflags.
pub const OPEN_READ: u32 = 0x0000_0001;
pub const OPEN_WRITE: u32 = 0x0000_0002;
pub const OPEN_CREAT: u32 = 0x0000_0008;
pub const OPEN_TRUNC: u32 = 0x0000_0010;

// POSIX file type bits carried in the permissions attribute.
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

/// Upper bound on a single packet, matching OpenSSH's limit.
const MAX_PACKET: usize = 256 * 1024;

/// Chunk size for READ/WRITE requests, matching OpenSSH's default block size.
pub const MAX_DATA: usize = 32 * 1024;

const POSIX_RENAME_EXT: &str = "posix-rename@openssh.com";
const STATVFS_EXT: &str = "statvfs@openssh.com";

// ── Wire helpers ────────────────────────────────────────────────────────────

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, b: &[u8]) {
    put_u32(buf, b.len() as u32);
    buf.extend_from_slice(b);
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_bytes(buf, s.as_bytes());
}

fn truncated() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "sftp: truncated packet")
}

fn unexpected(ptype: u8) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("sftp: unexpected packet type {ptype}"),
    )
}

fn status_error(code: u32, msg: &str) -> io::Error {
    let kind = match code {
        SSH_FX_NO_SUCH_FILE => io::ErrorKind::NotFound,
        SSH_FX_PERMISSION_DENIED => io::ErrorKind::PermissionDenied,
        SSH_FX_OP_UNSUPPORTED => io::ErrorKind::Unsupported,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("sftp: {msg} (status {code})"))
}

/// Cursor over a received packet payload.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(truncated());
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn bytes(&mut self) -> io::Result<&'a [u8]> {
        let len = self.u32()? as usize;
        self.take(len)
    }

    fn string(&mut self) -> io::Result<String> {
        Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
    }
}

/// File attributes as transmitted in SFTP v3; absent fields are `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attrs {
    pub size: Option<u64>,
    pub uid_gid: Option<(u32, u32)>,
    pub permissions: Option<u32>,
    /// (atime, mtime) in seconds since the epoch.
    pub times: Option<(u32, u32)>,
}

impl Attrs {
    pub fn is_dir(&self) -> bool {
        self.permissions.is_some_and(|p| p & S_IFMT == S_IFDIR)
    }

    pub fn is_symlink(&self) -> bool {
        self.permissions.is_some_and(|p| p & S_IFMT == S_IFLNK)
    }
}

fn read_attrs(r: &mut Reader<'_>) -> io::Result<Attrs> {
    let flags = r.u32()?;
    let mut attrs = Attrs::default();
    if flags & ATTR_SIZE != 0 {
        attrs.size = Some(r.u64()?);
    }
    if flags & ATTR_UIDGID != 0 {
        attrs.uid_gid = Some((r.u32()?, r.u32()?));
    }
    if flags & ATTR_PERMISSIONS != 0 {
        attrs.permissions = Some(r.u32()?);
    }
    if flags & ATTR_ACMODTIME != 0 {
        attrs.times = Some((r.u32()?, r.u32()?));
    }
    if flags & ATTR_EXTENDED != 0 {
        let count = r.u32()?;
        for _ in 0..count {
            r.bytes()?;
            r.bytes()?;
        }
    }
    Ok(attrs)
}

/// One directory entry from `SSH_FXP_READDIR` (lstat semantics).
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub attrs: Attrs,
}

// ── Client ──────────────────────────────────────────────────────────────────

pub struct Client<T> {
    transport: T,
    next_id: u32,
    extensions: Vec<(String, String)>,
}

impl<T: Read + Write> Client<T> {
    /// Perform the version handshake over `transport`.
    pub fn new(transport: T) -> io::Result<Self> {
        let mut client = Self {
            transport,
            next_id: 0,
            extensions: Vec::new(),
        };
        client.handshake()?;
        Ok(client)
    }

    fn handshake(&mut self) -> io::Result<()> {
        let mut body = Vec::new();
        put_u32(&mut body, 3);
        self.send_packet(SSH_FXP_INIT, &body)?;

        let (ptype, payload) = self.recv_packet()?;
        if ptype != SSH_FXP_VERSION {
            return Err(unexpected(ptype));
        }
        let mut r = Reader::new(&payload);
        let version = r.u32()?;
        if version < 3 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("sftp: server protocol version {version} < 3"),
            ));
        }
        while r.remaining() > 0 {
            let name = r.string()?;
            let data = r.string()?;
            self.extensions.push((name, data));
        }
        Ok(())
    }

    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|(n, _)| n == name)
    }

    fn send_packet(&mut self, ptype: u8, payload: &[u8]) -> io::Result<()> {
        let mut pkt = Vec::with_capacity(payload.len() + 5);
        put_u32(&mut pkt, payload.len() as u32 + 1);
        pkt.push(ptype);
        pkt.extend_from_slice(payload);
        self.transport.write_all(&pkt)?;
        self.transport.flush()
    }

    fn recv_packet(&mut self) -> io::Result<(u8, Vec<u8>)> {
        let mut len_buf = [0u8; 4];
        self.transport.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len == 0 || len > MAX_PACKET {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("sftp: invalid packet length {len}"),
            ));
        }
        let mut buf = vec![0u8; len];
        self.transport.read_exact(&mut buf)?;
        let payload = buf.split_off(1);
        Ok((buf[0], payload))
    }

    /// Send one request (`body` without the id) and return the matching reply
    /// with the id already consumed from the payload.
    fn request(&mut self, ptype: u8, body: &[u8]) -> io::Result<(u8, Vec<u8>)> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let mut payload = Vec::with_capacity(body.len() + 4);
        put_u32(&mut payload, id);
        payload.extend_from_slice(body);
        self.send_packet(ptype, &payload)?;

        let (rtype, rbuf) = self.recv_packet()?;
        let mut r = Reader::new(&rbuf);
        let rid = r.u32()?;
        if rid != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("sftp: reply id {rid} does not match request id {id}"),
            ));
        }
        Ok((rtype, rbuf[4..].to_vec()))
    }

    fn parse_status(payload: &[u8]) -> io::Result<(u32, String)> {
        let mut r = Reader::new(payload);
        let code = r.u32()?;
        // Some servers omit message/language on success.
        let msg = r.string().unwrap_or_default();
        Ok((code, msg))
    }

    /// A request whose only interesting reply is a `STATUS`.
    fn request_status(&mut self, ptype: u8, body: &[u8]) -> io::Result<()> {
        let (rtype, payload) = self.request(ptype, body)?;
        if rtype != SSH_FXP_STATUS {
            return Err(unexpected(rtype));
        }
        let (code, msg) = Self::parse_status(&payload)?;
        if code == SSH_FX_OK {
            Ok(())
        } else {
            Err(status_error(code, &msg))
        }
    }

    /// A request answered with `NAME`; returns the first (typically only) name.
    fn request_name(&mut self, ptype: u8, body: &[u8]) -> io::Result<String> {
        let (rtype, payload) = self.request(ptype, body)?;
        match rtype {
            SSH_FXP_NAME => {
                let mut r = Reader::new(&payload);
                let count = r.u32()?;
                if count == 0 {
                    return Err(truncated());
                }
                r.string()
            }
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                Err(status_error(code, &msg))
            }
            t => Err(unexpected(t)),
        }
    }

    fn request_attrs(&mut self, ptype: u8, path: &str) -> io::Result<Attrs> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        let (rtype, payload) = self.request(ptype, &body)?;
        match rtype {
            SSH_FXP_ATTRS => read_attrs(&mut Reader::new(&payload)),
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                Err(status_error(code, &msg))
            }
            t => Err(unexpected(t)),
        }
    }

    // ── Operations ──────────────────────────────────────────────────────

    pub fn realpath(&mut self, path: &str) -> io::Result<String> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        self.request_name(SSH_FXP_REALPATH, &body)
    }

    /// Attributes following symlinks.
    pub fn stat(&mut self, path: &str) -> io::Result<Attrs> {
        self.request_attrs(SSH_FXP_STAT, path)
    }

    /// Attributes without following symlinks.
    pub fn lstat(&mut self, path: &str) -> io::Result<Attrs> {
        self.request_attrs(SSH_FXP_LSTAT, path)
    }

    pub fn read_link(&mut self, path: &str) -> io::Result<String> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        self.request_name(SSH_FXP_READLINK, &body)
    }

    /// Create a symlink at `link` pointing to `target`. The argument order on
    /// the wire is OpenSSH's (target first), which deviates from the draft but
    /// is the de-facto standard.
    pub fn symlink(&mut self, target: &str, link: &str) -> io::Result<()> {
        let mut body = Vec::new();
        put_str(&mut body, target);
        put_str(&mut body, link);
        self.request_status(SSH_FXP_SYMLINK, &body)
    }

    /// List a directory. `.` and `..` are passed through as the server sent
    /// them; callers filter.
    pub fn read_dir(&mut self, path: &str) -> io::Result<Vec<DirEntry>> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        let (rtype, payload) = self.request(SSH_FXP_OPENDIR, &body)?;
        let handle = match rtype {
            SSH_FXP_HANDLE => Reader::new(&payload).bytes()?.to_vec(),
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                return Err(status_error(code, &msg));
            }
            t => return Err(unexpected(t)),
        };

        let mut entries = Vec::new();
        let result = loop {
            match self.read_dir_chunk(&handle) {
                Ok(Some(chunk)) => entries.extend(chunk),
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            }
        };
        let _ = self.close(&handle);
        result.map(|()| entries)
    }

    fn read_dir_chunk(&mut self, handle: &[u8]) -> io::Result<Option<Vec<DirEntry>>> {
        let mut body = Vec::new();
        put_bytes(&mut body, handle);
        let (rtype, payload) = self.request(SSH_FXP_READDIR, &body)?;
        match rtype {
            SSH_FXP_NAME => {
                let mut r = Reader::new(&payload);
                let count = r.u32()?;
                let mut chunk = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let name = r.string()?;
                    let _longname = r.bytes()?;
                    let attrs = read_attrs(&mut r)?;
                    chunk.push(DirEntry { name, attrs });
                }
                Ok(Some(chunk))
            }
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                if code == SSH_FX_EOF {
                    Ok(None)
                } else {
                    Err(status_error(code, &msg))
                }
            }
            t => Err(unexpected(t)),
        }
    }

    /// Open a file and return its handle.
    pub fn open(&mut self, path: &str, pflags: u32) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        put_u32(&mut body, pflags);
        put_u32(&mut body, 0); // empty attrs
        let (rtype, payload) = self.request(SSH_FXP_OPEN, &body)?;
        match rtype {
            SSH_FXP_HANDLE => Ok(Reader::new(&payload).bytes()?.to_vec()),
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                Err(status_error(code, &msg))
            }
            t => Err(unexpected(t)),
        }
    }

    pub fn close(&mut self, handle: &[u8]) -> io::Result<()> {
        let mut body = Vec::new();
        put_bytes(&mut body, handle);
        self.request_status(SSH_FXP_CLOSE, &body)
    }

    /// Read up to `len` bytes at `offset`; `None` signals end of file.
    pub fn read(&mut self, handle: &[u8], offset: u64, len: u32) -> io::Result<Option<Vec<u8>>> {
        let mut body = Vec::new();
        put_bytes(&mut body, handle);
        put_u64(&mut body, offset);
        put_u32(&mut body, len);
        let (rtype, payload) = self.request(SSH_FXP_READ, &body)?;
        match rtype {
            SSH_FXP_DATA => Ok(Some(Reader::new(&payload).bytes()?.to_vec())),
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                if code == SSH_FX_EOF {
                    Ok(None)
                } else {
                    Err(status_error(code, &msg))
                }
            }
            t => Err(unexpected(t)),
        }
    }

    pub fn write(&mut self, handle: &[u8], offset: u64, data: &[u8]) -> io::Result<()> {
        let mut body = Vec::new();
        put_bytes(&mut body, handle);
        put_u64(&mut body, offset);
        put_bytes(&mut body, data);
        self.request_status(SSH_FXP_WRITE, &body)
    }

    pub fn mkdir(&mut self, path: &str) -> io::Result<()> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        put_u32(&mut body, 0); // empty attrs
        self.request_status(SSH_FXP_MKDIR, &body)
    }

    pub fn rmdir(&mut self, path: &str) -> io::Result<()> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        self.request_status(SSH_FXP_RMDIR, &body)
    }

    pub fn remove(&mut self, path: &str) -> io::Result<()> {
        let mut body = Vec::new();
        put_str(&mut body, path);
        self.request_status(SSH_FXP_REMOVE, &body)
    }

    /// Rename, preferring the POSIX-semantics extension (replaces an existing
    /// target) over the plain v3 rename (which fails on one).
    pub fn rename(&mut self, old: &str, new: &str) -> io::Result<()> {
        if self.has_extension(POSIX_RENAME_EXT) {
            let mut body = Vec::new();
            put_str(&mut body, POSIX_RENAME_EXT);
            put_str(&mut body, old);
            put_str(&mut body, new);
            self.request_status(SSH_FXP_EXTENDED, &body)
        } else {
            let mut body = Vec::new();
            put_str(&mut body, old);
            put_str(&mut body, new);
            self.request_status(SSH_FXP_RENAME, &body)
        }
    }

    /// Available bytes on the filesystem holding `path`, via the
    /// `statvfs@openssh.com` extension. `None` when the server lacks it.
    pub fn statvfs_avail(&mut self, path: &str) -> io::Result<Option<u64>> {
        if !self.has_extension(STATVFS_EXT) {
            return Ok(None);
        }
        let mut body = Vec::new();
        put_str(&mut body, STATVFS_EXT);
        put_str(&mut body, path);
        let (rtype, payload) = self.request(SSH_FXP_EXTENDED, &body)?;
        match rtype {
            SSH_FXP_EXTENDED_REPLY => {
                let mut r = Reader::new(&payload);
                let _bsize = r.u64()?;
                let frsize = r.u64()?;
                let _blocks = r.u64()?;
                let _bfree = r.u64()?;
                let bavail = r.u64()?;
                Ok(Some(bavail.saturating_mul(frsize)))
            }
            SSH_FXP_STATUS => {
                let (code, msg) = Self::parse_status(&payload)?;
                Err(status_error(code, &msg))
            }
            t => Err(unexpected(t)),
        }
    }
}
