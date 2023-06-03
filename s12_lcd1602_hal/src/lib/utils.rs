pub(crate) enum BitState {
    Clear,
    Set,
}

pub(crate) fn set_bit(data: &mut u8, pos: u8) {
    assert!(pos <= 7, "bit offset larger than 7");

    *data |= 1 << pos;
}

pub(crate) fn clear_bit(data: &mut u8, pos: u8) {
    assert!(pos <= 7, "bit offset larger than 7");

    *data &= !(1 << pos);
}

pub(crate) fn check_bit(data: u8, pos: u8) -> BitState {
    assert!(pos <= 7, "bit offset larger than 7");

    match data.checked_shr(pos as u32).unwrap() & 1 == 1 {
        true => BitState::Set,
        false => BitState::Clear,
    }
}
