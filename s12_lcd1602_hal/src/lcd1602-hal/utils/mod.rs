mod impls;

pub(crate) enum BitState {
    Clear,
    Set,
}

pub(crate) trait BitOps {
    fn set_bit(&mut self, pos: u8);
    fn clear_bit(&mut self, pos: u8);
    fn check_bit(&self, pos: u8) -> BitState;
}
