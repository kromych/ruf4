// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//
// Portions derived from the Microsoft Edit project (https://github.com/microsoft/edit).
// See LICENSE-EDIT for the original MIT license.

mod definitions;

use std::path::Path;

use lsh::glob::glob_match;
use lsh::runtime::{Highlight, Language, Runtime};

pub use definitions::{ASSEMBLY, CHARSETS, FILE_ASSOCIATIONS, HighlightKind, LANGUAGES, STRINGS};

pub fn detect_language(path: &Path) -> Option<&'static Language> {
    let path_bytes: &[u8] = path.as_os_str().as_encoded_bytes();
    for &(pattern, lang) in FILE_ASSOCIATIONS {
        if glob_match(pattern.as_bytes(), path_bytes) {
            return Some(lang);
        }
    }
    None
}

pub fn highlight_lines(language: &Language, lines: &[&[u8]]) -> Vec<Vec<Highlight<HighlightKind>>> {
    let mut runtime = Runtime::new(&ASSEMBLY, &STRINGS, &CHARSETS, language.entrypoint);
    lines
        .iter()
        .map(|line| {
            let mut highlights = Vec::new();
            runtime.parse_next_line(line, &mut highlights);
            highlights
        })
        .collect()
}
