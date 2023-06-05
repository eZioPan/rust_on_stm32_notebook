use embedded_hal::{
    blocking::delay::{DelayMs, DelayUs},
    digital::v2::{InputPin, OutputPin},
};

use super::{
    command_set::{LineMode, MoveDirection, ShiftType, State},
    FlapType, LCDAnimation, LCDBasic, LCDExt, MoveType, LCD,
};

impl<ControlPin, DBPin, const PIN_CNT: usize, Delayer> LCDAnimation
    for LCD<ControlPin, DBPin, PIN_CNT, Delayer>
where
    ControlPin: OutputPin,
    DBPin: OutputPin + InputPin,
    Delayer: DelayMs<u32> + DelayUs<u32>,
{
    /// 以特定的时间间隔，切换整个屏幕特定次数
    /// 当 count 为 0 时，永续切换屏幕
    fn full_display_blink(&mut self, count: u32, interval_us: u32) {
        match count == 0 {
            true => loop {
                self.delay_us(interval_us);
                self.toggle_display();
            },
            false => {
                (0..count * 2).into_iter().for_each(|_| {
                    self.delay_us(interval_us);
                    self.toggle_display();
                });
            }
        }
    }

    fn typewriter_write(&mut self, str: &str, extra_delay_us: u32) {
        str.chars().for_each(|char| {
            self.delay_us(extra_delay_us);
            self.write_char_to_cur(char);
        })
    }

    fn split_flap_write(
        &mut self,
        str: &str,
        ft: FlapType,
        max_flap_count: u8,
        per_flap_delay_us: u32,
        per_char_delay_us: Option<u32>,
    ) {
        // 首先要检查的是，输入的字符串中的每个字符，是否能适合产生翻页效果（应该在 ASCII 0x20 到 0x7D 的区间）
        let test_result = str.chars().all(|char| {
            if char.is_ascii() && (0x20 <= char as u8) && (char as u8 <= 0x7D) {
                true
            } else {
                false
            }
        });

        assert!(test_result, "Currently only support ASCII 0x20 to 0x7D");

        let mut cursor_state_changed = false;

        // 如果显示光标，则光标总是会出现在下一个字符的位置，这里需要关掉
        if self.get_cursor_state() != State::Off {
            self.set_cursor_state(State::Off);
            cursor_state_changed = true;
        }

        match ft {
            FlapType::Sequential => {
                assert!(
                    per_char_delay_us.is_some(),
                    "Should set some per char delay in Sequential Mode"
                );
                str.chars().for_each(|char| {
                    let cur_byte = char as u8;

                    let flap_start_byte = if max_flap_count == 0 || cur_byte - max_flap_count < 0x20
                    {
                        0x20
                    } else {
                        cur_byte - max_flap_count
                    };

                    let cur_pos = self.get_cursor_pos();

                    self.delay_us(per_char_delay_us.unwrap());
                    (flap_start_byte..=cur_byte).for_each(|byte| {
                        self.delay_us(per_flap_delay_us);
                        self.write_u8_to_pos(byte, cur_pos);
                    });

                    self.shift_cursor_or_display(
                        ShiftType::CursorOnly,
                        self.get_default_direction(),
                    );
                })
            }
            FlapType::Simultaneous => {
                unimplemented!()
            }
        }

        if cursor_state_changed {
            self.set_cursor_state(State::On);
        }
    }

    fn shift_display_to_pos(
        &mut self,
        target_offset: u8,
        mt: MoveType,
        display_state_when_shift: State,
        delay_us_per_step: u32,
    ) {
        let before_offset = self.get_display_offset();

        // 如果当前的 offset 和指定的 offset 相同，直接返回即可
        if before_offset == target_offset {
            return;
        }

        let line_capacity = match self.get_line_mode() {
            LineMode::OneLine => {
                assert!(
                    target_offset < 80,
                    "display offset too big, should less than 80"
                );
                80
            }
            LineMode::TwoLine => {
                assert!(
                    target_offset < 40,
                    "display offset too big, should less than 40"
                );
                40
            }
        };

        let before_state = self.get_display_state();

        // 依照用户的设置，关闭或开启屏幕
        self.set_display_state(display_state_when_shift);

        // 没有必要在这里反复操作设备，这里只需要计算移动的距离和方向即可
        let (distance, direction) = match mt {
            MoveType::ForceMoveLeft => {
                if target_offset < before_offset {
                    (before_offset - target_offset, MoveDirection::RightToLeft)
                } else {
                    (
                        line_capacity - (target_offset - before_offset),
                        MoveDirection::RightToLeft,
                    )
                }
            }

            MoveType::ForceMoveRight => {
                if target_offset > before_offset {
                    (target_offset - before_offset, MoveDirection::LeftToRight)
                } else {
                    (
                        line_capacity - (before_offset - target_offset),
                        MoveDirection::LeftToRight,
                    )
                }
            }

            MoveType::NoCrossBoundary => {
                if target_offset > before_offset {
                    (target_offset - before_offset, MoveDirection::LeftToRight)
                } else {
                    (before_offset - target_offset, MoveDirection::RightToLeft)
                }
            }

            MoveType::Shortest => {
                if target_offset > before_offset {
                    if target_offset - before_offset <= line_capacity / 2 {
                        (target_offset - before_offset, MoveDirection::LeftToRight)
                    } else {
                        (
                            line_capacity - (target_offset - before_offset),
                            MoveDirection::RightToLeft,
                        )
                    }
                } else {
                    if before_offset - target_offset <= line_capacity / 2 {
                        (before_offset - target_offset, MoveDirection::RightToLeft)
                    } else {
                        (
                            line_capacity - (before_offset - target_offset),
                            MoveDirection::LeftToRight,
                        )
                    }
                }
            }
        };

        (0..(distance)).for_each(|_| {
            self.delay_us(delay_us_per_step);
            self.shift_cursor_or_display(ShiftType::CursorAndDisplay, direction);
        });

        // 无论上面做了怎样的操作，我们都还原初始的屏幕状态
        self.set_display_state(before_state);
    }

    fn delay_ms(&mut self, ms: u32) {
        if ms > 0 {
            self.delayer.delay_ms(ms);
        }
    }

    fn delay_us(&mut self, us: u32) {
        if us > 0 {
            self.delayer.delay_us(us);
        }
    }
}
