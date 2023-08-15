//! PLL 锁相环 Phase-Locked Loop
//! 一种倍增输入时钟的模块
//!
//! STM32F411RE 的内部时钟频率为 16 MHz，而我手上的核心板的板载晶振为 8 MHz，要让系统时钟超过这两个频率，则必然要使用到 PLL

//! 在这个案例中，我们尝试让 STM32F411RE 运行在 HCLK 能支持的最高频率 100 MHz 下
//!
//! 注意，为了演示原理，这里我们手动配置所有的寄存器以达到效果
//!
//! 另外，为了方便理解，这里的每个步骤都是完成一整个操作之后，再执行下一个操作
//! 我们这里的操作遵循以下的步骤：
//! 启用 HSE -> 等待 HSE 稳定 -> 配置 PLL -> 配置 PWR -> 启动 PLL -> 等待 PWR_VOS 稳定 -> 等待 PLL 稳定 -> 配置 FLASH -> 配置 APB1 分频 -> 切换系统时钟至 PLL -> 等待切换结束
//! 这种处理方式便于理解，但可能执行效率并不算高，下一个文件 100_pll_1rearranged 会提供一个执行效率稍高一些的方法

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 统计以下 CPU 为了等待设备稳定，所花费的空转次数
    #[allow(non_snake_case)]
    let mut HSE_WaitCount: u32 = 0;
    #[allow(non_snake_case)]
    let mut VOS_WaitCount: u32 = 0;
    #[allow(non_snake_case)]
    let mut PLL_WaitCount: u32 = 0;
    #[allow(non_snake_case)]
    let mut SYSCLK_WaitCount: u32 = 0;
    #[allow(non_snake_case, unused_assignments)]
    let mut Total = 0;

    if let Some(dp) = pac::Peripherals::take() {
        // 启用外部 8 MHz 晶振
        dp.RCC.cr.modify(|_, w| w.hseon().on());
        // 等待外部晶振频率稳定
        while dp.RCC.cr.read().hserdy().is_not_ready() {
            HSE_WaitCount += 1
        }

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

        // 依照 reference menual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
        // 在大于 84 MHz 时，应该将 PWR 寄存器的 VOS 位设置为 0x11，也就是 Scale 1 mode
        // （会额外增加能耗）
        // 在 datasheet 的 block diagram 中，PWR 处于图标的右侧中上部，
        // 名为 PWR interface，挂载在 APB1 总线上
        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
        // VOS: Voltage scaling Output Selection
        // 这里有两点要注意，
        // 第一，VOS 位的值，在 System Reset 之后，会重置为 Scale 2
        // 第二，VOS 位的值仅在 PLL 启用后才会被硬件读取，
        // 在 PLL 未启动时，硬件实际的 VOS 是运行在 Scale 3 模式下的
        //
        // PWR_CRS_VOSRDY 需要等到 PLL 启动后才能进行检测
        dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b11) });

        // 启动锁相环
        dp.RCC.cr.modify(|_, w| w.pllon().on());
        // 等待 VOC 调整完成
        while dp.PWR.csr.read().vosrdy().bit_is_clear() {
            VOS_WaitCount += 1;
        }
        // 等待锁相环稳定
        while dp.RCC.cr.read().pllrdy().is_not_ready() {
            PLL_WaitCount += 1;
        }

        // 依照 reference manual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
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

        // 将 PLL 的输出设置为系统时钟
        dp.RCC.cfgr.modify(|_, w| w.sw().pll());
        // 等待系统时钟切换完成
        while !dp.RCC.cfgr.read().sws().is_pll() {
            SYSCLK_WaitCount += 1;
        }

        rprintln!("System Clock @ 100 MHz with HSE and PLL\r");

        Total = HSE_WaitCount + VOS_WaitCount + PLL_WaitCount + SYSCLK_WaitCount; // 经过测试，这个方案 CPU 空转次数大概在 170 次左右
        rprintln!("Wait Count:\r\nHSE_WaitCount: {}\r\nVOS_WaitCount: {}\r\nPLL_WaitCount: {}\r\nSYSCLK_WaitCount:{}\r\nTotal: {}\r\n", HSE_WaitCount, VOS_WaitCount, PLL_WaitCount, SYSCLK_WaitCount, Total);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
