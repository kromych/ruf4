// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! File preview for the quick-view panel.

use std::path::Path;

use crate::lsh::{self, HighlightKind};
use crate::vfs;

const MAX_PREVIEW_BYTES: usize = 64 * 1024;
const BINARY_CHECK_BYTES: usize = 512;

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

    let check_len = buf.len().min(BINARY_CHECK_BYTES);
    let is_binary = buf[..check_len].contains(&0);

    if is_binary {
        hex_preview(&buf, file_size, &name)
    } else {
        text_preview(&buf, file_size, &name, path)
    }
}

fn text_preview(buf: &[u8], file_size: u64, name: &str, path: &Path) -> Preview {
    let text = String::from_utf8_lossy(buf);
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

    let highlights = if let Some(language) = lsh::detect_language(path) {
        let arena = stdext::arena::scratch_arena(None);
        let line_bytes: Vec<&[u8]> = text.lines().map(|l| l.as_bytes()).collect();
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
    } else {
        Vec::new()
    };

    Preview {
        lines,
        highlights,
        is_binary: false,
        file_size,
        title: name.to_string(),
    }
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
