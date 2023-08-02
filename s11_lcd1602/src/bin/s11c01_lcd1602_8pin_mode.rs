#![no_std]
#![no_main]

// A0/A1/A2 RS/RW/E
// B0~B7 D0~D7

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::pac;

mod utils;
use utils::{
    common::delay,
    mode_8pin::{
        send::{send, wait_and_send},
        setup::{setup_gpioa, setup_gpiob},
    },
};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();

    setup_gpioa(&dp);
    setup_gpiob(&dp);

    // 这里其实还缺少了一步，就是给 LCD1602 的 Vcc 断电再供电
    //
    // 第二个是，应该给 E 引脚一个外部的下拉电阻，因为我们可能会修改单片机的程序，在我们重置单片机的过程中，E 引脚必然是悬空的，
    // 而 LCD1602 又已经被我们初始化过了，因此 LCD1602 会随意捕获到混乱的数据，导致显示出错，因此给 E 一个外部下拉电阻，就可以避免这个问题

    delay(&cp, 100_000);
    send(&dp, 0, 0, 0b00111000);

    delay(&cp, 40);
    send(&dp, 0, 0, 0b00111000);

    wait_and_send(&dp, &cp, 0, 0, 0b00001111, 10);
    wait_and_send(&dp, &cp, 0, 0, 0b00000001, 10);
    wait_and_send(&dp, &cp, 0, 0, 0b00000110, 10);

    // Write data to DDRAM

    wait_and_send(&dp, &cp, 0, 0, 0b10000000, 10);

    for data in [
        0b0100_1101,
        0b0110_1001,
        0b0110_0001,
        0b0110_1111,
        0b0010_0000,
    ] {
        delay(&cp, 500_000);
        wait_and_send(&dp, &cp, 1, 0, data, 10);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
