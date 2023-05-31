pub fn set_bit(data: &mut u8, pos: u8) {
    assert!(pos <= 7, "bit offset larger than 7");

    *data |= 1 << pos;
}

pub fn clear_bit(data: &mut u8, pos: u8) {
    assert!(pos <= 7, "bit offset larger than 7");

    *data &= !(1 << pos);
}
