//! SysTick 系统定时器
//!
//! 系统定时器 SysTick 是 Cortex-M4 核心中自带的 24-bit 倒计时定时器，它会从 STK_LOAD 寄存器指定的数值一直倒数到 0，
//! 并在下一个时钟沿载入 STK_LOAD 寄存器的值，再次开始倒数
//! 当 Cortex 核心由于 Debug 指令停机时，SysTick 会随之暂停运行
//!
//! 注：由于 SysTick 是每个使用了 Cortex-M4 核心的 STM32 芯片都有的功能，因此它和其它 Cortex-M4 自带的 Core Peripherals 的说明
//! 都放在了 STM32 Cortex®-M4 MCUs and MPUs programming manual 这个文件中，并不在各个芯片的 Reference Manual 中

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot take device peripherals");

    // 使用外部晶振，获得 12 MHz 时钟
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}
    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}

    // 虽然 STK 是 Cortex 核心的部件，但 stm32f4 这个 crate 也将其抽象到了 pac::Peripherals 中
    // 注意，pac::CorePeripherals 依旧可以访问这个寄存器，我们这里会选择 pac::Peripherals，
    // 是因为 pac::Peripherals 对 SysTick 的寄存器的字段做了解析，更容易操作
    let systick = &dp.STK;

    // 依照 Cortex-M4 programming manual 的 4.5.5 SysTick design hints and tips 的说明
    // SysTick 是有正确的初始化流程的
    // 顺序为
    // 1. 编写 STK_LOAD 寄存器的值
    // 2. 清理 STK_VAL 寄存器的值
    // 3. 编写 STK_CTRL 寄存器的值

    // 将重载寄存器设置为 1_499_999，配合上下面的 AHB/8，就能获得 1s 一个下溢出触发的效果
    systick
        .load
        .modify(|_, w| unsafe { w.reload().bits(1_499_999) });

    // 这里我们清理了一下 STK_VAL 的值
    // 虽然这个操作看起来非常没有意义，但实际上，如果我们不清理（准确来说是不写一下）这个寄存器
    // SysTick 是不会运行的
    systick.val.reset();

    systick.ctrl.modify(|_, w| {
        // 时钟源选择 AHB/8，结合 HSE，获得 1 MHz 的频率
        w.clksource().bit(false);
        // 最后我们开启计数器
        w.enable().set_bit();
        w
    });

    let mut cnt = 0;

    // 下面展示的，是一个简易的秒计数器
    loop {
        // 这里我们监测 STK_CTRL 的 CountFlag 位，当该位置 1 时，表示 SysTick 至少已经发生了一次下溢出
        // 若我们监测到该值不为 0，就一直空循环
        // 这行代码，类似于 delay 函数了，一直阻塞核心，直到计数器溢出
        while !systick.ctrl.read().countflag().bit() {}

        // 当我们检测到 SysTick 下溢出之后，首先需要清理 CountFlag 位
        systick.ctrl.modify(|_, w| w.countflag().clear_bit());

        cnt += 1;

        // 然后我们可以打印一下计数器的值
        rprint!("\x1b[2K\r{}", cnt);
    }
}
