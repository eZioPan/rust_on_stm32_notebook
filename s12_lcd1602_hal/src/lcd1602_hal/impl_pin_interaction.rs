use embedded_hal::blocking::delay::DelayUs;

use super::{
    command_set::CommandSet,
    full_command::FullCommand,
    pins::PinsCrateLevelAPI,
    utils::{BitOps, BitState},
    LCDBasic, PinsInteraction, LCD,
};

impl<const PIN_CNT: usize> PinsInteraction for LCD<PIN_CNT> {
    fn delay_and_send(&mut self, command: impl Into<FullCommand>, delay_us: u32) -> Option<u8> {
        self.delayer.delay_us(delay_us);
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

        match busy_state.check_bit(7) {
            BitState::Clear => false,
            BitState::Set => true,
        }
    }
}
