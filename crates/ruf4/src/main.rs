// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! ruf4 - A dual-panel file commander inspired by FAR Manager 3.
//!
//! Built on the TUI framework derived from Microsoft Edit.

use std::process::ExitCode;

use ruf4::draw;
use ruf4::state;
use ruf4_tui::framebuffer::IndexedColor;
use ruf4_tui::helpers::*;
use ruf4_tui::input;
use ruf4_tui::tui::Tui;
use ruf4_tui::{sys, vt};
use std::time::Duration;
use stdext::arena::{self, scratch_arena};

const SCRATCH_ARENA_CAPACITY: usize = if cfg!(target_pointer_width = "32") {
    128 * MEBI
} else {
    512 * MEBI
};

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Self {
        // Alternate screen buffer, then enable SGR mouse mode + all-motion tracking.
        sys::write_stdout("\x1b[?1049h\x1b[?1003;1006h");
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Reset cursor style (DECSCUSR 0), show cursor (DECTCEM), reset attributes,
        // disable mouse tracking, then exit alternate screen LAST so the main screen
        // is restored cleanly without leftover SGR state.
        sys::write_stdout("\x1b[0 q\x1b[?25h\x1b[m\x1b[?1003;1006l\x1b[?1049l");
    }
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("ruf4: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> std::io::Result<()> {
    let _sys_deinit = sys::init();
    arena::init(SCRATCH_ARENA_CAPACITY)?;
    sys::switch_modes()?;

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        sys::write_stdout("\x1b[0 q\x1b[?25h\x1b[m\x1b[?1003;1006l\x1b[?1049l");
        default_hook(info);
    }));

    let _term_guard = TerminalGuard::new();

    let mut vt_parser = vt::Parser::new();
    let mut input_parser = input::Parser::new();
    let mut tui = Tui::new()?;
    tui.set_floater_default_bg(tui.indexed(IndexedColor::Cyan));
    tui.set_floater_default_fg(tui.indexed(IndexedColor::Black));
    let mut state = state::State::new();

    sys::inject_window_size_into_stdin();

    loop {
        let scratch = scratch_arena(None);
        let read_timeout = vt_parser
            .read_timeout()
            .min(tui.read_timeout())
            .min(Duration::from_secs(1));

        let input = match sys::read_stdin(&scratch, read_timeout) {
            Some(input) => input,
            None => break, // EOF or error
        };

        let vt_stream = vt_parser.parse(&input);
        let input_iter = input_parser.parse(vt_stream);

        for ev in input_iter {
            if state.handle_global_input(&ev) {
                break;
            }
            state.finalize_dialog();

            state.update_preview();

            let mut ctx = tui.create_context(Some(ev));
            let r = draw::draw(&mut ctx, &mut state);
            state.apply_draw_result(r.term_size, r.menu_active);

            state.update_preview();
        }

        if input.is_empty() {
            let mut ctx = tui.create_context(None);
            let r = draw::draw(&mut ctx, &mut state);
            state.apply_draw_result(r.term_size, r.menu_active);
        }

        if state.quit {
            break;
        }

        while tui.needs_settling() {
            let mut ctx = tui.create_context(None);
            let r = draw::draw(&mut ctx, &mut state);
            state.apply_draw_result(r.term_size, r.menu_active);
        }

        let scratch = scratch_arena(None);
        let output = tui.render(&scratch);
        sys::write_stdout(&output);
    }

    Ok(())
}
