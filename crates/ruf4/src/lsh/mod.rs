mod definitions;

use std::path::Path;

use lsh::runtime::{Highlight, Language, Runtime};
use stdext::arena::Arena;
use stdext::collections::BVec;
use stdext::glob::glob_match;

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

pub fn highlight_lines<'a>(
    arena: &'a Arena,
    language: &Language,
    lines: &[&[u8]],
) -> BVec<'a, BVec<'a, Highlight<HighlightKind>>> {
    let mut runtime = Runtime::new(&ASSEMBLY, &STRINGS, &CHARSETS, language.entrypoint);
    let mut result = BVec::empty();
    for line in lines {
        let highlights = runtime.parse_next_line::<HighlightKind>(arena, line);
        result.push(arena, highlights);
    }
    result
}
