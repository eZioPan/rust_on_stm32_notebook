use crate::full_command::{Bits, FullCommand, FullCommandAPI, ReadWriteOp, RegisterSelection};

use super::{Pins, PinsCrateLevelAPI, PinsInternalAPI};

impl PinsCrateLevelAPI for Pins {
    fn send(&mut self, command: impl Into<FullCommand>) -> Option<u8> {
        self.en_pin.set_low();

        let command = command.into();

        match command.get_register_selection() {
            RegisterSelection::Command => self.rs_pin.set_low(),
            RegisterSelection::Data => self.rs_pin.set_high(),
        }

        match command.get_read_write_op() {
            ReadWriteOp::Write => self.rw_pin.set_low(),
            ReadWriteOp::Read => self.rw_pin.set_high(),
        }

        match command.get_read_write_op() {
            ReadWriteOp::Write => {
                let bits = command
                    .get_data()
                    .expect("Write command but no data provide");
                match bits {
                    Bits::Bit4(raw_bits) => {
                        assert!(raw_bits < 2u8.pow(4), "data is greater than 4 bits");
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
            ReadWriteOp::Read => {
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
}
