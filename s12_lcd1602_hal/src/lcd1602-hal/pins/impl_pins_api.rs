use embedded_hal::digital::v2::{InputPin, OutputPin};

use super::{EightPinsAPI, FourPinsAPI, Pins};

impl<ControlPin, DBPin> FourPinsAPI<ControlPin, DBPin> for Pins<ControlPin, DBPin, 4>
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
    ) -> Self {
        Self {
            rs_pin: rs,
            rw_pin: rw,
            en_pin: en,
            db_pins: [db4, db5, db6, db7],
        }
    }
}

impl<ControlPin, DBPin> EightPinsAPI<ControlPin, DBPin> for Pins<ControlPin, DBPin, 8>
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
    ) -> Self {
        Self {
            rs_pin: rs,
            rw_pin: rw,
            en_pin: en,
            db_pins: [db0, db1, db2, db3, db4, db5, db6, db7],
        }
    }
}
