pub fn set_bit(data: &mut u8, pos: u8) {
    if pos > 7 {
        panic!("pos larger than 7");
    }
    *data |= 1 << pos;
}

pub fn clear_bit(data: &mut u8, pos: u8) {
    if pos > 7 {
        panic!("pos larger than 7");
    }
    *data &= !(1 << pos);
}
