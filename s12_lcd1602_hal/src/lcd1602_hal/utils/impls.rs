use super::{BitOps, BitState};

impl BitOps for u8 {
    fn set_bit(&mut self, pos: u8) {
        assert!(pos <= 7, "bit offset larger than 7");

        *self |= 1u8 << pos;
    }

    fn clear_bit(&mut self, pos: u8) {
        assert!(pos <= 7, "bit offset larger than 7");

        *self &= !(1u8 << pos);
    }

    fn check_bit(&self, pos: u8) -> BitState {
        assert!(pos <= 7, "bit offset larger than 7");

        match self.checked_shr(pos as u32).unwrap() & 1 == 1 {
            true => BitState::Set,
            false => BitState::Clear,
        }
    }
}
