use crate::{
    lcd::command_set::{CommandSet, DataWidth, Font, LineMode, MoveDirection, ShiftType, State},
    utils::{clear_bit, set_bit},
};

use super::{Bits, FullCommand, FullCommandAPI, ReadWriteOp, RegisterSelection};

impl From<CommandSet> for FullCommand {
    fn from(command: CommandSet) -> Self {
        match command {
            CommandSet::ClearDisplay => {
                let raw_bits: u8 = 0b0000_0001;
                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
            }

            CommandSet::ReturnHome => {
                let raw_bits: u8 = 0b0000_010;
                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
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

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
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

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
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

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
            }

            CommandSet::HalfFunctionSet => Self::new(
                RegisterSelection::Command,
                ReadWriteOp::Write,
                Some(Bits::Bit4(0b0010)),
            ),

            CommandSet::FunctionSet(width, line, font) => {
                let mut raw_bits = 0b0010_0000;

                match width {
                    DataWidth::Bit4 => clear_bit(&mut raw_bits, 4),
                    DataWidth::Bit8 => set_bit(&mut raw_bits, 4),
                }

                match line {
                    LineMode::OneLine => clear_bit(&mut raw_bits, 3),
                    LineMode::TwoLine => set_bit(&mut raw_bits, 3),
                }

                match font {
                    Font::Font5x8 => clear_bit(&mut raw_bits, 2),
                    Font::Font5x11 => set_bit(&mut raw_bits, 2),
                }

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
            }

            CommandSet::SetCGRAM(addr) => {
                let mut raw_bits = 0b0100_0000;

                assert!(addr < 2u8.pow(6), "CGRAM address out of range");

                raw_bits += addr;

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
            }

            CommandSet::SetDDRAM(addr) => {
                let mut raw_bits = 0b1000_0000;

                assert!(addr < 2u8.pow(7), "DDRAM address out of range");

                raw_bits += addr;

                Self::new(
                    RegisterSelection::Command,
                    ReadWriteOp::Write,
                    Some(Bits::Bit8(raw_bits)),
                )
            }

            CommandSet::ReadBusyFlagAndAddress => {
                Self::new(RegisterSelection::Command, ReadWriteOp::Read, None)
            }

            CommandSet::WriteDataToRAM(data) => Self::new(
                RegisterSelection::Data,
                ReadWriteOp::Write,
                Some(Bits::Bit8(data)),
            ),

            CommandSet::ReadDataFromRAM => {
                Self::new(RegisterSelection::Data, ReadWriteOp::Read, None)
            }
        }
    }
}
