use stm32f4xx_hal::timer::SysDelay;

use crate::lcd_pins::LCDPins;

use super::{
    command_set::{Font, LineMode, MoveDirection, ShiftType, State},
    lcd::LCD,
    lcd_builder_traits::LCDBuilderAPI,
    lcd_traits::LCDTopLevelAPI,
};

pub struct LCDBuilder {
    pub(crate) pins: Option<LCDPins>,
    pub(crate) delayer: Option<SysDelay>,
    pub(crate) line: LineMode,
    pub(crate) font: Font,
    pub(crate) display_on: State,
    pub(crate) cursor_on: State,
    pub(crate) cursor_blink: State,
    pub(crate) dir: MoveDirection,
    pub(crate) shift_type: ShiftType,
    pub(crate) cursor_pos: (u8, u8),
    pub(crate) wait_interval_us: u32,
}

impl LCDBuilderAPI for LCDBuilder {
    fn build_and_init(mut self) -> LCD {
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
            cursor_pos: self.get_cursor_pos(),
            wait_interval_us: self.get_wait_interval_us(),
        };
        lcd.init_lcd();

        lcd
    }

    fn new(pins: LCDPins, delayer: SysDelay) -> Self {
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
            cursor_pos: (0, 0),
            wait_interval_us: 10,
        }
    }

    fn pop_pins(&mut self) -> LCDPins {
        self.pins.take().expect("No Pins to pop")
    }

    fn pop_delayer(&mut self) -> SysDelay {
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

    fn set_cursor_pos(mut self, pos: (u8, u8)) -> Self {
        match self.line {
            LineMode::OneLine => {
                assert!(pos.0 < 80, "x offset too big");
                assert!(pos.1 < 1, "should set y at 0 on ");
            }
            LineMode::TwoLine => {
                assert!(pos.0 < 40, "x offset too big");
                assert!(pos.1 < 2, "y offset too big");
            }
        }

        self.cursor_pos = pos;
        self
    }

    fn get_cursor_pos(&self) -> (u8, u8) {
        self.cursor_pos
    }

    fn set_wait_interval_us(mut self, interval: u32) -> Self {
        self.wait_interval_us = interval;
        self
    }

    fn get_wait_interval_us(&self) -> u32 {
        self.wait_interval_us
    }
}
