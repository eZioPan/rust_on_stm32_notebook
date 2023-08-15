//! 这里我们倒过来，使用 SleepOnExit 的方式，让 Cortex 在调用 interrupt handle 后，主动进入睡眠模式
//!
//! 这种模式在 Reference Manual 中称为 Return from ISR

#![no_std]
#![no_main]

use core::cell::RefCell;
use cortex_m::{interrupt::Mutex, peripheral::NVIC};

use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};

use stm32f4xx_hal::{
    interrupt,
    pac::{CorePeripherals, Peripherals},
};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("\nProgram Start");

    let dp = Peripherals::take().unwrap();
    let mut cp = CorePeripherals::take().unwrap();

    // 这里启用了 Cortex 的 SCB 模块的 SCR 寄存器下的 SLEEPONEXIT 位
    //
    // 可以尝试注释掉下面这行，你就会发现，每次 LED 灯改变状态的时候，RTT 窗口都会更新 "empty looping" 的输出
    // 此时每次 MCU 进入 Sleep mode，都是由 loop 循环里的 WFI 指令发出的
    //
    // 如果我们启用了这行，那么 RTT 窗口是看不到 "empty looping" 的输出的
    // 此时，除了第一个 Sleep 是由 loop 里的 WFI 指令产生的，其后的 Sleep 都是设置了 SleepOnExit 的效果
    cp.SCB.set_sleeponexit();

    dp.DBGMCU.cr.reset();
    dp.DBGMCU.cr.modify(|_, w| w.dbg_sleep().set_bit());

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

    cortex_m::interrupt::free(|cs: &cortex_m::interrupt::CriticalSection| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    unsafe { NVIC::unmask(interrupt::TIM2) };

    let mut cnt: u16 = 1;

    loop {
        cortex_m::asm::wfi();
        rprint!("\x1b[2K\rempty looping: {}", cnt);
        cnt += 1;
    }
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.sr.modify(|_, w| w.uif().clear());

        let gpioc = &dp.GPIOC;
        if gpioc.odr.read().odr13().is_low() {
            gpioc.odr.modify(|_, w| w.odr13().high())
        } else {
            gpioc.odr.modify(|_, w| w.odr13().low())
        }
    });
}
