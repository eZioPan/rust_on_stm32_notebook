use stm32f4xx_hal::timer::SysDelay;

use crate::{
    command_set::{Font, Line, MoveDirection, ShiftType, State},
    lcd::LCD,
    lcd_pins::LCDPins,
};

pub struct LCDBuilder {
    pins: Option<LCDPins>,
    delayer: Option<SysDelay>,
    line: Line,
    font: Font,
    display_on: State,
    cursor_on: State,
    cursor_blink: State,
    dir: MoveDirection,
    shift_type: ShiftType,
}

impl LCDBuilder {
    pub fn build_and_init(mut self) -> LCD {
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
        };
        lcd.init_lcd();

        lcd
    }

    pub fn new(pins: LCDPins, delayer: SysDelay) -> Self {
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
        }
    }

    pub fn pop_pins(&mut self) -> LCDPins {
        self.pins.take().expect("No Pins to pop")
    }

    pub fn pop_delayer(&mut self) -> SysDelay {
        self.delayer.take().expect("No delayer to pop")
    }

    pub fn set_line(mut self, line: Line) -> Self {
        self.line = line;
        self
    }

    pub fn get_line(&self) -> Line {
        self.line
    }

    pub fn set_font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn get_font(&self) -> Font {
        self.font
    }

    pub fn set_display(mut self, display: State) -> Self {
        self.display_on = display;
        self
    }

    pub fn get_display(&self) -> State {
        self.display_on
    }

    pub fn set_cursor(mut self, cursor: State) -> Self {
        self.cursor_on = cursor;
        self
    }

    pub fn get_cursor(&self) -> State {
        self.cursor_on
    }

    pub fn set_blink(mut self, blink: State) -> Self {
        self.cursor_blink = blink;
        self
    }

    pub fn get_blink(&self) -> State {
        self.cursor_blink
    }

    pub fn set_direction(mut self, dir: MoveDirection) -> Self {
        self.dir = dir;
        self
    }

    pub fn get_direction(&self) -> MoveDirection {
        self.dir
    }

    pub fn set_shift(mut self, shift: ShiftType) -> Self {
        self.shift_type = shift;
        self
    }

    pub fn get_shift(&self) -> ShiftType {
        self.shift_type
    }
}
