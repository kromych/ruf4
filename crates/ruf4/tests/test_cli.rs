// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Command line options, which are handled before the TUI starts.

use std::process::{Command, Output};

fn ruf4(arg: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruf4"))
        .arg(arg)
        .output()
        .expect("run ruf4")
}

#[test]
fn test_version() {
    let expected = concat!("ruf4 ", env!("CARGO_PKG_VERSION"), "\n");
    for arg in ["--version", "-V", "-v"] {
        let out = ruf4(arg);
        assert!(out.status.success(), "{arg}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{arg}");
    }
}

#[test]
fn test_help() {
    for arg in ["--help", "-h"] {
        let out = ruf4(arg);
        assert!(out.status.success(), "{arg}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.starts_with("Usage: ruf4"), "{text}");
        assert!(text.contains("--version"), "{text}");
    }
}

#[test]
fn test_unknown_argument() {
    let out = ruf4("--bogus");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("unknown argument: --bogus"), "{text}");
}
