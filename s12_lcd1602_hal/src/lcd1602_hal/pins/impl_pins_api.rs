use stm32f4xx_hal::gpio::{ErasedPin, OpenDrain, Output};

use super::{EightPinsAPI, FourPinsAPI, Pins};

impl FourPinsAPI for Pins<4> {
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

impl EightPinsAPI for Pins<8> {
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
        OpenDrainPin: Into<ErasedPin<Output<OpenDrain>>>,
    {
        Self {
            rs_pin: rs.into(),
            rw_pin: rw.into(),
            en_pin: en.into(),
            db_pins: [
                db0.into(),
                db1.into(),
                db2.into(),
                db3.into(),
                db4.into(),
                db5.into(),
                db6.into(),
                db7.into(),
            ],
        }
    }
}
