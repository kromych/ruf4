use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ruf4::fileops::{copy_dir_recursive, ops_build_pairs, ops_delete, ops_execute_all, ops_mkdir};
use ruf4::platform::run_command;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ruf4_integ_{}_{id}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_ops_mkdir_creates_nested() {
    let root = temp_dir();
    let target = root.join("a/b/c");

    assert!(ops_mkdir(&target).is_ok());
    assert!(target.is_dir());

    cleanup(&root);
}

#[test]
fn test_ops_delete_files_and_dirs() {
    let root = temp_dir();
    let file = root.join("file.txt");
    let dir = root.join("subdir");
    fs::write(&file, "hello").unwrap();
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("inner.txt"), "world").unwrap();

    let errors = ops_delete(&[file.clone(), dir.clone()]);
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(!file.exists());
    assert!(!dir.exists());

    cleanup(&root);
}

#[test]
fn test_ops_delete_nonexistent_returns_error() {
    let root = temp_dir();
    let missing = root.join("ghost.txt");

    let errors = ops_delete(&[missing]);
    assert_eq!(errors.len(), 1);

    cleanup(&root);
}

#[test]
fn test_ops_build_pairs_single_file_to_path() {
    let src = PathBuf::from("/src/file.txt");
    let pairs = ops_build_pairs(std::slice::from_ref(&src), "/dst/renamed.txt");

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, src);
    assert_eq!(pairs[0].1, PathBuf::from("/dst/renamed.txt"));
}

#[test]
fn test_ops_build_pairs_multiple_into_dir() {
    let root = temp_dir();
    let dest = root.join("dest");
    fs::create_dir(&dest).unwrap();

    let src_a = PathBuf::from("/src/a.txt");
    let src_b = PathBuf::from("/src/b.txt");
    let pairs = ops_build_pairs(&[src_a.clone(), src_b.clone()], dest.to_str().unwrap());

    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].1, dest.join("a.txt"));
    assert_eq!(pairs[1].1, dest.join("b.txt"));

    cleanup(&root);
}

#[test]
fn test_ops_execute_all_copy() {
    let root = temp_dir();
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    fs::write(&src, "data").unwrap();

    let errors = ops_execute_all(&[(src.clone(), dst.clone())], true);
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(src.exists(), "source should still exist after copy");
    assert_eq!(fs::read_to_string(&dst).unwrap(), "data");

    cleanup(&root);
}

#[test]
fn test_ops_execute_all_move() {
    let root = temp_dir();
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    fs::write(&src, "data").unwrap();

    let errors = ops_execute_all(&[(src.clone(), dst.clone())], false);
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(!src.exists(), "source should be gone after move");
    assert_eq!(fs::read_to_string(&dst).unwrap(), "data");

    cleanup(&root);
}

#[test]
fn test_ops_execute_all_same_file_error() {
    let root = temp_dir();
    let file = root.join("file.txt");
    fs::write(&file, "data").unwrap();

    let errors = ops_execute_all(&[(file.clone(), file.clone())], true);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("cannot copy file to itself"));

    cleanup(&root);
}

#[test]
fn test_copy_dir_recursive() {
    let root = temp_dir();
    let src = root.join("src_dir");
    let dst = root.join("dst_dir");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("a.txt"), "a").unwrap();
    fs::write(src.join("sub/b.txt"), "b").unwrap();

    copy_dir_recursive(&src, &dst).unwrap();
    assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dst.join("sub/b.txt")).unwrap(), "b");

    cleanup(&root);
}

#[test]
fn test_copy_dir_into_itself_fails() {
    let root = temp_dir();
    let dir = root.join("dir");
    fs::create_dir(&dir).unwrap();
    let sub = dir.join("sub");

    // copy dir into dir/sub should fail
    let result = copy_dir_recursive(&dir, &sub);
    assert!(result.is_err());

    cleanup(&root);
}

#[test]
fn test_run_command() {
    let root = temp_dir();
    // "echo" works on all platforms (sh -c on unix, cmd /C on windows).
    let result = run_command("echo hello", &root);
    assert!(result.is_ok());
    let (output, code) = result.unwrap();
    assert!(output.contains("hello"));
    assert_eq!(code, 0);

    cleanup(&root);
}

#[test]
fn test_ops_run_command_failure() {
    let root = temp_dir();
    // Use a cross-platform command that exits non-zero.
    #[cfg(unix)]
    let cmd = "false";
    #[cfg(windows)]
    let cmd = "cmd /C exit 1";
    #[cfg(not(any(unix, windows)))]
    let cmd = "false";
    let result = run_command(cmd, &root);
    assert!(result.is_ok());
    let (_output, code) = result.unwrap();
    assert_ne!(code, 0);

    cleanup(&root);
}

#[test]
fn test_ops_mkdir_empty_path_still_works() {
    // create_dir_all("") is a no-op on some platforms (macOS) and an error
    // on others. Just verify it doesn't panic.
    let _ = ops_mkdir(Path::new(""));
}

#[test]
fn test_overwrite_flow_no_conflict() {
    let root = temp_dir();
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    fs::write(&src, "data").unwrap();

    // No conflict: dst doesn't exist, so it should just copy.
    let pairs = vec![(src.clone(), dst.clone())];
    let errors = ops_execute_all(&pairs, true);
    assert!(errors.is_empty());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "data");

    cleanup(&root);
}

#[test]
fn test_overwrite_existing_file() {
    let root = temp_dir();
    let src = root.join("src.txt");
    let dst = root.join("dst.txt");
    fs::write(&src, "new").unwrap();
    fs::write(&dst, "old").unwrap();

    // ops_execute_all overwrites unconditionally.
    let errors = ops_execute_all(&[(src.clone(), dst.clone())], true);
    assert!(errors.is_empty());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "new");

    cleanup(&root);
}
