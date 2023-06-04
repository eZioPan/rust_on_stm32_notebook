use crate::{command_set::Font, LCDBasic};

use super::{
    command_set::{LineMode, MoveDirection, ShiftType, State},
    RAMType, StructAPI, LCD,
};

impl<const PIN_CNT: usize> StructAPI for LCD<PIN_CNT> {
    fn internal_set_line_mode(&mut self, line: LineMode) {
        assert!(
            (self.get_font() == Font::Font5x11) && (line == LineMode::OneLine),
            "font is 5x11, line cannot be 2"
        );

        self.line = line;
    }

    fn internal_set_font(&mut self, font: Font) {
        assert!(
            (self.get_line_mode() == LineMode::TwoLine) && (font == Font::Font5x8),
            "there is 2 line, font cannot be 5x11"
        );

        self.font = font;
    }

    fn internal_set_display_state(&mut self, display: State) {
        self.display_on = display;
    }

    fn internal_set_cursor_state(&mut self, cursor: State) {
        self.cursor_on = cursor;
    }

    fn internal_set_cursor_blink(&mut self, blink: State) {
        self.cursor_blink = blink;
    }

    fn internal_set_direction(&mut self, dir: MoveDirection) {
        self.direction = dir;
    }

    fn internal_set_shift(&mut self, shift: ShiftType) {
        self.shift_type = shift;
    }

    fn internal_set_cursor_pos(&mut self, pos: (u8, u8)) {
        // TODO: ST7066U 的 DDRAM 地址是循环的，我们应该在内存中实现这个效果么？

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

    fn internal_set_display_offset(&mut self, offset: u8) {
        match self.get_line_mode() {
            LineMode::OneLine => assert!(
                offset < 80,
                "Display Offset too big, should not bigger than 79"
            ),
            LineMode::TwoLine => assert!(
                offset < 40,
                "Display Offset too big, should not bigger than 39"
            ),
        }

        self.display_offset = offset;
    }

    fn internal_shift_cursor_or_display(&mut self, st: ShiftType, dir: MoveDirection) {
        // 在偏移显示窗口的时候，ST7066U 展示的内容为首尾相连的模式
        let cur_display_offset = self.get_display_offset();
        let cur_cursor_pos = self.get_cursor_pos();

        match st {
            ShiftType::CursorOnly => match dir {
                MoveDirection::LeftToRight => match self.get_line_mode() {
                    LineMode::OneLine => {
                        if cur_cursor_pos.0 == 79 {
                            self.internal_set_cursor_pos((0, 0));
                        } else {
                            self.internal_set_cursor_pos((cur_cursor_pos.0 + 1, 0));
                        }
                    }
                    LineMode::TwoLine => {
                        if cur_cursor_pos.0 == 39 {
                            if cur_cursor_pos.1 == 0 {
                                self.internal_set_cursor_pos((0, 1));
                            } else {
                                self.internal_set_cursor_pos((0, 0));
                            }
                        } else {
                            self.internal_set_cursor_pos((cur_cursor_pos.0 + 1, cur_cursor_pos.1));
                        }
                    }
                },
                MoveDirection::RightToLeft => match self.get_line_mode() {
                    LineMode::OneLine => {
                        if cur_cursor_pos.0 == 0 {
                            self.internal_set_cursor_pos((79, 0));
                        } else {
                            self.internal_set_cursor_pos((cur_cursor_pos.0 - 1, 0));
                        }
                    }
                    LineMode::TwoLine => {
                        if cur_cursor_pos.0 == 0 {
                            if cur_cursor_pos.1 == 0 {
                                self.internal_set_cursor_pos((39, 1));
                            } else {
                                self.internal_set_cursor_pos((39, 0));
                            }
                        } else {
                            self.internal_set_cursor_pos((cur_cursor_pos.0 - 1, cur_cursor_pos.1));
                        }
                    }
                },
            },
            ShiftType::CursorAndDisplay => match dir {
                MoveDirection::LeftToRight => {
                    match self.get_line_mode() {
                        LineMode::OneLine if cur_display_offset == 79 => {
                            self.internal_set_display_offset(0)
                        }

                        LineMode::TwoLine if cur_display_offset == 39 => {
                            self.internal_set_display_offset(0)
                        }

                        _ => self.internal_set_display_offset(cur_display_offset + 1),
                    };
                }
                MoveDirection::RightToLeft => {
                    match self.get_line_mode() {
                        LineMode::OneLine if cur_display_offset == 0 => {
                            self.internal_set_display_offset(79)
                        }

                        LineMode::TwoLine if cur_display_offset == 0 => {
                            self.internal_set_display_offset(39)
                        }

                        _ => self.internal_set_display_offset(cur_display_offset - 1),
                    };
                }
            },
        }
    }

    fn internal_set_ram_type(&mut self, ram_type: RAMType) {
        self.ram_type = ram_type;
    }
}
