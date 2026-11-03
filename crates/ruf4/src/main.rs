// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! ruf4 - A dual-panel file commander inspired by FAR Manager 3.
//!
//! Built on the TUI framework derived from Microsoft Edit.

use std::process::ExitCode;

use ruf4::draw;
use ruf4::platform;
use ruf4::state;
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
        sys::write_stdout(platform::TUI_ENTER_SEQ);
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        sys::write_stdout(platform::TUI_LEAVE_SEQ);
    }
}

const USAGE: &str = "\
Usage: ruf4 [OPTIONS]

A dual-panel file commander for the terminal.

Options:
  -h, --help     Print this help and exit
  -V, --version  Print the version and exit
";

fn main() -> ExitCode {
    // Before anything touches the terminal, so that these print normally.
    if let Some(arg) = std::env::args_os().nth(1) {
        match arg.to_str() {
            Some("-h" | "--help") => print!("{USAGE}"),
            Some("-V" | "-v" | "--version") => println!("ruf4 {}", env!("CARGO_PKG_VERSION")),
            _ => {
                eprintln!("ruf4: unknown argument: {}", arg.display());
                eprintln!("Try 'ruf4 --help'.");
                return ExitCode::from(2);
            }
        }
        return ExitCode::SUCCESS;
    }

    if let Err(e) = run() {
        eprintln!("ruf4: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> std::io::Result<()> {
    let _sys_deinit = sys::init()?;
    arena::init(SCRATCH_ARENA_CAPACITY)?;
    let size = sys::switch_modes()?;

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Hand the terminal back if this panic ends the process. A worker
        // thread unwinding (debug builds) doesn't: its job reports the failure
        // and the UI carries on.
        if cfg!(panic = "abort") || std::thread::current().name() == Some("main") {
            sys::write_stdout(platform::TUI_LEAVE_SEQ);
            // With `panic = "abort"`, `_sys_deinit` never gets dropped. Leave raw
            // mode here instead, which also lets the message below print in
            // cooked mode.
            sys::restore_modes();
        }
        default_hook(info);
    }));

    let _term_guard = TerminalGuard::new();

    let mut vt_parser = vt::Parser::new();
    let mut input_parser = input::Parser::new();
    let mut tui = Tui::new()?;
    tui.set_size(size);
    let mut state = state::State::new();
    tui.set_floater_default_bg(tui.indexed(state.theme.floater_bg));
    tui.set_floater_default_fg(tui.indexed(state.theme.floater_fg));

    // The state learns the size the same way as any later change: from a resize event.
    sys::inject_window_size_into_stdin();

    loop {
        let scratch = scratch_arena(None);
        // While a background job runs, poll often so the progress dialog stays live.
        let job_timeout = if state.job_active() {
            Duration::from_millis(50)
        } else {
            Duration::MAX
        };
        let read_timeout = vt_parser
            .read_timeout()
            .min(tui.read_timeout())
            .min(Duration::from_secs(1))
            .min(job_timeout);

        let (resize, input) = match sys::read_stdin(&scratch, read_timeout) {
            Some(read) => read,
            None => break, // EOF or error
        };

        let vt_stream = vt_parser.parse(&input);
        let input_iter = resize
            .map(input::Input::Resize)
            .into_iter()
            .chain(input_parser.parse(vt_stream));

        for ev in input_iter {
            if state.handle_global_input(&ev) {
                break;
            }
            state.poll_job();

            state.update_preview();

            let mut ctx = tui.create_context(Some(ev));
            let r = draw::draw(&mut ctx, &mut state);
            state.apply_draw_result(r.term_size, r.menu_active);

            state.update_preview();
        }

        // A timeout (empty read) is also our background-job heartbeat: pull the
        // latest worker progress and repaint.
        if input.is_empty() {
            state.poll_job();
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

        // After an external program took over the terminal, the incremental
        // diff would draw nothing onto the freshly re-entered alternate screen.
        if state.take_repaint_request() {
            tui.request_full_redraw();
        }

        let scratch = scratch_arena(None);
        let output = tui.render(&scratch);
        sys::write_stdout(&output);
        drop(scratch);

        // Ctrl+O: hand the screen back to the user until dismissed. Runs at the
        // frame boundary so no draw pass is in flight while the TUI is left.
        if state.take_user_screen_request() {
            platform::view_user_screen(&state.user_screen_exit_keys());
            tui.request_full_redraw();
        }
    }

    Ok(())
}
