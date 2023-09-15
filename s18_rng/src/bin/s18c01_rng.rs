//! RNG
//!
//! 真随机数生成器
//!
//! STM32F413 的真随机数发生器，通过采样内部的模拟噪声源，可以多次生成 32-bit 的随机数
//! 从 data sheet 的 block diagram 我们可以看到 RNG 是挂载在 AHB2 总线上的，而且我们还需要注意到 USB OTG FS 也挂载在 AHB2 总线上
//! 而且，从 Reference Manual 的 RNG 的 block diagram 和介绍中，我们可以看到，RNG 还需要一个额外的 rng_clk 时钟，
//! 这个时钟可以看一下 Reference Manual 的 Clock tree 图，它处于图的右沿下侧，上一级是 CK48MSEL，再上一级有两个来源，分别为 PLL 和 PLLI2C
//! 于是，我们在使用 RNG 时，就需要同时开启这两个时钟
//! 另外在确定 rng_clk 的频率的时候，由于这个频率也直接控制 USB OTG FS，而且 USB OTG FS 需要 48 MHz，这里我们也可以将 rng_clk 的频率设置到 48 MHz
//! 不过实际上 rng_clk 需要满足的频率为大于 AHB2 频率的 1/12，这里我们调节到 48 MHz，也是满足这个条件的

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    interrupt,
    pac::{Peripherals, NVIC},
};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = Peripherals::take().unwrap();

    let rcc = &dp.RCC;

    // 我手上这块开发板的 HSE 外置晶振的频率为 12 MHz
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}

    // 设置 PLL
    // 这里的目标是让 RNG 的 rng_clk 运行在 48 MHz，让其它设备的频率低一些
    // 这里我们让 sysclk 运行在 24 MHz 上，后面会继续降低 AHB 的频率
    rcc.pllcfgr.modify(|_, w| {
        w.pllsrc().hse();
        unsafe {
            w.pllm().bits(6); // 进入 VCO 时降低到 2 MHz
            w.plln().bits(96); // 增加到 192 MHz
            w.pllq().bits(4); // 输出给 CK48MSEL 为 48 MHz
        }
        w.pllp().div8(); // 输出给 sysclk 为 24 MHz
        w
    });

    // AHB 再将到 3 MHz
    rcc.cfgr.modify(|_, w| w.hpre().div8());

    // CK48MSEL 确保为使用主 PLL 的输出
    // 其实为默认值，不需要额外设置
    rcc.dckcfgr2.modify(|_, w| w.ck48msel().pll());

    rcc.cr.modify(|_, w| w.pllon().on());
    while rcc.cr.read().pllrdy().is_not_ready() {}

    rcc.cfgr.modify(|_, w| w.sw().pll());
    while !rcc.cfgr.read().sws().is_pll() {}

    // 使用 TIM2 作为定时器，每秒触发一个中断，在中断中我们读取一下 RNG 生成的随机数
    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());
    let tim2 = &dp.TIM2;
    tim2.psc.write(|w| w.psc().bits(3_000));
    tim2.arr.write(|w| w.bits(1_000));
    tim2.dier.modify(|_, w| w.uie().enabled());

    // RNG 模块本身没有什么好设置的
    // 开启时钟即可
    rcc.ahb2enr.modify(|_, w| w.rngen().enabled());
    let rng = &dp.RNG;
    rng.cr.modify(|_, w| w.rngen().set_bit());

    unsafe { NVIC::unmask(interrupt::TIM2) };

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();

        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.cr1.modify(|_, w| w.cen().enabled());
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.sr.modify(|_, w| w.uif().clear());

        let rng = &dp.RNG;

        // 检测 RNG 生成完成，并读取一下数据
        match rng.sr.read().drdy().bit() {
            false => rprintln!("RNG Data not ready"),
            true => {
                let rng_data = dp.RNG.dr.read().rndata().bits();
                rprintln!("{:0.04}", rng_data as f32 / u32::MAX as f32);
            }
        }
    });
}
