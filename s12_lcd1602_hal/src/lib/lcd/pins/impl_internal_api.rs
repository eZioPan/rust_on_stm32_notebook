use super::{Pins, PinsInternalAPI};

impl PinsInternalAPI for Pins {
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
