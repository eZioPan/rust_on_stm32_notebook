use stm32f4xx_hal::gpio::{ErasedPin, OpenDrain, Output};

use crate::full_command::FullCommand;

mod impl_crate_level_api;
mod impl_internal_api;
mod impl_pins_api;

pub struct Pins<const PIN_CNT: usize> {
    rs_pin: ErasedPin<Output>,
    rw_pin: ErasedPin<Output>,
    en_pin: ErasedPin<Output>,
    db_pins: [ErasedPin<Output<OpenDrain>>; PIN_CNT],
}

pub trait FourPinsAPI {
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

pub trait EightPinsAPI {
    fn new<PullPushPin, OpenDrainPin>(
        rs: PullPushPin,
        rw: PullPushPin,
        en: PullPushPin,
        db0: OpenDrainPin,
        db1: OpenDrainPin,
        db2: OpenDrainPin,
        db3: OpenDrainPin,
        db4: OpenDrainPin,
        db5: OpenDrainPin,
        db6: OpenDrainPin,
        db7: OpenDrainPin,
    ) -> Self
    where
        PullPushPin: Into<ErasedPin<Output>>,
        OpenDrainPin: Into<ErasedPin<Output<OpenDrain>>>;
}

trait PinsInternalAPI {
    fn push_bits(&mut self, raw_bits: u8);
    fn fetch_bits(&mut self) -> u8;
}

pub(super) trait PinsCrateLevelAPI {
    fn send(&mut self, command: impl Into<FullCommand>) -> Option<u8>;
}
