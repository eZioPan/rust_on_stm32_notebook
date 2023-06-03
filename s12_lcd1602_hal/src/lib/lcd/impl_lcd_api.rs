use crate::lcd::{LCDExt, StructAPI};

use super::{
    command_set::{CommandSet, DataWidth, Font, LineMode, MoveDirection, ShiftType, State},
    LCDBasic, PinsInteraction, RAMType, LCD,
};

impl LCDBasic for LCD {
    fn init_lcd(&mut self) {
        // 在初始化流程中，我们最好每次都发送“裸指令”
        // 不要使用 LCD 结构体提供的其它方法
        self.delay_and_send(CommandSet::HalfFunctionSet, 40_000);

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.get_line_mode(), self.get_font()),
            40,
        );

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.get_line_mode(), self.get_font()),
            40,
        );

        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.get_display_state(),
            cursor: self.get_cursor_state(),
            cursor_blink: self.get_cursor_blink_state(),
        });

        self.wait_and_send(CommandSet::ClearDisplay);

        self.wait_and_send(CommandSet::EntryModeSet(
            self.get_default_direction(),
            self.get_default_shift_type(),
        ));
    }

    fn clean_display(&mut self) {
        self.wait_and_send(CommandSet::ClearDisplay);
    }

    fn return_home(&mut self) {
        self.wait_and_send(CommandSet::ReturnHome);
    }

    fn set_cgram_addr(&mut self, addr: u8) {
        assert!(addr < 2u8.pow(6), "CGRAM Address overflow");

        self.internal_set_ram_type(RAMType::CGRAM);

        self.wait_and_send(CommandSet::SetCGRAM(addr));
    }

    fn write_custom_char_to_cur(&mut self, index: u8) {
        assert!(index < 8, "Only 8 graphs allowed in CGRAM");
        self.write_u8_to_cur(index);
    }

    fn write_u8_to_cur(&mut self, character: impl Into<u8>) {
        assert!(
            self.get_ram_type() == RAMType::DDRAM,
            "Current in CGRAM, use .set_cursor_pos() to change to DDRAM"
        );

        if self.direction == MoveDirection::RightToLeft {
            unimplemented!("Haven't implement right to left write");
        };

        let cur_pos = self.get_cursor_pos();

        match self.line {
            LineMode::OneLine => {
                // 1 行模式比较简单，直接判定 x 是否达到最大值，若已经到达了最大值，就直接报错
                assert!(cur_pos.0 < 79, "DDRAM Overflow");
                self.wait_and_send(CommandSet::WriteDataToRAM(character.into()));
                // 由于 LCD1602 的计数器会自动自增，因此这里只需要更新结构体的计数即可
                self.internal_set_cursor_pos((cur_pos.0 + 1, 0));
            }
            LineMode::TwoLine => {
                if cur_pos.0 == 39 {
                    // 如果显示模式为 2 行，
                    // 且第一行已经写到了末尾，则转移到下一行开头
                    // 若是第二行的结尾，就 DDRAM 溢出错误
                    if cur_pos.1 == 0 {
                        // 这里比较特殊，由于 LCD1602 的设计，在两行模式下，DDRAM 的内存地址在换行时并不连续
                        // 因此这里我们需要手动告知 LCD1602 换行后的位置
                        self.write_u8_to_pos(character.into(), (0, 1));
                        // 同上，我们只需要更新结构体内部的计数即可
                        self.internal_set_cursor_pos((1, 1));
                    } else {
                        panic!("DDRAM Overflowed");
                    }
                } else {
                    // 两行模式的其它情况，直接 x 值 +1 即可
                    self.wait_and_send(CommandSet::WriteDataToRAM(character.into()));
                    // 同上，我们只需要更新结构体内部的计数即可
                    self.internal_set_cursor_pos((cur_pos.0 + 1, cur_pos.1));
                }
            }
        }
    }

    fn read_u8_from_cur(&mut self) -> u8 {
        self.wait_and_send(CommandSet::ReadDataFromRAM).unwrap()
    }

    fn draw_graph_to_cgram(&mut self, index: u8, graph_data: [u8; 8]) {
        assert!(index < 8, "Only 8 graphs allowed in CGRAM");

        // 所有的行，设置为 1 的位，都应仅限于低 4 位
        assert!(
            graph_data.iter().all(|&line| line < 2u8.pow(5)),
            "Only lower 5 bits use to construct display"
        );

        // 有一个问题是，如果写入方向是从右到左，那么这里需要临时调整一下方向
        // 调整为从左到右，这样我们绘制字符的时候，就是从上到下绘制
        let mut direction_fliped = false;
        if self.get_default_direction() == MoveDirection::RightToLeft {
            self.set_default_direction(MoveDirection::LeftToRight);
            direction_fliped = true;
        }

        let cgram_data_addr_start = index.checked_shl(3).unwrap();

        // 注意 AC 在 CGRAM 里也是会自增的，因此不需要每一步都设置位置
        self.set_cgram_addr(cgram_data_addr_start as u8);
        graph_data.iter().for_each(|&line_data| {
            self.wait_and_send(CommandSet::WriteDataToRAM(line_data));
        });

        // 最后我们检查一下书写方向是否被翻转，
        // 如果被翻转表示原始书写的方向为从右向左，记得需要翻转回去
        if direction_fliped {
            self.set_default_direction(MoveDirection::RightToLeft)
        }
    }

    fn shift_cursor_or_display(&mut self, st: ShiftType, dir: MoveDirection) {
        self.internal_shift_cursor_or_display(st, dir);
        self.wait_and_send(CommandSet::CursorOrDisplayShift(st, dir));
    }

    fn get_display_offset(&self) -> u8 {
        self.display_offset
    }

    fn set_line_mode(&mut self, line: LineMode) {
        self.internal_set_line_mode(line);
        self.wait_and_send(CommandSet::FunctionSet(
            DataWidth::Bit4,
            self.line,
            self.font,
        ));
    }

    fn get_line_mode(&self) -> LineMode {
        self.line
    }

    fn set_font(&mut self, font: Font) {
        self.internal_set_font(font);
        self.wait_and_send(CommandSet::FunctionSet(
            DataWidth::Bit4,
            self.line,
            self.font,
        ));
    }

    fn get_font(&self) -> Font {
        self.font
    }

    fn set_display_state(&mut self, display: State) {
        self.internal_set_display_state(display);
        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.display_on,
            cursor: self.cursor_on,
            cursor_blink: self.cursor_blink,
        });
    }

    fn get_display_state(&self) -> State {
        self.display_on
    }

    fn set_cursor_state(&mut self, cursor: State) {
        self.internal_set_cursor_state(cursor);
        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.display_on,
            cursor: self.cursor_on,
            cursor_blink: self.cursor_blink,
        });
    }

    fn get_cursor_state(&self) -> State {
        self.cursor_on
    }

    fn set_cursor_blink_state(&mut self, blink: State) {
        self.internal_set_cursor_blink(blink);
        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.display_on,
            cursor: self.cursor_on,
            cursor_blink: self.cursor_blink,
        });
    }

    fn get_cursor_blink_state(&self) -> State {
        self.cursor_blink
    }

    fn set_default_direction(&mut self, dir: MoveDirection) {
        self.internal_set_direction(dir);
        self.wait_and_send(CommandSet::EntryModeSet(self.direction, self.shift_type));
    }

    fn get_default_direction(&self) -> MoveDirection {
        self.direction
    }

    fn set_default_shift_type(&mut self, shift: ShiftType) {
        self.internal_set_shift(shift);
        self.wait_and_send(CommandSet::EntryModeSet(self.direction, self.shift_type));
    }

    fn get_default_shift_type(&self) -> ShiftType {
        self.shift_type
    }

    fn set_cursor_pos(&mut self, pos: (u8, u8)) {
        self.internal_set_ram_type(RAMType::DDRAM);
        self.internal_set_cursor_pos(pos);

        // 这里比较特殊，
        // 如果处于单行模式，没有啥好说的，y 永远是 0，x 是几，实际的地址就是几
        // 如果处于双行模式，y 对于实际地址的偏移量为第二行开头的地址 0x40，x 的偏移量为该行中的偏移量
        // 虽然 LCD1602 说明书中，每一行都没有取到 x 的最大范围，但是我们这里并不怕这个问题，因为我们已经在 internal_set_cursor_pos 方法中检查过这个问题了
        let raw_pos: u8 = pos.1 * 0x40 + pos.0;

        self.wait_and_send(CommandSet::SetDDRAM(raw_pos));
    }

    fn get_cursor_pos(&self) -> (u8, u8) {
        assert!(
            self.get_ram_type() == RAMType::DDRAM,
            "Current in CGRAM, use .set_cursor_pos() to change to DDRAM"
        );

        self.cursor_pos
    }

    fn set_wait_interval_us(&mut self, interval: u32) {
        self.wait_interval_us = interval
    }

    fn get_wait_interval_us(&self) -> u32 {
        self.wait_interval_us
    }

    fn get_ram_type(&self) -> RAMType {
        self.ram_type
    }
}
