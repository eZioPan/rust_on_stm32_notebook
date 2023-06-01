use stm32f4xx_hal::timer::SysDelay;

mod impl_builder_api;

use super::{
    command_set::{Font, LineMode, MoveDirection, ShiftType, State},
    pins::Pins,
    LCD,
};

pub struct Builder {
    pins: Option<Pins>,
    delayer: Option<SysDelay>,
    line: LineMode,
    font: Font,
    display_on: State,
    cursor_on: State,
    cursor_blink: State,
    dir: MoveDirection,
    shift_type: ShiftType,
    cursor_pos: (u8, u8),
    wait_interval_us: u32,
}

pub trait BuilderAPI {
    fn build_and_init(self) -> LCD;
    fn new(pins: Pins, delayer: SysDelay) -> Self;
    fn pop_pins(&mut self) -> Pins;
    fn pop_delayer(&mut self) -> SysDelay;
    fn set_line(self, line: LineMode) -> Self;
    fn get_line(&self) -> LineMode;
    fn set_font(self, font: Font) -> Self;
    fn get_font(&self) -> Font;
    fn set_display(self, display: State) -> Self;
    fn get_display(&self) -> State;
    fn set_cursor(self, cursor: State) -> Self;
    fn get_cursor(&self) -> State;
    fn set_blink(self, blink: State) -> Self;
    fn get_blink(&self) -> State;
    fn set_direction(self, dir: MoveDirection) -> Self;
    fn get_direction(&self) -> MoveDirection;
    fn set_shift(self, shift: ShiftType) -> Self;
    fn get_shift(&self) -> ShiftType;
    fn set_cursor_pos(self, pos: (u8, u8)) -> Self;
    fn get_cursor_pos(&self) -> (u8, u8);
    fn set_wait_interval_us(self, interval: u32) -> Self;
    fn get_wait_interval_us(&self) -> u32;
}
