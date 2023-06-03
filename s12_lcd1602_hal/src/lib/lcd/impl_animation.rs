use super::{LCDAnimation, LCDExt, LCD};
use embedded_hal::blocking::delay::{DelayMs, DelayUs};

impl LCDAnimation for LCD {
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

    fn typewriter_write(&mut self, str: &str, extra_delay_us: u32) {
        for char in str.chars() {
            self.delay_us(extra_delay_us);
            self.write_char(char);
        }
    }
    fn delay_ms(&mut self, ms: u32) {
        self.delayer.delay_ms(ms);
    }

    fn delay_us(&mut self, us: u32) {
        self.delayer.delay_us(us);
    }
}
