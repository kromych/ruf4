// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! File preview for the quick-view panel.

use std::borrow::Cow;
use std::path::Path;

use crate::lsh::{self, HighlightKind};
use crate::vfs;

const MAX_PREVIEW_BYTES: usize = 64 * 1024;
const BINARY_CHECK_BYTES: usize = 512;
// What terminals, `less`, and the TUI's own text measurement use.
const TAB_WIDTH: usize = 8;

#[derive(Clone)]
pub struct HighlightSpan {
    pub start: usize,
    pub kind: HighlightKind,
}

pub struct Preview {
    pub lines: Vec<String>,
    pub highlights: Vec<Vec<HighlightSpan>>,
    pub is_binary: bool,
    pub file_size: u64,
    pub title: String,
}

impl Preview {
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            highlights: Vec::new(),
            is_binary: false,
            file_size: 0,
            title: String::new(),
        }
    }
}

pub fn generate(path: &Path) -> Preview {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    if vfs::is_dir(path) {
        return dir_preview(path, &name);
    }

    // Opening a FIFO would block the UI until something writes to it.
    if vfs::stat_meta(path).is_ok_and(|m| m.kind == vfs::Kind::Other) {
        return Preview {
            lines: vec!["Not a regular file (pipe, socket or device)".to_string()],
            highlights: Vec::new(),
            is_binary: false,
            file_size: 0,
            title: name,
        };
    }

    // TODO: remote previews block the UI thread for the duration of the read.
    let (buf, file_size) = match vfs::read_prefix(path, MAX_PREVIEW_BYTES) {
        Ok(v) => v,
        Err(e) => {
            return Preview {
                lines: vec![format!("Error: {e}")],
                highlights: Vec::new(),
                is_binary: false,
                file_size: 0,
                title: name,
            };
        }
    };

    match decode_text(&buf) {
        Some(text) => text_preview(&text, file_size, &name, path),
        None => hex_preview(&buf, file_size, &name),
    }
}

/// Decode a file prefix as text, honoring a UTF-8 or UTF-16 BOM.
/// Returns `None` if the data looks binary.
fn decode_text(buf: &[u8]) -> Option<Cow<'_, str>> {
    let from_bytes: fn([u8; 2]) -> u16 = match buf {
        [0xff, 0xfe, ..] => u16::from_le_bytes,
        [0xfe, 0xff, ..] => u16::from_be_bytes,
        _ => {
            let text = buf.strip_prefix(b"\xef\xbb\xbf").unwrap_or(buf);
            let check_len = text.len().min(BINARY_CHECK_BYTES);
            return (!text[..check_len].contains(&0)).then(|| String::from_utf8_lossy(text));
        }
    };

    // UTF-16 text is full of zero bytes, so look for what it can't contain
    // instead: NUL characters and unpaired surrogates. Arbitrary binary data
    // that happens to start with a BOM has plenty of the latter.
    let units = buf[2..].as_chunks::<2>().0.iter().map(|&c| from_bytes(c));
    let is_binary = char::decode_utf16(units.clone())
        .take(BINARY_CHECK_BYTES / 2)
        .any(|r| r.is_err() || r == Ok('\0'));
    let text = char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER));
    (!is_binary).then(|| Cow::Owned(text.collect()))
}

fn text_preview(text: &str, file_size: u64, name: &str, path: &Path) -> Preview {
    let raw: Vec<&str> = text.lines().collect();

    // Highlight before expanding tabs, as grammars may match on them
    // (e.g. the `<<-EOF` heredoc terminator in shell scripts).
    let mut highlights: Vec<Vec<HighlightSpan>> = match lsh::detect_language(path) {
        Some(language) => {
            let arena = stdext::arena::scratch_arena(None);
            let line_bytes: Vec<&[u8]> = raw.iter().map(|l| l.as_bytes()).collect();
            let hl = lsh::highlight_lines(&arena, language, &line_bytes);
            hl.iter()
                .map(|spans| {
                    spans
                        .iter()
                        .map(|h| HighlightSpan {
                            start: h.start,
                            kind: h.kind,
                        })
                        .collect()
                })
                .collect()
        }
        None => Vec::new(),
    };

    let lines = raw
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let spans = highlights.get_mut(i).map_or(&mut [][..], Vec::as_mut_slice);
            expand_tabs(line, spans)
        })
        .collect();

    Preview {
        lines,
        highlights,
        is_binary: false,
        file_size,
        title: name.to_string(),
    }
}

/// Expand tabs to spaces, moving the `spans` (byte offsets into `line`, in
/// ascending order) along with the text. Left alone, the TUI would draw each
/// tab as a `␉` placeholder, since a raw tab makes the terminal skip cells.
fn expand_tabs(line: &str, spans: &mut [HighlightSpan]) -> String {
    use ruf4_tui::helpers::CoordType;
    use ruf4_tui::unicode::MeasurementConfig;

    if !line.contains('\t') {
        return line.to_string();
    }

    let bytes = line.as_bytes();
    let mut cfg = MeasurementConfig::new(&bytes).with_tab_size(TAB_WIDTH as CoordType);
    let mut spans = spans.iter_mut().peekable();
    let mut out = String::with_capacity(line.len() + TAB_WIDTH);
    let mut copied = 0;
    for (i, _) in line.match_indices('\t') {
        out.push_str(&line[copied..i]);
        let added = out.len() - i;
        while let Some(span) = spans.next_if(|s| s.start <= i) {
            span.start += added;
        }

        let col = cfg.goto_offset(i).column as usize;
        out.extend(std::iter::repeat_n(' ', TAB_WIDTH - col % TAB_WIDTH));
        copied = i + 1;
    }
    out.push_str(&line[copied..]);

    let added = out.len() - line.len();
    for span in spans {
        span.start += added;
    }
    out
}

fn hex_preview(buf: &[u8], file_size: u64, name: &str) -> Preview {
    let mut lines = Vec::new();
    lines.push(format!("Binary file, {} bytes", file_size));
    lines.push(String::new());

    for (i, chunk) in buf.chunks(16).enumerate() {
        let offset = i * 16;
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();

        let hex_str = if chunk.len() > 8 {
            format!("{} {}", hex[..8].join(" "), hex[8..].join(" "))
        } else {
            hex.join(" ")
        };

        lines.push(format!("{offset:08x}  {hex_str:<48}  {ascii}"));

        if lines.len() > 2000 {
            lines.push("...".to_string());
            break;
        }
    }

    Preview {
        lines,
        highlights: Vec::new(),
        is_binary: true,
        file_size,
        title: name.to_string(),
    }
}

fn dir_preview(path: &Path, name: &str) -> Preview {
    let mut lines = Vec::new();
    lines.push(format!("Directory: {name}"));
    lines.push(String::new());

    match vfs::list_names(path) {
        Ok(entries) => {
            let mut items: Vec<String> = entries
                .into_iter()
                .map(|(fname, is_dir)| if is_dir { format!("{fname}/") } else { fname })
                .collect();
            items.sort();
            let total = items.len();
            lines.extend(items);
            lines.push(String::new());
            lines.push(format!("{total} items"));
        }
        Err(e) => {
            lines.push(format!("Error: {e}"));
        }
    }

    Preview {
        lines,
        highlights: Vec::new(),
        is_binary: false,
        file_size: 0,
        title: name.to_string(),
    }
}
