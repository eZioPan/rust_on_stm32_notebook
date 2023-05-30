use stm32f4xx_hal::gpio::{ErasedPin, OpenDrain, Output};

use super::full_command::{Bits, FullCommand, ReadWrite, RegisterSelection};

pub struct LCDPins {
    rs_pin: ErasedPin<Output>,
    rw_pin: ErasedPin<Output>,
    en_pin: ErasedPin<Output>,
    db_pins: [ErasedPin<Output<OpenDrain>>; 4],
}

impl LCDPins {
    pub fn new<PullPushPin, OpenDrainPin>(
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

    pub(crate) fn send<IFC: Into<FullCommand>>(&mut self, command: IFC) -> Option<u8> {
        self.en_pin.set_low();

        let command = command.into();

        match command.rs {
            RegisterSelection::Command => self.rs_pin.set_low(),
            RegisterSelection::Data => self.rs_pin.set_high(),
        }

        match command.rw {
            ReadWrite::Write => self.rw_pin.set_low(),
            ReadWrite::Read => self.rw_pin.set_high(),
        }

        match command.rw {
            ReadWrite::Write => {
                let bits = command.data.expect("Write command but no data provide");
                match bits {
                    Bits::Bit4(raw_bits) => {
                        assert!(raw_bits <= 0b1111, "data is greater than 4 bits");
                        self.push_4_bits(raw_bits);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                    }
                    Bits::Bit8(raw_bits) => {
                        self.push_4_bits(raw_bits >> 4);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                        self.push_4_bits(raw_bits & 0b1111);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                    }
                }
                None
            }
            ReadWrite::Read => {
                self.en_pin.set_high();
                let high_4_bits = self.fetch_4_bits().checked_shl(4).unwrap();
                self.en_pin.set_low();
                self.en_pin.set_high();
                let low_4_bits = self.fetch_4_bits();
                self.en_pin.set_low();
                Some(high_4_bits + low_4_bits)
            }
        }
    }

    fn push_4_bits(&mut self, raw_bits: u8) {
        for (index, pin) in self.db_pins.iter_mut().enumerate() {
            if raw_bits.checked_shr(index as u32).unwrap() & 1 == 1 {
                pin.set_high()
            } else {
                pin.set_low()
            }
        }
    }

    fn fetch_4_bits(&mut self) -> u8 {
        let mut data: u8 = 0;
        for (index, pin) in self.db_pins.iter_mut().enumerate() {
            pin.set_high();
            let cur_pos = 1u8.checked_shl(index as u32).unwrap();
            if pin.is_high() {
                data |= cur_pos;
            } else {
                data &= !cur_pos;
            }
        }
        data
    }
}
