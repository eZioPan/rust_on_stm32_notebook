pub struct FullCommand {
    pub(crate) rs: RegisterSelection,
    pub(crate) rw: ReadWrite,
    pub(crate) data: Option<Bits>, // if it's a read command, then data should be filled by reading process
}

pub(crate) enum RegisterSelection {
    Command,
    Data,
}

#[derive(PartialEq)]
pub(crate) enum ReadWrite {
    Write,
    Read,
}

pub(crate) enum Bits {
    Bit4(u8),
    Bit8(u8),
}
