// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Background execution of long-running operations.
//!
//! A [`Job`] runs one operation (copy, move, delete, or an external command) on a
//! worker thread so the UI thread stays responsive. The worker communicates only
//! through channels and an atomic cancel flag; it never touches `State`, `Panel`,
//! the terminal (`sys`), or the scratch arena. The pure file primitives in
//! `fileops` are reused for the actual work.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvError, Sender, TryRecvError, channel};
use std::thread::JoinHandle;

use crate::fileops;
use crate::sftp;
use crate::vfs;

/// The operation a [`Job`] performs. Drives completion handling (which panels to
/// refresh) and the progress-dialog title.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Copy,
    Move,
    Delete,
    /// Copy a remote file to a local temporary target, opened on completion.
    Download,
}

impl JobKind {
    pub fn title(self) -> &'static str {
        match self {
            JobKind::Copy => "Copying",
            JobKind::Move => "Moving",
            JobKind::Delete => "Deleting",
            JobKind::Download => "Downloading",
        }
    }
}

/// A progress snapshot mirrored from the worker into the UI each frame.
#[derive(Clone, Default)]
pub struct Progress {
    pub current: String,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Worker -> UI messages.
enum JobEvent {
    Progress(Progress),
    /// The worker hit an existing target and is blocked waiting for a [`Decision`].
    NeedOverwrite(String),
    /// Terminal message; carries the accumulated per-file errors.
    Finished {
        errors: Vec<String>,
    },
}

/// UI -> worker answer to an overwrite prompt.
#[derive(Clone, Copy)]
pub enum Decision {
    Overwrite,
    Skip,
    OverwriteAll,
    Cancel,
}

/// A handle to a running background operation, owned by the UI thread.
pub struct Job {
    pub kind: JobKind,
    pub progress: Progress,
    /// `Some(name)` while the worker is blocked on an overwrite decision.
    pub awaiting_overwrite: Option<String>,
    pub cancelling: bool,
    handle: Option<JoinHandle<()>>,
    events: Receiver<JobEvent>,
    decisions: Sender<Decision>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn is_copy(&self) -> bool {
        self.kind == JobKind::Copy
    }

    /// Request cancellation. The worker stops at the next file boundary.
    pub fn cancel(&mut self) {
        self.cancelling = true;
        self.cancel.store(true, Ordering::Relaxed);
        // If the worker is blocked on an overwrite decision, release it.
        if self.awaiting_overwrite.take().is_some() {
            let _ = self.decisions.send(Decision::Cancel);
        }
    }

    /// Answer a pending overwrite prompt.
    pub fn answer_overwrite(&mut self, decision: Decision) {
        if self.awaiting_overwrite.take().is_some() {
            if matches!(decision, Decision::Cancel) {
                self.cancelling = true;
                self.cancel.store(true, Ordering::Relaxed);
            }
            let _ = self.decisions.send(decision);
        }
    }

    /// Drain pending worker messages. Returns the terminal errors once the worker
    /// has finished (the job should then be dropped by the caller); `None` while
    /// it is still running.
    pub fn poll(&mut self) -> Option<Vec<String>> {
        let mut finished = None;
        loop {
            match self.events.try_recv() {
                Ok(JobEvent::Progress(p)) => self.progress = p,
                Ok(JobEvent::NeedOverwrite(name)) => self.awaiting_overwrite = Some(name),
                Ok(JobEvent::Finished { errors }) => finished = Some(errors),
                Err(TryRecvError::Empty) => break,
                // Gone without a word: the worker panicked.
                Err(TryRecvError::Disconnected) => {
                    finished
                        .get_or_insert_with(|| vec!["the operation failed unexpectedly".into()]);
                    break;
                }
            }
        }
        if finished.is_some()
            && let Some(handle) = self.handle.take()
        {
            let _ = handle.join();
        }
        finished
    }
}

/// Spawn a copy or move job over the given (source, target) pairs.
pub fn spawn_copy_move(pairs: Vec<(PathBuf, PathBuf)>, is_copy: bool) -> Job {
    let kind = if is_copy {
        JobKind::Copy
    } else {
        JobKind::Move
    };
    spawn(kind, move |tx, rx, cancel| {
        run_copy_move(pairs, is_copy, false, &tx, &rx, &cancel)
    })
}

/// Spawn a download of one file to a temporary target, overwriting silently.
pub fn spawn_download(src: PathBuf, target: PathBuf) -> Job {
    spawn(JobKind::Download, move |tx, rx, cancel| {
        // Opening a file wants its contents. Copied as is, a symlink would be
        // recreated locally, pointing wherever the remote side says.
        match resolve_symlinks(&src) {
            Ok(src) => run_copy_move(vec![(src, target)], true, true, &tx, &rx, &cancel),
            Err(e) => {
                let errors = vec![format!("{}: {e}", fileops::base_name(&src))];
                let _ = tx.send(JobEvent::Finished { errors });
            }
        }
    })
}

/// Follow `path` through any chain of symlinks to what it finally names.
fn resolve_symlinks(path: &Path) -> std::io::Result<PathBuf> {
    let mut path = path.to_path_buf();
    // Linux's limit, so that a link loop ends in an error.
    for _ in 0..40 {
        if vfs::symlink_meta(&path)?.kind != vfs::Kind::Symlink {
            return Ok(path);
        }
        let target = vfs::read_link(&path)?;
        let dir = vfs::parent(&path).unwrap_or_else(|| path.clone());
        path = vfs::join(&dir, &target);
    }
    Err(std::io::Error::other("too many levels of symbolic links"))
}

/// Spawn a delete job over the given paths.
pub fn spawn_delete(paths: Vec<PathBuf>) -> Job {
    spawn(JobKind::Delete, move |tx, _rx, cancel| {
        run_delete(paths, &tx, &cancel)
    })
}

fn spawn<F>(kind: JobKind, body: F) -> Job
where
    F: FnOnce(Sender<JobEvent>, Receiver<Decision>, Arc<AtomicBool>) + Send + 'static,
{
    let (event_tx, event_rx) = channel();
    let (decision_tx, decision_rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let handle = std::thread::spawn(move || body(event_tx, decision_rx, worker_cancel));
    Job {
        kind,
        progress: Progress::default(),
        awaiting_overwrite: None,
        cancelling: false,
        handle: Some(handle),
        events: event_rx,
        decisions: decision_tx,
        cancel,
    }
}

// ── Workers ─────────────────────────────────────────────────────────────────

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn run_copy_move(
    pairs: Vec<(PathBuf, PathBuf)>,
    is_copy: bool,
    overwrite_all: bool,
    tx: &Sender<JobEvent>,
    rx: &Receiver<Decision>,
    cancel: &AtomicBool,
) {
    // Pre-scan each pair so the gauge reflects total files and bytes.
    let counts: Vec<(u64, u64)> = pairs.iter().map(|(src, _)| vfs::scan_tree(src)).collect();
    let mut t = Transfer {
        is_copy,
        overwrite_all,
        stopped: false,
        prog: Progress {
            files_total: counts.iter().map(|c| c.0).sum(),
            bytes_total: counts.iter().map(|c| c.1).sum(),
            ..Progress::default()
        },
        errors: Vec::new(),
        root: PathBuf::new(),
        written: HashMap::new(),
        tx,
        rx,
        cancel,
    };

    for ((src, target), count) in pairs.iter().zip(counts) {
        if t.stopped() {
            break;
        }
        if fileops::same_file(src, target) {
            t.errors
                .push(fileops::same_file_error(&fileops::base_name(src), is_copy));
            t.skip(count);
            continue;
        }
        t.root = vfs::parent(target).unwrap_or_default();
        match vfs::symlink_meta(src) {
            Ok(meta) => {
                t.transfer(src, target, meta, Some(count));
            }
            Err(e) => {
                t.fail(target, e);
                t.skip(count);
            }
        }
    }

    let _ = tx.send(JobEvent::Finished { errors: t.errors });
}

/// Worker-side state of one copy or move job.
struct Transfer<'a> {
    is_copy: bool,
    overwrite_all: bool,
    /// The user cancelled at an overwrite prompt.
    stopped: bool,
    prog: Progress,
    errors: Vec<String>,
    /// Parent of the current top-level target; names are shown relative to it.
    root: PathBuf,
    /// Everything this job created, keyed by the lowercased path. On a
    /// case-insensitive destination, `Makefile` and `makefile` from one source
    /// land on the same file; the second must not replace the first.
    written: HashMap<String, PathBuf>,
    tx: &'a Sender<JobEvent>,
    rx: &'a Receiver<Decision>,
    cancel: &'a AtomicBool,
}

impl Transfer<'_> {
    fn stopped(&self) -> bool {
        self.stopped || cancelled(self.cancel)
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn fail(&mut self, target: &Path, e: impl std::fmt::Display) {
        let msg = format!("{}: {e}", self.rel(target));
        self.errors.push(msg);
    }

    /// Count `count` (files, bytes) as done without transferring them.
    fn skip(&mut self, count: (u64, u64)) {
        advance(&mut self.prog, count);
        let _ = self.tx.send(JobEvent::Progress(self.prog.clone()));
    }

    /// Copy or move `src` to `target`, asking before replacing anything that
    /// isn't a directory to merge with. `count` is the (files, bytes) of `src`
    /// if known. Returns whether all of `src` arrived: only then does a move
    /// remove it, so whatever was skipped or failed stays at the source.
    fn transfer(
        &mut self,
        src: &Path,
        target: &Path,
        meta: vfs::Meta,
        count: Option<(u64, u64)>,
    ) -> bool {
        if self.stopped() {
            return false;
        }
        let count_now = || match count {
            Some(count) => count,
            None if meta.kind == vfs::Kind::Dir => vfs::scan_tree(src),
            None => (1, meta.size),
        };
        if meta.kind == vfs::Kind::Other {
            // Opening a FIFO blocks until someone writes to it.
            self.fail(
                target,
                "not a regular file (pipe, socket or device), skipped",
            );
            self.skip(count_now());
            return false;
        }

        let mut merge = false;
        if let Ok(existing) = vfs::symlink_meta(target) {
            let key = target.to_string_lossy().to_lowercase();
            if let Some(other) = self.written.get(&key)
                && other != target
                && vfs::same_file(other, target)
            {
                let msg = format!(
                    "same name as {} on this destination, which ignores case; skipped",
                    self.rel(other)
                );
                self.fail(target, msg);
                self.skip(count_now());
                return false;
            }
            match (meta.kind, existing.kind) {
                (vfs::Kind::Dir, vfs::Kind::Dir) => merge = true,
                (vfs::Kind::Dir, _) | (_, vfs::Kind::Dir) => {
                    let what = if existing.kind == vfs::Kind::Dir {
                        "a directory"
                    } else {
                        "a file"
                    };
                    self.fail(target, format!("{what} of that name exists, skipped"));
                    self.skip(count_now());
                    return false;
                }
                _ => {
                    if !self.overwrite_all {
                        let _ = self.tx.send(JobEvent::NeedOverwrite(self.rel(target)));
                        match self.rx.recv() {
                            Ok(Decision::Overwrite) => {}
                            Ok(Decision::OverwriteAll) => self.overwrite_all = true,
                            Ok(Decision::Skip) => {
                                self.skip(count_now());
                                return false;
                            }
                            Ok(Decision::Cancel) | Err(RecvError) => {
                                self.stopped = true;
                                return false;
                            }
                        }
                    }
                    // A link is replaced rather than written through, and
                    // nothing may be in the way of a new link.
                    if (existing.kind == vfs::Kind::Symlink || meta.kind == vfs::Kind::Symlink)
                        && let Err(e) = vfs::remove_tree(target)
                    {
                        self.fail(target, e);
                        return false;
                    }
                }
            }
        }
        let key = target.to_string_lossy().to_lowercase();
        self.written.insert(key, target.to_path_buf());

        // A move tries a rename first: atomic and instant within one domain.
        if !self.is_copy && !merge && vfs::same_domain(src, target) {
            let count = count_now();
            match vfs::rename(src, target) {
                Ok(()) => {
                    self.prog.current = fileops::base_name(src).into_owned();
                    self.skip(count);
                    return true;
                }
                // A local rename across devices falls back to copy + remove.
                // A failed remote rename carries no distinguishable error
                // code, so it falls back the same way; a real error
                // resurfaces from the copy.
                Err(e) if vfs::is_remote(src) || is_cross_device(&e) => {}
                Err(e) => {
                    self.fail(target, e);
                    self.skip(count);
                    return false;
                }
            }
        }

        let complete = match meta.kind {
            vfs::Kind::Dir => self.transfer_dir(src, target, meta, merge),
            vfs::Kind::Symlink => {
                let result = vfs::copy_symlink(src, target);
                self.bump(src, 0);
                match result {
                    Ok(()) => true,
                    Err(e) => {
                        self.fail(target, e);
                        false
                    }
                }
            }
            _ => match self.copy_file(src, target, meta) {
                Ok(complete) => complete,
                Err(e) => {
                    self.fail(target, e);
                    false
                }
            },
        };
        if !complete || self.is_copy {
            return complete;
        }
        // Everything arrived, so the source can go. A directory is empty by
        // now, its contents having been moved one by one.
        let removed = if meta.kind == vfs::Kind::Dir {
            vfs::remove_dir(src)
        } else {
            vfs::remove_tree(src)
        };
        match removed {
            Ok(()) => true,
            Err(e) => {
                self.fail(target, format!("copied, but the source stays: {e}"));
                false
            }
        }
    }

    fn transfer_dir(&mut self, src: &Path, target: &Path, meta: vfs::Meta, merge: bool) -> bool {
        if vfs::dir_contains(src, target) {
            self.fail(target, "cannot copy directory into itself");
            return false;
        }
        if !merge && let Err(e) = vfs::create_dir_all(target, meta.mode) {
            self.fail(target, e);
            return false;
        }
        let children = match vfs::read_dir_meta(src) {
            Ok(children) => children,
            Err(e) => {
                self.fail(target, e);
                return false;
            }
        };
        let mut complete = true;
        for (name, child) in children {
            if self.stopped() {
                return false;
            }
            match (vfs::join_child(src, &name), vfs::join_child(target, &name)) {
                (Ok(s), Ok(t)) => complete &= self.transfer(&s, &t, child, None),
                (Err(e), _) | (_, Err(e)) => {
                    self.fail(target, e);
                    complete = false;
                }
            }
        }
        complete
    }

    /// Returns whether the whole file arrived, i.e. it wasn't cancelled.
    fn copy_file(&mut self, src: &Path, target: &Path, meta: vfs::Meta) -> std::io::Result<bool> {
        if !vfs::is_remote(src) && !vfs::is_remote(target) {
            std::fs::copy(src, target)?;
            self.bump(src, meta.size);
            return Ok(true);
        }
        self.prog.current = fileops::base_name(src).into_owned();
        let _ = self.tx.send(JobEvent::Progress(self.prog.clone()));
        let complete =
            copy_file_stream(src, target, meta.mode, &mut self.prog, self.tx, self.cancel)?;
        self.bump(src, 0);
        Ok(complete)
    }

    fn bump(&mut self, src: &Path, bytes: u64) {
        bump(&mut self.prog, src, bytes, self.tx);
    }
}

/// Report streamed progress every this many chunks (chunks are
/// [`sftp::MAX_DATA`] bytes, so this is roughly once per mebibyte).
const STREAM_PROGRESS_INTERVAL: u64 = 32;

/// Copy one file through read/write streams, advancing the byte gauge as
/// chunks complete. Returns false if cancelled midway, which leaves a partial
/// target, like the local path.
fn copy_file_stream(
    src: &Path,
    dst: &Path,
    mode: Option<u32>,
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::io::Result<bool> {
    let mut reader = vfs::open_read(src)?;
    let mut writer = vfs::open_write(dst, mode)?;
    let mut buf = vec![0u8; sftp::MAX_DATA];
    let mut chunks = 0u64;
    loop {
        if cancelled(cancel) {
            return Ok(false);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        prog.bytes_done += n as u64;
        chunks += 1;
        if chunks.is_multiple_of(STREAM_PROGRESS_INTERVAL) {
            let _ = tx.send(JobEvent::Progress(prog.clone()));
        }
    }
    writer.flush()?;
    Ok(true)
}

fn run_delete(paths: Vec<PathBuf>, tx: &Sender<JobEvent>, cancel: &AtomicBool) {
    let mut prog = Progress {
        files_total: paths.iter().map(|p| count_files(p)).sum(),
        ..Progress::default()
    };
    let mut errors = Vec::new();

    for path in &paths {
        if cancelled(cancel) {
            break;
        }
        prog.current = fileops::base_name(path).into_owned();
        let errs = fileops::ops_delete(std::slice::from_ref(path));
        errors.extend(errs);
        prog.files_done += count_files(path).max(1);
        let _ = tx.send(JobEvent::Progress(prog.clone()));
    }

    let _ = tx.send(JobEvent::Finished { errors });
}

// ── Progress helpers ─────────────────────────────────────────────────────────

fn bump(prog: &mut Progress, src: &Path, bytes: u64, tx: &Sender<JobEvent>) {
    prog.current = fileops::base_name(src).into_owned();
    prog.files_done += 1;
    prog.bytes_done += bytes;
    let _ = tx.send(JobEvent::Progress(prog.clone()));
}

fn advance(prog: &mut Progress, count: (u64, u64)) {
    prog.files_done += count.0;
    prog.bytes_done += count.1;
}

fn count_files(path: &Path) -> u64 {
    vfs::scan_tree(path).0
}

fn is_cross_device(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    const CROSS_DEVICE: i32 = libc::EXDEV;
    #[cfg(windows)]
    const CROSS_DEVICE: i32 = 17; // ERROR_NOT_SAME_DEVICE
    #[cfg(not(any(unix, windows)))]
    const CROSS_DEVICE: i32 = -1;
    e.raw_os_error() == Some(CROSS_DEVICE)
}
