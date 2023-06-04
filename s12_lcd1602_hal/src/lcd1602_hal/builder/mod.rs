use stm32f4xx_hal::timer::SysDelay;

mod impl_builder_api;

use super::{
    command_set::{Font, LineMode, MoveDirection, ShiftType, State},
    pins::Pins,
    LCD,
};

pub struct Builder<const PIN_CNT: usize> {
    pins: Option<Pins<PIN_CNT>>,
    delayer: Option<SysDelay>,
    line: LineMode,
    font: Font,
    display_on: State,
    cursor_on: State,
    cursor_blink: State,
    dir: MoveDirection,
    shift_type: ShiftType,
    wait_interval_us: u32,
}

pub trait BuilderAPI<const PIN_CNT: usize> {
    fn build_and_init(self) -> LCD<PIN_CNT>;
    fn new(pins: Pins<PIN_CNT>, delayer: SysDelay) -> Self;
    fn pop_pins(&mut self) -> Pins<PIN_CNT>;
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
    fn set_wait_interval_us(self, interval: u32) -> Self;
    fn get_wait_interval_us(&self) -> u32;
}
