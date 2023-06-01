use stm32f4xx_hal::gpio::{ErasedPin, OpenDrain, Output};

use super::{Pins, PinsAPI};

impl PinsAPI for Pins {
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
        OpenDrainPin: Into<ErasedPin<Output<OpenDrain>>>,
    {
        Self {
            rs_pin: rs.into(),
            rw_pin: rw.into(),
            en_pin: en.into(),
            db_pins: [db4.into(), db5.into(), db6.into(), db7.into()],
        }
    }
}
