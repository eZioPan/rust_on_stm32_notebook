mod impls;

pub enum BitState {
    Clear,
    Set,
}

pub trait BitOps {
    fn set_bit(&mut self, pos: u8);
    fn clear_bit(&mut self, pos: u8);
    fn check_bit(&self, pos: u8) -> BitState;
}
