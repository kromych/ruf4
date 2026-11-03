use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use ruf4::lsh::HighlightKind;
use ruf4::preview::{self, Preview};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Previews `contents` as a file called `name`, cleaning up afterwards.
fn preview_of(name: &str, contents: &[u8]) -> Preview {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir: PathBuf =
        std::env::temp_dir().join(format!("ruf4_preview_{}_{id}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    let p = preview::generate(&path);
    let _ = fs::remove_dir_all(&dir);
    p
}

fn utf16(text: &str, to_bytes: fn(u16) -> [u8; 2]) -> Vec<u8> {
    let mut bytes = to_bytes(0xfeff).to_vec();
    for u in text.encode_utf16() {
        bytes.extend_from_slice(&to_bytes(u));
    }
    bytes
}

#[test]
fn test_text_preview_strips_bom() {
    let p = preview_of("bom.log", b"\xef\xbb\xbf*****\r\nline two\r\n");
    assert_eq!(p.lines, ["*****", "line two"]);
}

#[test]
fn test_text_preview_expands_tabs() {
    let p = preview_of("tabs.txt", "a\tb\n\tc\n\u{4e2d}\tx\n".as_bytes());
    assert_eq!(p.lines, ["a       b", "        c", "\u{4e2d}      x"]);
}

#[test]
fn test_tab_expansion_moves_highlights() {
    let p = preview_of("tabs.sh", b"\techo\t$HOME\n");
    assert_eq!(p.lines, ["        echo    $HOME"]);
    let var = p.highlights[0]
        .iter()
        .find(|s| s.kind == HighlightKind::Variable)
        .expect("$HOME is highlighted");
    assert_eq!(&p.lines[0][var.start..], "$HOME");
}

#[test]
fn test_tab_indented_heredoc_terminator_ends_string() {
    // `<<-` heredocs may indent their terminator with tabs; the grammar
    // matches the tabs, so highlighting must see them before expansion.
    let p = preview_of("heredoc.sh", b"cat <<-EOF\n\tbody\n\tEOF\necho $HOME\n");
    assert!(
        p.highlights[3]
            .iter()
            .any(|s| s.kind == HighlightKind::Variable),
        "line after the heredoc is still highlighted as a string"
    );
}

#[test]
fn test_text_preview_decodes_utf16le_with_bom() {
    // What Windows PowerShell 5.1's `>` redirection writes.
    let p = preview_of(
        "utf16le.log",
        &utf16("Привет\r\nline two\r\n", u16::to_le_bytes),
    );
    assert!(!p.is_binary);
    assert_eq!(p.lines, ["Привет", "line two"]);
}

#[test]
fn test_text_preview_decodes_utf16be_with_bom() {
    let p = preview_of("utf16be.txt", &utf16("big\nendian 👍\n", u16::to_be_bytes));
    assert!(!p.is_binary);
    assert_eq!(p.lines, ["big", "endian 👍"]);
}

#[test]
fn test_utf16_bom_with_nul_is_binary() {
    let p = preview_of("fake.bin", &[0xff, 0xfe, 0x00, 0x00, 0x41, 0x00]);
    assert!(p.is_binary);
}

#[test]
fn test_utf16_bom_with_unpaired_surrogate_is_binary() {
    // No aligned NUL anywhere, but U+D801 is never followed by a low surrogate.
    let p = preview_of(
        "fake.bin",
        &[0xff, 0xfe, 0x01, 0xd8, 0x41, 0x42, 0x43, 0x44],
    );
    assert!(p.is_binary);
}

#[test]
fn test_nul_bytes_without_bom_are_binary() {
    let p = preview_of("nul.bin", b"vhdxfile\x00M\x00i\x00");
    assert!(p.is_binary);
}

#[test]
fn test_nul_after_check_window_is_text() {
    let mut bytes = "Привет ".repeat(60).into_bytes();
    assert!(bytes.len() > 512);
    bytes.push(0);
    let p = preview_of("late_nul.txt", &bytes);
    assert!(!p.is_binary);
}
