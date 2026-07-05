// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Tests for the SMB location model: `smb://` parsing, UNC and GVFS name
//! mapping, and cd/url dispatch. Nothing here mounts a share.

use std::path::{Path, PathBuf};

use ruf4::smb::{self, SmbPath, gvfs_entry_name, parse_gvfs_entry_name, unc_path};
use ruf4::{fileops, vfs};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ── Parsing ─────────────────────────────────────────────────────────────────

#[test]
fn parse_full_smb_url() {
    let s = smb::parse(&p("smb://alice@nas:1445/media/movies/2024")).unwrap();
    assert_eq!(s.user.as_deref(), Some("alice"));
    assert_eq!(s.host, "nas");
    assert_eq!(s.port, Some(1445));
    assert_eq!(s.share, "media");
    assert_eq!(s.path, "/movies/2024");
    assert_eq!(s.display(), "smb://alice@nas:1445/media/movies/2024");
}

#[test]
fn parse_share_only_and_host_only() {
    let s = smb::parse(&p("smb://nas/media")).unwrap();
    assert_eq!(s.share, "media");
    assert_eq!(s.path, "");
    assert_eq!(s.display(), "smb://nas/media");

    let bare = smb::parse(&p("smb://nas")).unwrap();
    assert_eq!(bare.share, "");
    assert_eq!(bare.display(), "smb://nas");
    // A bare host cannot resolve; the error asks for a share.
    let err = smb::resolve_dir(&bare).unwrap_err();
    assert!(err.contains("share name required"), "{err}");
}

#[test]
fn parse_normalizes_subpath() {
    let s = smb::parse(&p("smb://nas/media//a/./b/../c")).unwrap();
    assert_eq!(s.path, "/a/c");
    let root = smb::parse(&p("smb://nas/media/")).unwrap();
    assert_eq!(root.path, "");
}

#[test]
fn parse_rejects_other_schemes_and_malformed() {
    assert!(smb::parse(Path::new("/local/path")).is_none());
    assert!(smb::parse(Path::new("ssh://host/x")).is_none());
    assert!(smb::parse(Path::new("smb://")).is_none());
    assert!(smb::parse(Path::new("smb:///share")).is_none());
}

#[test]
fn parse_ipv6_host() {
    let s = smb::parse(&p("smb://[fe80::1]:445/backup")).unwrap();
    assert_eq!(s.host, "fe80::1");
    assert_eq!(s.port, Some(445));
    assert_eq!(s.display(), "smb://[fe80::1]:445/backup");
}

// ── UNC mapping ─────────────────────────────────────────────────────────────

#[test]
fn unc_path_from_url() {
    let s = smb::parse(&p("smb://nas/media/movies/2024")).unwrap();
    assert_eq!(unc_path(&s), p(r"\\nas\media\movies\2024"));
    let root = smb::parse(&p("smb://nas/media")).unwrap();
    assert_eq!(unc_path(&root), p(r"\\nas\media"));
}

// ── GVFS entry names ────────────────────────────────────────────────────────

#[test]
fn gvfs_name_round_trip() {
    let s = smb::parse(&p("smb://alice@NAS:1445/Media")).unwrap();
    let name = gvfs_entry_name(&s);
    assert_eq!(
        name,
        "smb-share:server=nas,share=media,port=1445,user=alice"
    );
    let parsed = parse_gvfs_entry_name(&name).unwrap();
    assert_eq!(parsed.host, "nas");
    assert_eq!(parsed.share, "media");
    assert_eq!(parsed.port, Some(1445));
    assert_eq!(parsed.user.as_deref(), Some("alice"));
}

#[test]
fn gvfs_name_parse_tolerates_extra_fields_and_rejects_others() {
    let parsed = parse_gvfs_entry_name("smb-share:server=nas,share=media,flags=1").unwrap();
    assert_eq!(parsed.host, "nas");
    assert_eq!(parsed.share, "media");
    assert!(parse_gvfs_entry_name("sftp:host=x").is_none());
    assert!(parse_gvfs_entry_name("smb-share:server=nas").is_none()); // no share
}

// ── Dispatch ────────────────────────────────────────────────────────────────

#[test]
fn is_url_covers_both_schemes() {
    assert!(vfs::is_url("ssh://host/x"));
    assert!(vfs::is_url("smb://host/share"));
    assert!(!vfs::is_url("/usr/local"));
    assert!(!vfs::is_url("http://host/x"));
}

#[test]
fn cd_accepts_smb_urls_from_anywhere() {
    assert_eq!(
        fileops::resolve_cd_target(Path::new("/tmp"), "smb://nas/media"),
        p("smb://nas/media")
    );
    assert_eq!(
        fileops::resolve_cd_target(&p("ssh://box/home"), "smb://nas/media"),
        p("smb://nas/media")
    );
}

#[test]
fn mounted_roots_is_well_formed() {
    // Machine-dependent contents; every entry must at least be an absolute
    // local directory path, not a URL.
    for root in smb::mounted_roots() {
        assert!(root.is_absolute());
        assert!(smb::parse(&root).is_none());
    }
}

#[test]
fn display_round_trips_through_parse() {
    for url in [
        "smb://nas/media",
        "smb://alice@nas/media/sub",
        "smb://nas:139/media",
        "smb://[::1]/share",
    ] {
        let s = smb::parse(Path::new(url)).unwrap();
        assert_eq!(smb::parse(Path::new(&s.display())), Some(s));
    }
}

/// SmbPath is constructible for callers building locations programmatically.
#[test]
fn constructed_paths_display() {
    let s = SmbPath {
        user: None,
        host: "nas".to_string(),
        port: None,
        share: "media".to_string(),
        path: "/x".to_string(),
    };
    assert_eq!(s.display(), "smb://nas/media/x");
}

// ── Live end-to-end (env-gated) ─────────────────────────────────────────────

/// Full mount-and-browse pass against a live share. Runs only when
/// `RUF4_SMB_E2E_URL` names one (e.g. `smb://user@host/share`); credentials
/// must be available non-interactively or entered at the prompt.
#[test]
fn live_smb_end_to_end() {
    let Ok(url) = std::env::var("RUF4_SMB_E2E_URL") else {
        return;
    };
    let dir = vfs::prepare_dir(Path::new(&url)).expect("mount and resolve");
    assert!(dir.is_dir());
    // After resolution everything is ordinary local I/O.
    std::fs::read_dir(&dir).expect("list mounted share");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    assert!(
        smb::mounted_roots().iter().any(|r| dir.starts_with(r)),
        "mounted share must appear in the roots list"
    );
}
