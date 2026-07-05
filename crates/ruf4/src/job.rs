// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Background execution of long-running operations.
//!
//! A [`Job`] runs one operation (copy, move, delete, or an external command) on a
//! worker thread so the UI thread stays responsive. The worker communicates only
//! through channels and an atomic cancel flag; it never touches `State`, `Panel`,
//! the terminal (`sys`), or the scratch arena. The pure file primitives in
//! `fileops` are reused for the actual work.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvError, Sender, channel};
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
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                JobEvent::Progress(p) => self.progress = p,
                JobEvent::NeedOverwrite(name) => self.awaiting_overwrite = Some(name),
                JobEvent::Finished { errors } => finished = Some(errors),
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
        run_copy_move(vec![(src, target)], true, true, &tx, &rx, &cancel)
    })
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
    mut overwrite_all: bool,
    tx: &Sender<JobEvent>,
    rx: &Receiver<Decision>,
    cancel: &AtomicBool,
) {
    // Pre-scan each pair so the gauge reflects total files and bytes.
    let counts: Vec<(u64, u64)> = pairs.iter().map(|(src, _)| vfs::scan_tree(src)).collect();
    let mut prog = Progress {
        files_total: counts.iter().map(|c| c.0).sum(),
        bytes_total: counts.iter().map(|c| c.1).sum(),
        ..Progress::default()
    };

    let mut errors = Vec::new();

    for (i, (src, target)) in pairs.iter().enumerate() {
        if cancelled(cancel) {
            break;
        }

        if fileops::same_file(src, target) {
            errors.push(fileops::same_file_error(&fileops::base_name(src), is_copy));
            advance(&mut prog, counts[i]);
            let _ = tx.send(JobEvent::Progress(prog.clone()));
            continue;
        }

        if !overwrite_all && vfs::exists(target) {
            let name = fileops::base_name(target).into_owned();
            let _ = tx.send(JobEvent::NeedOverwrite(name));
            match rx.recv() {
                Ok(Decision::Overwrite) => {}
                Ok(Decision::OverwriteAll) => overwrite_all = true,
                Ok(Decision::Skip) => {
                    advance(&mut prog, counts[i]);
                    let _ = tx.send(JobEvent::Progress(prog.clone()));
                    continue;
                }
                Ok(Decision::Cancel) | Err(RecvError) => break,
            }
        }

        copy_move_one(
            src,
            target,
            is_copy,
            counts[i],
            &mut prog,
            tx,
            cancel,
            &mut errors,
        );
    }

    let _ = tx.send(JobEvent::Finished { errors });
}

/// Execute one top-level (src, target) pair, reporting per-file progress.
#[allow(clippy::too_many_arguments)]
fn copy_move_one(
    src: &Path,
    target: &Path,
    is_copy: bool,
    count: (u64, u64),
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
    errors: &mut Vec<String>,
) {
    let name = fileops::base_name(src).into_owned();
    if is_copy {
        if let Err(e) = copy_tree(src, target, prog, tx, cancel) {
            errors.push(format!("{name}: {e}"));
        }
        return;
    }

    // Move: try a rename first; it is atomic and instant within one
    // filesystem domain.
    if vfs::same_domain(src, target) {
        match vfs::rename(src, target) {
            Ok(()) => {
                prog.current = name;
                advance(prog, count);
                let _ = tx.send(JobEvent::Progress(prog.clone()));
                return;
            }
            // A local rename across devices falls back to copy + remove. A
            // failed remote rename carries no distinguishable error code, so
            // it falls back the same way; a real error resurfaces from the copy.
            Err(e) if vfs::is_remote(src) || is_cross_device(&e) => {}
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                return;
            }
        }
    }

    // Copy + remove: across domains, or a same-domain rename fell through.
    if let Err(e) = copy_tree(src, target, prog, tx, cancel) {
        errors.push(format!("{name}: {e}"));
        return;
    }
    if cancelled(cancel) {
        return;
    }
    if let Err(e) = vfs::remove_tree(src) {
        errors.push(format!("{name}: {e}"));
    }
}

/// Recursively copy `src` to `dst`, reporting progress and honouring
/// cancellation. Mirrors `fileops::copy_dir_recursive` but per file, so a
/// large copy stays cancellable and the gauge advances smoothly. Either side
/// may be remote; remote-to-remote copies stream through this host.
fn copy_tree(
    src: &Path,
    dst: &Path,
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    let meta = vfs::symlink_meta(src)?;
    copy_tree_inner(src, dst, meta, prog, tx, cancel)
}

fn copy_tree_inner(
    src: &Path,
    dst: &Path,
    meta: vfs::Meta,
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    if cancelled(cancel) {
        return Ok(());
    }

    match meta.kind {
        vfs::Kind::Symlink => {
            vfs::copy_symlink(src, dst)?;
            bump(prog, src, 0, tx);
        }
        vfs::Kind::Dir => {
            if vfs::dir_contains(src, dst) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cannot copy directory into itself",
                ));
            }
            vfs::create_dir_all(dst)?;
            for (child, child_meta) in vfs::read_dir_meta(src)? {
                if cancelled(cancel) {
                    return Ok(());
                }
                copy_tree_inner(
                    &vfs::join(src, &child),
                    &vfs::join(dst, &child),
                    child_meta,
                    prog,
                    tx,
                    cancel,
                )?;
            }
        }
        vfs::Kind::File => {
            if !vfs::is_remote(src) && !vfs::is_remote(dst) {
                std::fs::copy(src, dst)?;
                bump(prog, src, meta.size, tx);
            } else {
                prog.current = fileops::base_name(src).into_owned();
                let _ = tx.send(JobEvent::Progress(prog.clone()));
                copy_file_stream(src, dst, prog, tx, cancel)?;
                bump(prog, src, 0, tx);
            }
        }
    }
    Ok(())
}

/// Report streamed progress every this many chunks (chunks are
/// [`sftp::MAX_DATA`] bytes, so this is roughly once per mebibyte).
const STREAM_PROGRESS_INTERVAL: u64 = 32;

/// Copy one file through read/write streams, advancing the byte gauge as
/// chunks complete. Cancellation leaves a partial target, like the local path.
fn copy_file_stream(
    src: &Path,
    dst: &Path,
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    let mut reader = vfs::open_read(src)?;
    let mut writer = vfs::open_write(dst)?;
    let mut buf = vec![0u8; sftp::MAX_DATA];
    let mut chunks = 0u64;
    loop {
        if cancelled(cancel) {
            return Ok(());
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
    writer.flush()
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
