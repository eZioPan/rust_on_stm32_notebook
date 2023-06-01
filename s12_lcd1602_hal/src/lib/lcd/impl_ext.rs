use super::{command_set::State, Ext, LCD, LCDAPI};

impl Ext for LCD {
    /// 以特定的时间间隔，切换整个屏幕特定次数
    /// 当 count 为 0 时，永续切换屏幕
    fn full_display_blink(&mut self, count: u32, interval_us: u32) {
        if count == 0 {
            loop {
                self.delay_us(interval_us);
                self.toggle_display();
            }
        } else {
            for _ in 0..count * 2 {
                self.delay_us(interval_us);
                self.toggle_display();
            }
        }
    }

    fn toggle_display(&mut self) {
        match self.get_display() {
            State::Off => self.set_display(State::On),
            State::On => self.set_display(State::Off),
        }
    }

    fn typewriter_write(&mut self, str: &str, extra_delay_us: u32) {
        for char in str.chars() {
            self.delay_us(extra_delay_us);
            self.write_char(char);
        }
    }

    fn write_str(&mut self, str: &str) {
        for char in str.chars() {
            self.write_char(char);
        }
    }

    /// 这里的字符仅覆盖了如下范围：
    /// ASCII 0x20 到 0x7D
    fn write_char(&mut self, char: char) {
        let out_byte = match char.is_ascii() {
            true => {
                let out_byte = char as u8;
                if out_byte >= 0x20 && out_byte <= 0x7D {
                    out_byte
                } else {
                    0xFF
                }
            }
            false => 0xFF,
        };

        self.write_to_cur(out_byte);
    }
}
