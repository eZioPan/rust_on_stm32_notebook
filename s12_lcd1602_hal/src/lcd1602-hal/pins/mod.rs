use embedded_hal::digital::v2::{InputPin, OutputPin};

use crate::full_command::FullCommand;

mod impl_crate_level_api;
mod impl_internal_api;
mod impl_pins_api;

pub struct Pins<ControlPin, DBPin, const PIN_CNT: usize>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
{
    rs_pin: ControlPin,
    rw_pin: ControlPin,
    en_pin: ControlPin,
    db_pins: [DBPin; PIN_CNT],
}

pub trait FourPinsAPI<ControlPin, DBPin>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
{
    fn new(
        rs: ControlPin,
        rw: ControlPin,
        en: ControlPin,
        db4: DBPin,
        db5: DBPin,
        db6: DBPin,
        db7: DBPin,
    ) -> Self
    where
        ControlPin: OutputPin,
        DBPin: OutputPin + InputPin;
}

pub trait EightPinsAPI<ControlPin, DBPin>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
{
    fn new(
        rs: ControlPin,
        rw: ControlPin,
        en: ControlPin,
        db0: DBPin,
        db1: DBPin,
        db2: DBPin,
        db3: DBPin,
        db4: DBPin,
        db5: DBPin,
        db6: DBPin,
        db7: DBPin,
    ) -> Self
    where
        ControlPin: OutputPin,
        DBPin: OutputPin + InputPin;
}

trait PinsInternalAPI {
    fn push_bits(&mut self, raw_bits: u8);
    fn fetch_bits(&mut self) -> u8;
}

pub(super) trait PinsCrateLevelAPI {
    fn send(&mut self, command: impl Into<FullCommand>) -> Option<u8>;
}
