#![no_std]
#![no_main]

// A0/A1/A2 RS/RW/E
// B4~B7 D4~D7

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::pac;

mod utils;

use utils::{
    common::delay,
    mode_4pin::{
        send::{send_4bit, send_8bit, wait_and_send_8bit},
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

    // 初始化流程
    //
    // 这里其实还缺少了一步，就是给 LCD1602 的 Vcc 断电再供电
    // 可能在 8 bit 模式下，这个问题还不明显，但在 4 bit 模式下，
    // 如果给一个已经初始化过的 ST7066U 再次执行 4 bit 的初始化，会立刻让 LCD1602 变成不可用状态
    // 需要再次进行一次初始化，才能正确运行，
    // 这种效果看起来就是：4 bit 模式下，除了上电那次一次初始化就成功，其他情况下都得重置两次，才能成功
    // 所以其实应该 Vcc 加一个三极管或 mos 管，然后每次初始化的第一步就是给 Vcc 断电再供电
    //
    // 第二个是，应该给 E 引脚一个外部的下拉电阻，因为我们可能会修改单片机的程序，在我们重置单片机的过程中，E 引脚必然是悬空的，
    // 而 LCD1602 又已经被我们初始化过了，因此 LCD1602 会随意捕获到混乱的数据，导致显示出错，因此给 E 一个外部下拉电阻，就可以避免这个问题

    delay(&cp, 100_000);
    send_4bit(&dp, 0, 0, 0b0010);

    delay(&cp, 40);
    send_8bit(&dp, 0, 0, 0b0010_1000);

    delay(&cp, 40);
    send_8bit(&dp, 0, 0, 0b0010_1000);

    wait_and_send_8bit(&dp, &cp, 0, 0, 0b0000_1111, 10);
    wait_and_send_8bit(&dp, &cp, 0, 0, 0b0000_0001, 10);
    wait_and_send_8bit(&dp, &cp, 0, 0, 0b0000_0110, 10);

    //init end

    wait_and_send_8bit(&dp, &cp, 0, 0, 0b10000000, 10);

    for data in [
        0b0100_1101,
        0b0110_1001,
        0b0110_0001,
        0b0110_1111,
        0b0010_0000,
    ] {
        delay(&cp, 500_000);
        wait_and_send_8bit(&dp, &cp, 1, 0, data, 10);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
