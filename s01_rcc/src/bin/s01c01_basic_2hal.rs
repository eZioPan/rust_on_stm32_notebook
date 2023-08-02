//! RCC - Reset and Clock Control

//! 本源码通过 操作 HAL（Hardware Access Layer） 来实现如下效果：
//! 启动 ADC1 的 SCAN 模式

//! HAL 对寄存器操作做了大量的抽象，极大的简化了模块的配置流程

#![no_std]
#![no_main]

use panic_rtt_target as _;

use rtt_target::rtt_init_print;

use stm32f4xx_hal::{
    adc::{self, config::Scan},
    pac, // 其实这个 pac 就是 stm32f4 crate 的再导出
    prelude::*,
};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 使用 stm32f4xx_hal crate 实现类似的效果
    // 准确来说，当我们使用了 stm32f4xx_hal 这个 crate 或者更高抽象层级的 crate 之后，
    // 正常情况下，我们就不再需要与寄存器直接打交道了。
    // 我们要做的就只是在内存中做好“配置文件”，然后交由 stm32f4xx_hal 来调用 stm32f411 这个 crate 来修改寄存器的状态了

    let device_peripherals = pac::Peripherals::take().unwrap();

    let rcc = device_peripherals.RCC.constrain();

    let cfgr = rcc.cfgr.use_hse(8.MHz());

    // 将结构体 cfgr 的配置写入到底层的 RCC Configuration Register 上
    // 使用 freeze 作为函数名，表示该结构体不可以再被修改
    cfgr.freeze();

    // 然后我们再配置 ADC
    let adc1_config = adc::config::AdcConfig::default().scan(Scan::Enabled);

    // 最后我们也是将配置写入底层的 ADC Configuration Register 上
    adc::Adc::adc1(device_peripherals.ADC1, true, adc1_config);

    #[allow(clippy::empty_loop)]
    loop {}
}
