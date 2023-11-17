//! 让 DAC 输出余弦波
//!
//! 做法是，我们先预备好余弦波的一个周期的电压值（量化到 0~4095 范围），且确定一个周期的采样数量，该数量是确定输出的余弦波实的际周期的因素之一
//! 让后我们通过轮询的方式让 Cortex 核心一直改写 DAC 的输出寄存器，则 Cortex 核心的改写速度则是另一个影响余弦波实的际周期的因素
//!
//! 注意：使用 Cortex 核心改写 DAC 输出电压的方式，有一个很大的弊病，就是其输出频率并不能非常高，
//! 在一个余弦波需要 100 个采样点，HCLK 100 MHz 的情况下，最终生成的余弦波的频率在 2 kHz 左右
//! 在相同的时钟频率下，使用 DMA 操作，最终的余弦波频率在 100 kHz 左右，差距还是比较大的

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

mod wave_data;
use wave_data::COS_WAVE_100 as COS_WAVE;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    let dp = pac::Peripherals::take().unwrap();

    setup_rcc(&dp);
    setup_gpio(&dp);
    setup_dac(&dp);

    const STEP: usize = 1;

    let mut index: usize = 0;

    // 在循环中不断改写 DHR 的值
    loop {
        dp.DAC.dhr12r1.write(|w| w.dacc1dhr().bits(COS_WAVE[index]));
        if index + STEP < COS_WAVE.len() {
            index += STEP;
        } else {
            index = COS_WAVE.len() - index;
        }
    }
}

// 把 STM32F413 的频率拉到 100 MHz
fn setup_rcc(dp: &pac::Peripherals) {
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}

    dp.RCC.pllcfgr.modify(|_, w| {
        w.pllsrc().hse();
        unsafe {
            w.pllm().bits(6);
            w.plln().bits(100)
        };
        w.pllp().div2();
        w
    });

    dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
    dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b01) });

    dp.RCC.cr.modify(|_, w| w.pllon().on());
    while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
    while dp.RCC.cr.read().pllrdy().is_not_ready() {}

    dp.FLASH.acr.modify(|_, w| {
        w.latency().ws3();
        w.dcen().enabled();
        w.icen().enabled();
        w.prften().enabled();
        w
    });

    dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

    dp.RCC.cfgr.modify(|_, w| w.sw().pll());
    while !dp.RCC.cfgr.read().sws().is_pll() {}
}

// 将 GPIO PA4 切换到 analog 模式
fn setup_gpio(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    dp.GPIOA.moder.modify(|_, w| w.moder4().analog());
}

// 启动 DAC
fn setup_dac(dp: &pac::Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.dacen().enabled());
    dp.DAC.cr.modify(|_, w| w.en1().enabled());
}
