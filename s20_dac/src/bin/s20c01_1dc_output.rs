//! DAC 数模转换器
//!
//! 一种将数字信号转换为模拟信号的外设
//!
//! 该模块将 V_{DDA} 和 V_{SSA} 作为输出电压的上下界，可额外给定一个 V_{REF+}，作为一个更精确的上界
//!
//! 该模块的精度为 12 bit，也就是说最多可以将电压均分为 2^12 = 4096 阶，不过其也支持 8 bit 模式，精度会稍稍下降一些
//!
//! STM32F413 上的 DAC 模块有 2 个通道，每个通道都可以独立输出模拟电压
//!
//! 我们这里简单地输出一个直流电压，看看 DAC 的简易配置流程

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac::Peripherals;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    let dp = Peripherals::take().unwrap();

    // DAC 的 Channel 1 对应的输出引脚为 GPIO PA4
    // RM 中建议我们要预先将 GPIO PA4 设置为模拟输入，再开启 DAC 的输出
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    dp.GPIOA.moder.modify(|_, w| w.moder4().analog());

    dp.RCC.apb1enr.modify(|_, w| w.dacen().enabled());

    let dac = &dp.DAC;

    // DHR12R1 是 Data Holding Register 12-bit Right-aligned channel 1 的缩写
    // 标识我们要给出一个右对齐的 12 bit 的数据，该数据最后会控制 channel 1 输出的模拟电压
    // 这里我们给出的是 4096 的一半，因此我们检测到的输出电压应该也在 1/2 V_{REF+} 左右（大概是 1.6V ~ 1.7V）
    dac.dhr12r1.write(|w| w.dacc1dhr().bits(2048));

    // 然后我们启动 DAC 的输出
    dac.cr.modify(|_, w| w.en1().enabled());

    #[allow(clippy::empty_loop)]
    loop {}
}
