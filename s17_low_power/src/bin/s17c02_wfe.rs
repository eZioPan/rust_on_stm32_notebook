//! WFE
//!
//! 有关 Wakeup Event，我们还是需要看一下 EXTI 的框图 External interrupt/event controller block diagram
//!
//! 需要注意的是，Event 的发送，走的是框图最底部的路线，Event 产生之后，
//! 既没有输入到 Pending request register 里，也不会发送到 NVIC 中，Event 算是单独的线路，
//! 然后是，虽然 EXTI 表示的是外部中断中断/事件控制器，但是实际上，它也能捕获几个特殊的片上外设的事件，
//! 具体的见 External interrupt/event line mapping 节对于 EXTI line 16/17/18/21/22 的论述
//!
//! 使用 Wakeup 的好处大概就是，除了 Cortex 核心明确在等待 WFE，否则它不干扰 Cortex 核心的运行，这一点 interrupt 是不行的
//! 另外 Event 没有 pending bit，所以如果 Cortex 核心没有捕获到某个 Event，那么这个 Event 就消失了，不会再次出现，
//! 而且由于 Event 是由 WFE 统一捕获的，除非外设上有寄存器保存了相关的 Flag，否则我们是没法知道到底是谁触发了 Event 的
//!
//! 在本案例中，我们让 GPIO PB0 的下降沿唤醒 Cortex 核心

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};
use stm32f4xx_hal::pac::Peripherals;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("Start Programe");

    let dp = Peripherals::take().unwrap();

    dp.DBGMCU.cr.modify(|_, w| w.dbg_sleep().set_bit());
    dp.RCC.ahb1enr.modify(|_, w| w.dma1en().enabled());

    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    dp.GPIOA.pupdr.modify(|_, w| w.pupdr15().pull_up());
    dp.GPIOA.moder.modify(|_, w| w.moder15().output());

    dp.RCC.ahb1enr.modify(|_, w| w.gpioben().enabled());
    dp.GPIOB.pupdr.modify(|_, w| w.pupdr0().pull_down());

    dp.RCC.apb2enr.modify(|_, w| w.syscfgen().enabled());
    dp.SYSCFG
        .exticr1
        .modify(|_, w| unsafe { w.exti0().bits(1) });

    dp.EXTI.ftsr.modify(|_, w| w.tr0().enabled());
    // 注意，我们这里修改的是 EMR（Event Mask Register）寄存器，而非前面 EXTI 章节中启用的 IMR（Interrupt Mask Register）寄存器
    dp.EXTI.emr.modify(|_, w| w.mr0().unmasked());

    let mut cnt = 1;

    loop {
        cortex_m::asm::wfe();
        // 从 Wakeup Event 中唤醒之后，打印一下唤醒次数
        rprint!("\x1b[2K\rhello!: {}", cnt);

        // 顺便闪一闪灯
        dp.GPIOA
            .odr
            .modify(|r, w| w.odr15().bit(r.odr15().bit() ^ true));

        cnt += 1;
    }
}
