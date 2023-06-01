use stm32f4xx_hal::timer::SysDelay;

use self::{
    command_set::{Font, LineMode, MoveDirection, ShiftType, State},
    full_command::FullCommand,
    pins::Pins,
};

pub mod builder;
pub mod command_set;
mod full_command;
mod impl_ext;
mod impl_lcd_api;
mod impl_pin_interaction;
mod impl_struct_api;
pub mod pins;

pub struct LCD {
    pins: Pins,
    delayer: SysDelay,
    line: LineMode,
    font: Font,
    display_on: State,
    cursor_on: State,
    cursor_blink: State,
    direction: MoveDirection,
    shift_type: ShiftType,
    cursor_pos: (u8, u8),
    wait_interval_us: u32,
}

pub trait Ext {
    fn write_char(&mut self, char: char);
    fn write_str(&mut self, str: &str);
    fn typewriter_write(&mut self, str: &str, extra_delay_us: u32);
    fn toggle_display(&mut self);

    fn full_display_blink(&mut self, count: u32, change_interval_us: u32);
}

pub trait LCDAPI {
    fn init_lcd(&mut self);
    fn write_to_cur(&mut self, character: impl Into<u8>);
    fn write_to_pos(&mut self, character: impl Into<u8>, pos: (u8, u8));
    fn delay_ms(&mut self, ms: u32);
    fn delay_us(&mut self, us: u32);
    fn clean_display(&mut self);
    fn set_line(&mut self, line: LineMode);
    fn get_line(&self) -> LineMode;
    fn set_font(&mut self, font: Font);
    fn get_font(&self) -> Font;
    fn set_display(&mut self, display: State);
    fn get_display(&self) -> State;
    fn set_cursor(&mut self, cursor: State);
    fn get_cursor(&self) -> State;
    fn set_blink(&mut self, blink: State);
    fn get_blink(&self) -> State;
    fn set_direction(&mut self, dir: MoveDirection);
    fn get_direction(&self) -> MoveDirection;
    fn set_shift(&mut self, shift: ShiftType);
    fn get_shift(&self) -> ShiftType;
    fn set_cursor_pos(&mut self, pos: (u8, u8));
    fn get_cursor_pos(&self) -> (u8, u8);
    fn set_wait_interval_us(&mut self, interval: u32);
    fn get_wait_interval_us(&self) -> u32;
}

trait StructAPI {
    fn internal_set_line(&mut self, line: LineMode);
    fn internal_set_font(&mut self, font: Font);
    fn internal_set_display(&mut self, display: State);
    fn internal_set_cursor(&mut self, cursor: State);
    fn internal_set_blink(&mut self, blink: State);
    fn internal_set_direction(&mut self, dir: MoveDirection);
    fn internal_set_shift(&mut self, shift: ShiftType);
    fn internal_set_cursor_pos(&mut self, pos: (u8, u8));
}

trait PinsInteraction {
    fn delay_and_send(&mut self, command: impl Into<FullCommand>, wait_ms: u32) -> Option<u8>;
    fn wait_and_send(&mut self, command: impl Into<FullCommand>) -> Option<u8>;
    fn wait_for_idle(&mut self);
    fn check_busy(&mut self) -> bool;
}
