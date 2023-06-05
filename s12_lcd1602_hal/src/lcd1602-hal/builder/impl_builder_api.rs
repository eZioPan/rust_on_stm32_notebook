use embedded_hal::{
    blocking::delay::{DelayMs, DelayUs},
    digital::v2::{InputPin, OutputPin},
};

use crate::{
    command_set::{Font, LineMode, MoveDirection, ShiftType, State},
    pins::Pins,
    LCDBasic, RAMType, LCD,
};

use super::{Builder, BuilderAPI};

impl<ControlPin, DBPin, const PIN_CNT: usize, Delayer>
    BuilderAPI<ControlPin, DBPin, PIN_CNT, Delayer> for Builder<ControlPin, DBPin, PIN_CNT, Delayer>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
    Delayer: DelayMs<u32> + DelayUs<u32>,
{
    fn build_and_init(mut self) -> LCD<ControlPin, DBPin, PIN_CNT, Delayer> {
        let mut lcd = LCD {
            pins: self.pop_pins(),
            delayer: self.pop_delayer(),
            line: self.get_line(),
            font: self.get_font(),
            display_on: self.get_display(),
            cursor_on: self.get_cursor(),
            cursor_blink: self.get_blink(),
            direction: self.get_direction(),
            shift_type: self.get_shift(),
            cursor_pos: (0, 0), // 锁定为初始位置
            display_offset: 0,  // 锁定为初始位置
            wait_interval_us: self.get_wait_interval_us(),
            ram_type: RAMType::DDRAM, // 锁定为进入 DDRAM
        };
        lcd.init_lcd();
        lcd
    }

    fn new(pins: Pins<ControlPin, DBPin, PIN_CNT>, delayer: Delayer) -> Self {
        Self {
            pins: Some(pins),
            delayer: Some(delayer),
            line: Default::default(),
            font: Default::default(),
            display_on: Default::default(),
            cursor_on: Default::default(),
            cursor_blink: Default::default(),
            dir: Default::default(),
            shift_type: Default::default(),
            wait_interval_us: 10,
        }
    }

    fn pop_pins(&mut self) -> Pins<ControlPin, DBPin, PIN_CNT> {
        self.pins.take().expect("No Pins to pop")
    }

    fn pop_delayer(&mut self) -> Delayer {
        self.delayer.take().expect("No delayer to pop")
    }

    fn set_line(mut self, line: LineMode) -> Self {
        if (self.get_font() == Font::Font5x11) && (line == LineMode::TwoLine) {
            panic!("font is 5x11, line cannot be 2");
        };

        self.line = line;
        self
    }

    fn get_line(&self) -> LineMode {
        self.line
    }

    fn set_font(mut self, font: Font) -> Self {
        if (self.get_line() == LineMode::TwoLine) && (font == Font::Font5x11) {
            panic!("there is 2 line, font cannot be 5x11")
        };

        self.font = font;
        self
    }

    fn get_font(&self) -> Font {
        self.font
    }

    fn set_display(mut self, display: State) -> Self {
        self.display_on = display;
        self
    }

    fn get_display(&self) -> State {
        self.display_on
    }

    fn set_cursor(mut self, cursor: State) -> Self {
        self.cursor_on = cursor;
        self
    }

    fn get_cursor(&self) -> State {
        self.cursor_on
    }

    fn set_blink(mut self, blink: State) -> Self {
        self.cursor_blink = blink;
        self
    }

    fn get_blink(&self) -> State {
        self.cursor_blink
    }

    fn set_direction(mut self, dir: MoveDirection) -> Self {
        self.dir = dir;
        self
    }

    fn get_direction(&self) -> MoveDirection {
        self.dir
    }

    fn set_shift(mut self, shift: ShiftType) -> Self {
        self.shift_type = shift;
        self
    }

    fn get_shift(&self) -> ShiftType {
        self.shift_type
    }

    fn set_wait_interval_us(mut self, interval: u32) -> Self {
        self.wait_interval_us = interval;
        self
    }

    fn get_wait_interval_us(&self) -> u32 {
        self.wait_interval_us
    }
}
