use super::{Bits, FullCommand, FullCommandAPI, ReadWriteOp, RegisterSelection};

impl FullCommandAPI for FullCommand {
    fn new(rs: RegisterSelection, rw: ReadWriteOp, data: Option<Bits>) -> Self {
        if (rw == ReadWriteOp::Write) && (data.is_none()) {
            panic!("Write Operation Should have Data");
        }

        Self { rs, rw, data }
    }

    fn get_register_selection(&self) -> RegisterSelection {
        self.rs
    }

    fn set_register_selection(&mut self, rs: RegisterSelection) {
        self.rs = rs
    }

    fn get_read_write_op(&self) -> ReadWriteOp {
        self.rw
    }

    fn set_read_write_op(&mut self, rw: ReadWriteOp) {
        self.rw = rw
    }

    fn get_data(&self) -> Option<Bits> {
        self.data
    }

    fn set_data(&mut self, data: Option<Bits>) {
        self.data = data
    }
}
