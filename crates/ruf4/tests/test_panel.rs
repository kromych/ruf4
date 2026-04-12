use std::path::PathBuf;

use ruf4::panel::{Panel, SortBy, SortDir, format_size, glob_match, make_entry};

fn test_panel(names: &[(&str, bool, u64)]) -> Panel {
    let mut entries = vec![make_entry("..", true, 0)];
    for &(name, is_dir, size) in names {
        entries.push(make_entry(name, is_dir, size));
    }
    Panel::with_entries(PathBuf::from("/test"), entries)
}

// --- Navigation ---

#[test]
fn test_cursor_down_and_up() {
    let mut p = test_panel(&[("a", false, 1), ("b", false, 2), ("c", false, 3)]);
    assert_eq!(p.cursor, 0);
    p.cursor_down(1);
    assert_eq!(p.cursor, 1);
    p.cursor_down(1);
    assert_eq!(p.cursor, 2);
    p.cursor_up(1);
    assert_eq!(p.cursor, 1);
}

#[test]
fn test_cursor_clamps_at_bounds() {
    let mut p = test_panel(&[("a", false, 1)]);
    // 2 entries: ".." and "a"
    p.cursor_down(100);
    assert_eq!(p.cursor, 1);
    p.cursor_up(100);
    assert_eq!(p.cursor, 0);
}

#[test]
fn test_home_end() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0), ("c", false, 0)]);
    p.cursor_end();
    assert_eq!(p.cursor, 3); // "..", a, b, c
    p.cursor_home();
    assert_eq!(p.cursor, 0);
}

#[test]
fn test_page_navigation() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0), ("c", false, 0)]);
    p.cursor_down(20); // page down past end
    assert_eq!(p.cursor, 3);
    p.cursor_up(20); // page up past start
    assert_eq!(p.cursor, 0);
}

// --- Selection ---

#[test]
fn test_toggle_select() {
    let mut p = test_panel(&[("a", false, 10), ("b", false, 20)]);
    p.cursor = 1; // "a"
    p.toggle_select();
    assert!(p.entries[1].selected);
    assert_eq!(p.cursor, 2); // moved down
}

#[test]
fn test_toggle_select_skips_dotdot() {
    let mut p = test_panel(&[("a", false, 10)]);
    p.cursor = 0; // ".."
    p.toggle_select();
    assert!(!p.entries[0].selected);
}

#[test]
fn test_select_all_and_clear() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0)]);
    p.select_all();
    assert!(!p.entries[0].selected); // ".." not selected
    assert!(p.entries[1].selected);
    assert!(p.entries[2].selected);

    p.clear_selection();
    assert!(!p.entries[1].selected);
    assert!(!p.entries[2].selected);
}

#[test]
fn test_invert_selection() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0)]);
    p.entries[1].selected = true;
    p.invert_selection();
    assert!(!p.entries[0].selected); // ".." unchanged
    assert!(!p.entries[1].selected); // was true, now false
    assert!(p.entries[2].selected); // was false, now true
}

#[test]
fn test_select_by_pattern() {
    let mut p = test_panel(&[
        ("foo.rs", false, 0),
        ("bar.rs", false, 0),
        ("baz.txt", false, 0),
    ]);
    p.select_by_pattern("*.rs", true);
    assert!(p.entries[1].selected); // foo.rs
    assert!(p.entries[2].selected); // bar.rs
    assert!(!p.entries[3].selected); // baz.txt

    p.select_by_pattern("*.rs", false);
    assert!(!p.entries[1].selected);
    assert!(!p.entries[2].selected);
}

#[test]
fn test_selection_info() {
    let mut p = test_panel(&[("a", false, 100), ("b", false, 200)]);
    p.entries[1].selected = true;
    let (count, total) = p.selection_info();
    assert_eq!(count, 1);
    assert_eq!(total, 100);
}

// --- Sorting ---

#[test]
fn test_sort_by_name() {
    let mut p = test_panel(&[("c", false, 0), ("a", false, 0), ("b", false, 0)]);
    // Panel defaults to SortBy::Name, so switch away first to avoid toggle.
    p.sort_by = SortBy::Size;
    p.set_sort(SortBy::Name);
    let names: Vec<&str> = p.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["..", "a", "b", "c"]);
}

#[test]
fn test_sort_dirs_first() {
    let mut p = test_panel(&[("z_file", false, 0), ("a_dir", true, 0)]);
    p.set_sort(SortBy::Name);
    assert_eq!(p.entries[1].name, "a_dir"); // dir first
    assert_eq!(p.entries[2].name, "z_file");
}

#[test]
fn test_sort_by_size() {
    let mut p = test_panel(&[("big", false, 999), ("small", false, 1), ("mid", false, 50)]);
    p.set_sort(SortBy::Size);
    let sizes: Vec<u64> = p.entries.iter().skip(1).map(|e| e.size).collect();
    assert_eq!(sizes, vec![1, 50, 999]);
}

#[test]
fn test_sort_toggle_direction() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0)]);
    p.set_sort(SortBy::Name); // already Name+Asc → flips to Desc
    assert_eq!(p.sort_dir, SortDir::Descending);
    let names: Vec<&str> = p.entries.iter().skip(1).map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["b", "a"]);
}

#[test]
fn test_sort_by_extension() {
    let mut p = test_panel(&[("b.txt", false, 0), ("a.rs", false, 0), ("c.md", false, 0)]);
    p.set_sort(SortBy::Extension);
    let names: Vec<&str> = p.entries.iter().skip(1).map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["c.md", "a.rs", "b.txt"]);
}

// --- selected_or_current ---

#[test]
fn test_selected_or_current_with_selection() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0)]);
    p.entries[2].selected = true; // "b"
    let paths = p.selected_or_current();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], PathBuf::from("/test/b"));
}

#[test]
fn test_selected_or_current_falls_back_to_cursor() {
    let mut p = test_panel(&[("a", false, 0), ("b", false, 0)]);
    p.cursor = 1; // "a"
    let paths = p.selected_or_current();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], PathBuf::from("/test/a"));
}

#[test]
fn test_selected_or_current_skips_dotdot() {
    let mut p = test_panel(&[("a", false, 0)]);
    p.cursor = 0; // ".."
    let paths = p.selected_or_current();
    assert!(paths.is_empty());
}

// --- scroll ---

#[test]
fn test_adjust_scroll_follows_cursor() {
    let mut p = test_panel(&[
        ("a", false, 0),
        ("b", false, 0),
        ("c", false, 0),
        ("d", false, 0),
        ("e", false, 0),
    ]);
    p.cursor = 5; // "e"
    p.adjust_scroll(3); // visible = 3
    assert_eq!(p.scroll_offset, 3); // scroll so cursor is visible
}

// --- Glob matching ---

#[test]
fn test_glob_star() {
    assert!(glob_match("*.rs", "main.rs"));
    assert!(!glob_match("*.rs", "main.txt"));
}

#[test]
fn test_glob_question() {
    assert!(glob_match("?.rs", "a.rs"));
    assert!(!glob_match("?.rs", "ab.rs"));
}

#[test]
fn test_glob_case_insensitive() {
    assert!(glob_match("*.RS", "main.rs"));
    assert!(glob_match("*.rs", "MAIN.RS"));
}

#[test]
fn test_glob_exact() {
    assert!(glob_match("hello", "hello"));
    assert!(!glob_match("hello", "world"));
}

// --- Display ---

#[test]
fn test_display_size() {
    assert_eq!(make_entry("d", true, 0).display_size(), "<DIR>");
    assert_eq!(make_entry("f", false, 500).display_size(), "500");
    assert_eq!(make_entry("f", false, 1500).display_size(), "1K");
    assert_eq!(make_entry("f", false, 2_500_000).display_size(), "2M");
    assert_eq!(make_entry("f", false, 3_000_000_000).display_size(), "3G");
}

#[test]
fn test_display_size_link() {
    let mut e = make_entry("link", false, 100);
    e.is_symlink = true;
    assert_eq!(e.display_size(), "<LNK>");

    let mut e = make_entry("hard", false, 100);
    e.is_hardlink = true;
    assert_eq!(e.display_size(), "<LNK>");

    let mut e = make_entry("dlink", true, 0);
    e.is_symlink = true;
    assert_eq!(e.display_size(), "<LNK>");
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(500), "500");
    assert_eq!(format_size(1500), "1K");
    assert_eq!(format_size(1_500_000), "1.5M");
    assert_eq!(format_size(2_500_000_000), "2.5G");
    assert_eq!(format_size(1_200_000_000_000), "1.2T");
}
