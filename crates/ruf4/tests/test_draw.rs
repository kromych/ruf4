// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Rendering tests: draw frames into memory, feeding input the way the main
//! loop does, and inspect what the terminal would receive.

use std::path::PathBuf;

use ruf4::action::Action;
use ruf4::draw;
use ruf4::panel::{Panel, make_entry};
use ruf4::state::{Dialog, State};
use ruf4_tui::helpers::{CoordType, Point, Size};
use ruf4_tui::input::{Input, InputMouse, InputMouseState, kbmod, vk};
use ruf4_tui::tui::Tui;

/// A terminal of a fixed size, driven like `main.rs` drives the real one.
struct Screen {
    tui: Tui,
    size: Size,
}

impl Screen {
    fn new(width: CoordType, height: CoordType) -> Self {
        let size = Size { width, height };
        Self {
            tui: Tui::new().unwrap(),
            size,
        }
    }

    /// Handle `input` (a resize for the first frame), draw until settled, and
    /// return the raw output of a full redraw.
    fn frame(&mut self, state: &mut State, input: Option<Input>) -> String {
        let input = input.unwrap_or(Input::Resize(self.size));
        state.handle_global_input(&input);
        {
            let mut ctx = self.tui.create_context(Some(input));
            let r = draw::draw(&mut ctx, state);
            state.apply_draw_result(r.term_size, r.menu_active);
        }
        while self.tui.needs_settling() {
            let mut ctx = self.tui.create_context(None);
            let r = draw::draw(&mut ctx, state);
            state.apply_draw_result(r.term_size, r.menu_active);
        }
        self.tui.request_full_redraw();
        let scratch = stdext::arena::scratch_arena(None);
        self.tui.render(&scratch).to_string()
    }

    /// Like [`Self::frame`], returning the screen's rows as plain text.
    fn rows(&mut self, state: &mut State, input: Option<Input>) -> Vec<String> {
        let out = self.frame(state, input);
        let mut rows = vec![vec![' '; self.size.width as usize]; self.size.height as usize];
        let (mut y, mut x) = (0, 0);
        let mut chars = out.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                if let Some(cell) = rows.get_mut(y).and_then(|r| r.get_mut(x)) {
                    *cell = c;
                }
                x += 1;
                continue;
            }
            // CSI: parameters, then a final byte. Only cursor moves matter.
            chars.next();
            let mut params = String::new();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() || d == '~' {
                    if d == 'H' {
                        let mut it = params.split(';').map(|n| n.parse().unwrap_or(1usize));
                        y = it.next().unwrap_or(1) - 1;
                        x = it.next().unwrap_or(1) - 1;
                    }
                    break;
                }
                params.push(d);
            }
        }
        rows.into_iter().map(|r| r.into_iter().collect()).collect()
    }
}

fn render(state: &mut State) -> String {
    Screen::new(60, 8).frame(state, None)
}

fn test_state() -> State {
    let panel = || Panel::with_entries(PathBuf::from("/test"), vec![make_entry("..", true, 0)]);
    State::for_testing(panel(), panel())
}

fn click(x: CoordType, y: CoordType) -> Input<'static> {
    Input::Mouse(InputMouse {
        state: InputMouseState::Left,
        modifiers: kbmod::NONE,
        position: Point { x, y },
        scroll: Point { x: 0, y: 0 },
        drag: false,
    })
}

#[test]
fn test_panel_scrollbar_moves_in_partial_rows() {
    let entries: Vec<_> = std::iter::once(make_entry("..", true, 0))
        .chain((0..200).map(|i| make_entry(&format!("file{i:03}"), false, 1)))
        .collect();
    let mut s = State::for_testing(
        Panel::with_entries(PathBuf::from("/test"), entries),
        Panel::with_entries(PathBuf::from("/test"), vec![make_entry("..", true, 0)]),
    );
    let mut screen = Screen::new(80, 24);
    // The left panel's scrollbar is the column right inside its border,
    // from the first entry (the one under the column headers) on.
    let bar = |rows: &[String]| -> String {
        let first = rows.iter().position(|r| r.contains("Name")).unwrap() + 1;
        rows[first..]
            .iter()
            .filter_map(|r| r.chars().nth(80 / 2 - 2))
            .take_while(|c| " ▁▂▃▄▅▆▇█".contains(*c))
            .collect()
    };

    let top = bar(&screen.rows(&mut s, None));
    assert!(top.starts_with('█'), "{top:?}");

    // Scrolled a little, the thumb starts partway through a row.
    for _ in 0..40 {
        screen.rows(&mut s, Some(Input::Keyboard(vk::DOWN)));
    }
    let scrolled = bar(&screen.rows(&mut s, None));
    assert!(scrolled.contains(|c| "▁▂▃▄▅▆▇".contains(c)), "{scrolled:?}");
    assert_ne!(top, scrolled);
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

#[test]
fn test_click_in_scrolled_list_dialog_picks_the_entry_under_the_mouse() {
    let mut s = test_state();
    // More entries than fit: the dialog is clamped to the screen and scrolls.
    s.dir_history = (0..30)
        .map(|i| PathBuf::from(format!("/dir{i:02}")))
        .collect();
    s.execute_action(Action::DirHistory);
    for _ in 0..25 {
        s.handle_global_input(&Input::Keyboard(vk::DOWN));
    }
    let mut screen = Screen::new(80, 24);
    let mut rows = screen.rows(&mut s, None);
    for target in ["/dir25", "/dir12", "/dir00"] {
        let y = rows
            .iter()
            .position(|r| r.contains(target))
            .unwrap_or_else(|| panic!("{target} not on screen:\n{}", rows.join("\n")));
        rows = screen.rows(&mut s, Some(click(40, y as CoordType)));
        let Dialog::ListSelect { labels, cursor, .. } = &s.dialog else {
            panic!("the click closed the dialog");
        };
        assert_eq!(labels[*cursor], target);
    }
}

#[test]
fn test_alt_letter_is_quick_search_not_a_menu() {
    let entries = ["..", "alpha", "charlie", "foxtrot"]
        .iter()
        .map(|&n| make_entry(n, n == "..", 0))
        .collect();
    let mut s = State::for_testing(
        Panel::with_entries(PathBuf::from("/test"), entries),
        Panel::with_entries(PathBuf::from("/test"), vec![make_entry("..", true, 0)]),
    );
    let mut screen = Screen::new(80, 24);
    screen.frame(&mut s, None);

    // Alt+F and Alt+C used to also open the Files and Commands menus.
    for (key, name) in [(vk::F, "foxtrot"), (vk::C, "charlie")] {
        screen.frame(&mut s, Some(Input::Keyboard(kbmod::ALT | key)));
        assert_eq!(s.left.entries[s.left.cursor].name, name);
        assert!(!s.menu_active, "Alt+{name} opened a menu");
        // A different key ends the quick search before the next one.
        screen.frame(&mut s, Some(Input::Keyboard(vk::HOME)));
    }

    // The menus are still a key away, starting with the first one.
    let rows = screen.rows(&mut s, Some(Input::Keyboard(vk::F9)));
    assert!(s.menu_active);
    let screen_text = rows.join(
        "
",
    );
    assert!(screen_text.contains("Make directory"), "{screen_text}");
    assert!(!screen_text.contains("Show hidden files"), "{screen_text}");
}

#[test]
fn test_help_lists_command_line_keys_and_counts_pages() {
    let mut s = test_state();
    s.execute_action(Action::Help);
    let mut screen = Screen::new(80, 24);
    let mut text = screen.rows(&mut s, None).join(
        "
",
    );
    assert!(text.contains("Help (1/"), "{text}");
    let title = concat!("ruf4 ", env!("CARGO_PKG_VERSION"), " Help");
    assert!(text.contains(title), "{text}");
    // Scroll to the bottom: the command line keys, on the last page.
    for _ in 0..60 {
        text = screen.rows(&mut s, Some(Input::Keyboard(vk::DOWN))).join(
            "
",
        );
    }
    assert!(text.contains("Ctrl+Right"), "{text}");
    let pages = s.help_text.len().div_ceil(24 - 8);
    assert!(text.contains(&format!("Help ({pages}/{pages})")), "{text}");

    // Clicking a row that only describes keys runs nothing.
    let y = text.lines().position(|l| l.contains("Ctrl+Right")).unwrap();
    screen.frame(&mut s, Some(click(40, y as CoordType)));
    assert!(matches!(s.dialog, Dialog::None));
    assert!(!s.command_line_active && !s.menu_active);
}
