// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Color theme system.
//!
//! Every color used in the UI is addressed through a [`Theme`] struct so that
//! the entire palette can be swapped without touching drawing code.

use ruf4_tui::framebuffer::IndexedColor;

use crate::lsh::HighlightKind;

/// Complete color palette for the UI.
#[derive(Clone)]
pub struct Theme {
    // ── Menubar ────────────────────────────────────────────────────────
    pub menubar_bg: IndexedColor,
    pub menubar_fg: IndexedColor,

    // ── Panels ─────────────────────────────────────────────────────────
    pub panel_border_active: IndexedColor,
    pub panel_border_inactive: IndexedColor,
    pub panel_header_bg: IndexedColor,
    pub panel_header_fg: IndexedColor,
    pub panel_footer_bg: IndexedColor,
    pub panel_footer_fg: IndexedColor,

    // ── File entries ───────────────────────────────────────────────────
    pub cursor_bg: IndexedColor,
    pub cursor_fg: IndexedColor,
    pub file_selected: IndexedColor,
    pub file_dir: IndexedColor,
    pub file_executable: IndexedColor,
    pub file_readonly: IndexedColor,
    pub file_normal: IndexedColor,

    // ── Preview panel ──────────────────────────────────────────────────
    pub preview_border: IndexedColor,
    pub preview_text: IndexedColor,

    // ── Path bar ───────────────────────────────────────────────────────
    pub pathbar_bg: IndexedColor,
    pub pathbar_path: IndexedColor,
    pub pathbar_path_active: IndexedColor,
    pub pathbar_command: IndexedColor,
    pub pathbar_search: IndexedColor,
    pub pathbar_prompt: IndexedColor,
    pub pathbar_prompt_root: IndexedColor,

    // ── Clock ──────────────────────────────────────────────────────────
    pub clock_bg: IndexedColor,
    pub clock_fg: IndexedColor,

    // ── Function key bar ───────────────────────────────────────────────
    pub fnbar_bg: IndexedColor,
    pub fnbar_number: IndexedColor,
    pub fnbar_label: IndexedColor,
    pub fnbar_spacer: IndexedColor,

    // ── Dialogs ────────────────────────────────────────────────────────
    pub dialog_info_bg: IndexedColor,
    pub dialog_error_bg: IndexedColor,
    pub dialog_fg: IndexedColor,
    pub dialog_input_bg: IndexedColor,
    pub dialog_input_fg: IndexedColor,
    pub dialog_file_list: IndexedColor,
    pub dialog_list_cursor_bg: IndexedColor,
    pub dialog_list_cursor_fg: IndexedColor,
    pub dialog_list_fg: IndexedColor,
    pub dialog_shell_bg: IndexedColor,
    pub dialog_shell_fg: IndexedColor,
    pub dialog_shell_text: IndexedColor,
    pub dialog_shell_scroll_info: IndexedColor,

    // ── Floater defaults (used by TUI for modal backdrop) ──────────────
    pub floater_bg: IndexedColor,
    pub floater_fg: IndexedColor,

    // ── Syntax highlight ───────────────────────────────────────────────
    pub hl_comment: IndexedColor,
    pub hl_string: IndexedColor,
    pub hl_keyword: IndexedColor,
    pub hl_number: IndexedColor,
    pub hl_constant: IndexedColor,
    pub hl_method: IndexedColor,
    pub hl_variable: IndexedColor,
    pub hl_markup_heading: IndexedColor,
    pub hl_markup_bold: IndexedColor,
    pub hl_markup_italic: IndexedColor,
    pub hl_markup_link: IndexedColor,
    pub hl_markup_list: IndexedColor,
    pub hl_markup_inserted: IndexedColor,
    pub hl_markup_deleted: IndexedColor,
    pub hl_markup_changed: IndexedColor,
    pub hl_markup_strikethrough: IndexedColor,
    pub hl_meta_header: IndexedColor,
    pub hl_other: IndexedColor,
}

// ── Color serialization ───────────────────────────────────────────────────

pub fn color_str(c: IndexedColor) -> &'static str {
    match c {
        IndexedColor::Black => "black",
        IndexedColor::Red => "red",
        IndexedColor::Green => "green",
        IndexedColor::Yellow => "yellow",
        IndexedColor::Blue => "blue",
        IndexedColor::Magenta => "magenta",
        IndexedColor::Cyan => "cyan",
        IndexedColor::White => "white",
        IndexedColor::BrightBlack => "bright_black",
        IndexedColor::BrightRed => "bright_red",
        IndexedColor::BrightGreen => "bright_green",
        IndexedColor::BrightYellow => "bright_yellow",
        IndexedColor::BrightBlue => "bright_blue",
        IndexedColor::BrightMagenta => "bright_magenta",
        IndexedColor::BrightCyan => "bright_cyan",
        IndexedColor::BrightWhite => "bright_white",
        _ => "white",
    }
}

pub fn parse_color(s: &str) -> Option<IndexedColor> {
    Some(match s {
        "black" => IndexedColor::Black,
        "red" => IndexedColor::Red,
        "green" => IndexedColor::Green,
        "yellow" => IndexedColor::Yellow,
        "blue" => IndexedColor::Blue,
        "magenta" => IndexedColor::Magenta,
        "cyan" => IndexedColor::Cyan,
        "white" => IndexedColor::White,
        "bright_black" => IndexedColor::BrightBlack,
        "bright_red" => IndexedColor::BrightRed,
        "bright_green" => IndexedColor::BrightGreen,
        "bright_yellow" => IndexedColor::BrightYellow,
        "bright_blue" => IndexedColor::BrightBlue,
        "bright_magenta" => IndexedColor::BrightMagenta,
        "bright_cyan" => IndexedColor::BrightCyan,
        "bright_white" => IndexedColor::BrightWhite,
        _ => return None,
    })
}

// ── Theme field access by name ────────────────────────────────────────────

/// All theme field names, for iteration during save.
pub const THEME_FIELDS: &[&str] = &[
    "menubar_bg", "menubar_fg",
    "panel_border_active", "panel_border_inactive",
    "panel_header_bg", "panel_header_fg",
    "panel_footer_bg", "panel_footer_fg",
    "cursor_bg", "cursor_fg",
    "file_selected", "file_dir", "file_executable", "file_readonly", "file_normal",
    "preview_border", "preview_text",
    "pathbar_bg", "pathbar_path", "pathbar_path_active", "pathbar_command",
    "pathbar_search", "pathbar_prompt", "pathbar_prompt_root",
    "clock_bg", "clock_fg",
    "fnbar_bg", "fnbar_number", "fnbar_label", "fnbar_spacer",
    "dialog_info_bg", "dialog_error_bg", "dialog_fg",
    "dialog_input_bg", "dialog_input_fg",
    "dialog_file_list",
    "dialog_list_cursor_bg", "dialog_list_cursor_fg", "dialog_list_fg",
    "dialog_shell_bg", "dialog_shell_fg", "dialog_shell_text", "dialog_shell_scroll_info",
    "floater_bg", "floater_fg",
    "hl_comment", "hl_string", "hl_keyword", "hl_number", "hl_constant",
    "hl_method", "hl_variable",
    "hl_markup_heading", "hl_markup_bold", "hl_markup_italic", "hl_markup_link",
    "hl_markup_list", "hl_markup_inserted", "hl_markup_deleted", "hl_markup_changed",
    "hl_markup_strikethrough", "hl_meta_header", "hl_other",
];

impl Theme {
    /// Get a theme field by name.
    pub fn get_field(&self, name: &str) -> Option<IndexedColor> {
        Some(match name {
            "menubar_bg" => self.menubar_bg,
            "menubar_fg" => self.menubar_fg,
            "panel_border_active" => self.panel_border_active,
            "panel_border_inactive" => self.panel_border_inactive,
            "panel_header_bg" => self.panel_header_bg,
            "panel_header_fg" => self.panel_header_fg,
            "panel_footer_bg" => self.panel_footer_bg,
            "panel_footer_fg" => self.panel_footer_fg,
            "cursor_bg" => self.cursor_bg,
            "cursor_fg" => self.cursor_fg,
            "file_selected" => self.file_selected,
            "file_dir" => self.file_dir,
            "file_executable" => self.file_executable,
            "file_readonly" => self.file_readonly,
            "file_normal" => self.file_normal,
            "preview_border" => self.preview_border,
            "preview_text" => self.preview_text,
            "pathbar_bg" => self.pathbar_bg,
            "pathbar_path" => self.pathbar_path,
            "pathbar_path_active" => self.pathbar_path_active,
            "pathbar_command" => self.pathbar_command,
            "pathbar_search" => self.pathbar_search,
            "pathbar_prompt" => self.pathbar_prompt,
            "pathbar_prompt_root" => self.pathbar_prompt_root,
            "clock_bg" => self.clock_bg,
            "clock_fg" => self.clock_fg,
            "fnbar_bg" => self.fnbar_bg,
            "fnbar_number" => self.fnbar_number,
            "fnbar_label" => self.fnbar_label,
            "fnbar_spacer" => self.fnbar_spacer,
            "dialog_info_bg" => self.dialog_info_bg,
            "dialog_error_bg" => self.dialog_error_bg,
            "dialog_fg" => self.dialog_fg,
            "dialog_input_bg" => self.dialog_input_bg,
            "dialog_input_fg" => self.dialog_input_fg,
            "dialog_file_list" => self.dialog_file_list,
            "dialog_list_cursor_bg" => self.dialog_list_cursor_bg,
            "dialog_list_cursor_fg" => self.dialog_list_cursor_fg,
            "dialog_list_fg" => self.dialog_list_fg,
            "dialog_shell_bg" => self.dialog_shell_bg,
            "dialog_shell_fg" => self.dialog_shell_fg,
            "dialog_shell_text" => self.dialog_shell_text,
            "dialog_shell_scroll_info" => self.dialog_shell_scroll_info,
            "floater_bg" => self.floater_bg,
            "floater_fg" => self.floater_fg,
            "hl_comment" => self.hl_comment,
            "hl_string" => self.hl_string,
            "hl_keyword" => self.hl_keyword,
            "hl_number" => self.hl_number,
            "hl_constant" => self.hl_constant,
            "hl_method" => self.hl_method,
            "hl_variable" => self.hl_variable,
            "hl_markup_heading" => self.hl_markup_heading,
            "hl_markup_bold" => self.hl_markup_bold,
            "hl_markup_italic" => self.hl_markup_italic,
            "hl_markup_link" => self.hl_markup_link,
            "hl_markup_list" => self.hl_markup_list,
            "hl_markup_inserted" => self.hl_markup_inserted,
            "hl_markup_deleted" => self.hl_markup_deleted,
            "hl_markup_changed" => self.hl_markup_changed,
            "hl_markup_strikethrough" => self.hl_markup_strikethrough,
            "hl_meta_header" => self.hl_meta_header,
            "hl_other" => self.hl_other,
            _ => return None,
        })
    }

    /// Set a theme field by name. Returns `true` if the field was found.
    pub fn set_field(&mut self, name: &str, color: IndexedColor) -> bool {
        match name {
            "menubar_bg" => self.menubar_bg = color,
            "menubar_fg" => self.menubar_fg = color,
            "panel_border_active" => self.panel_border_active = color,
            "panel_border_inactive" => self.panel_border_inactive = color,
            "panel_header_bg" => self.panel_header_bg = color,
            "panel_header_fg" => self.panel_header_fg = color,
            "panel_footer_bg" => self.panel_footer_bg = color,
            "panel_footer_fg" => self.panel_footer_fg = color,
            "cursor_bg" => self.cursor_bg = color,
            "cursor_fg" => self.cursor_fg = color,
            "file_selected" => self.file_selected = color,
            "file_dir" => self.file_dir = color,
            "file_executable" => self.file_executable = color,
            "file_readonly" => self.file_readonly = color,
            "file_normal" => self.file_normal = color,
            "preview_border" => self.preview_border = color,
            "preview_text" => self.preview_text = color,
            "pathbar_bg" => self.pathbar_bg = color,
            "pathbar_path" => self.pathbar_path = color,
            "pathbar_path_active" => self.pathbar_path_active = color,
            "pathbar_command" => self.pathbar_command = color,
            "pathbar_search" => self.pathbar_search = color,
            "pathbar_prompt" => self.pathbar_prompt = color,
            "pathbar_prompt_root" => self.pathbar_prompt_root = color,
            "clock_bg" => self.clock_bg = color,
            "clock_fg" => self.clock_fg = color,
            "fnbar_bg" => self.fnbar_bg = color,
            "fnbar_number" => self.fnbar_number = color,
            "fnbar_label" => self.fnbar_label = color,
            "fnbar_spacer" => self.fnbar_spacer = color,
            "dialog_info_bg" => self.dialog_info_bg = color,
            "dialog_error_bg" => self.dialog_error_bg = color,
            "dialog_fg" => self.dialog_fg = color,
            "dialog_input_bg" => self.dialog_input_bg = color,
            "dialog_input_fg" => self.dialog_input_fg = color,
            "dialog_file_list" => self.dialog_file_list = color,
            "dialog_list_cursor_bg" => self.dialog_list_cursor_bg = color,
            "dialog_list_cursor_fg" => self.dialog_list_cursor_fg = color,
            "dialog_list_fg" => self.dialog_list_fg = color,
            "dialog_shell_bg" => self.dialog_shell_bg = color,
            "dialog_shell_fg" => self.dialog_shell_fg = color,
            "dialog_shell_text" => self.dialog_shell_text = color,
            "dialog_shell_scroll_info" => self.dialog_shell_scroll_info = color,
            "floater_bg" => self.floater_bg = color,
            "floater_fg" => self.floater_fg = color,
            "hl_comment" => self.hl_comment = color,
            "hl_string" => self.hl_string = color,
            "hl_keyword" => self.hl_keyword = color,
            "hl_number" => self.hl_number = color,
            "hl_constant" => self.hl_constant = color,
            "hl_method" => self.hl_method = color,
            "hl_variable" => self.hl_variable = color,
            "hl_markup_heading" => self.hl_markup_heading = color,
            "hl_markup_bold" => self.hl_markup_bold = color,
            "hl_markup_italic" => self.hl_markup_italic = color,
            "hl_markup_link" => self.hl_markup_link = color,
            "hl_markup_list" => self.hl_markup_list = color,
            "hl_markup_inserted" => self.hl_markup_inserted = color,
            "hl_markup_deleted" => self.hl_markup_deleted = color,
            "hl_markup_changed" => self.hl_markup_changed = color,
            "hl_markup_strikethrough" => self.hl_markup_strikethrough = color,
            "hl_meta_header" => self.hl_meta_header = color,
            "hl_other" => self.hl_other = color,
            _ => return false,
        }
        true
    }

    /// The default FAR Manager 3 inspired color scheme.
    pub fn far() -> Self {
        Self {
            // Menubar
            menubar_bg: IndexedColor::Black,
            menubar_fg: IndexedColor::BrightWhite,

            // Panels
            panel_border_active: IndexedColor::BrightCyan,
            panel_border_inactive: IndexedColor::Cyan,
            panel_header_bg: IndexedColor::Cyan,
            panel_header_fg: IndexedColor::Black,
            panel_footer_bg: IndexedColor::Cyan,
            panel_footer_fg: IndexedColor::Black,

            // File entries
            cursor_bg: IndexedColor::Cyan,
            cursor_fg: IndexedColor::Black,
            file_selected: IndexedColor::BrightYellow,
            file_dir: IndexedColor::BrightWhite,
            file_executable: IndexedColor::BrightGreen,
            file_readonly: IndexedColor::BrightBlack,
            file_normal: IndexedColor::White,

            // Preview
            preview_border: IndexedColor::BrightGreen,
            preview_text: IndexedColor::White,

            // Path bar
            pathbar_bg: IndexedColor::Black,
            pathbar_path: IndexedColor::White,
            pathbar_path_active: IndexedColor::BrightWhite,
            pathbar_command: IndexedColor::BrightWhite,
            pathbar_search: IndexedColor::BrightYellow,
            pathbar_prompt: IndexedColor::White,
            pathbar_prompt_root: IndexedColor::BrightRed,

            // Clock
            clock_bg: IndexedColor::Cyan,
            clock_fg: IndexedColor::Black,

            // Function key bar
            fnbar_bg: IndexedColor::Black,
            fnbar_number: IndexedColor::BrightWhite,
            fnbar_label: IndexedColor::Cyan,
            fnbar_spacer: IndexedColor::Black,

            // Dialogs
            dialog_info_bg: IndexedColor::Blue,
            dialog_error_bg: IndexedColor::Red,
            dialog_fg: IndexedColor::BrightWhite,
            dialog_input_bg: IndexedColor::Cyan,
            dialog_input_fg: IndexedColor::Black,
            dialog_file_list: IndexedColor::BrightYellow,
            dialog_list_cursor_bg: IndexedColor::Cyan,
            dialog_list_cursor_fg: IndexedColor::Black,
            dialog_list_fg: IndexedColor::BrightWhite,
            dialog_shell_bg: IndexedColor::Black,
            dialog_shell_fg: IndexedColor::White,
            dialog_shell_text: IndexedColor::BrightWhite,
            dialog_shell_scroll_info: IndexedColor::BrightBlack,

            // Floaters
            floater_bg: IndexedColor::Cyan,
            floater_fg: IndexedColor::Black,

            // Syntax highlighting
            hl_comment: IndexedColor::BrightBlack,
            hl_string: IndexedColor::Green,
            hl_keyword: IndexedColor::BrightYellow,
            hl_number: IndexedColor::BrightCyan,
            hl_constant: IndexedColor::BrightMagenta,
            hl_method: IndexedColor::BrightGreen,
            hl_variable: IndexedColor::Cyan,
            hl_markup_heading: IndexedColor::BrightYellow,
            hl_markup_bold: IndexedColor::BrightWhite,
            hl_markup_italic: IndexedColor::White,
            hl_markup_link: IndexedColor::BrightCyan,
            hl_markup_list: IndexedColor::Yellow,
            hl_markup_inserted: IndexedColor::BrightGreen,
            hl_markup_deleted: IndexedColor::BrightRed,
            hl_markup_changed: IndexedColor::BrightYellow,
            hl_markup_strikethrough: IndexedColor::BrightBlack,
            hl_meta_header: IndexedColor::BrightMagenta,
            hl_other: IndexedColor::White,
        }
    }

    /// Resolve a syntax highlight kind to its theme color.
    pub fn highlight_color(&self, kind: HighlightKind) -> IndexedColor {
        match kind {
            HighlightKind::Comment => self.hl_comment,
            HighlightKind::String => self.hl_string,
            HighlightKind::KeywordControl | HighlightKind::KeywordOther => self.hl_keyword,
            HighlightKind::ConstantNumeric => self.hl_number,
            HighlightKind::ConstantLanguage => self.hl_constant,
            HighlightKind::Method => self.hl_method,
            HighlightKind::Variable => self.hl_variable,
            HighlightKind::MarkupHeading => self.hl_markup_heading,
            HighlightKind::MarkupBold => self.hl_markup_bold,
            HighlightKind::MarkupItalic => self.hl_markup_italic,
            HighlightKind::MarkupLink => self.hl_markup_link,
            HighlightKind::MarkupList => self.hl_markup_list,
            HighlightKind::MarkupInserted => self.hl_markup_inserted,
            HighlightKind::MarkupDeleted => self.hl_markup_deleted,
            HighlightKind::MarkupChanged => self.hl_markup_changed,
            HighlightKind::MarkupStrikethrough => self.hl_markup_strikethrough,
            HighlightKind::MetaHeader => self.hl_meta_header,
            HighlightKind::Other => self.hl_other,
        }
    }
}
