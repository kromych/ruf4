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
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvError, Sender, channel};
use std::thread::JoinHandle;

use crate::fileops;
use crate::platform;

/// The operation a [`Job`] performs. Drives completion handling (which panels to
/// refresh) and the progress-dialog title.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Copy,
    Move,
    Delete,
    Command,
}

impl JobKind {
    pub fn title(self) -> &'static str {
        match self {
            JobKind::Copy => "Copying",
            JobKind::Move => "Moving",
            JobKind::Delete => "Deleting",
            JobKind::Command => "Running",
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
    /// Captured output of a [`JobKind::Command`] job.
    CommandOutput {
        command: String,
        text: String,
        code: i32,
    },
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
    /// `Some` once a command job produced output: (command, text, exit code).
    pub command_output: Option<(String, String, i32)>,
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
                JobEvent::CommandOutput {
                    command,
                    text,
                    code,
                } => {
                    self.command_output = Some((command, text, code));
                }
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
        run_copy_move(pairs, is_copy, &tx, &rx, &cancel)
    })
}

/// Spawn a delete job over the given paths.
pub fn spawn_delete(paths: Vec<PathBuf>) -> Job {
    spawn(JobKind::Delete, move |tx, _rx, cancel| {
        run_delete(paths, &tx, &cancel)
    })
}

/// Spawn an external command job. `cmd` runs through the platform shell in `cwd`.
pub fn spawn_command(cmd: String, cwd: PathBuf) -> Job {
    spawn(JobKind::Command, move |tx, _rx, cancel| {
        run_command(&cmd, &cwd, &tx, &cancel)
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
        command_output: None,
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
    tx: &Sender<JobEvent>,
    rx: &Receiver<Decision>,
    cancel: &AtomicBool,
) {
    // Pre-scan each pair so the gauge reflects total files and bytes.
    let counts: Vec<(u64, u64)> = pairs.iter().map(|(src, _)| scan_tree(src)).collect();
    let mut prog = Progress {
        files_total: counts.iter().map(|c| c.0).sum(),
        bytes_total: counts.iter().map(|c| c.1).sum(),
        ..Progress::default()
    };

    let mut errors = Vec::new();
    let mut overwrite_all = false;

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

        if target.exists() && !overwrite_all {
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

        copy_move_one(src, target, is_copy, &mut prog, tx, cancel, &mut errors);
    }

    let _ = tx.send(JobEvent::Finished { errors });
}

/// Execute one top-level (src, target) pair, reporting per-file progress.
fn copy_move_one(
    src: &Path,
    target: &Path,
    is_copy: bool,
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

    // Move: try a rename first; it is atomic and instant for same-filesystem moves.
    match std::fs::rename(src, target) {
        Ok(()) => {
            prog.current = name;
            prog.files_done += count_files(src).max(1);
            let _ = tx.send(JobEvent::Progress(prog.clone()));
        }
        Err(e) if is_cross_device(&e) => {
            // Fall back to copy + remove with progress.
            if let Err(e) = copy_tree(src, target, prog, tx, cancel) {
                errors.push(format!("{name}: {e}"));
                return;
            }
            if cancelled(cancel) {
                return;
            }
            let removed = if src.is_dir() {
                std::fs::remove_dir_all(src)
            } else {
                std::fs::remove_file(src)
            };
            if let Err(e) = removed {
                errors.push(format!("{name}: {e}"));
            }
        }
        Err(e) => errors.push(format!("{name}: {e}")),
    }
}

/// Recursively copy `src` to `dst`, reporting progress and honouring cancellation.
/// Mirrors `fileops::copy_dir_recursive` but per file, so a large copy stays
/// cancellable and the gauge advances smoothly.
fn copy_tree(
    src: &Path,
    dst: &Path,
    prog: &mut Progress,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::io::Result<()> {
    if cancelled(cancel) {
        return Ok(());
    }

    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        platform::copy_symlink(src, dst)?;
        bump(prog, src, 0, tx);
        return Ok(());
    }

    if meta.is_dir() {
        // Guard against copying a directory into itself.
        if let Ok(cs) = std::fs::canonicalize(src)
            && let Ok(cd) = std::fs::canonicalize(dst)
            && cd.starts_with(&cs)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot copy directory into itself",
            ));
        }
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            if cancelled(cancel) {
                return Ok(());
            }
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &dst.join(entry.file_name()),
                prog,
                tx,
                cancel,
            )?;
        }
        return Ok(());
    }

    std::fs::copy(src, dst)?;
    bump(prog, src, meta.len(), tx);
    Ok(())
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

fn run_command(cmd: &str, cwd: &Path, tx: &Sender<JobEvent>, cancel: &AtomicBool) {
    use std::io::Read;
    use std::process::Stdio;

    let _ = tx.send(JobEvent::Progress(Progress {
        current: cmd.to_string(),
        ..Progress::default()
    }));

    #[cfg(windows)]
    let mut builder = {
        let mut c = Command::new("cmd.exe");
        c.arg("/C").arg(cmd);
        c
    };
    #[cfg(not(windows))]
    let mut builder = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    builder
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match builder.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(JobEvent::Finished {
                errors: vec![format!("Failed to execute \"{cmd}\": {e}")],
            });
            return;
        }
    };

    // Drain stdout/stderr on their own threads so a chatty child cannot deadlock
    // by filling a pipe buffer while we poll for completion.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = stdout_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = stderr_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let mut killed = false;
    let status = loop {
        if cancelled(cancel) {
            let _ = child.kill();
            killed = true;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(e) => {
                let _ = tx.send(JobEvent::Finished {
                    errors: vec![format!("{cmd}: {e}")],
                });
                break None;
            }
        }
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    let Some(status) = status else { return };
    if killed {
        let _ = tx.send(JobEvent::Finished { errors: Vec::new() });
        return;
    }

    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr);
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    if text.is_empty() {
        text = format!("(exit code: {status})");
    }
    let code = status.code().unwrap_or(-1);
    let _ = tx.send(JobEvent::CommandOutput {
        command: cmd.to_string(),
        text,
        code,
    });
    let _ = tx.send(JobEvent::Finished { errors: Vec::new() });
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

/// Count files and total bytes under `path` (the path itself counts as one entry
/// when it is a file or symlink).
fn scan_tree(path: &Path) -> (u64, u64) {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (1, 0),
    };
    if meta.file_type().is_symlink() {
        return (1, 0);
    }
    if meta.is_dir() {
        let mut files = 0;
        let mut bytes = 0;
        if let Ok(rd) = std::fs::read_dir(path) {
            for entry in rd.flatten() {
                let (f, b) = scan_tree(&entry.path());
                files += f;
                bytes += b;
            }
        }
        (files, bytes)
    } else {
        (1, meta.len())
    }
}

fn count_files(path: &Path) -> u64 {
    scan_tree(path).0
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
