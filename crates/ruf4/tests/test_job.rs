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
fn command_captures_output() {
    let cwd = temp_dir();
    let mut job = job::spawn_command("echo job-output".to_string(), cwd.clone());

    let errors = drive(&mut job, no_overwrite);
    assert!(errors.is_empty(), "errors: {errors:?}");

    let (_cmd, text, code) = job.command_output.clone().expect("command output");
    assert!(text.contains("job-output"), "got: {text:?}");
    assert_eq!(code, 0);
    cleanup(&cwd);
}
