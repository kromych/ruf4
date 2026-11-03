// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Rendering tests: draw a frame into memory and inspect the escape sequences
//! the terminal would receive.

use std::path::PathBuf;

use ruf4::draw;
use ruf4::panel::{Panel, make_entry};
use ruf4::state::State;
use ruf4_tui::helpers::Size;
use ruf4_tui::input::Input;
use ruf4_tui::tui::Tui;

fn render(state: &mut State) -> String {
    let mut tui = Tui::new().unwrap();
    let size = Size {
        width: 60,
        height: 8,
    };
    let scratch = stdext::arena::scratch_arena(None);
    {
        let mut ctx = tui.create_context(Some(Input::Resize(size)));
        let r = draw::draw(&mut ctx, state);
        state.apply_draw_result(r.term_size, r.menu_active);
    }
    while tui.needs_settling() {
        let mut ctx = tui.create_context(None);
        let r = draw::draw(&mut ctx, state);
        state.apply_draw_result(r.term_size, r.menu_active);
    }
    tui.render(&scratch).to_string()
}

fn test_state() -> State {
    let panel = || Panel::with_entries(PathBuf::from("/test"), vec![make_entry("..", true, 0)]);
    State::for_testing(panel(), panel())
}

#[test]
fn test_command_line_suggestion_is_dimmed() {
    let mut s = test_state();
    s.cmd_history = vec!["cargo test".to_string()];
    s.handle_global_input(&Input::Text("car"));
    let out = render(&mut s);

    let typed = out.find(" car").expect("typed text is drawn");
    let suggested = out[typed..].find("o test").expect("suggestion is drawn") + typed;
    let between = &out[typed..suggested];
    // The suggestion's color, then the cursor (underline) on its first letter.
    let dim = Tui::new().unwrap().indexed(s.theme.pathbar_suggestion);
    let sgr = format!("\x1b[38;2;{};{};{}m", dim.red(), dim.green(), dim.blue());
    assert!(between.contains(&sgr), "{between:?} lacks {sgr:?}");
    assert!(between.contains("\x1b[4mg"), "{between:?}");
}
