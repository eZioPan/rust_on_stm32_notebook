//! 用 STM32F411RET6 驱动一个 LCD1602
//! 使用了 LCD1602 的 4 bit 模式

//! 接线图
//!
//! 其实这个连线图还是比较随意的，除了 GND 和 V5 是固定的引脚之外，其它的 GPIO 引脚是可以随便调整的
//!
//! LCD <-> STM32
//! Vss <-> GND
//! Vdd <-> 5V
//! V0 <-> 可变电阻 <-> 5V（调节显示对比度）
//! RS <-> PA0
//! RW <-> PA1
//! EN [<-> PA2, <-> 4.7 kOhm 下拉电阻 <-> GND]
//! D4 <-> PA3
//! D5 <-> PA4
//! D6 <-> PA5
//! D7 <-> PA6
//! A <-> 可变电阻 <-> 5V（这里路的可变电阻我设计用来调节背光亮度，是可选的，而且准确来说应该用 PWM 调光，我这里就不再设计了）
//! K <-> GND

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::{pac, prelude::*};

use lcd1602::{
    command_set::{CommandSet, Font, Line, MoveDirection, ShiftType, State},
    lcd_builder::LCDBuilder,
    lcd_pins::LCDPins,
};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot take device peripherals");
    let cp = pac::CorePeripherals::take().expect("Cannot take core peripherals");

    let rcc = dp.RCC.constrain();

    // 其实这个 Clocks 还挺有趣的，它记录了各种总线、Cortex 核心，以及 I2S 的运行频率，以及两个 APB 的分频值
    // 算是 STM32CubeMX Clock 视图的替换了
    let clocks = rcc.cfgr.use_hse(8.MHz()).freeze();

    let delayer = cp.SYST.delay(&clocks);

    let gpioa = dp.GPIOA.split();

    // 准确来说，这三个引脚应该在外部接分别接一个小一点的上拉电阻（比如 4.7KOhm 的）
    // 不过我手上没有合适的电阻，这里就先用 pull_push 模式替代了
    let rs_pin = gpioa.pa0.into_push_pull_output().erase();
    let rw_pin = gpioa.pa1.into_push_pull_output().erase();

    // EN 引脚的问题，我还么有想好，准确来说，它应该在外部接一个下拉电阻，防止单片机重启的时候，电平跳动，导致 LCD1602 收到奇怪的信号
    // 但如果我们将这个口设置为开漏输出，则它又要求接一个上拉电阻，这和我们默认需要将其下拉的要求相冲突
    let en_pin = gpioa.pa2.into_push_pull_output().erase();

    let db4_pin = gpioa
        .pa3
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db5_pin = gpioa
        .pa4
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db6_pin = gpioa
        .pa5
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db7_pin = gpioa
        .pa6
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();

    let lcd_pins = LCDPins::new(rs_pin, rw_pin, en_pin, db4_pin, db5_pin, db6_pin, db7_pin);

    let lcd_builder = LCDBuilder::new(lcd_pins, delayer)
        .set_blink(State::On)
        .set_cursor(State::On)
        .set_direction(MoveDirection::Right)
        .set_display(State::On)
        .set_font(Font::Font5x8)
        .set_line(Line::Line2)
        .set_shift(ShiftType::Cursor);

    let mut lcd = lcd_builder.build_and_init();

    lcd.wait_and_send(CommandSet::SetDDRAM(0b000_0000), 10);

    for data in "hello, world!".as_bytes() {
        lcd.delay_ms(250u32);
        lcd.wait_and_send(CommandSet::WriteDataToRAM(*data), 10);
    }

    lcd.delay_ms(250u32);
    lcd.wait_and_send(CommandSet::SetDDRAM(0x40), 10);

    for data in "hello, LCD1602!".as_bytes() {
        lcd.delay_ms(250u32);
        lcd.wait_and_send(CommandSet::WriteDataToRAM(*data), 10);
    }

    loop {}
}
