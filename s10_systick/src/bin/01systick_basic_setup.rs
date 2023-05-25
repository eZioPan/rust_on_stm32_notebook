//! SysTick 系统定时器
//!
//! 系统定时器 SysTick 是 Cortex-M4 核心中自带的 24-bit 倒计时定时器，它会从 STK_LOAD 寄存器指定的数值一直倒数到 0，
//! 并在下一个时钟沿载入 STK_LOAD 寄存器的值，再次开始倒数
//! 当 Cortex 核心由于 Debug 指令停机时，SysTick 会随之暂停运行
//!
//! 注：由于 SysTick 是每个使用了 Cortex-M4 核心的 STM32 芯片都有的功能，因此它和其它 Cortex-M4 自带的 Core Peripherals 的说明
//! 都放在了 STM32 Cortex®-M4 MCUs and MPUs programming manual 这个文件中，并不在各个芯片的 Reference Manual 中

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use stm32f4xx_hal::pac::{CorePeripherals, Peripherals};

#[allow(unused)]
const ENABLE_OFFSET: u8 = 0;
#[allow(unused)]
const TICKINTL_OFFSET: u8 = 1;
#[allow(unused)]
const CLKSOOURCE_OFFSET: u8 = 2;
#[allow(unused)]
const COUNTFLAG_OFFSET: u8 = 16;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let cp = CorePeripherals::take().expect("Cannot take Cortex peripherals");
    let dp = Peripherals::take().expect("Cannot take device peripherals");

    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}
    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}

    let systick = &cp.SYST;

    unsafe {
        // HSE 8 MHz 输入，被 8 分频之后，剩下 1 MHz
        // 向 RVR 写入 999_999，可以以 1 次每秒的速率让系统时钟重载
        // RVR: Reload Value Register
        systick.rvr.write(1_000_000 - 1);

        systick.csr.modify(|w| set_bit(w, ENABLE_OFFSET));
    };

    let mut counter = 0;

    loop {
        // 实际上系统时钟常见的功能是 delay，而非触发中断
        if systick.csr.read() >> COUNTFLAG_OFFSET & 1 == 1 {
            rprint!("\x1b[2K\r{}", counter);
            counter += 1;
        }
    }
}

#[allow(dead_code)]
fn set_bit(value: u32, offset: u8) -> u32 {
    value | (1 << offset)
}

#[allow(dead_code)]
fn clear_bit(value: u32, offset: u8) -> u32 {
    value & !(1 << offset)
}
