// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Drawing routines for the dual-panel file commander UI.
//!
//! Layout:
//! ┌─────── Left Panel ────────┬──────── Right Panel ───────┐
//! │ Name          Size  Date  │ Name          Size  Date   │
//! │ ..                        │ ..                         │
//! │ Documents/     <DIR>      │ file.txt       1234  01-01 │
//! │ ...                       │ ...                        │
//! ├───────────────────────────┴────────────────────────────┤
//! │ /current/path>                                         │
//! ├────────────────────────────────────────────────────────┤
//! │ 1Help 2Save 3View 4Edit 5Copy 6Move 7Mkdir 8Del 9Menu 10Quit │
//! └────────────────────────────────────────────────────────┘

use ruf4_tui::framebuffer::IndexedColor;
use ruf4_tui::helpers::*;
use ruf4_tui::tui::Context;
use stdext::arena_format;

use ruf4_tui::input::vk;

use crate::action::{self, Action};
use crate::platform;
use crate::theme::Theme;

use crate::panel::{self, Panel, SortBy, SortDir};
use crate::preview::HighlightSpan;
use crate::state::{ActivePanel, Dialog, State};

pub struct DrawResult {
    pub menu_active: bool,
    pub term_size: Size,
}

pub fn draw(ctx: &mut Context, state: &mut State) -> DrawResult {
    let size = ctx.size();
    if size.width < 10 || size.height < 5 {
        return DrawResult {
            menu_active: false,
            term_size: size,
        };
    }

    let theme = state.theme.clone();

    ctx.table_begin("root");
    ctx.attr_intrinsic_size(size);
    let menu_active;
    {
        ctx.table_next_row();
        menu_active = draw_menubar(ctx, state);

        ctx.table_next_row();
        draw_panels(ctx, state, &theme, size);

        ctx.table_next_row();
        draw_path_bar(ctx, state, &theme, size);

        ctx.table_next_row();
        draw_fn_bar(ctx, &theme, size);
    }
    ctx.table_end();

    draw_clock(ctx, &theme, size);
    draw_dialog(ctx, state, &theme, size);

    DrawResult {
        menu_active,
        term_size: size,
    }
}

fn draw_menubar(ctx: &mut Context, state: &mut State) -> bool {
    // Snapshot binding keys into locals so we don't hold a borrow on state
    // across mutable execute_action() calls.
    let bindings = state.bindings.clone();
    let key = |a: Action| action::key_for(&bindings, a);
    let theme = &state.theme;

    ctx.menubar_begin();
    ctx.attr_background_rgba(ctx.indexed(theme.menubar_bg));
    ctx.attr_foreground_rgba(ctx.indexed(theme.menubar_fg));
    let menu_active;
    {
        let contains_focus = ctx.contains_focus();
        menu_active = contains_focus;

        // "Files" menu
        if ctx.menubar_menu_begin("Files", 'F') {
            if ctx.menubar_menu_button("Copy", 'C', key(Action::Copy)) {
                state.execute_action(Action::Copy);
            }
            if ctx.menubar_menu_button("Rename/Move", 'R', key(Action::Move)) {
                state.execute_action(Action::Move);
            }
            if ctx.menubar_menu_button("Make directory", 'M', key(Action::MkDir)) {
                state.execute_action(Action::MkDir);
            }
            if ctx.menubar_menu_button("Delete", 'D', key(Action::Delete)) {
                state.execute_action(Action::Delete);
            }
            if ctx.menubar_menu_button("Change root", 'H', key(Action::ChangeRoot)) {
                state.execute_action(Action::ChangeRoot);
            }
            if ctx.menubar_menu_button("Directory history", 'D', key(Action::DirHistory)) {
                state.execute_action(Action::DirHistory);
            }
            if ctx.menubar_menu_button("Command history", 'O', key(Action::CmdHistory)) {
                state.execute_action(Action::CmdHistory);
            }
            if ctx.menubar_menu_button("Refresh", 'E', key(Action::Refresh)) {
                state.execute_action(Action::Refresh);
            }
            if ctx.menubar_menu_button("Exit", 'X', key(Action::Quit)) {
                state.execute_action(Action::Quit);
            }
            ctx.menubar_menu_end();
        }

        // "Commands" menu
        if ctx.menubar_menu_begin("Commands", 'C') {
            let hidden = state.active_panel().show_hidden;
            if ctx.menubar_menu_checkbox(
                "Show hidden files",
                'H',
                key(Action::ToggleHidden),
                hidden,
            ) {
                state.execute_action(Action::ToggleHidden);
            }
            if ctx.menubar_menu_checkbox(
                "Quick view",
                'Q',
                key(Action::ToggleQuickView),
                state.quick_view,
            ) {
                state.execute_action(Action::ToggleQuickView);
            }

            // Sort modes (FAR-style: Ctrl+F3..F6)
            let sort = state.active_panel().sort_by;
            if ctx.menubar_menu_checkbox(
                "Sort by name",
                'N',
                key(Action::SortBy(SortBy::Name)),
                sort == SortBy::Name,
            ) {
                state.execute_action(Action::SortBy(SortBy::Name));
            }
            if ctx.menubar_menu_checkbox(
                "Sort by extension",
                'E',
                key(Action::SortBy(SortBy::Extension)),
                sort == SortBy::Extension,
            ) {
                state.execute_action(Action::SortBy(SortBy::Extension));
            }
            if ctx.menubar_menu_checkbox(
                "Sort by date",
                'D',
                key(Action::SortBy(SortBy::Modified)),
                sort == SortBy::Modified,
            ) {
                state.execute_action(Action::SortBy(SortBy::Modified));
            }
            if ctx.menubar_menu_checkbox(
                "Sort by size",
                'S',
                key(Action::SortBy(SortBy::Size)),
                sort == SortBy::Size,
            ) {
                state.execute_action(Action::SortBy(SortBy::Size));
            }

            // Selection operations
            if ctx.menubar_menu_button("Select group  (+)", 'G', vk::NULL) {
                state.execute_action(Action::SelectGroup);
            }
            if ctx.menubar_menu_button("Deselect group  (-)", 'L', vk::NULL) {
                state.execute_action(Action::DeselectGroup);
            }
            if ctx.menubar_menu_button("Invert selection  (*)", 'I', vk::NULL) {
                state.execute_action(Action::InvertSelection);
            }
            if ctx.menubar_menu_button("Select all", 'A', key(Action::SelectAll)) {
                state.execute_action(Action::SelectAll);
            }
            if ctx.menubar_menu_button("Deselect all", 'T', vk::NULL) {
                state.execute_action(Action::DeselectAll);
            }

            ctx.menubar_menu_end();
        }

        // Global shortcuts (outside menu blocks so they fire when menus are closed).
        // Non-immediate actions are consumed here.  When the menu is active,
        // handle_keyboard() in state.rs is skipped, so we must dispatch here.
        // When the menu is NOT active, handle_keyboard() already dispatched,
        // so we only consume (to prevent the TUI from reprocessing the key).
        for binding in &bindings {
            if !binding.action.is_immediate() && ctx.consume_shortcut(binding.key) {
                if contains_focus {
                    state.execute_action(binding.action);
                }
            }
        }

        if !contains_focus && (ctx.consume_shortcut(vk::F9) || state.want_menu_focus) {
            ctx.steal_focus();
        }
    }
    ctx.menubar_end();
    menu_active
}

fn draw_panels(ctx: &mut Context, state: &State, theme: &Theme, size: Size) {
    let panel_height = size.height - 3; // minus menubar, path bar, fn bar
    let panel_width = size.width / 2;

    ctx.table_begin("panels");
    ctx.attr_intrinsic_size(Size {
        width: size.width,
        height: panel_height,
    });
    ctx.table_set_columns(&[panel_width, size.width - panel_width]);
    {
        ctx.table_next_row();

        if state.quick_view && state.active == ActivePanel::Right {
            draw_preview_panel(ctx, state, theme, panel_width, panel_height);
            draw_single_panel(
                ctx,
                &state.right,
                theme,
                "right-panel",
                true,
                size.width - panel_width,
                panel_height,
            );
        } else {
            draw_single_panel(
                ctx,
                &state.left,
                theme,
                "left-panel",
                state.active == ActivePanel::Left,
                panel_width,
                panel_height,
            );
            if state.quick_view && state.active == ActivePanel::Left {
                draw_preview_panel(ctx, state, theme, size.width - panel_width, panel_height);
            } else {
                draw_single_panel(
                    ctx,
                    &state.right,
                    theme,
                    "right-panel",
                    state.active == ActivePanel::Right,
                    size.width - panel_width,
                    panel_height,
                );
            }
        }
    }
    ctx.table_end();
}

/// Compute the display (column) width of a string, accounting for
/// multi-byte characters and wide (CJK) characters.
fn str_display_width(s: &str) -> usize {
    use ruf4_tui::unicode::tables::ucd_grapheme_cluster_character_width;
    use ruf4_tui::unicode::tables::ucd_grapheme_cluster_lookup;
    s.chars()
        .map(|c| {
            let props = ucd_grapheme_cluster_lookup(c);
            ucd_grapheme_cluster_character_width(props, 1)
        })
        .sum()
}

/// Truncate a string to fit within `max_cols` display columns.
fn truncate_to_display_width(s: &str, max_cols: usize) -> &str {
    use ruf4_tui::unicode::tables::ucd_grapheme_cluster_character_width;
    use ruf4_tui::unicode::tables::ucd_grapheme_cluster_lookup;
    let mut cols = 0;
    for (i, c) in s.char_indices() {
        let props = ucd_grapheme_cluster_lookup(c);
        let w = ucd_grapheme_cluster_character_width(props, 1);
        if cols + w > max_cols {
            return &s[..i];
        }
        cols += w;
    }
    s
}

fn draw_single_panel(
    ctx: &mut Context,
    panel: &Panel,
    theme: &Theme,
    classname: &'static str,
    is_active: bool,
    width: CoordType,
    height: CoordType,
) {
    let title_lines = 1; // path title label
    let header_lines = 1; // column header
    let footer_lines = 1; // selection/free space info
    let border_lines = 2; // top + bottom border
    let visible_height =
        (height - title_lines - header_lines - footer_lines - border_lines).max(1) as usize;

    ctx.block_begin(classname);
    // Set intrinsic size to the *inner* dimensions (excluding border).
    // The border (added by attr_border) contributes +2 to intrinsic_to_outer(),
    // so the table column sees outer = width, matching the column spec exactly.
    // This prevents column expansion and keeps both panels at identical offsets.
    ctx.attr_intrinsic_size(Size {
        width: width - 2,
        height: height - 2,
    });
    ctx.attr_border();

    if is_active {
        ctx.attr_foreground_rgba(ctx.indexed(theme.panel_border_active));
    } else {
        ctx.attr_foreground_rgba(ctx.indexed(theme.panel_border_inactive));
    }

    {
        let path_str = panel.path.to_string_lossy();
        let max_title = (width - 4).max(1) as usize;
        let title = if path_str.len() > max_title {
            &path_str[path_str.ceil_char_boundary(path_str.len() - max_title)..]
        } else {
            &path_str
        };
        ctx.label("title", title);
    }

    {
        ctx.block_begin("header");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: 1,
        });
        {
            let bg = ctx.indexed(theme.panel_header_bg);
            let fg = ctx.indexed(theme.panel_header_fg);
            ctx.attr_background_rgba(bg);
            ctx.attr_foreground_rgba(fg);

            let name_w = (width - 2 - 16 - 7 - 2).max(4);
            let header = arena_format!(
                ctx.arena(),
                "{:<nw$} {:>7} {:>16}",
                "Name",
                "Size",
                "Modified",
                nw = name_w as usize
            );
            ctx.label("header-text", &header);
        }
        ctx.block_end();
    }

    let start = panel.scroll_offset;
    let end = (start + visible_height).min(panel.entries.len());

    for i in start..end {
        let entry = &panel.entries[i];
        let is_cursor = i == panel.cursor;

        ctx.next_block_id_mixin(i as u64);
        ctx.block_begin("entry");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: 1,
        });

        if is_cursor && is_active {
            ctx.attr_background_rgba(ctx.indexed(theme.cursor_bg));
            if entry.selected {
                ctx.attr_foreground_rgba(ctx.indexed(theme.file_selected));
            } else {
                ctx.attr_foreground_rgba(ctx.indexed(theme.cursor_fg));
            }
        } else if entry.selected {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_selected));
        } else if entry.is_hidden {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_hidden));
        } else if entry.is_dir {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_dir));
        } else if entry.is_executable {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_executable));
        } else if entry.is_readonly {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_readonly));
        } else {
            ctx.attr_foreground_rgba(ctx.indexed(theme.file_normal));
        }

        let date_col = 16; // "2026-04-09 12:34"
        let size_col = 7;
        let name_w = (width - 2 - date_col - size_col - 2).max(4) as usize;

        let size_str = entry.display_size();
        let date_str = entry.display_date();

        let name_display_w = str_display_width(&entry.name);
        let line = if name_display_w <= name_w {
            let pad = name_w - name_display_w;
            arena_format!(
                ctx.arena(),
                "{}{:pad$} {:>7} {:>16}",
                entry.name,
                "",
                size_str,
                date_str,
            )
        } else {
            // Truncate the stem, keep the extension visible.
            // "very_long_name.txt" -> "very_lo….txt"
            let (stem, ext) = match entry.name.rfind('.') {
                Some(dot) if dot > 0 && dot < entry.name.len() - 1 => {
                    (&entry.name[..dot], &entry.name[dot..]) // ext includes '.'
                }
                _ => (entry.name.as_str(), ""),
            };
            let ellipsis = "\u{2026}"; // …
            let ext_w = str_display_width(ext);
            let ellipsis_w = 1; // … is 1 column wide
            let avail = name_w.saturating_sub(ext_w + ellipsis_w);
            let truncated_stem = truncate_to_display_width(stem, avail);
            let display_name = arena_format!(ctx.arena(), "{truncated_stem}{ellipsis}{ext}");
            let display_w = str_display_width(&display_name);
            let pad = name_w.saturating_sub(display_w);
            arena_format!(
                ctx.arena(),
                "{}{:pad$} {:>7} {:>16}",
                &*display_name,
                "",
                size_str,
                date_str,
            )
        };
        ctx.label("entry-text", &line);

        ctx.block_end();
    }

    let drawn = end - start;
    if drawn < visible_height {
        ctx.block_begin("filler");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: (visible_height - drawn) as CoordType,
        });
        ctx.block_end();
    }

    {
        ctx.block_begin("footer");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: 1,
        });
        {
            let bg = ctx.indexed(theme.panel_footer_bg);
            let fg = ctx.indexed(theme.panel_footer_fg);
            ctx.attr_background_rgba(bg);
            ctx.attr_foreground_rgba(fg);

            let (sel_count, sel_size) = panel.selection_info();
            let free = panel
                .free_space()
                .map(panel::format_size)
                .unwrap_or_else(|| "N/A".to_string());

            let sort_label = match panel.sort_by {
                SortBy::Name => "Name",
                SortBy::Extension => "Ext",
                SortBy::Size => "Size",
                SortBy::Modified => "Date",
            };
            let sort_arrow = match panel.sort_dir {
                SortDir::Ascending => "+",
                SortDir::Descending => "-",
            };

            let hidden_label = if panel.show_hidden { "[H]" } else { "[ ]" };
            let refreshed = &panel.last_refresh;
            let info = if sel_count > 0 {
                arena_format!(
                    ctx.arena(),
                    " {sel_count} sel, {} bytes | Sort:{sort_label}{sort_arrow} | {hidden_label} | {} free | updated {refreshed}",
                    sel_size,
                    free
                )
            } else {
                let total = panel.entries.len();
                arena_format!(
                    ctx.arena(),
                    " {total} items | Sort:{sort_label}{sort_arrow} | {hidden_label} | {} free | updated {refreshed}",
                    free
                )
            };
            ctx.label("footer-text", &info);
        }
        ctx.block_end();
    }

    ctx.block_end(); // panel
}

fn draw_preview_panel(
    ctx: &mut Context,
    state: &State,
    theme: &Theme,
    width: CoordType,
    height: CoordType,
) {
    let border_lines = 2;
    let visible_height = (height - border_lines).max(1) as usize;

    ctx.block_begin("preview-panel");
    ctx.attr_intrinsic_size(Size {
        width: width - 2,
        height: height - 2,
    });
    ctx.attr_border();
    ctx.attr_foreground_rgba(ctx.indexed(theme.preview_border));

    {
        let title = if state.preview.title.is_empty() {
            "Quick View"
        } else {
            &state.preview.title
        };
        let max_title = (width - 4).max(1) as usize;
        let display = if title.len() > max_title {
            &title[title.ceil_char_boundary(title.len() - max_title)..]
        } else {
            title
        };
        ctx.label("preview-title", display);
    }

    let lines = &state.preview.lines;
    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = state.preview_scroll.min(max_scroll);
    let end = (scroll + visible_height).min(lines.len());

    let highlights = &state.preview.highlights;

    for (i, line) in lines.iter().enumerate().skip(scroll).take(end - scroll) {
        ctx.next_block_id_mixin(i as u64);
        ctx.block_begin("preview-line");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: 1,
        });
        ctx.attr_foreground_rgba(ctx.indexed(theme.preview_text));

        let max_w = (width - 2).max(0) as usize;
        let display = if line.len() > max_w {
            &line[..line.floor_char_boundary(max_w)]
        } else {
            line
        };

        let hl = highlights.get(i);
        if hl.is_some_and(|h| !h.is_empty()) {
            draw_highlighted_line(ctx, theme, display, hl.unwrap());
        } else {
            ctx.label("preview-text", display);
        }
        ctx.block_end();
    }

    let drawn = end - scroll;
    if drawn < visible_height {
        ctx.block_begin("preview-filler");
        ctx.attr_intrinsic_size(Size {
            width: width - 2,
            height: (visible_height - drawn) as CoordType,
        });
        ctx.block_end();
    }

    ctx.block_end();
}

fn draw_highlighted_line(ctx: &mut Context, theme: &Theme, line: &str, spans: &[HighlightSpan]) {
    ctx.styled_label_begin("preview-text");

    let default_fg = ctx.indexed(theme.preview_text);
    let mut pos = 0;

    for (idx, span) in spans.iter().enumerate() {
        let start = span.start.min(line.len());
        let end = spans
            .get(idx + 1)
            .map(|s| s.start.min(line.len()))
            .unwrap_or(line.len());

        // Emit any unhighlighted gap before this span.
        if pos < start {
            let gap = &line[line.ceil_char_boundary(pos)..line.floor_char_boundary(start)];
            if !gap.is_empty() {
                ctx.styled_label_set_foreground(default_fg);
                ctx.styled_label_add_text(gap);
            }
        }

        if start < end {
            let text = &line[line.ceil_char_boundary(start)..line.floor_char_boundary(end)];
            if !text.is_empty() {
                ctx.styled_label_set_foreground(ctx.indexed(theme.highlight_color(span.kind)));
                ctx.styled_label_add_text(text);
            }
        }

        pos = end;
    }

    // Emit any trailing unhighlighted text.
    if pos < line.len() {
        ctx.styled_label_set_foreground(default_fg);
        ctx.styled_label_add_text(&line[line.ceil_char_boundary(pos)..]);
    }

    ctx.styled_label_end();
}

fn draw_path_bar(ctx: &mut Context, state: &State, theme: &Theme, size: Size) {
    ctx.block_begin("pathbar");
    ctx.attr_intrinsic_size(Size {
        width: size.width,
        height: 1,
    });
    {
        let bg = ctx.indexed(theme.pathbar_bg);
        ctx.attr_background_rgba(bg);

        let path = state.active_panel().path.to_string_lossy();
        let prompt_char = platform::prompt_symbol();

        if state.command_line_active {
            use ruf4_tui::framebuffer::Attributes;

            ctx.styled_label_begin("path-text");
            ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_path));
            let prefix = arena_format!(ctx.arena(), " {path}");
            ctx.styled_label_add_text(&prefix);
            draw_prompt_char(ctx, theme, prompt_char);
            ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_command));
            ctx.styled_label_add_text(" ");

            let cmd = &state.command_line;
            let byte_pos = cmd
                .char_indices()
                .nth(state.cmd_cursor)
                .map_or(cmd.len(), |(i, _)| i);
            let (before, after) = cmd.split_at(byte_pos);
            ctx.styled_label_add_text(before);

            ctx.styled_label_set_attributes(Attributes::Underlined);
            if after.is_empty() {
                ctx.styled_label_add_text(" ");
            } else {
                let next = after.char_indices().nth(1).map_or(after.len(), |(i, _)| i);
                ctx.styled_label_add_text(&after[..next]);
                ctx.styled_label_set_attributes(Attributes::None);
                ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_command));
                ctx.styled_label_add_text(&after[next..]);
            }
            ctx.styled_label_end();
        } else if !state.alt_search.is_empty() {
            use ruf4_tui::framebuffer::Attributes;

            ctx.styled_label_begin("path-text");
            ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_path));
            ctx.styled_label_add_text(" search: ");
            ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_search));
            ctx.styled_label_add_text(&state.alt_search);
            ctx.styled_label_set_attributes(Attributes::Underlined);
            ctx.styled_label_add_text(" ");
            ctx.styled_label_end();
        } else {
            use ruf4_tui::framebuffer::Attributes;

            ctx.styled_label_begin("path-text");
            ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_path_active));
            let text = arena_format!(ctx.arena(), " {path}");
            ctx.styled_label_add_text(&text);
            draw_prompt_char(ctx, theme, prompt_char);
            ctx.styled_label_add_text(" ");
            ctx.styled_label_set_attributes(Attributes::Underlined);
            ctx.styled_label_add_text(" ");
            ctx.styled_label_end();
        }
    }
    ctx.block_end();
}

fn draw_prompt_char(ctx: &mut Context, theme: &Theme, ch: &str) {
    if ch == "#" {
        ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_prompt_root));
        ctx.styled_label_add_text(ch);
    } else {
        ctx.styled_label_set_foreground(ctx.indexed(theme.pathbar_prompt));
        ctx.styled_label_add_text(ch);
    }
}

fn draw_clock(ctx: &mut Context, theme: &Theme, size: Size) {
    let lt = platform::local_time_now();
    let sep = if lt.sec.is_multiple_of(2) { ':' } else { ' ' };
    let time_str = arena_format!(
        ctx.arena(),
        " {:04}-{:02}-{:02} {:02}{sep}{:02} ",
        lt.year,
        lt.month,
        lt.day,
        lt.hour,
        lt.min
    );
    let time_width = time_str.len() as CoordType;

    ctx.block_begin("clock");
    ctx.attr_float(ruf4_tui::tui::FloatSpec {
        anchor: ruf4_tui::tui::Anchor::Root,
        gravity_x: 1.0,
        gravity_y: 0.0,
        offset_x: size.width as f32 - 1.0,
        offset_y: 0.0,
    });
    ctx.attr_intrinsic_size(Size {
        width: time_width,
        height: 1,
    });
    ctx.attr_background_rgba(ctx.indexed(theme.clock_bg));
    ctx.attr_foreground_rgba(ctx.indexed(theme.clock_fg));
    ctx.label("clock-text", &time_str);
    ctx.block_end();
}

fn draw_fn_bar(ctx: &mut Context, theme: &Theme, size: Size) {
    ctx.block_begin("fnbar");
    ctx.attr_intrinsic_size(Size {
        width: size.width,
        height: 1,
    });
    ctx.attr_background_rgba(ctx.indexed(theme.fnbar_bg));
    {
        let keys: &[(&str, &str)] = &[
            ("1", "Help"),
            ("2", "Save"),
            ("3", "QView"),
            ("4", "Ren"),
            ("5", "Copy"),
            ("6", "RenMov"),
            ("7", "MkDir"),
            ("8", "Delete"),
            ("9", "Menu"),
            ("10", "Quit"),
        ];

        let total_width = size.width as usize;
        let slot_width = total_width / 10;

        ctx.styled_label_begin("fn-keys");

        for (i, (num, label)) in keys.iter().enumerate() {
            let num_len = num.len();
            let label_len = label.len();
            let pad = slot_width.saturating_sub(num_len + label_len);

            ctx.styled_label_set_foreground(ctx.indexed(theme.fnbar_number));
            ctx.styled_label_add_text(num);
            ctx.styled_label_set_foreground(ctx.indexed(theme.fnbar_label));
            ctx.styled_label_add_text(label);
            if i < keys.len() - 1 {
                ctx.styled_label_set_foreground(ctx.indexed(theme.fnbar_spacer));
                let spaces = arena_format!(ctx.arena(), "{:>pad$}", "", pad = pad);
                ctx.styled_label_add_text(&spaces);
            }
        }

        ctx.styled_label_end();
    }
    ctx.block_end();
}

fn draw_dialog(ctx: &mut Context, state: &mut State, theme: &Theme, size: Size) {
    let input_cursor = state.input_cursor;
    match &mut state.dialog {
        Dialog::None => {}
        Dialog::Help { scroll } => draw_help_dialog(ctx, theme, &state.help_text, *scroll, size),
        Dialog::MkDir { name } => draw_mkdir_dialog(ctx, theme, name, input_cursor, size),
        Dialog::Rename { name } => draw_rename_dialog(ctx, theme, name, input_cursor, size),
        Dialog::Delete { files } => draw_delete_dialog(ctx, theme, files, size),
        Dialog::Copy { files, dest } => {
            draw_copy_move_dialog(ctx, theme, "Copy", files, dest, input_cursor, size)
        }
        Dialog::Move { files, dest } => {
            draw_copy_move_dialog(ctx, theme, "Rename/Move", files, dest, input_cursor, size)
        }
        Dialog::Info { message } => draw_info_dialog(ctx, theme, message, size),
        Dialog::Error { message } => draw_error_dialog(ctx, theme, message, size),
        Dialog::ShellOutput {
            command,
            output,
            scroll,
        } => draw_shell_output_dialog(ctx, theme, command, output, *scroll, size),
        Dialog::ConfirmQuit { save_settings } => {
            draw_confirm_quit_dialog(ctx, theme, save_settings, size)
        }
        Dialog::SelectGroup { pattern, select } => {
            draw_select_group_dialog(ctx, theme, pattern, *select, input_cursor, size)
        }
        Dialog::ListSelect {
            title,
            prompt,
            labels,
            cursor,
            min_width,
            ..
        } => {
            draw_list_dialog(
                ctx,
                theme,
                "list-dialog",
                title,
                prompt,
                labels,
                *cursor,
                *min_width,
                size,
            );
        }
        Dialog::ConfirmOverwrite {
            target_name,
            is_copy,
            ..
        } => draw_confirm_overwrite_dialog(ctx, theme, target_name, *is_copy, size),
    }
}

// Dialog framework

struct DialogSpec {
    id: &'static str,
    bg: IndexedColor,
    preferred_width: CoordType,
}

const DIALOG_BLUE_50: DialogSpec = DialogSpec {
    id: "blue-dialog",
    bg: IndexedColor::Blue,
    preferred_width: 50,
};

const DIALOG_BLUE_60: DialogSpec = DialogSpec {
    id: "blue-dialog-wide",
    bg: IndexedColor::Blue,
    preferred_width: 60,
};

const DIALOG_RED_44: DialogSpec = DialogSpec {
    id: "red-dialog",
    bg: IndexedColor::Red,
    preferred_width: 44,
};

const DIALOG_RED_60: DialogSpec = DialogSpec {
    id: "red-dialog-wide",
    bg: IndexedColor::Red,
    preferred_width: 60,
};

fn dialog_begin(
    ctx: &mut Context,
    theme: &Theme,
    spec: &DialogSpec,
    title: &str,
    height: CoordType,
    term: Size,
) -> CoordType {
    // Ensure width fits both the preferred content width and the title (+ border decoration).
    let title_width = title.len() as CoordType + 4;
    let w = spec.preferred_width.max(title_width).min(term.width - 4);
    let h = height.min(term.height - 4);
    ctx.modal_begin(spec.id, title);
    ctx.attr_intrinsic_size(Size {
        width: w,
        height: h,
    });
    ctx.attr_background_rgba(ctx.indexed(spec.bg));
    ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_fg));
    dialog_spacer(ctx, "sp-top");
    w
}

fn dialog_end(ctx: &mut Context) {
    dialog_spacer(ctx, "sp-bot");
    ctx.modal_end();
}

fn dialog_spacer(ctx: &mut Context, id: &'static str) {
    ctx.block_begin(id);
    ctx.attr_intrinsic_size(Size {
        width: 1,
        height: 1,
    });
    ctx.block_end();
}

fn dialog_prompt(ctx: &mut Context, id: &'static str, text: &str) {
    ctx.label(id, text);
}

fn dialog_input(
    ctx: &mut Context,
    theme: &Theme,
    id: &'static str,
    text: &str,
    cursor: usize,
    width: CoordType,
) {
    use ruf4_tui::framebuffer::Attributes;

    ctx.block_begin(id);
    ctx.attr_intrinsic_size(Size {
        width: width - 4,
        height: 1,
    });
    ctx.attr_background_rgba(ctx.indexed(theme.dialog_input_bg));
    ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_input_fg));
    {
        let byte_pos = text
            .char_indices()
            .nth(cursor)
            .map_or(text.len(), |(i, _)| i);
        let (before, after) = text.split_at(byte_pos);

        ctx.styled_label_begin("input-text");
        ctx.styled_label_add_text(before);

        // Draw the character under the cursor underlined; use a space if at end.
        ctx.styled_label_set_attributes(Attributes::Underlined);
        if after.is_empty() {
            ctx.styled_label_add_text(" ");
        } else {
            let next = after.char_indices().nth(1).map_or(after.len(), |(i, _)| i);
            ctx.styled_label_add_text(&after[..next]);
            ctx.styled_label_set_attributes(Attributes::None);
            ctx.styled_label_add_text(&after[next..]);
        }
        ctx.styled_label_end();
    }
    ctx.block_end();
}

fn dialog_file_list(
    ctx: &mut Context,
    theme: &Theme,
    files: &[String],
    max_show: usize,
    width: CoordType,
) {
    for (i, name) in files.iter().enumerate().take(max_show) {
        ctx.next_block_id_mixin(i as u64);
        ctx.block_begin("file-entry");
        ctx.attr_intrinsic_size(Size {
            width: width - 4,
            height: 1,
        });
        ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_file_list));
        {
            let entry = arena_format!(ctx.arena(), "  {name}");
            ctx.label("file-name", &entry);
        }
        ctx.block_end();
    }
    if files.len() > max_show {
        let more = arena_format!(ctx.arena(), "  ...and {} more", files.len() - max_show);
        ctx.label("file-more", &more);
    }
}

// Individual dialogs

fn draw_mkdir_dialog(ctx: &mut Context, theme: &Theme, name: &str, cursor: usize, size: Size) {
    let spec = DialogSpec {
        bg: theme.dialog_info_bg,
        ..DIALOG_BLUE_50
    };
    let w = dialog_begin(
        ctx,
        theme,
        &spec,
        "Make Directory - Enter=OK  Esc=Cancel",
        6,
        size,
    );
    dialog_prompt(ctx, "prompt", "Enter directory name:");
    dialog_spacer(ctx, "sp-mid");
    dialog_input(ctx, theme, "input", name, cursor, w);
    dialog_end(ctx);
}

fn draw_rename_dialog(ctx: &mut Context, theme: &Theme, name: &str, cursor: usize, size: Size) {
    let spec = DialogSpec {
        bg: theme.dialog_info_bg,
        ..DIALOG_BLUE_50
    };
    let w = dialog_begin(ctx, theme, &spec, "Rename - Enter=OK  Esc=Cancel", 6, size);
    dialog_prompt(ctx, "prompt", "Enter new name:");
    dialog_spacer(ctx, "sp-mid");
    dialog_input(ctx, theme, "input", name, cursor, w);
    dialog_end(ctx);
}

fn draw_delete_dialog(ctx: &mut Context, theme: &Theme, files: &[String], size: Size) {
    let max_name = files.iter().map(|f| f.len()).max().unwrap_or(0);
    let spec = DialogSpec {
        bg: theme.dialog_error_bg,
        preferred_width: (max_name as CoordType + 10).max(DIALOG_RED_44.preferred_width),
        ..DIALOG_RED_44
    };
    let h = files.len() as CoordType + 5;
    let w = dialog_begin(
        ctx,
        theme,
        &spec,
        "Delete - Y/Enter=Delete  N/Esc=Cancel",
        h,
        size,
    );
    {
        let msg = if files.len() == 1 {
            arena_format!(ctx.arena(), "Delete \"{}\"?", files[0])
        } else {
            arena_format!(ctx.arena(), "Delete {} files?", files.len())
        };
        dialog_prompt(ctx, "prompt", &msg);
        dialog_spacer(ctx, "sp-mid");
        let max_show = (h.min(size.height - 4) - 4).max(0) as usize;
        dialog_file_list(ctx, theme, files, max_show, w);
    }
    dialog_end(ctx);
}

fn draw_copy_move_dialog(
    ctx: &mut Context,
    theme: &Theme,
    title: &str,
    files: &[String],
    dest: &str,
    cursor: usize,
    size: Size,
) {
    let spec = DialogSpec {
        bg: theme.dialog_info_bg,
        ..DIALOG_BLUE_60
    };
    let caption = arena_format!(ctx.arena(), "{title} - Enter=OK  Esc=Cancel");
    let w = dialog_begin(ctx, theme, &spec, &caption, 8, size);
    {
        let msg = if files.len() == 1 {
            arena_format!(ctx.arena(), "{title} \"{}\" to:", files[0])
        } else {
            arena_format!(ctx.arena(), "{title} {} files to:", files.len())
        };
        dialog_prompt(ctx, "prompt", &msg);
        dialog_spacer(ctx, "sp-mid");
        dialog_input(ctx, theme, "input", dest, cursor, w);
    }
    dialog_end(ctx);
}

fn draw_error_dialog(ctx: &mut Context, theme: &Theme, message: &str, size: Size) {
    let msg_width = message.lines().map(|l| l.len()).max().unwrap_or(10);
    let msg_lines = message.lines().count();
    let spec = DialogSpec {
        bg: theme.dialog_error_bg,
        preferred_width: (msg_width as CoordType + 6).max(30),
        ..DIALOG_RED_44
    };
    let h = msg_lines as CoordType + 4;
    let w = dialog_begin(ctx, theme, &spec, "Error - Enter/Esc=Close", h, size);
    {
        for (i, line) in message.lines().enumerate() {
            ctx.next_block_id_mixin(i as u64);
            ctx.block_begin("err-line");
            ctx.attr_intrinsic_size(Size {
                width: w - 4,
                height: 1,
            });
            ctx.label("err-text", line);
            ctx.block_end();
        }
    }
    dialog_end(ctx);
}

fn draw_info_dialog(ctx: &mut Context, theme: &Theme, message: &str, size: Size) {
    let msg_width = message.lines().map(|l| l.len()).max().unwrap_or(10);
    let msg_lines = message.lines().count();
    let spec = DialogSpec {
        bg: theme.dialog_info_bg,
        preferred_width: (msg_width as CoordType + 6).max(30),
        ..DIALOG_BLUE_50
    };
    let h = msg_lines as CoordType + 4;
    let w = dialog_begin(ctx, theme, &spec, "Info - Enter/Esc=Close", h, size);
    {
        for (i, line) in message.lines().enumerate() {
            ctx.next_block_id_mixin(i as u64);
            ctx.block_begin("info-line");
            ctx.attr_intrinsic_size(Size {
                width: w - 4,
                height: 1,
            });
            ctx.label("info-text", line);
            ctx.block_end();
        }
    }
    dialog_end(ctx);
}

fn draw_help_dialog(
    ctx: &mut Context,
    theme: &Theme,
    help_text: &[(String, &str, Action)],
    scroll: usize,
    size: Size,
) {
    let key_width = help_text.iter().map(|(k, _, _)| k.len()).max().unwrap_or(0);
    let max_line = help_text
        .iter()
        .map(|(k, v, _)| {
            if k.is_empty() {
                0
            } else {
                k.len() + 3 + v.len()
            }
        })
        .max()
        .unwrap_or(20);
    let total = help_text.len() as CoordType;
    let max_visible = (size.height - 8).max(4);
    let content_h = total.min(max_visible);
    let h = content_h + 4;
    let spec = DialogSpec {
        preferred_width: (max_line as CoordType + 6).max(40),
        bg: theme.dialog_info_bg,
        ..DIALOG_BLUE_50
    };
    let caption = if total > max_visible {
        let page = scroll + 1;
        let pages = (help_text.len() as f32 / max_visible as f32).ceil() as usize;
        arena_format!(
            ctx.arena(),
            "Help ({page}/{pages}) - Up/Down=Scroll  Click=Run  Esc=Close"
        )
    } else {
        arena_format!(ctx.arena(), "Help - Click=Run  Esc=Close")
    };
    let w = dialog_begin(ctx, theme, &spec, &caption, h, size);
    {
        let visible = content_h as usize;
        for i in 0..visible {
            let idx = scroll + i;
            let (key, desc) = if idx < help_text.len() {
                (help_text[idx].0.as_str(), help_text[idx].1)
            } else {
                ("", "")
            };
            ctx.next_block_id_mixin(i as u64);
            ctx.block_begin("help-line");
            ctx.attr_intrinsic_size(Size {
                width: w - 4,
                height: 1,
            });
            if key.is_empty() {
                ctx.label("help-blank", "");
            } else {
                let line =
                    arena_format!(ctx.arena(), "{:<width$}   {}", key, desc, width = key_width);
                ctx.label("help-text", &line);
            }
            ctx.block_end();
        }
    }
    dialog_end(ctx);
}

fn draw_shell_output_dialog(
    ctx: &mut Context,
    theme: &Theme,
    command: &str,
    output: &str,
    scroll: usize,
    size: Size,
) {
    let w = (size.width - 4).max(20);
    let h = (size.height - 4).max(8);

    let title = arena_format!(ctx.arena(), "$ {command} - Ctrl+C=Copy  Esc/Enter=Close");
    ctx.modal_begin("shell-dialog", &title);
    ctx.attr_intrinsic_size(Size {
        width: w,
        height: h,
    });
    ctx.attr_background_rgba(ctx.indexed(theme.dialog_shell_bg));
    ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_shell_fg));
    {
        let lines: Vec<&str> = output.lines().collect();
        let visible = (h - 2).max(1) as usize;
        let max_scroll = lines.len().saturating_sub(visible);
        let scroll = scroll.min(max_scroll);
        let end = (scroll + visible).min(lines.len());

        for (i, line) in lines[scroll..end].iter().enumerate() {
            ctx.next_block_id_mixin(i as u64);
            ctx.block_begin("out-line");
            ctx.attr_intrinsic_size(Size {
                width: w - 4,
                height: 1,
            });
            ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_shell_text));
            ctx.label("out-text", line);
            ctx.block_end();
        }

        if lines.len() > visible {
            let indicator =
                arena_format!(ctx.arena(), " [{}-{} of {}]", scroll + 1, end, lines.len());
            ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_shell_scroll_info));
            ctx.label("scroll-info", &indicator);
        }
    }
    ctx.modal_end();
}

fn draw_select_group_dialog(
    ctx: &mut Context,
    theme: &Theme,
    pattern: &str,
    select: bool,
    cursor: usize,
    size: Size,
) {
    let title = if select {
        "Select Group - Enter=OK  Esc=Cancel"
    } else {
        "Deselect Group - Enter=OK  Esc=Cancel"
    };
    let spec = DialogSpec {
        bg: theme.dialog_info_bg,
        ..DIALOG_BLUE_50
    };
    let w = dialog_begin(ctx, theme, &spec, title, 6, size);
    {
        let prompt = if select {
            "Select files matching pattern:"
        } else {
            "Deselect files matching pattern:"
        };
        dialog_prompt(ctx, "prompt", prompt);
        dialog_spacer(ctx, "sp-mid");
        dialog_input(ctx, theme, "input", pattern, cursor, w);
    }
    dialog_end(ctx);
}

fn draw_confirm_overwrite_dialog(
    ctx: &mut Context,
    theme: &Theme,
    target_name: &str,
    is_copy: bool,
    size: Size,
) {
    let op = if is_copy { "Copy" } else { "Move" };
    let caption = arena_format!(ctx.arena(), "{op} - Y=Overwrite  N=Skip  A=All  Esc=Cancel");
    let spec = DialogSpec {
        bg: theme.dialog_error_bg,
        ..DIALOG_RED_60
    };
    dialog_begin(ctx, theme, &spec, &caption, 4, size);
    {
        let msg = arena_format!(ctx.arena(), "Overwrite \"{}\"?", target_name);
        dialog_prompt(ctx, "prompt", &msg);
    }
    dialog_end(ctx);
}

#[allow(clippy::too_many_arguments)]
fn draw_list_dialog(
    ctx: &mut Context,
    theme: &Theme,
    id: &'static str,
    caption: &str,
    prompt: &str,
    entries: &[impl AsRef<str>],
    cursor: usize,
    min_width: CoordType,
    size: Size,
) {
    let max_len = entries.iter().map(|e| e.as_ref().len()).max().unwrap_or(10);
    let spec = DialogSpec {
        id,
        bg: theme.dialog_info_bg,
        preferred_width: (max_len as CoordType + 8).max(min_width),
        ..DIALOG_BLUE_50
    };
    let h = entries.len() as CoordType + 4;
    let w = dialog_begin(ctx, theme, &spec, caption, h, size);
    dialog_prompt(ctx, "prompt", prompt);

    let max_show = (h.min(size.height - 4) - 3).max(0) as usize;
    for (i, entry) in entries.iter().enumerate().take(max_show) {
        ctx.next_block_id_mixin(i as u64);
        ctx.block_begin("list-entry");
        ctx.attr_intrinsic_size(Size {
            width: w - 4,
            height: 1,
        });
        if i == cursor {
            ctx.attr_background_rgba(ctx.indexed(theme.dialog_list_cursor_bg));
            ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_list_cursor_fg));
        } else {
            ctx.attr_foreground_rgba(ctx.indexed(theme.dialog_list_fg));
        }
        ctx.label("list-name", entry.as_ref());
        ctx.block_end();
    }

    dialog_end(ctx);
}

fn draw_confirm_quit_dialog(
    ctx: &mut Context,
    theme: &Theme,
    save_settings: &mut bool,
    size: Size,
) {
    let spec = DialogSpec {
        id: "quit-dialog",
        bg: theme.dialog_error_bg,
        preferred_width: 44,
        ..DIALOG_RED_44
    };
    dialog_begin(
        ctx,
        theme,
        &spec,
        "Quit - Y/Enter=Exit  N/Esc=Cancel",
        6,
        size,
    );
    dialog_prompt(ctx, "prompt", "Do you want to quit ruf4?");
    ctx.label("spacer", "");
    ctx.checkbox("save-checkbox", "Save settings on exit", save_settings);
    dialog_end(ctx);
}
