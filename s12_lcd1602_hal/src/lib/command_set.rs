use crate::{
    full_command::{Bits, FullCommand, ReadWrite, RegisterSelection},
    utils::{clear_bit, set_bit},
};

#[derive(Clone, Copy)]
pub enum CommandSet {
    ClearDisplay,
    ReturnHome,
    EntryModeSet(MoveDirection, ShiftType),
    DisplayOnOff {
        display: State,
        cursor: State,
        cursor_blink: State,
    },
    CursorOrDisplayShift(ShiftType, MoveDirection),
    // 这个 HalfFunctionSet 比较特殊，是在初始化 LCD1602 到 4 bit 模式所特有的“半条指令”
    // 而且 ST7066U 中并没有给这半条指令取新的名字，这里是我为了规整自行确定的名称
    HalfFunctionSet,
    FunctionSet(DataWidth, Line, Font),
    SetCGRAM(u8),
    SetDDRAM(u8),
    ReadBusyFlagAndAddress,
    WriteDataToRAM(u8),
    ReadDataFromRAM,
}

#[derive(Clone, Copy)]
pub enum MoveDirection {
    Left,
    Right,
}

#[derive(Clone, Copy)]
pub enum ShiftType {
    Cursor,
    Screen,
}

#[derive(Clone, Copy)]
pub enum State {
    Off,
    On,
}

#[derive(Clone, Copy)]
pub enum DataWidth {
    Bit4,
    Bit8,
}

#[derive(Clone, Copy)]
pub enum Line {
    Line1,
    Line2,
}

#[derive(Clone, Copy)]
pub enum Font {
    Font5x8,
    Font5x11,
}

impl From<CommandSet> for FullCommand {
    fn from(command: CommandSet) -> Self {
        match command {
            CommandSet::ClearDisplay => {
                let raw_bits: u8 = 0b0000_0001;
                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::ReturnHome => {
                let raw_bits: u8 = 0b0000_010;
                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::EntryModeSet(dir, st) => {
                let mut raw_bits: u8 = 0b0000_0100;

                match dir {
                    MoveDirection::Left => clear_bit(&mut raw_bits, 1),
                    MoveDirection::Right => set_bit(&mut raw_bits, 1),
                }

                match st {
                    ShiftType::Cursor => clear_bit(&mut raw_bits, 0),
                    ShiftType::Screen => set_bit(&mut raw_bits, 0),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::DisplayOnOff {
                display,
                cursor,
                cursor_blink,
            } => {
                let mut raw_bits = 0b0000_1000;

                match display {
                    State::Off => clear_bit(&mut raw_bits, 2),
                    State::On => set_bit(&mut raw_bits, 2),
                }
                match cursor {
                    State::Off => clear_bit(&mut raw_bits, 1),
                    State::On => set_bit(&mut raw_bits, 1),
                }
                match cursor_blink {
                    State::Off => clear_bit(&mut raw_bits, 0),
                    State::On => set_bit(&mut raw_bits, 0),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::CursorOrDisplayShift(st, dir) => {
                let mut raw_bits = 0b0001_0000;

                match st {
                    ShiftType::Cursor => clear_bit(&mut raw_bits, 3),
                    ShiftType::Screen => set_bit(&mut raw_bits, 3),
                }

                match dir {
                    MoveDirection::Left => clear_bit(&mut raw_bits, 2),
                    MoveDirection::Right => set_bit(&mut raw_bits, 2),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::HalfFunctionSet => Self {
                rs: RegisterSelection::Command,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit4(0b0010)),
            },

            CommandSet::FunctionSet(width, line, font) => {
                let mut raw_bits = 0b0010_0000;

                match width {
                    DataWidth::Bit4 => clear_bit(&mut raw_bits, 4),
                    DataWidth::Bit8 => set_bit(&mut raw_bits, 4),
                }

                match line {
                    Line::Line1 => clear_bit(&mut raw_bits, 3),
                    Line::Line2 => set_bit(&mut raw_bits, 3),
                }

                match font {
                    Font::Font5x8 => clear_bit(&mut raw_bits, 2),
                    Font::Font5x11 => set_bit(&mut raw_bits, 2),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::SetCGRAM(addr) => {
                let mut raw_bits = 0b0100_0000;

                if addr > 0b0011_1111 {
                    panic!("CGRAM address out of range")
                }

                raw_bits += addr;

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::SetDDRAM(addr) => {
                let mut raw_bits = 0b1000_0000;

                if addr > 0b0111_1111 {
                    panic!("DDRAM address out of range")
                }

                raw_bits += addr;

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::ReadBusyFlagAndAddress => Self {
                rs: RegisterSelection::Command,
                rw: ReadWrite::Read,
                data: None,
            },

            CommandSet::WriteDataToRAM(data) => Self {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit8(data)),
            },

            CommandSet::ReadDataFromRAM => Self {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Read,
                data: None,
            },
        }
    }
}
