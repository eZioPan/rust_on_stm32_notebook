use crate::lcd::RAMType;

use super::{
    command_set::{CommandSet, State},
    LCDBasic, LCDExt, PinsInteraction, LCD,
};

impl LCDExt for LCD {
    fn toggle_display(&mut self) {
        match self.get_display_state() {
            State::Off => self.set_display_state(State::On),
            State::On => self.set_display_state(State::Off),
        }
    }

    fn write_str(&mut self, str: &str) {
        str.chars().for_each(|char| self.write_char(char));
    }

    /// 这里的字符仅覆盖了如下范围：
    /// ASCII 0x20 到 0x7D
    fn write_char(&mut self, char: char) {
        assert!(
            self.get_ram_type() == RAMType::DDRAM,
            "Current in CGRAM, use .set_cursor_pos() to change to DDRAM"
        );

        let out_byte = match char.is_ascii() {
            // 在 Rust 判定该字节为 ASCII 的同时，我们还得判定这个字符落 LCD1602 CGRAM 与 ASCII 重叠的位置
            true if (0x20 <= char as u8) && (char as u8 <= 0x7D) => char as u8,
            _ => 0xFF,
        };

        self.write_u8_to_cur(out_byte);
    }

    fn write_custom_char_to_pos(&mut self, index: u8, pos: (u8, u8)) {
        assert!(index < 8, "Only 8 graphs allowed in CGRAM");
        self.write_u8_to_pos(index, pos);
    }

    fn write_u8_to_pos(&mut self, character: impl Into<u8>, pos: (u8, u8)) {
        self.set_cursor_pos(pos);
        self.wait_and_send(CommandSet::WriteDataToRAM(character.into()));
    }

    fn extract_graph_from_cgram(&mut self, index: u8) -> [u8; 8] {
        assert!(index < 8, "index too big, should less than 8");

        // 将 index 偏移为 CGRAM 中的地址
        self.set_cgram_addr(index.checked_shl(3).unwrap());

        let mut graph: [u8; 8] = [0u8; 8];

        graph
            .iter_mut()
            .for_each(|line| *line = self.read_u8_from_cur());

        graph
    }
}
