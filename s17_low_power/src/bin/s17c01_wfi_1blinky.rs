//! STM32F4 的低功耗模式
//!
//! 注：Low-power mode 出现在 Reference Manual 的 Power controller (PWR) 章节
//!
//! 低功耗模式有三档，其功耗由高到低，唤醒难度由低到高：
//!
//! Sleep mode:
//! 最浅的低功耗模式，
//! 仅 Cortex 核心 停止运行，其他外设均正常运行，外设可通过唤醒事件（注1）（若睡眠由 WFE 触发）或任何中断（若睡眠由 WFI 或 SleepOnExit 触发）都可以唤醒 Cortex 核心
//!
//! Stop mode:
//! 较深的低功耗模式，此模式下，硬件会尝试在保持 SRAM 和外设寄存器值的情况下，降低功耗。
//! 所有处于 1.2V 供电域（注2）的时钟均会被停止，PLL、HSI 和 HSE 被关闭
//! 硬件系统可以通过 EXTI 中断唤醒
//!
//! Standby mode:
//! 最低功耗模式，此模式下，硬件内部的电压调节器（Voltage Regulator）会完全停止，导致 1.2V 供电域完全断电，
//! 且 HSI 和 HSE 会停止工作
//! 仅可以被几个特定的方法离开该模式：WKUP 引脚上升沿、RTC 闹钟、RTC Wakeup 事件、RTC tamper 事件、RTC timestamp 事件、
//! NRST 引脚触发的外部重置、IWDG 触发的重置
//!
//! 注1：关于唤醒事件 Wakeup Event 参见 wfe.rs 中的说明
//! 注2: 1.2V 供电域，从 Reference Manual 的 Power supply overview 图可知，为 Voltage regulator 供能的 IO Logic、Kernel logic、和 Flash（注意 Flash 也被 VDD 供电）
//!
//! 关于低功耗的进入、唤醒已经影响的设备，可以看一看 Reference Manual 的 Low-power mode summary 表格
//!
//! 就进入的部分，总的来说，是这个样子的
//!
//! Sleep mode 可以通过 WFI、特定的中断处理返回方式、WFE 三种方式进入，
//! 其中 WFI 和 WFE 都是特定的 Cortex 指令，WFI 是 Wait For Interrupt 的简称，WFE 是 Wait For Event 的简称，
//! 特定的中断处理返回方式指的是，若设置了 SLEEPONEXIT 位（SleepOnExit）（注3），那么当一个中断处理流程完成退出时，没有其它待处理的中断，就进入睡眠的状态
//!
//! Stop mode 也可以通过上述三种方式进入，但需要提前设置 SLEEPDEEP 位（Sleep Deep）（注3）
//!
//! Standby mode 也可以通过上述三种方式进入，除了设置 SLEEPDEEP 位，还需要设置 PWR_CR 的 PDDS 位
//!
//! 注3: SLEEPONEXIT、SLEEPDEEP 的说明见 STM32 Cortex-M4 MCUs and MPUs programming manual（下简称 Cortex PM） 文档，System control register (SCR) 的说明

#![no_std]
#![no_main]

use core::cell::RefCell;
use cortex_m::{interrupt::Mutex, peripheral::NVIC};

use panic_halt as _;

use stm32f4xx_hal::{interrupt, pac::Peripherals};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    let dp = Peripherals::take().unwrap();

    // 设置在 Sleep Mode 时，不要真的将 Cortex 核心的时钟关闭
    // 在 debug 时，还需要保持 Cortex 的时钟运行
    // 如果我们不设置 GBD_SLEEP 位，则 OpenOCD 会丢失与 MCU 的通信
    // OpenOCD 的表现形式为反复出现下面的字样
    // Conecting DP: stalled AP operation, issuing ABORT
    // 此时我们需要做的就是，使用 OpenOCD 的 reset init 命令，在设备重启之后立刻停止其运行，
    // 具体操作见仓库根目录下 README.adoc 的“常见的操作注意事项”节

    // DGBMCU 仅被 Power On Reset 重置
    // 不会被 System Reset 重置，因此若我们想准确测试 DBG_SLEEP 位的影响，则必须手动重置这个寄存器的值
    dp.DBGMCU.cr.reset();
    // 而且将 DBG_SLEEP / DBG_STOP / DBG_STANDBY 三个位中的任何一个设置巍为 1
    // 均会导致三个位全部设置为 1
    dp.DBGMCU.cr.modify(|_, w| w.dbg_sleep().set_bit());
    // 注意，在设置了这个位之后，OpenOCD 也有可能检测不到设备
    // 比如，MCU 连接在 DAPLink 上，DAPLink 也连接在 Host 主机上，但是没有启动 OpenOCD
    // 此时，MCU 已经运行，此时我们再开启 OpenOCD，会发现 OpenOCD 产生了如下错误
    // Error: Target not examined yet
    // 此时我们可以尝试按一下开发板的 RESET 按钮，再在 OpenOCD 中输入如下命令重新检测一下目标板
    // stm32f4x.cpu arp_examine

    let rcc = &dp.RCC;

    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}

    rcc.cfgr.modify(|_, w| w.hpre().div8());

    rcc.ahb1enr.modify(|_, w| w.gpiocen().enabled());

    let gpioc = &dp.GPIOC;

    gpioc.odr.modify(|_, w| w.odr13().high());

    gpioc.moder.modify(|_, w| w.moder13().output());

    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());

    let tim2 = &dp.TIM2;

    tim2.psc.write(|w| w.psc().bits(1_000));
    tim2.arr.write(|w| w.arr().bits(1_000));
    tim2.dier.modify(|_, w| w.uie().enabled());
    tim2.cr1.modify(|_, w| w.cen().enabled());

    cortex_m::interrupt::free(|cs: &cortex_m::interrupt::CriticalSection| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    unsafe { NVIC::unmask(interrupt::TIM2) };

    loop {
        cortex_m::interrupt::free(|cs| {
            let dp_ref = G_DP.borrow(cs).borrow();
            let dp = dp_ref.as_ref().unwrap();

            let gpioc = &dp.GPIOC;
            if gpioc.odr.read().odr13().is_low() {
                gpioc.odr.modify(|_, w| w.odr13().high())
            } else {
                gpioc.odr.modify(|_, w| w.odr13().low())
            }
        });

        // WFI 这个指令会直接让 MCU 处于 Sleep mode
        //
        // 也就是说，loop {} 循环每次执行完这个指令之后就会停下来，
        // 然后等待任意一个中断跳出 Sleep，然后再次执行循环中的代码
        // 接着又会因为这个指令而进入 Sleep
        cortex_m::asm::wfi();
    }
}

// 在这里，由于切换灯亮灭的逻辑放在了主循环中
// 这里我们要做的其实只有清理 TIM2 UIF 标识这一个任务
// 或者，我们可以这样理解，实际上，这个中断处理函数的作用，是让主循环中 WFI 指令真的有用
// 因为触发中断后，我们必须用这个 handle 清理掉中断触发的源头，否则中断就会一直触发
#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.sr.modify(|_, w| w.uif().clear());
    });
}
