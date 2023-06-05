use embedded_hal::{
    blocking::delay::{DelayMs, DelayUs},
    digital::v2::{InputPin, OutputPin},
};

use super::{
    command_set::CommandSet,
    full_command::FullCommand,
    pins::PinsCrateLevelAPI,
    utils::{BitOps, BitState},
    LCDBasic, PinsInteraction, LCD,
};

impl<ControlPin, DBPin, const PIN_CNT: usize, Delayer> PinsInteraction
    for LCD<ControlPin, DBPin, PIN_CNT, Delayer>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
    Delayer: DelayMs<u32> + DelayUs<u32>,
{
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
