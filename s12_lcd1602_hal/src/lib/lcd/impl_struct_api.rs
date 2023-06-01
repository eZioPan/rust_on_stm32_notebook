use crate::lcd::{command_set::Font, LCDAPI};

use super::{
    command_set::{LineMode, MoveDirection, ShiftType, State},
    RAMType, StructAPI, LCD,
};

impl StructAPI for LCD {
    fn internal_set_line(&mut self, line: LineMode) {
        assert!(
            (self.get_font() == Font::Font5x11) && (line == LineMode::OneLine),
            "font is 5x11, line cannot be 2"
        );

        self.line = line;
    }

    fn internal_set_font(&mut self, font: Font) {
        assert!(
            (self.get_line() == LineMode::TwoLine) && (font == Font::Font5x8),
            "there is 2 line, font cannot be 5x11"
        );

        self.font = font;
    }

    fn internal_set_display(&mut self, display: State) {
        self.display_on = display;
    }

    fn internal_set_cursor(&mut self, cursor: State) {
        self.cursor_on = cursor;
    }

    fn internal_set_blink(&mut self, blink: State) {
        self.cursor_blink = blink;
    }

    fn internal_set_direction(&mut self, dir: MoveDirection) {
        self.direction = dir;
    }

    fn internal_set_shift(&mut self, shift: ShiftType) {
        self.shift_type = shift;
    }

    fn internal_set_cursor_pos(&mut self, pos: (u8, u8)) {
        match self.line {
            LineMode::OneLine => {
                assert!(pos.0 < 80, "x offset too big");
                assert!(pos.1 < 1, "always keep y as 0 on OneLine mode");
            }
            LineMode::TwoLine => {
                assert!(pos.0 < 40, "x offset too big");
                assert!(pos.1 < 2, "y offset too big");
            }
        }

        self.cursor_pos = pos;
    }

    fn internal_set_ram_type(&mut self, ram_type: RAMType) {
        self.ram_type = ram_type;
    }
}
