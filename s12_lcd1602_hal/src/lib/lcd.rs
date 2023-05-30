use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use stm32f4xx_hal::timer::SysDelay;

use super::{
    command_set::{CommandSet, DataWidth, Font, Line, MoveDirection, ShiftType, State},
    full_command::FullCommand,
    lcd_pins::LCDPins,
};

pub struct LCD {
    pub(crate) pins: LCDPins,
    pub(crate) delayer: SysDelay,
    pub(crate) line: Line,
    pub(crate) font: Font,
    pub(crate) display_on: State,
    pub(crate) cursor_on: State,
    pub(crate) cursor_blink: State,
    pub(crate) direction: MoveDirection,
    pub(crate) shift_type: ShiftType,
}

impl LCD {
    pub(crate) fn init_lcd(&mut self) {
        self.delay_and_send(CommandSet::HalfFunctionSet, 40_000);

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            40,
        );

        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            40,
        );

        self.wait_and_send(
            CommandSet::DisplayOnOff {
                display: State::On,
                cursor: self.cursor_on,
                cursor_blink: self.cursor_blink,
            },
            10,
        );

        self.wait_and_send(CommandSet::ClearDisplay, 10);

        self.wait_and_send(
            CommandSet::EntryModeSet(self.direction, self.shift_type),
            10,
        );
    }

    pub fn delay_and_send<IFC: Into<FullCommand>>(
        &mut self,
        command: IFC,
        wait_micro_sec: u32,
    ) -> Option<u8> {
        self.delayer.delay_us(wait_micro_sec);
        self.pins.send(command.into())
    }

    pub fn wait_and_send<IFC: Into<FullCommand>>(
        &mut self,
        command: IFC,
        poll_interval_micro_sec: u32,
    ) -> Option<u8> {
        self.wait_for_idle(poll_interval_micro_sec);
        self.pins.send(command.into())
    }

    pub fn wait_for_idle(&mut self, poll_interval_micro_sec: u32) {
        while self.check_busy() {
            self.delayer.delay_us(poll_interval_micro_sec);
        }
    }

    pub fn check_busy(&mut self) -> bool {
        let busy_state = self.pins.send(CommandSet::ReadBusyFlagAndAddress).unwrap();

        busy_state.checked_shr(7).unwrap() & 1 == 1
    }

    pub fn delay_ms(&mut self, ms: u32) {
        self.delayer.delay_ms(ms);
    }

    pub fn delay_us(&mut self, us: u32) {
        self.delayer.delay_us(us);
    }
}

impl LCD {
    pub(crate) fn internal_set_line(&mut self, line: Line) {
        self.line = line;
    }

    pub fn set_line(&mut self, line: Line) {
        self.internal_set_line(line);
        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            10,
        );
    }

    pub fn get_line(&self) -> Line {
        self.line
    }

    pub(crate) fn internal_set_font(&mut self, font: Font) {
        self.font = font;
    }

    pub fn set_font(&mut self, font: Font) {
        self.internal_set_font(font);
        self.delay_and_send(
            CommandSet::FunctionSet(DataWidth::Bit4, self.line, self.font),
            10,
        );
    }

    pub fn get_font(&self) -> Font {
        self.font
    }

    pub(crate) fn internal_set_display(&mut self, display: State) {
        self.display_on = display;
    }

    pub fn set_display(&mut self, display: State) {
        self.internal_set_display(display);
        self.wait_and_send(
            CommandSet::DisplayOnOff {
                display: self.display_on,
                cursor: self.cursor_on,
                cursor_blink: self.cursor_blink,
            },
            10,
        );
    }

    pub fn get_display(&self) -> State {
        self.display_on
    }

    pub(crate) fn internal_set_cursor(&mut self, cursor: State) {
        self.cursor_on = cursor;
    }

    pub fn set_cursor(&mut self, cursor: State) {
        self.internal_set_cursor(cursor);
        self.delay_and_send(
            CommandSet::DisplayOnOff {
                display: self.display_on,
                cursor: self.cursor_on,
                cursor_blink: self.cursor_blink,
            },
            10,
        );
    }

    pub fn get_cursor(&self) -> State {
        self.cursor_on
    }

    pub(crate) fn internal_set_blink(&mut self, blink: State) {
        self.cursor_blink = blink;
    }

    pub fn set_blink(&mut self, blink: State) {
        self.internal_set_blink(blink);
        self.delay_and_send(
            CommandSet::DisplayOnOff {
                display: self.display_on,
                cursor: self.cursor_on,
                cursor_blink: self.cursor_blink,
            },
            10,
        );
    }

    pub fn get_blink(&self) -> State {
        self.cursor_blink
    }

    pub(crate) fn internal_set_direction(&mut self, dir: MoveDirection) {
        self.direction = dir;
    }

    pub fn set_direction(&mut self, dir: MoveDirection) {
        self.internal_set_direction(dir);
        self.wait_and_send(
            CommandSet::EntryModeSet(self.direction, self.shift_type),
            10,
        );
    }

    pub fn get_direction(&self) -> MoveDirection {
        self.direction
    }

    pub(crate) fn internal_set_shift(&mut self, shift: ShiftType) {
        self.shift_type = shift;
    }

    pub fn set_shift(&mut self, shift: ShiftType) {
        self.internal_set_shift(shift);
        self.wait_and_send(
            CommandSet::EntryModeSet(self.direction, self.shift_type),
            10,
        );
    }

    pub fn get_shift(&self) -> ShiftType {
        self.shift_type
    }
}
