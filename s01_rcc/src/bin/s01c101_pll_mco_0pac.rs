//! 输出基于 HSE 的、来自 PLL 的 100 MHz 的时钟到外部引脚上

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    if let Some(dp) = pac::Peripherals::take() {
        // 依照 RCC_CFGR 的 MCO1 位的说明
        // 由于我们要输出的是 PLL 产生的时钟，
        // 我们应该在启动 HSE 和 PLL 之前就设置好 MCO
        dp.RCC.cfgr.modify(|_, w| {
            w.mco1().pll();
            w
        });

        // 接着是启动 PLL 前，对 PLL 的配置
        dp.RCC.pllcfgr.modify(|_, w| {
            w.pllsrc().hse();
            unsafe {
                w.pllm().bits(4);
                w.plln().bits(100);
            }
            w.pllp().div2();
            w
        });

        // 由于使用了 100 MHz 的极高频率，因此要修改
        // CPU 读内置 FLASH 的等待周期数
        // 并额外启动了一些额外的参数，稍稍优化 CPU 的执行流水线
        dp.FLASH.acr.modify(|_, w| {
            w.latency().ws3();
            w.dcen().enabled();
            w.icen().enabled();
            w.prften().enabled();
            w
        });

        // 当然，由于最终我们会将系统时钟修改为 100 MHz
        // 因此我们要为只能运行在 50 MHz 的 APB1 设置预分频值为 2
        dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

        // 为了达到 100 MHz 的最高速率，需要稍稍提高电源模块的输出电压
        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
        dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b11) });

        // 启用并等待 HSE 稳定，启用 PLL，并等待 VOC 和 PLL 稳定
        dp.RCC.cr.modify(|_, w| w.hseon().on());
        while dp.RCC.cr.read().hserdy().is_not_ready() {}
        dp.RCC.cr.modify(|_, w| w.pllon().on());
        while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
        while dp.RCC.cr.read().pllrdy().is_not_ready() {}

        // 将系统时钟切换为 PLL，并等待切换结束
        dp.RCC.cfgr.modify(|_, w| w.sw().pll());
        while !dp.RCC.cfgr.read().sws().is_pll() {}

        rprintln!("System Clock @ 100 MHz with HSE and PLL\r");

        // 启用 GPIO PA8，并将 PA8 的 AF 修改为 SYS_AF
        dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
        dp.GPIOA.afrh.modify(|_, w| w.afrh8().af0());

        // 接着，由于我们要输出的频率非常高，
        // 因此需要修改 OSPEEDR 输出速率寄存器 的值
        // 依照 reference manual 上该寄存器的说明，
        // 我们需需要从 datasheet 中，找到 OSPEEDR 与 VDD 的关系，
        // 以确认 OSPEEDR 的值
        //
        // 从 datasheet 的 I/O AC characteristics 表中我们可以知道
        // 要实现 100 MHz 的输出，我们需要将 OSPEEDR 设置为 0b10 或 0b11
        // 为了更好的切换时间，我们这里选择 0b11 模式（也就是 .very_high_speed()）
        dp.GPIOA
            .ospeedr
            .modify(|_, w| w.ospeedr8().very_high_speed());

        // 同时 datasheet 还指出，在大于等于 50 MHz，且 VDD 大于 2.4V 的情况下
        // 应该启用 compensation cell
        // compensation cell 是 SYSCFG 管理的，而后者又是挂在 APB2 下的
        // 于是我们先启动 SYSCFG
        dp.RCC.apb2enr.modify(|_, w| w.syscfgen().enabled());

        // 接着，通过 SYSCFG_CMPCR 启动 compensation cell
        //
        // 注：由于 .svd 文件中的 bug，整个 CMPCR register 被标记为 read-only
        // 但其实只有 READY field 是 read-only 的，CMP_PD field 则应该是 read-write 的
        // 已经提交了 issue https://github.com/stm32-rs/stm32-rs/issues/826
        // 这里我们只能手动配置一下寄存器了
        unsafe {
            let cmpcr = dp.SYSCFG.cmpcr.as_ptr();
            let cur_value = cmpcr.read_volatile();
            cmpcr.write_volatile(cur_value | (1 << 0));
        }
        // 等待 compensation cell 稳定
        while dp.SYSCFG.cmpcr.read().ready().bit_is_clear() {}

        // 最后，我们将 GPIO PA8 切换到 alternate 模式，开启输出
        dp.GPIOA.moder.modify(|_, w| w.moder8().alternate());

        rprintln!("PLL clock is output on GPIO PA8\r");
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
