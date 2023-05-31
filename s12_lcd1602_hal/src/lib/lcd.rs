use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use stm32f4xx_hal::timer::SysDelay;

use super::{
    command_set::{CommandSet, DataWidth, Font, LineMode, MoveDirection, ShiftType, State},
    full_command::FullCommand,
    lcd_pins::LCDPins,
    lcd_pins_traits::LCDPinsCrateLevelAPI,
    lcd_traits::{LCDExt, LCDPinsInteraction, LCDStructAPI, LCDTopLevelAPI},
};

pub struct LCD {
    pub(crate) pins: LCDPins,
    pub(crate) delayer: SysDelay,
    pub(crate) line: LineMode,
    pub(crate) font: Font,
    pub(crate) display_on: State,
    pub(crate) cursor_on: State,
    pub(crate) cursor_blink: State,
    pub(crate) direction: MoveDirection,
    pub(crate) shift_type: ShiftType,
    pub(crate) cursor_pos: (u8, u8),
    pub(crate) wait_interval_us: u32,
}

impl LCDExt for LCD {
    /// 以特定的时间间隔，切换整个屏幕特定次数
    /// 当 count 为 0 时，永续切换屏幕
    fn full_display_blink(&mut self, count: u32, interval_us: u32) {
        if count == 0 {
            loop {
                self.delay_us(interval_us);
                self.toggle_display();
            }
        } else {
            for _ in 0..count * 2 {
                self.delay_us(interval_us);
                self.toggle_display();
            }
        }
    }

    fn toggle_display(&mut self) {
        match self.get_display() {
            State::Off => self.set_display(State::On),
            State::On => self.set_display(State::Off),
        }
    }

    fn typewriter_write(&mut self, str: &str, extra_delay_us: u32) {
        for char in str.chars() {
            self.delay_us(extra_delay_us);
            self.write_char(char);
        }
    }

    fn write_str(&mut self, str: &str) {
        for char in str.chars() {
            self.write_char(char);
        }
    }

    /// 这里的字符仅覆盖了如下范围：
    /// ASCII 0x20 到 0x7D
    fn write_char(&mut self, char: char) {
        let out_byte = match char.is_ascii() {
            true => {
                let out_byte = char as u8;
                if out_byte >= 0x20 && out_byte <= 0x7D {
                    out_byte
                } else {
                    0xFF
                }
            }
            false => 0xFF,
        };

        self.write_to_cur(out_byte);
    }
}

impl LCDTopLevelAPI for LCD {
    fn init_lcd(&mut self) {
        // 在初始化流程中，我们最好每次都发送“裸指令”
        // 不要使用 LCD 结构体提供的其它方法
        self.delay_and_send(CommandSet::HalfFunctionSet, 40_000);

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.get_line(), self.get_font()),
            40,
        );

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.get_line(), self.get_font()),
            40,
        );

        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.get_display(),
            cursor: self.get_cursor(),
            cursor_blink: self.get_blink(),
        });

        self.wait_and_send(CommandSet::ClearDisplay);

        self.wait_and_send(CommandSet::EntryModeSet(
            self.get_direction(),
            self.get_shift(),
        ));

        // 这里比官方给定的步骤要多一步，因为我们允许用户设置初始位置，因此这里我们需要设置初始位置
        // 这个步骤由于出现在 LCD1602  datasheet 规定的初始化流程之外，因此我们可以安全地使用 .set_xxx() 方法
        self.set_cursor_pos(self.get_cursor_pos());
    }

    fn write_to_cur(&mut self, character: impl Into<u8>) {
        if self.direction == MoveDirection::Left {
            unimplemented!("Haven't inplement right to left write");
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
                        self.write_to_pos(character.into(), (0, 1));
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

    fn write_to_pos(&mut self, character: impl Into<u8>, pos: (u8, u8)) {
        self.set_cursor_pos(pos);
        self.wait_and_send(CommandSet::WriteDataToRAM(character.into()));
    }

    fn clean_display(&mut self) {
        self.wait_and_send(CommandSet::ClearDisplay);
    }

    fn delay_ms(&mut self, ms: u32) {
        self.delayer.delay_ms(ms);
    }

    fn delay_us(&mut self, us: u32) {
        self.delayer.delay_us(us);
    }

    fn set_line(&mut self, line: LineMode) {
        self.internal_set_line(line);
        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            self.get_wait_interval_us(),
        );
    }

    fn get_line(&self) -> LineMode {
        self.line
    }

    fn set_font(&mut self, font: Font) {
        self.internal_set_font(font);
        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            self.get_wait_interval_us(),
        );
    }

    fn get_font(&self) -> Font {
        self.font
    }

    fn set_display(&mut self, display: State) {
        self.internal_set_display(display);
        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.display_on,
            cursor: self.cursor_on,
            cursor_blink: self.cursor_blink,
        });
    }

    fn get_display(&self) -> State {
        self.display_on
    }

    fn set_cursor(&mut self, cursor: State) {
        self.internal_set_cursor(cursor);
        self.delay_and_send(
            CommandSet::DisplayOnOff {
                display: self.display_on,
                cursor: self.cursor_on,
                cursor_blink: self.cursor_blink,
            },
            self.get_wait_interval_us(),
        );
    }

    fn get_cursor(&self) -> State {
        self.cursor_on
    }

    fn set_blink(&mut self, blink: State) {
        self.internal_set_blink(blink);
        self.wait_and_send(CommandSet::DisplayOnOff {
            display: self.display_on,
            cursor: self.cursor_on,
            cursor_blink: self.cursor_blink,
        });
    }

    fn get_blink(&self) -> State {
        self.cursor_blink
    }

    fn set_direction(&mut self, dir: MoveDirection) {
        self.internal_set_direction(dir);
        self.wait_and_send(CommandSet::EntryModeSet(self.direction, self.shift_type));
    }

    fn get_direction(&self) -> MoveDirection {
        self.direction
    }

    fn set_shift(&mut self, shift: ShiftType) {
        self.internal_set_shift(shift);
        self.wait_and_send(CommandSet::EntryModeSet(self.direction, self.shift_type));
    }

    fn get_shift(&self) -> ShiftType {
        self.shift_type
    }

    fn set_cursor_pos(&mut self, pos: (u8, u8)) {
        self.internal_set_cursor_pos(pos);

        // 这里比较特殊，
        // 如果处于单行模式，没有啥好说的，y 永远是 0，x 是几，实际的地址就是几
        // 如果处于双行模式，y 对于实际地址的偏移量为第二行开头的地址 0x40，x 的偏移量为该行中的偏移量
        // 虽然 LCD1602 说明书中，每一行都没有取到 x 的最大范围，但是我们这里并不怕这个问题，因为我们已经在 internal_set_cursor_pos 方法中检查过这个问题了
        let raw_pos: u8 = pos.1 * 0x40 + pos.0;

        self.wait_and_send(CommandSet::SetDDRAM(raw_pos));
    }

    fn get_cursor_pos(&self) -> (u8, u8) {
        self.cursor_pos
    }

    fn set_wait_interval_us(&mut self, interval: u32) {
        self.wait_interval_us = interval
    }

    fn get_wait_interval_us(&self) -> u32 {
        self.wait_interval_us
    }
}

impl LCDStructAPI for LCD {
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
}

impl LCDPinsInteraction for LCD {
    fn delay_and_send(&mut self, command: impl Into<FullCommand>, delay_ms: u32) -> Option<u8> {
        self.delayer.delay_us(delay_ms);
        self.pins.send(command.into())
    }

    fn wait_and_send(&mut self, command: impl Into<FullCommand>) -> Option<u8> {
        self.wait_for_idle();
        self.pins.send(command.into())
    }

    fn wait_for_idle(&mut self) {
        while self.check_busy() {
            self.delayer.delay_us(self.get_wait_interval_us());
        }
    }

    fn check_busy(&mut self) -> bool {
        let busy_state = self.pins.send(CommandSet::ReadBusyFlagAndAddress).unwrap();

        busy_state.checked_shr(7).unwrap() & 1 == 1
    }
}
