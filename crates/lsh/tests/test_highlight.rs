// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use lsh::compiler::{Assembly, Compiler, Generator, builtin_definitions_path};
use lsh::runtime::{Instruction, Runtime};
use stdext::arena::{Arena, scratch_arena};

/// Highlight spans of a single line as `(start, kind)`, `kind` being the DSL identifier.
type Spans = Vec<(usize, String)>;

/// Compiles `src` and highlights `lines` with its `test` function.
fn highlight(src: &str, lines: &[&str]) -> Vec<Spans> {
    let scratch = scratch_arena(None);
    let mut compiler = Compiler::new(&scratch);
    compiler.parse("test.lsh", src).unwrap();
    let assembly = compiler.assemble().unwrap();
    run(&scratch, &assembly, "test", lines)
}

/// Highlights `lines` with the built-in definition `language`.
fn highlight_builtin(language: &str, lines: &[&str]) -> Vec<Spans> {
    let scratch = scratch_arena(None);
    let mut generator = Generator::new(&scratch);
    generator
        .read_directory(builtin_definitions_path())
        .unwrap();
    let assembly = generator.assemble().unwrap();
    run(&scratch, &assembly, language, lines)
}

fn run(arena: &Arena, assembly: &Assembly, entrypoint: &str, lines: &[&str]) -> Vec<Spans> {
    // Like the generator, pad the instruction stream with invalid opcodes,
    // so that the runtime can safely decode past the end.
    let mut instructions = assembly.instructions.clone();
    instructions.extend([0xff; Instruction::MAX_ENCODED_SIZE]);
    let charsets: Vec<_> = assembly.charsets.iter().map(|cs| cs.serialize()).collect();
    let entrypoint = assembly
        .entrypoints
        .iter()
        .find(|ep| ep.name == entrypoint)
        .unwrap()
        .address;
    let mut runtime = Runtime::new(
        &instructions,
        &assembly.strings,
        &charsets,
        entrypoint as u32,
    );

    lines
        .iter()
        .map(|line| {
            runtime
                .parse_next_line::<u32>(arena, line.as_bytes())
                .iter()
                .map(|hl| {
                    let kind = assembly
                        .highlight_kinds
                        .iter()
                        .find(|hk| hk.value == hl.kind)
                        .unwrap();
                    (hl.start, kind.identifier.to_string())
                })
                .collect()
        })
        .collect()
}

/// Returns the highlight kind of the byte at `column`.
fn kind_at(spans: &Spans, column: usize) -> &str {
    let (_, kind) = spans
        .iter()
        .rev()
        .find(|(start, _)| *start <= column)
        .unwrap();
    kind
}

/// Runs `f` on another thread and fails, instead of hanging, if it doesn't finish.
fn with_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || _ = tx.send(f()));
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(v) => v,
        Err(RecvTimeoutError::Timeout) => panic!("highlighting did not terminate"),
        Err(RecvTimeoutError::Disconnected) => panic!("highlighting panicked"),
    }
}

/// A multi-line `"""` string nested in a per-line `until` loop, like in python.lsh.
const MULTILINE_STRING: &str = r#"
pub fn test() {
    until /$/ {
        yield other;

        if /"""/ {
            loop {
                yield string;
                if /"""/ { yield string; break; }
                await input;
            }
        } else if /"[^"]*"/ {
            yield string;
        } else if /\w+/ {
            yield keyword;
        }

        yield other;
    }
}
"#;

#[test]
fn test_loop_guard_ignores_offset_saved_on_previous_line() {
    // Each loop iteration saves `off`, and forces an advance by 1 if `off` is unchanged at its end.
    // In these layouts the closing `"""` is at the column where the last iteration on the previous
    // line started (the empty line's 0, the end of the opening line's 3), which must not count as
    // "no progress" or the closing delimiter would be skipped and the string never terminated.
    for lines in [
        &[r#"x = """"#, "doc", "", r#"""""#, "y"][..],
        &[r#"""""#, r#"doc""""#, "y"][..],
        &[r#"""""#, "", r#"""""#, "y"][..],
    ] {
        let hl = highlight(MULTILINE_STRING, lines);
        let closing = &hl[lines.len() - 2];
        let closing_line = lines[lines.len() - 2];
        let closing_col = closing_line.find(r#"""""#).unwrap();
        for col in closing_col..closing_col + 3 {
            assert_eq!(kind_at(closing, col), "string", "{lines:?}: {hl:?}");
        }
        assert_eq!(
            kind_at(&hl[lines.len() - 1], 0),
            "keyword",
            "{lines:?}: {hl:?}"
        );
    }
}

#[test]
fn test_outer_loop_guard_ignores_offset_saved_on_previous_line() {
    // The `until /$/` iteration that matched the opening `"""` started at column 4 on line 0.
    // After the string closes on line 1, that iteration ends at column 4 again (the `"` of `"s"`),
    // which is progress as well, and must not skip the `"` of the next string.
    let lines = [r#"x = """"#, r#"""" "s""#, "y"];
    let hl = highlight(MULTILINE_STRING, &lines);
    assert_eq!(
        hl[1],
        [
            (0, "string".to_string()),
            (3, "other".to_string()),
            (4, "string".to_string()),
            (7, "other".to_string())
        ],
    );
    assert_eq!(kind_at(&hl[2], 0), "keyword", "{hl:?}");
}

#[test]
fn test_loop_guard_still_breaks_zero_width_iterations() {
    // Neither the body nor the fast-skip consume the `x` in `xz`, so only the guard ensures progress.
    // This must hold for the iterations after resuming on a new line too.
    let src = r#"
        pub fn test() {
            loop {
                yield other;
                if /xy/ { yield keyword; }
                await input;
            }
        }
    "#;
    let hl = with_timeout(move || highlight(src, &["xzxy", "xzxy", "", "xx", "xy"]));
    for spans in [&hl[0], &hl[1]] {
        assert_eq!(kind_at(spans, 1), "other", "{hl:?}");
        assert_eq!(kind_at(spans, 2), "keyword", "{hl:?}");
        assert_eq!(kind_at(spans, 3), "keyword", "{hl:?}");
    }
    assert_eq!(kind_at(&hl[3], 1), "other", "{hl:?}");
    assert_eq!(kind_at(&hl[4], 0), "keyword", "{hl:?}");
}

#[test]
fn test_python_docstring_closes_after_empty_line() {
    let lines = ["def f():", r#"    """Doc."#, "", r#"""""#, "    return 1"];
    let hl = highlight_builtin("python", &lines);
    assert_eq!(kind_at(&hl[3], 0), "string", "{hl:?}");
    assert_eq!(kind_at(&hl[4], 4), "keyword.control", "{hl:?}");
}

#[test]
fn test_python_single_quote_docstring_closes_at_same_column() {
    let lines = ["'''", "doc'''", "return 1"];
    let hl = highlight_builtin("python", &lines);
    assert_eq!(kind_at(&hl[1], 3), "string", "{hl:?}");
    assert_eq!(kind_at(&hl[2], 0), "keyword.control", "{hl:?}");
}

#[test]
fn test_javascript_block_comment_closes_at_same_column() {
    let lines = ["/*", "ab*/", "return 1;"];
    let hl = highlight_builtin("javascript", &lines);
    assert_eq!(kind_at(&hl[1], 2), "comment", "{hl:?}");
    assert_eq!(kind_at(&hl[2], 0), "keyword.control", "{hl:?}");
}

#[test]
fn test_javascript_template_literal_closes_after_empty_line() {
    let lines = ["x = `", "", "`", "return 1;"];
    let hl = highlight_builtin("javascript", &lines);
    assert_eq!(kind_at(&hl[2], 0), "string", "{hl:?}");
    assert_eq!(kind_at(&hl[3], 0), "keyword.control", "{hl:?}");
}

#[test]
fn test_powershell_block_comment_closes_after_empty_line() {
    let lines = ["<#", "", "#>", "return"];
    let hl = highlight_builtin("powershell", &lines);
    assert_eq!(kind_at(&hl[2], 0), "comment", "{hl:?}");
    assert_eq!(kind_at(&hl[3], 0), "keyword.control", "{hl:?}");
}
