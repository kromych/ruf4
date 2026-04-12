// Stub module providing editor buffer types needed by tui.rs.
// These are unused in ruf4 but required for compilation.
// The actual implementations live in Microsoft Edit's edit crate.

use std::rc::Rc;

use crate::cell::SemiRefCell;
use crate::framebuffer::Framebuffer;
use crate::helpers::*;

pub type TextBufferCell = SemiRefCell<TextBuffer>;
pub type RcTextBuffer = Rc<TextBufferCell>;

pub enum CursorMovement {
    Grapheme,
    Word,
}

pub enum MoveLineDirection {
    Up,
    Down,
}

/// Stub text buffer - not used in ruf4.
pub struct TextBuffer {
    _private: (),
}

pub struct TextBufferRenderResult {
    pub visual_pos_x_max: CoordType,
}

impl TextBuffer {
    pub fn new_rc(_single_line: bool) -> std::io::Result<RcTextBuffer> {
        Ok(Rc::new(SemiRefCell::new(TextBuffer { _private: () })))
    }

    pub fn copy_from_str(&mut self, _s: &dyn crate::document::WriteableDocument) {}
    pub fn take_cursor_visibility_request(&mut self) -> bool {
        false
    }
    pub fn set_width(&mut self, _w: CoordType) -> bool {
        false
    }
    pub fn is_dirty(&self) -> bool {
        false
    }
    pub fn save_as_string(&mut self, _dst: &mut dyn crate::document::WriteableDocument) {}
    pub fn visual_line_count(&self) -> CoordType {
        0
    }
    pub fn cursor_visual_pos(&self) -> Point {
        Point::default()
    }
    pub fn cursor_logical_pos(&self) -> Point {
        Point::default()
    }
    pub fn cursor_move_to_visual(&mut self, _pos: Point) {}
    pub fn cursor_move_to_logical(&mut self, _pos: Point) {}
    pub fn cursor_move_delta(&mut self, _g: CursorMovement, _d: i32) {}
    pub fn selection_update_visual(&mut self, _pos: Point) {}
    pub fn selection_update_logical(&mut self, _pos: Point) {}
    pub fn selection_update_delta(&mut self, _g: CursorMovement, _d: i32) {}
    pub fn selection_range(&self) -> Option<(TextBufferCursor, TextBufferCursor)> {
        None
    }
    pub fn clear_selection(&mut self) -> bool {
        false
    }
    pub fn select_all(&mut self) {}
    pub fn select_word(&mut self) {}
    pub fn select_line(&mut self) {}
    pub fn delete(&mut self, _g: CursorMovement, _d: i32) {}
    pub fn indent_change(&mut self, _d: i32) {}
    pub fn indent_end_logical_pos(&self) -> Point {
        Point::default()
    }
    pub fn write_canon(&mut self, _data: &[u8]) {}
    pub fn is_overtype(&self) -> bool {
        false
    }
    pub fn set_overtype(&mut self, _v: bool) {}
    pub fn is_word_wrap_enabled(&self) -> bool {
        false
    }
    pub fn set_word_wrap(&mut self, _v: bool) {}
    pub fn margin_width(&self) -> CoordType {
        0
    }
    pub fn text_width(&self) -> CoordType {
        0
    }
    pub fn undo(&mut self) {}
    pub fn redo(&mut self) {}
    pub fn copy(&mut self, _clip: &mut crate::clipboard::Clipboard) {}
    pub fn cut(&mut self, _clip: &mut crate::clipboard::Clipboard) {}
    pub fn paste(&mut self, _clip: &crate::clipboard::Clipboard) {}
    pub fn move_selected_lines(&mut self, _d: MoveLineDirection) {}
    pub fn render(
        &mut self,
        _scroll: Point,
        _dest: Rect,
        _focused: bool,
        _fb: &mut Framebuffer,
    ) -> Option<TextBufferRenderResult> {
        None
    }
    pub unsafe fn set_cursor(&mut self, _c: TextBufferCursor) {}
}

#[derive(Clone, Copy)]
pub struct TextBufferCursor {
    pub visual_pos: Point,
}
