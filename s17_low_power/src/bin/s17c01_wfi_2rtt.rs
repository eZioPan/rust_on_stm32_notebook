//! 在 Sleep mode 下使用 RTT
//!
//! 实际上，除了启用 DBG_SLEEP 以外，我们还得额外设置一个东西，才能让 RTT 正常被 DAPLink 拉取
//!
//! 依照 STM32F411 的勘误表（Errata sheet）（我手上这份编号是 ES0287）
//! Debugging Sleep/Stop mode with WFE/WFI entry 节的说法
//! 我们可以启用 DMA 让 AHB 在 Sleep 的时候保持活跃
//!
//! 以便我们可以随时从 SRAM 中获取数据

#![no_std]
#![no_main]

use core::cell::RefCell;
use cortex_m::{interrupt::Mutex, peripheral::NVIC};

use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};

use stm32f4xx_hal::{interrupt, pac::Peripherals};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("\nProgram Start");

    let dp = Peripherals::take().unwrap();

    dp.DBGMCU.cr.modify(|_, w| w.dbg_sleep().set_bit());

    // 为了 RTT，我们额外使用 RMA 保持 AHB 的活跃
    dp.RCC.ahb1enr.modify(|_, w| w.dma1en().enabled());

    let rcc = &dp.RCC;

    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}

    rcc.cfgr.modify(|_, w| w.hpre().div8());

    rcc.ahb1enr.modify(|_, w| w.gpiocen().enabled());

    let gpioc = &dp.GPIOC;

    gpioc.odr.modify(|_, w| w.odr13().high());

    gpioc.moder.modify(|_, w| w.moder13().output());

    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());

    let tim2 = &dp.TIM2;

    tim2.psc.write(|w| w.psc().bits(1_000));
    tim2.arr.write(|w| w.arr().bits(1_000));
    tim2.dier.modify(|_, w| w.uie().enabled());
    tim2.cr1.modify(|_, w| w.cen().enabled());

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    unsafe { NVIC::unmask(interrupt::TIM2) };

    let mut cnt: u16 = 1;

    loop {
        cortex_m::interrupt::free(|cs| {
            let dp_ref = G_DP.borrow(cs).borrow();
            let dp = dp_ref.as_ref().unwrap();

            let gpioc = &dp.GPIOC;
            if gpioc.odr.read().odr13().is_low() {
                gpioc.odr.modify(|_, w| w.odr13().high())
            } else {
                gpioc.odr.modify(|_, w| w.odr13().low())
            }
        });

        rprint!("\x1b[2K\rWake up: {}", cnt);
        cnt += 1;
        cortex_m::asm::wfi();
    }
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.sr.modify(|_, w| w.uif().clear());
    });
}
