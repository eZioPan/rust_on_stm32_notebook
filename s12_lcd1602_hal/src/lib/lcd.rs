use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use stm32f4xx_hal::timer::SysDelay;

use super::{
    command_set::{CommandSet, DataWidth, Font, Line, MoveDirection, ShiftType, State},
    full_command::FullCommand,
    lcd_pins::LCDPins,
};

pub struct LCD {
    pins: LCDPins,
    delayer: SysDelay,
}

impl LCD {
    pub fn new(pins: LCDPins, delayer: SysDelay) -> Self {
        Self { pins, delayer }
    }

    pub fn init_lcd(
        &mut self,
        line: Line,
        font: Font,
        cursor_on: State,
        cursor_blink: State,
        dir: MoveDirection,
        shift_type: ShiftType,
    ) {
        self.delay_and_send(CommandSet::HalfFunctionSet, 40_000);

        self.delay_and_send(CommandSet::FunctionSet(DataWidth::Bit4, line, font), 40);

        self.delay_and_send(CommandSet::FunctionSet(DataWidth::Bit4, line, font), 40);

        self.wait_and_send(
            CommandSet::DisplayOnOff {
                display: State::On,
                cursor: cursor_on,
                cursor_blink: cursor_blink,
            },
            10,
        );

        self.wait_and_send(CommandSet::ClearDisplay, 10);

        self.wait_and_send(CommandSet::EntryModeSet(dir, shift_type), 10);
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
