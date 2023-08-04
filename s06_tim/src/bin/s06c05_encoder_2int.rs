//! 编码器接口 - 最高频率中断触发
//!
//! 这里我们还是要实现前一篇的编码器效果，但是会使用中断的方法来告知 Cortex 核心处理数据

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use stm32f4xx_hal::{interrupt, pac};

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

static G_DP: Mutex<RefCell<Option<pac::Peripherals>>> = Mutex::new(RefCell::new(None));
// 由于我们要求最高频率触发中断，因此计数的功能就不能交给 TIM 的 CNT 寄存器实现了
// 我们需要自己维护一个计数器
static G_NUM: Mutex<Cell<i16>> = Mutex::new(Cell::new(0));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();

    let rcc = &dp.RCC;

    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}

    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    dp.GPIOA.pupdr.modify(|_, w| {
        w.pupdr0().pull_down();
        w.pupdr1().pull_down();
        w
    });
    dp.GPIOA.afrl.modify(|_, w| {
        w.afrl0().af1();
        w.afrl1().af1();
        w
    });
    dp.GPIOA.moder.modify(|_, w| {
        w.moder0().alternate();
        w.moder1().alternate();
        w
    });

    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());

    let tim2 = &dp.TIM2;

    let ccmr1 = tim2.ccmr1_input();

    tim2.smcr.modify(|_, w| {
        w.sms().encoder_mode_3();
        w
    });

    ccmr1.modify(|_, w| {
        w.cc1s().ti1();
        w.ic1f().fdts_div32_n8();
        w.cc2s().ti2();
        w.ic2f().variant(15);
        w
    });

    tim2.ccer.modify(|_, w| {
        w.cc1p().clear_bit();
        w.cc1np().clear_bit();
        w.cc2p().clear_bit();
        w.cc2np().clear_bit();
        w
    });

    // 在这里，我们要求获得最高频率的中断触发，那么 ARR 的值就应该尽可能的小
    // 在不永续触发中断的情况下，ARR 的最小值为 1
    // 此时 ARR 会每 2 个 Encoder Interface 信号触发一次中断
    //
    // 然后这个每 2 次触发一个中断，正好会让 TI1 对应的按键**看起来**没有修改 TIM 的状态
    // 其实这是一个错觉，因为一个编码周期刚好触发 4 个中断，刚好是 2 的倍数
    // 如果你把 ARR 的值改到 2，就能看到两个按钮都有触发中断的时候，不过这样做就不是最高频率的中断了
    // 因此这里我们不使用 ARR 为 2，而使用 ARR 为 1
    tim2.arr.modify(|_, w| w.arr().bits(1));

    // 开启 Update 中断标识位
    tim2.dier.modify(|_, w| w.uie().enabled());

    // 启动定时器
    tim2.cr1.modify(|_, w| w.cen().enabled());

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 启用 NVIC 的 TIM2 中断
    unsafe { NVIC::unmask(interrupt::TIM2) };

    #[allow(clippy::empty_loop)]
    loop {}
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let mut num = G_NUM.borrow(cs).get();

        let tim2 = &dp.TIM2;

        // 清理 TIM2 的 Update 中断标识位
        tim2.sr.modify(|_, w| w.uif().clear_bit());

        // 然后依照 TIM 的 CR1 寄存器的 DIR 值，修改我们自己维护的计数器的值
        // 读取 CNT 的当前值没有意义，因为在中断触发的时候，CNT 的值必然是 0
        // 只要 Cortex 核心的处理速度远高于编码器的输出速度，那么 Cortex 读取 CNT 的值就总会是 0
        num = match tim2.cr1.read().dir().bit() {
            true => num - 1,
            false => num + 1,
        };

        rprint!("\x1b[2K\r{}", num);

        G_NUM.borrow(cs).set(num);
    });
}
