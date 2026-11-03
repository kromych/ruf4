// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Integration tests for background jobs. Each test spawns a worker and drives
//! it to completion off the UI thread, mirroring what `State::poll_job` does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ruf4::job::{self, Decision, Job};
use ruf4::panel::{Panel, make_entry};
use ruf4::state::{Dialog, State};
use ruf4_tui::input::{Input, vk};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ruf4_job_{}_{id}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

/// Drive a job to completion, answering any overwrite prompt via `on_overwrite`.
fn drive(job: &mut Job, mut on_overwrite: impl FnMut() -> Decision) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(errors) = job.poll() {
            return errors;
        }
        if job.awaiting_overwrite.is_some() {
            let d = on_overwrite();
            job.answer_overwrite(d);
        }
        assert!(Instant::now() < deadline, "job did not finish in time");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn no_overwrite() -> Decision {
    panic!("unexpected overwrite prompt");
}

#[test]
fn delete_removes_files_and_dirs() {
    let root = temp_dir();
    let file = root.join("a.txt");
    let dir = root.join("sub");
    fs::write(&file, b"x").unwrap();
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("b.txt"), b"y").unwrap();

    let mut job = job::spawn_delete(vec![file.clone(), dir.clone()]);
    let errors = drive(&mut job, no_overwrite);

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(!file.exists());
    assert!(!dir.exists());
    cleanup(&root);
}

#[test]
fn copy_duplicates_a_tree() {
    let root = temp_dir();
    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("f1"), b"hello").unwrap();
    fs::create_dir(src.join("nested")).unwrap();
    fs::write(src.join("nested/f2"), b"world").unwrap();
    let dst = root.join("dst");

    let mut job = job::spawn_copy_move(vec![(src.clone(), dst.clone())], true);
    let errors = drive(&mut job, no_overwrite);

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(src.join("f1").exists(), "source preserved on copy");
    assert_eq!(fs::read(dst.join("f1")).unwrap(), b"hello");
    assert_eq!(fs::read(dst.join("nested/f2")).unwrap(), b"world");
    // Pre-scan totals were reported.
    assert_eq!(job.progress.files_total, 2);
    assert_eq!(job.progress.files_done, 2);
    cleanup(&root);
}

#[test]
fn move_relocates_a_file() {
    let root = temp_dir();
    let src = root.join("a.txt");
    fs::write(&src, b"data").unwrap();
    let dst = root.join("b.txt");

    let mut job = job::spawn_copy_move(vec![(src.clone(), dst.clone())], false);
    let errors = drive(&mut job, no_overwrite);

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(!src.exists(), "source removed on move");
    assert_eq!(fs::read(&dst).unwrap(), b"data");
    cleanup(&root);
}

#[test]
fn overwrite_prompt_skip_keeps_target() {
    let root = temp_dir();
    let src = root.join("a.txt");
    let dst = root.join("b.txt");
    fs::write(&src, b"new").unwrap();
    fs::write(&dst, b"old").unwrap();

    let mut job = job::spawn_copy_move(vec![(src, dst.clone())], true);
    let errors = drive(&mut job, || Decision::Skip);

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(fs::read(&dst).unwrap(), b"old", "skip preserves target");
    cleanup(&root);
}

#[test]
fn overwrite_prompt_overwrite_replaces_target() {
    let root = temp_dir();
    let src = root.join("a.txt");
    let dst = root.join("b.txt");
    fs::write(&src, b"new").unwrap();
    fs::write(&dst, b"old").unwrap();

    let mut job = job::spawn_copy_move(vec![(src, dst.clone())], true);
    let errors = drive(&mut job, || Decision::Overwrite);

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(fs::read(&dst).unwrap(), b"new", "overwrite replaces target");
    cleanup(&root);
}

/// Drive `State::poll_job` until the background job clears the progress dialog.
fn poll_state_to_idle(state: &mut State) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(
        state.dialog,
        Dialog::Progress { .. } | Dialog::ConfirmOverwrite { .. }
    ) || state.job_active()
    {
        state.poll_job();
        assert!(Instant::now() < deadline, "state job did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn state_delete_flow_runs_and_refreshes() {
    let root = temp_dir();
    let file = root.join("doomed.txt");
    fs::write(&file, b"bye").unwrap();

    let left = Panel::with_entries(
        root.clone(),
        vec![
            make_entry("..", true, 0),
            make_entry("doomed.txt", false, 3),
        ],
    );
    let right = Panel::with_entries(root.clone(), vec![make_entry("..", true, 0)]);
    let mut state = State::for_testing(left, right);
    state.active_panel_mut().cursor = 1; // "doomed.txt"

    // Delete key opens the confirm dialog; Enter confirms and spawns the job.
    state.handle_global_input(&Input::Keyboard(vk::DELETE));
    assert!(matches!(state.dialog, Dialog::Delete { .. }));
    state.handle_global_input(&Input::Keyboard(vk::RETURN));
    assert!(state.job_active(), "delete should have spawned a job");

    poll_state_to_idle(&mut state);

    assert!(!file.exists(), "file should be deleted");
    assert!(
        matches!(state.dialog, Dialog::None),
        "dialog should close on success"
    );
    // Panel was refreshed: the deleted entry is gone.
    assert!(
        !state
            .active_panel()
            .entries
            .iter()
            .any(|e| e.name == "doomed.txt")
    );
    cleanup(&root);
}

#[test]
fn download_copies_and_overwrites_silently() {
    let root = temp_dir();
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    fs::write(&src, b"payload").unwrap();
    // The target exists; a download must not raise an overwrite prompt.
    fs::write(&dst, b"old").unwrap();

    let mut job = job::spawn_download(src.clone(), dst.clone());
    assert_eq!(job.kind.title(), "Downloading");
    let errors = drive(&mut job, no_overwrite);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(fs::read(&dst).unwrap(), b"payload");
    assert_eq!(fs::read(&src).unwrap(), b"payload");
    cleanup(&root);
}

#[test]
fn nested_conflict_prompts_and_skip_keeps_source_on_move() {
    let root = temp_dir();
    let src = root.join("proj");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("sub/a.txt"), b"new").unwrap();
    fs::write(src.join("b.txt"), b"b").unwrap();
    // The destination already has the folder, with a conflicting nested file.
    let dst = root.join("dest/proj");
    fs::create_dir_all(dst.join("sub")).unwrap();
    fs::write(dst.join("sub/a.txt"), b"old").unwrap();

    let mut prompts = Vec::new();
    let mut job = job::spawn_copy_move(vec![(src.clone(), dst.clone())], false);
    let deadline = Instant::now() + Duration::from_secs(10);
    let errors = loop {
        if let Some(errors) = job.poll() {
            break errors;
        }
        if let Some(name) = job.awaiting_overwrite.clone() {
            prompts.push(name);
            job.answer_overwrite(Decision::Skip);
        }
        assert!(Instant::now() < deadline, "job did not finish in time");
        std::thread::sleep(Duration::from_millis(2));
    };

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(
        prompts.len(),
        1,
        "only the nested file conflicts: {prompts:?}"
    );
    assert!(prompts[0].ends_with("a.txt"), "{prompts:?}");
    assert_eq!(fs::read(dst.join("sub/a.txt")).unwrap(), b"old");
    assert_eq!(fs::read(dst.join("b.txt")).unwrap(), b"b");
    // The skipped file stays at the source, with the folders leading to it.
    assert_eq!(fs::read(src.join("sub/a.txt")).unwrap(), b"new");
    assert!(!src.join("b.txt").exists());
    cleanup(&root);
}

#[test]
fn overwrite_all_answers_every_later_conflict() {
    let root = temp_dir();
    let src = root.join("src");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("a"), b"new").unwrap();
    fs::write(src.join("sub/b"), b"new").unwrap();
    let dst = root.join("dst");
    fs::create_dir_all(dst.join("sub")).unwrap();
    fs::write(dst.join("a"), b"old").unwrap();
    fs::write(dst.join("sub/b"), b"old").unwrap();

    let mut prompts = 0;
    let mut job = job::spawn_copy_move(vec![(src, dst.clone())], true);
    let errors = drive(&mut job, || {
        prompts += 1;
        Decision::OverwriteAll
    });

    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(prompts, 1);
    assert_eq!(fs::read(dst.join("a")).unwrap(), b"new");
    assert_eq!(fs::read(dst.join("sub/b")).unwrap(), b"new");
    cleanup(&root);
}

#[cfg(unix)]
#[test]
fn copy_skips_fifos_instead_of_blocking_on_them() {
    let root = temp_dir();
    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("plain"), b"x").unwrap();
    let fifo = std::ffi::CString::new(src.join("pipe").to_str().unwrap()).unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let dst = root.join("dst");

    let mut job = job::spawn_copy_move(vec![(src, dst.clone())], true);
    let errors = drive(&mut job, no_overwrite);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("not a regular file"), "{errors:?}");
    assert_eq!(fs::read(dst.join("plain")).unwrap(), b"x");
    assert!(!dst.join("pipe").exists());
    cleanup(&root);
}

/// Moving `Makefile` and `makefile` from a case-sensitive filesystem onto a
/// case-insensitive one must not let one replace the other and then delete
/// both sources. Needs such a pair of filesystems, e.g. under WSL a Linux
/// temp dir and `RUF4_CASE_INSENSITIVE_DIR=/mnt/c/...`.
#[test]
fn move_onto_case_insensitive_destination_keeps_colliding_source() {
    let Ok(insensitive) = std::env::var("RUF4_CASE_INSENSITIVE_DIR") else {
        return;
    };
    let root = temp_dir();
    let src = root.join("proj");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("Makefile"), b"upper").unwrap();
    fs::write(src.join("makefile"), b"lower").unwrap();
    if fs::read(src.join("Makefile")).unwrap() != b"upper" {
        cleanup(&root);
        return; // The temp dir isn't case-sensitive either.
    }
    let dst_root = PathBuf::from(insensitive).join(format!("ruf4_case_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dst_root);
    fs::create_dir_all(&dst_root).unwrap();
    let dst = dst_root.join("proj");

    let mut job = job::spawn_copy_move(vec![(src.clone(), dst.clone())], false);
    let errors = drive(&mut job, no_overwrite);

    let names: Vec<_> = fs::read_dir(&dst)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let kept: Vec<_> = fs::read_dir(&src)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let _ = fs::remove_dir_all(&dst_root);
    cleanup(&root);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("ignores case"), "{errors:?}");
    assert_eq!(names.len(), 1, "{names:?}");
    // Whichever came second is still at the source, the other one moved.
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_ne!(kept[0], names[0]);
}
