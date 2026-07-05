// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Tests for the vfs path model: `ssh://` parsing, path algebra, ssh_config
//! host extraction, and the pure dispatch predicates. Nothing here opens a
//! connection.

use std::path::{Path, PathBuf};

use ruf4::fileops;
use ruf4::sftp::Attrs;
use ruf4::vfs::{
    self, dir_contains, is_remote, join, normalize_posix, parent, parse_remote, remote_entry,
    same_domain, shell_quote, ssh_config_hosts,
};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ── Parsing ─────────────────────────────────────────────────────────────────

#[test]
fn parse_full_remote_path() {
    let r = parse_remote(&p("ssh://alice@box:2222/var/log")).unwrap();
    assert_eq!(r.host.user.as_deref(), Some("alice"));
    assert_eq!(r.host.host, "box");
    assert_eq!(r.host.port, Some(2222));
    assert_eq!(r.path, "/var/log");
    assert_eq!(r.to_path_buf(), p("ssh://alice@box:2222/var/log"));
}

#[test]
fn parse_host_only() {
    let r = parse_remote(&p("ssh://box")).unwrap();
    assert_eq!(r.host.user, None);
    assert_eq!(r.host.port, None);
    assert_eq!(r.path, "");
}

#[test]
fn parse_ipv6_host() {
    let r = parse_remote(&p("ssh://[::1]:2200/tmp")).unwrap();
    assert_eq!(r.host.host, "::1");
    assert_eq!(r.host.port, Some(2200));
    assert_eq!(r.path, "/tmp");
    // Display round-trips with brackets.
    assert_eq!(r.to_path_buf(), p("ssh://[::1]:2200/tmp"));
}

#[test]
fn parse_rejects_non_remote_and_malformed() {
    assert!(parse_remote(Path::new("/usr/local")).is_none());
    assert!(parse_remote(Path::new("ssh://")).is_none());
    assert!(parse_remote(Path::new("ssh:///path")).is_none());
    assert!(!is_remote(Path::new("/ssh://host")));
    assert!(is_remote(Path::new("ssh://host/")));
}

#[test]
fn parse_nonnumeric_port_is_part_of_no_port() {
    // ssh_config aliases cannot contain ':'; treat a non-numeric suffix as
    // part of the host rather than failing.
    let r = parse_remote(&p("ssh://weird:alias/x")).unwrap();
    assert_eq!(r.host.host, "weird:alias");
    assert_eq!(r.host.port, None);
}

// ── Path algebra ────────────────────────────────────────────────────────────

#[test]
fn normalize_posix_collapses() {
    assert_eq!(normalize_posix(""), "/");
    assert_eq!(normalize_posix("/"), "/");
    assert_eq!(normalize_posix("/a//b/./c"), "/a/b/c");
    assert_eq!(normalize_posix("/a/b/../c"), "/a/c");
    assert_eq!(normalize_posix("/a/../../b"), "/b");
}

#[test]
fn join_remote_relative_absolute_and_dotdot() {
    let base = p("ssh://box/home/user");
    assert_eq!(join(&base, "docs"), p("ssh://box/home/user/docs"));
    assert_eq!(join(&base, "/etc"), p("ssh://box/etc"));
    assert_eq!(join(&base, ".."), p("ssh://box/home"));
    assert_eq!(join(&p("ssh://box/"), "x"), p("ssh://box/x"));
}

#[test]
fn join_local_still_joins() {
    let joined = join(Path::new("/tmp"), "file.txt");
    assert_eq!(joined, Path::new("/tmp").join("file.txt"));
}

#[test]
fn parent_remote_chain_ends_at_root() {
    let start = p("ssh://alice@box/a/b");
    let up1 = parent(&start).unwrap();
    assert_eq!(up1, p("ssh://alice@box/a"));
    let up2 = parent(&up1).unwrap();
    assert_eq!(up2, p("ssh://alice@box/"));
    assert_eq!(parent(&up2), None);
}

// ── Dispatch predicates ─────────────────────────────────────────────────────

#[test]
fn same_domain_and_same_file_remote() {
    let a = p("ssh://box/a/b/../c");
    let b = p("ssh://box/a/c");
    let other = p("ssh://other/a/c");
    assert!(same_domain(&a, &b));
    assert!(!same_domain(&a, &other));
    assert!(!same_domain(&a, Path::new("/a/c")));
    assert!(fileops::same_file(&a, &b));
    assert!(!fileops::same_file(&a, &other));
}

#[test]
fn dir_contains_remote() {
    assert!(dir_contains(&p("ssh://box/a"), &p("ssh://box/a/b")));
    assert!(dir_contains(&p("ssh://box/a"), &p("ssh://box/a")));
    assert!(dir_contains(&p("ssh://box/"), &p("ssh://box/x")));
    assert!(!dir_contains(&p("ssh://box/a"), &p("ssh://box/ab")));
    assert!(!dir_contains(&p("ssh://box/a"), &p("ssh://other/a/b")));
    assert!(!dir_contains(&p("ssh://box/a"), Path::new("/a/b")));
}

#[test]
fn dir_contains_local() {
    let root = std::env::temp_dir().join(format!("ruf4_vfs_{}", std::process::id()));
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(dir_contains(&root, &sub));
    assert!(dir_contains(&root, &sub.join("not-yet-created")));
    assert!(!dir_contains(&sub, &root));
    let _ = std::fs::remove_dir_all(&root);
}

// ── Remote entry mapping ────────────────────────────────────────────────────

#[test]
fn remote_entry_maps_attrs() {
    let attrs = Attrs {
        size: Some(1234),
        uid_gid: Some((501, 20)),
        permissions: Some(0o100755),
        times: Some((0, 1_700_000_000)),
    };
    let e = remote_entry("run.sh".to_string(), false, &attrs);
    assert!(!e.is_dir);
    assert!(e.is_executable);
    assert!(!e.is_readonly);
    assert!(!e.is_hidden);
    assert_eq!(e.size, 1234);
    assert!(e.modified.is_some());

    let dir_attrs = Attrs {
        permissions: Some(0o040555), // no write bits
        ..Attrs::default()
    };
    let d = remote_entry(".config".to_string(), false, &dir_attrs);
    assert!(d.is_dir);
    assert!(d.is_hidden);
    assert!(d.is_readonly);
    assert!(!d.is_executable);
}

// ── cd resolution ───────────────────────────────────────────────────────────

#[test]
fn resolve_cd_target_remote() {
    let cwd = p("ssh://box/home/user");
    assert_eq!(
        fileops::resolve_cd_target(&cwd, "docs"),
        p("ssh://box/home/user/docs")
    );
    assert_eq!(fileops::resolve_cd_target(&cwd, ".."), p("ssh://box/home"));
    assert_eq!(fileops::resolve_cd_target(&cwd, "/etc"), p("ssh://box/etc"));
    // Home: the empty path resolves to the remote login home on connect.
    assert_eq!(fileops::resolve_cd_target(&cwd, "~"), p("ssh://box"));
    // Switching hosts by URL works from anywhere.
    assert_eq!(
        fileops::resolve_cd_target(&cwd, "ssh://other/x"),
        p("ssh://other/x")
    );
    assert_eq!(
        fileops::resolve_cd_target(Path::new("/tmp"), "ssh://box"),
        p("ssh://box")
    );
}

#[test]
fn resolve_cd_target_local() {
    assert_eq!(
        fileops::resolve_cd_target(Path::new("/tmp"), "sub"),
        Path::new("/tmp").join("sub")
    );
    assert_eq!(
        fileops::resolve_cd_target(Path::new("/tmp"), "/var"),
        p("/var")
    );
}

// ── Shell quoting ───────────────────────────────────────────────────────────

#[test]
fn shell_quote_escapes_single_quotes() {
    assert_eq!(shell_quote("plain"), "'plain'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote("a b;c"), "'a b;c'");
}

// ── ssh_config parsing ──────────────────────────────────────────────────────

#[test]
fn ssh_config_hosts_extracts_concrete_aliases() {
    let text = "\
# comment
Host dev
    HostName dev.example.com

host prod staging
Host *.internal !bastion
Host \"quoted\"
Host=eqform
Match host something
";
    let hosts = ssh_config_hosts(text);
    assert_eq!(hosts, vec!["dev", "prod", "staging", "quoted", "eqform"]);
}

#[test]
fn ssh_config_hosts_dedupes_and_skips_patterns() {
    let text = "Host a\nHost a b\nHost b?x c*\n";
    let hosts = ssh_config_hosts(text);
    assert_eq!(hosts, vec!["a", "b"]);
}

#[test]
fn ssh_roots_form() {
    // Independent of the machine's real config: every returned root must be a
    // parseable remote path with no path component.
    for root in vfs::ssh_roots() {
        let r = parse_remote(&root).expect("ssh root must parse");
        assert_eq!(r.path, "");
    }
}

// ── Live end-to-end (env-gated) ─────────────────────────────────────────────

/// Full-stack pass against a live SSH destination. Runs only when
/// `RUF4_SSH_E2E_DEST` names one (e.g. `ssh://127.0.0.1:2299`); pair with
/// `RUF4_SSH_CONFIG` pointing at a client config that authenticates
/// non-interactively.
#[test]
fn live_ssh_end_to_end() {
    let Ok(dest) = std::env::var("RUF4_SSH_E2E_DEST") else {
        return;
    };
    let root = vfs::prepare_dir(Path::new(&dest)).expect("connect and resolve home");
    let r = parse_remote(&root).unwrap();
    assert!(r.path.starts_with('/'), "home must be absolute: {r:?}");

    // Panel listing of the home directory works.
    let _ = vfs::scan_dir(&root, true);

    // Work in an isolated subtree.
    let work = join(&root, &format!("ruf4-e2e-{}", std::process::id()));
    vfs::create_dir_all(&work).expect("mkdir");
    assert!(vfs::is_dir(&work));

    // Write, stat, and read back a file crossing the chunk size.
    let file = join(&work, "data.bin");
    let payload: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
    {
        let mut w = vfs::open_write(&file).expect("open for write");
        w.write_all(&payload).unwrap();
        w.flush().unwrap();
    }
    assert!(vfs::exists(&file));
    let meta = vfs::symlink_meta(&file).unwrap();
    assert_eq!(meta.size, payload.len() as u64);
    let (prefix, size) = vfs::read_prefix(&file, 4096).unwrap();
    assert_eq!(size, payload.len() as u64);
    assert_eq!(prefix, payload[..4096]);
    let mut back = Vec::new();
    vfs::open_read(&file)
        .expect("open for read")
        .read_to_end(&mut back)
        .unwrap();
    assert_eq!(back, payload);

    // Directory metadata listing and rename.
    let listed = vfs::read_dir_meta(&work).unwrap();
    assert!(
        listed
            .iter()
            .any(|(n, m)| n == "data.bin" && m.kind == vfs::Kind::File)
    );
    let renamed = join(&work, "renamed.bin");
    vfs::rename(&file, &renamed).unwrap();
    assert!(!vfs::exists(&file));
    assert!(vfs::exists(&renamed));

    // Free space is reported by OpenSSH servers.
    assert!(vfs::free_space(&work).is_some());

    // Recursive removal cleans up.
    vfs::remove_tree(&work).unwrap();
    assert!(!vfs::exists(&work));
}
