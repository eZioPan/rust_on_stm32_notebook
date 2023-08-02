//! SysTick 的“中断”用法
//!
//! 注意到我们的“中断”打了引号，这是因为 SysTick 属于核心，它触发实际上是“异常（exception）”而非“中断（interrupt）”
//! 这里有两个注意点
//! 1. SysTick 抛出异常不需要经过 NVIC，只需要在 SysTick 自己的寄存器里启用中断（TickInt）即可
//! 2. SysTick 的 Handle 的标记是 `#[exception]` 而非 `#[interrupt]`

#![no_std]
#![no_main]

use core::cell::Cell;

use cortex_m::interrupt::Mutex;
use cortex_m_rt::exception;
use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot take device peripherals");

    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}
    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}

    let systick = &dp.STK;

    systick
        .load
        .modify(|_, w| unsafe { w.reload().bits(999_999) });

    systick.val.reset();

    systick.ctrl.modify(|_, w| {
        w.clksource().bit(false);
        // 打开 SysTick 下溢出的中断
        w.tickint().bit(true);
        w.enable().set_bit();
        w
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|cs| {
        // 实际上我们没有必要清理 CountFlag，因为异常的抛出不依赖于这标志位
        let cnt_handle = G_CNT.borrow(cs);
        let mut cnt = cnt_handle.get();
        cnt += 1;
        rprint!("\x1b[2K\r{}", cnt);
        cnt_handle.set(cnt);
    });
}
