//! PLL 锁相环 Phase-Locked Loop
//! 一种倍增输入时钟的模块
//!
//! STM32F411RE 的内部时钟频率为 16 MHz，而我手上的核心板的板载晶振为 8 MHz，要让系统时钟超过这两个频率，则必然要使用到 PLL

//! 在这个案例中，我们尝试让 STM32F411RE 运行在 HCLK 能支持的最高频率 100 MHz 下
//!
//! 注意，为了演示原理，这里我们手动配置所有的寄存器以达到效果

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    if let Some(dp) = pac::Peripherals::take() {
        // 由于，我们希望系统时钟运行在 100 MHz
        // 因此我们需要借助锁相环调整来自 HSE 的频率；
        // 与系统时钟相关的 PLL 内部的频率如下
        // 来源频率 / PLLM 寄存器的值 = VCO_INPUT 的频率（其中 2 <= PLLM <= 63）
        // VCO_INPUT 的频率 * PLLN 寄存器的值 = VCO_OUTPUT 的频率（其中 50 <= PLLN <= 432 且 VCO_OUTPUT 的频率必须介于 100 MHz 至 432 MHz）
        // VCO_OUTPUT 的频率 * PLLP 寄存器的值 = PLL 主输出频率（其中 PLLP 的值为 2 4 6 8 中的一个）
        // 使用逆推法推断
        // 最终目标是 100 MHz，依照 PLLP 的可取值，VCO_OUTPUT 的取值可以是 200 400 600 800 中的一个
        // 然后由于 VCO_OUTPUT 必须介于 100 到 432 中的一个，因此只剩下 200 以及 400 两个值，这里我们取 200，于是确定了 PLLP 的值为 2
        // VCO_OUTPUT 为 200，而 PLLN 的取值介于 50 ~ 432（这里取 400 就好了），于是 VCO_INPUT 的值的范围为 4 ~ 0.5
        // 然后，由于 输入源的频率为 8MHz，且 VCO_INPUT 的范围为 4 ~ 0.5，因此 PLLM 的取值范围为 2 至 16
        // 这里我们这样取值
        // 输入频率为 8 MHz，PLLM 寄存器为 4，VCO_INPUT 的频率为 2 MHz，PLLN 寄存器为 100，VCO_OUTPUT 的频率为 200 MHz，PLLP 寄存器的值为 2，最终输出频率为 100 MHz
        dp.RCC.pllcfgr.modify(|_, w| {
            w.pllsrc().hse();
            unsafe {
                w.pllm().bits(4);
                w.plln().bits(100);
            }
            w.pllp().div2();
            w
        });

        // 依照 reference manual 的说明，
        // 在供电电压较低或系统时钟频率较高的情况下，CPU 访问 FLASH 均需要一定时间的等待
        //
        // 在供电电压在 2.7V ~ 3.6V 之间，且系统时钟运行在 90 ~ 100 MHz 的情况下
        // CPU 每次读取 FLASH 均需要 4 个 CPU 周期，因此这里需要将额外的等待周期设置为 3 WS
        dp.FLASH.acr.modify(|_, w| {
            w.latency().ws3();
            w
        });

        // 这里还可以额外设置一些指令缓存、数据缓存、预获取等等的配置
        // 非必须，但设置了有助于提高 CPU 的运行效率
        dp.FLASH.acr.modify(|_, w| {
            w.dcen().enabled();
            w.icen().enabled();
            w.prften().enabled();
            w
        });

        // APB1 总线的时钟不能超过 50 MHz，
        // 我们需要预先保证在 100 MHz 的情况下，APB1 的频率不大于 50 MHz
        // 此处将与 APB1 总线时钟的预分频设置为 2
        dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

        // 依照 reference menual，在大于 84 MHz 时，应该将 PWR_VOS 设置为 Scale 1 mode
        // （会额外增加能耗）
        // 在 datasheet 的 block diagram 中，PWR 处于图标的右侧中上部，
        // 名为 PWR interface，挂载在 APB1 总线上
        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
        // VOS: Voltage scaling Output Selection
        dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b11) });
        // 注意，VOS 只有在实际启动 PLL 后才会执行
        // 因此 PWR_CRS_VOSRDY 需要等到 PLL 启动后才能侦测

        // 启用外部 8 MHz 晶振
        dp.RCC.cr.modify(|_, w| w.hseon().on());
        // 等待外部晶振频率稳定
        while dp.RCC.cr.read().hserdy().is_not_ready() {}

        // 启动锁相环
        dp.RCC.cr.modify(|_, w| w.pllon().on());
        // 等待 VOC 调整完成
        while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
        // 等待锁相环稳定
        while dp.RCC.cr.read().pllrdy().is_not_ready() {}

        // 将 PLL 的输出设置为系统时钟
        dp.RCC.cfgr.modify(|_, w| w.sw().pll());
        // 等待系统时钟切换完成
        while !dp.RCC.cfgr.read().sws().is_pll() {}

        rprintln!("System Clock @ 100 MHz with HSE and PLL\r");
    }
    loop {}
}
