mod impl_from_command_set;
mod impl_full_command_api;

pub(super) struct FullCommand {
    rs: RegisterSelection,
    rw: ReadWriteOp,
    data: Option<Bits>, // if it's a read command, then data should be filled by reading process
}

#[derive(Clone, Copy)]
pub(super) enum RegisterSelection {
    Command,
    Data,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ReadWriteOp {
    Write,
    Read,
}

#[derive(Clone, Copy)]
pub(super) enum Bits {
    Bit4(u8),
    Bit8(u8),
}

pub(super) trait FullCommandAPI {
    fn new(rs: RegisterSelection, rw: ReadWriteOp, data: Option<Bits>) -> Self;

    fn get_register_selection(&self) -> RegisterSelection;
    fn set_register_selection(&mut self, rs: RegisterSelection);

    fn get_read_write_op(&self) -> ReadWriteOp;
    fn set_read_write_op(&mut self, rw: ReadWriteOp);

    fn get_data(&self) -> Option<Bits>;
    fn set_data(&mut self, data: Option<Bits>);
}
