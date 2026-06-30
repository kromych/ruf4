use std::path::PathBuf;

use ruf4::panel::{Panel, make_entry};
use ruf4::preview::Preview;
use ruf4::state::{ActivePanel, Dialog, State};
use ruf4_tui::helpers::{Point, Size};
use ruf4_tui::input::{Input, InputMouse, InputMouseState, kbmod, vk};

fn test_panel(names: &[(&str, bool)]) -> Panel {
    let mut entries = vec![make_entry("..", true, 0)];
    for &(name, is_dir) in names {
        entries.push(make_entry(name, is_dir, 100));
    }
    Panel::with_entries(PathBuf::from("/test"), entries)
}

fn test_state() -> State {
    State::for_testing(
        test_panel(&[("a.txt", false), ("b.txt", false), ("subdir", true)]),
        test_panel(&[("c.txt", false)]),
    )
}

// --- Tab switching ---

#[test]
fn test_tab_switches_panels() {
    let mut s = test_state();
    assert_eq!(s.active, ActivePanel::Left);
    s.handle_global_input(&Input::Keyboard(vk::TAB));
    assert_eq!(s.active, ActivePanel::Right);
    s.handle_global_input(&Input::Keyboard(vk::TAB));
    assert_eq!(s.active, ActivePanel::Left);
}

// --- Dialog opening ---

#[test]
fn test_f10_opens_quit_dialog() {
    let mut s = test_state();
    s.handle_global_input(&Input::Keyboard(vk::F10));
    assert!(matches!(s.dialog, Dialog::ConfirmQuit { .. }));
}

#[test]
fn test_alt_x_triggers_quick_search() {
    let mut s = test_state();
    s.handle_global_input(&Input::Keyboard(kbmod::ALT | vk::X));
    assert_eq!(s.alt_search, "x");
}

#[test]
fn test_delete_key_opens_delete_dialog() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 1; // "a.txt"
    s.handle_global_input(&Input::Keyboard(vk::DELETE));
    assert!(matches!(s.dialog, Dialog::Delete { .. }));
}

#[test]
fn test_open_copy_dialog() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 1; // "a.txt"
    s.open_copy_dialog();
    assert!(matches!(s.dialog, Dialog::Copy { .. }));
    if let Dialog::Copy { files, dest } = &s.dialog {
        assert_eq!(files, &["a.txt"]);
        assert_eq!(dest, "/test"); // inactive panel path
    }
}

#[test]
fn test_open_move_dialog() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 2; // "b.txt"
    s.open_move_dialog();
    assert!(matches!(s.dialog, Dialog::Move { .. }));
}

#[test]
fn test_open_delete_on_dotdot_shows_error() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 0; // ".."
    s.open_delete_dialog();
    assert!(s.dialog_is_error());
}

// --- Selection shortcuts ---

#[test]
fn test_plus_opens_select_group() {
    let mut s = test_state();
    s.handle_global_input(&Input::Text("+"));
    assert!(matches!(s.dialog, Dialog::SelectGroup { select: true, .. }));
}

#[test]
fn test_minus_opens_deselect_group() {
    let mut s = test_state();
    s.handle_global_input(&Input::Text("-"));
    assert!(matches!(
        s.dialog,
        Dialog::SelectGroup { select: false, .. }
    ));
}

#[test]
fn test_star_inverts_selection() {
    let mut s = test_state();
    s.active_panel_mut().entries[1].selected = true;
    s.handle_global_input(&Input::Text("*"));
    assert!(!s.active_panel().entries[1].selected);
    assert!(s.active_panel().entries[2].selected);
}

// --- Navigation ---

#[test]
fn test_arrow_keys_navigate() {
    let mut s = test_state();
    s.handle_global_input(&Input::Keyboard(vk::DOWN));
    assert_eq!(s.active_panel().cursor, 1);
    s.handle_global_input(&Input::Keyboard(vk::DOWN));
    assert_eq!(s.active_panel().cursor, 2);
    s.handle_global_input(&Input::Keyboard(vk::UP));
    assert_eq!(s.active_panel().cursor, 1);
}

#[test]
fn test_home_end_keys() {
    let mut s = test_state();
    s.handle_global_input(&Input::Keyboard(vk::END));
    assert_eq!(s.active_panel().cursor, 3); // "..", a, b, subdir
    s.handle_global_input(&Input::Keyboard(vk::HOME));
    assert_eq!(s.active_panel().cursor, 0);
}

// --- Dialog Y/N handling ---

#[test]
fn test_quit_dialog_y_confirms() {
    let mut s = test_state();
    s.dialog = Dialog::ConfirmQuit {
        save_settings: false,
    };
    s.handle_global_input(&Input::Text("y"));
    assert!(s.quit);
}

#[test]
fn test_quit_dialog_n_cancels() {
    let mut s = test_state();
    s.dialog = Dialog::ConfirmQuit {
        save_settings: false,
    };
    s.handle_global_input(&Input::Text("n"));
    assert!(s.dialog_is_none());
    assert!(!s.quit);
}

#[test]
fn test_quit_dialog_escape_cancels() {
    let mut s = test_state();
    s.dialog = Dialog::ConfirmQuit {
        save_settings: false,
    };
    s.handle_global_input(&Input::Keyboard(vk::ESCAPE));
    assert!(s.dialog_is_none());
}

// --- Command line ---

#[test]
fn test_text_activates_command_line() {
    let mut s = test_state();
    s.handle_global_input(&Input::Text("l"));
    assert!(s.command_line_active);
    assert_eq!(s.command_line, "l");
}

#[test]
fn test_command_line_escape_cancels() {
    let mut s = test_state();
    s.command_line_active = true;
    s.command_line = "test".to_string();
    s.handle_global_input(&Input::Keyboard(vk::ESCAPE));
    assert!(!s.command_line_active);
    assert!(s.command_line.is_empty());
}

#[test]
fn test_command_line_backspace_clears() {
    let mut s = test_state();
    s.command_line_active = true;
    s.command_line = "x".to_string();
    s.cmd_cursor = 1;
    s.handle_global_input(&Input::Keyboard(vk::BACK));
    assert!(!s.command_line_active); // auto-deactivate when empty
}

#[test]
fn test_command_line_insert_at_cursor() {
    let mut s = test_state();
    s.command_line_active = true;
    s.command_line = "ac".to_string();
    s.cmd_cursor = 1;
    s.handle_global_input(&Input::Text("b"));
    assert_eq!(s.command_line, "abc");
    assert_eq!(s.cmd_cursor, 2);
}

#[test]
fn test_command_line_delete_and_cursor_moves() {
    let mut s = test_state();
    s.command_line_active = true;
    s.command_line = "abc".to_string();
    // HOME then DELETE removes the first char (forward delete at cursor 0).
    s.cmd_cursor = 3;
    s.handle_global_input(&Input::Keyboard(vk::HOME));
    assert_eq!(s.cmd_cursor, 0);
    s.handle_global_input(&Input::Keyboard(vk::DELETE));
    assert_eq!(s.command_line, "bc");
    // RIGHT then BACK removes the char before the cursor.
    s.handle_global_input(&Input::Keyboard(vk::RIGHT));
    assert_eq!(s.cmd_cursor, 1);
    s.handle_global_input(&Input::Keyboard(vk::BACK));
    assert_eq!(s.command_line, "c");
    assert_eq!(s.cmd_cursor, 0);
    // END moves to the end.
    s.handle_global_input(&Input::Keyboard(vk::END));
    assert_eq!(s.cmd_cursor, 1);
}

#[test]
fn test_mkdir_dialog_text_editing() {
    let mut s = test_state();
    s.handle_global_input(&Input::Keyboard(vk::F7)); // open MkDir
    assert!(matches!(s.dialog, Dialog::MkDir { .. }));
    s.handle_global_input(&Input::Text("ac"));
    s.handle_global_input(&Input::Keyboard(vk::LEFT));
    s.handle_global_input(&Input::Text("b"));
    if let Dialog::MkDir { name } = &s.dialog {
        assert_eq!(name, "abc");
    } else {
        panic!("expected MkDir dialog");
    }
}

// --- Insert / Shift+Space selection ---

#[test]
fn test_insert_toggles_selection() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 1; // "a.txt"
    s.handle_global_input(&Input::Keyboard(vk::INSERT));
    assert!(s.active_panel().entries[1].selected);
    assert_eq!(s.active_panel().cursor, 2); // moved down
}

#[test]
fn test_ctrl_space_toggles_selection() {
    let mut s = test_state();
    s.active_panel_mut().cursor = 1;
    s.handle_global_input(&Input::Keyboard(kbmod::CTRL | vk::SPACE));
    assert!(s.active_panel().entries[1].selected);
}

// --- MkDir dialog ---

#[test]
fn test_mkdir_dialog_text_input() {
    let mut s = test_state();
    s.dialog = Dialog::MkDir {
        name: String::new(),
    };
    s.handle_global_input(&Input::Text("t"));
    s.handle_global_input(&Input::Text("e"));
    s.handle_global_input(&Input::Text("s"));
    s.handle_global_input(&Input::Text("t"));
    if let Dialog::MkDir { name } = &s.dialog {
        assert_eq!(name, "test");
    } else {
        panic!("expected MkDir dialog");
    }
}

#[test]
fn test_mkdir_dialog_backspace() {
    let mut s = test_state();
    s.dialog = Dialog::MkDir {
        name: "abc".to_string(),
    };
    s.input_cursor = 3; // cursor at end of "abc"
    s.handle_global_input(&Input::Keyboard(vk::BACK));
    if let Dialog::MkDir { name } = &s.dialog {
        assert_eq!(name, "ab");
    }
}

#[test]
fn test_mkdir_dialog_escape() {
    let mut s = test_state();
    s.dialog = Dialog::MkDir {
        name: "test".to_string(),
    };
    s.handle_global_input(&Input::Keyboard(vk::ESCAPE));
    assert!(s.dialog_is_none());
}

// --- Select group dialog ---

#[test]
fn test_select_group_dialog_flow() {
    let mut s = test_state();
    s.dialog = Dialog::SelectGroup {
        pattern: "*.txt".to_string(),
        select: true,
    };
    s.handle_global_input(&Input::Keyboard(vk::RETURN));
    assert!(s.dialog_is_none());
    // "a.txt" and "b.txt" should be selected
    assert!(s.active_panel().entries[1].selected);
    assert!(s.active_panel().entries[2].selected);
    assert!(!s.active_panel().entries[3].selected); // "subdir"
}

// --- Dialog blocks global input ---

#[test]
fn test_dialog_blocks_navigation() {
    let mut s = test_state();
    s.dialog = Dialog::ConfirmQuit {
        save_settings: false,
    };
    let cursor_before = s.active_panel().cursor;
    s.handle_global_input(&Input::Keyboard(vk::DOWN));
    // DOWN should be consumed by dialog handler, not navigation
    assert_eq!(s.active_panel().cursor, cursor_before);
}

// --- Quick view scrolling ---

fn preview_with_lines(n: usize) -> Preview {
    let mut p = Preview::empty();
    p.lines = (0..n).map(|i| format!("line {i}")).collect();
    p
}

fn scroll_event(x: isize, dy: isize) -> Input<'static> {
    Input::Mouse(InputMouse {
        state: InputMouseState::Scroll,
        modifiers: kbmod::NONE,
        position: Point { x, y: 5 },
        scroll: Point { x: 0, y: dy },
        drag: false,
    })
}

#[test]
fn test_quick_view_wheel_scrolls_preview() {
    let mut s = test_state(); // active == Left, so the preview is on the right half
    s.quick_view = true;
    s.term_size = Size {
        width: 80,
        height: 24,
    };
    s.preview = preview_with_lines(100);

    // Wheel down over the right half (x >= 40) scrolls the preview down.
    s.handle_global_input(&scroll_event(60, 1));
    assert_eq!(s.preview_scroll, 3);
    s.handle_global_input(&scroll_event(60, 1));
    assert_eq!(s.preview_scroll, 6);
    // Wheel up scrolls back, saturating at 0.
    s.handle_global_input(&scroll_event(60, -1));
    assert_eq!(s.preview_scroll, 3);
    s.handle_global_input(&scroll_event(60, -1));
    s.handle_global_input(&scroll_event(60, -1));
    assert_eq!(s.preview_scroll, 0);
}

#[test]
fn test_quick_view_wheel_over_file_list_moves_cursor_not_preview() {
    let mut s = test_state(); // active == Left; the active file list is the left half
    s.quick_view = true;
    s.term_size = Size {
        width: 80,
        height: 24,
    };
    s.preview = preview_with_lines(100);
    s.preview_scroll = 5;

    let before = s.active_panel().cursor;
    s.handle_global_input(&scroll_event(10, 1)); // left half = active file list
    assert_eq!(
        s.preview_scroll, 5,
        "preview must not move when scrolling files"
    );
    assert!(
        s.active_panel().cursor >= before,
        "file cursor should advance"
    );
}

#[test]
fn test_preview_scroll_resets_on_file_change() {
    let mut s = test_state();
    s.quick_view = true;
    s.preview_scroll = 5;
    s.preview_path = Some(PathBuf::from("/test/previous"));
    s.active_panel_mut().cursor = 1; // selects a real different entry

    s.update_preview();
    assert_eq!(
        s.preview_scroll, 0,
        "a new previewed file starts at the top"
    );
}
