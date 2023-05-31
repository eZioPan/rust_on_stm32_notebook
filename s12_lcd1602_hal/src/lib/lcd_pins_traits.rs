use super::full_command::FullCommand;
use stm32f4xx_hal::gpio::{ErasedPin, OpenDrain, Output};

pub(crate) trait LCDPinsInternalAPI {
    fn push_4_bits(&mut self, raw_bits: u8);
    fn fetch_4_bits(&mut self) -> u8;
}

pub(crate) trait LCDPinsCrateLevelAPI {
    fn send(&mut self, command: impl Into<FullCommand>) -> Option<u8>;
}

pub trait LCDPinsTopLevelAPI {
    fn new<PullPushPin, OpenDrainPin>(
        rs: PullPushPin,
        rw: PullPushPin,
        en: PullPushPin,
        db4: OpenDrainPin,
        db5: OpenDrainPin,
        db6: OpenDrainPin,
        db7: OpenDrainPin,
    ) -> Self
    where
        PullPushPin: Into<ErasedPin<Output>>,
        OpenDrainPin: Into<ErasedPin<Output<OpenDrain>>>;
}
