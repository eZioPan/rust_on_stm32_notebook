use embedded_hal::blocking::delay::DelayUs;

use crate::utils::{check_bit, BitState};

use super::{
    command_set::CommandSet, full_command::FullCommand, pins::PinsCrateLevelAPI, LCDBasic,
    PinsInteraction, LCD,
};

impl PinsInteraction for LCD {
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

        match check_bit(busy_state, 7) {
            BitState::Clear => false,
            BitState::Set => true,
        }
    }
}
