//! 依旧是启用 HSE，并使用 PLL 将系统时钟设置为 100 MHz
//!
//! 不过这里的特点是，我们让一些工作交叉执行，让总等待时间有所缩短
//! 目前使用的路径是
//! 启动 HSE -> 配置 PLL -> 配置 PWR -> 等待 HSE 稳定 -> 启用 PLL -> 配置 FLASH -> 配置 APB1 预分频 -> 等待 PWR_VOS 稳定 -> 等待 PLL 稳定 -> 切换系统时钟为 PLL -> 等待系统时钟切换完成

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
        // 预先启动 HSE，由于晶振稳定需要一段时间，这段时间我们完全可以去做其它的事情
        // 比如配置 PLL，配置电源电压模式
        {
            // 启用外部 12 MHz 晶振
            dp.RCC.cr.modify(|_, w| w.hseon().on());

            // 配置 PLL 寄存器，配置电源电压
            {
                // 配置 PLL
                dp.RCC.pllcfgr.modify(|_, w| {
                    w.pllsrc().hse();
                    unsafe {
                        w.pllm().bits(6);
                        w.plln().bits(100);
                    }
                    w.pllp().div2();
                    w
                });

                // 启用 PWR 控制接口
                dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
                // 稍稍提高供电电压
                dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b01) });
            }

            // 等待外部晶振频率稳定
            while dp.RCC.cr.read().hserdy().is_not_ready() {
                HSE_WaitCount += 1
            }
        }

        // 晶振稳定后，可以开启 PLL，在等待 PLL 稳定的过程中
        // 我们还可以配置一下 CPU 访问 FLASH 的速度，并修改 APB2 的预分频数
        {
            // 启动锁相环
            dp.RCC.cr.modify(|_, w| w.pllon().on());

            {
                // 修改 CPU 访问 FLASH 的速率，额外设置一些参数
                dp.FLASH.acr.modify(|_, w| {
                    w.latency().ws3();
                    w.dcen().enabled();
                    w.icen().enabled();
                    w.prften().enabled();
                    w
                });

                // 将 APB1 总线时钟的预分频设置为 2
                dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());
            }

            // 等待 VOC 调整完成
            while dp.PWR.csr.read().vosrdy().bit_is_clear() {
                VOS_WaitCount += 1
            }
            // 等待锁相环稳定
            while dp.RCC.cr.read().pllrdy().is_not_ready() {
                PLL_WaitCount += 1;
            }
        }

        // 完成系统时钟的切换
        {
            // 将 PLL 的输出设置为系统时钟
            dp.RCC.cfgr.modify(|_, w| w.sw().pll());
            // 等待系统时钟切换完成
            while !dp.RCC.cfgr.read().sws().is_pll() {
                SYSCLK_WaitCount += 1;
            }
        }
        rprintln!("System Clock @ 100 MHz with HSE and PLL\r");

        Total = HSE_WaitCount + VOS_WaitCount + PLL_WaitCount + SYSCLK_WaitCount; // 经过测试，这个方案 CPU 空转次数大概在 155 次左右
        rprintln!("Wait Count:\r\nHSE_WaitCount: {}\r\nVOS_WaitCount: {}\r\nPLL_WaitCount: {}\r\nSYSCLK_WaitCount:{}\r\nTotal: {}\r\n", HSE_WaitCount, VOS_WaitCount, PLL_WaitCount, SYSCLK_WaitCount, Total);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
