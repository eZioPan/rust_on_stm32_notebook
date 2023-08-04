//! 使用 hal 库提供的 Qei 结构体，使用旋转编码器
//!
//! stm32f4xx_hal 提供的 Qei，是最大化使用 CNT 计数的正交编码计数器
//! 因此，我们就不能靠 Qei 内部的中断触发，来快速获取计数器的值了
//! 在这里，我们采取的方案是，使用 SysTick 以等时间间隔的方式，获得 Qei 计数器的值
//!
//! 另外，这里我们真的使用了一个旋转编码器来测试我们的功能
//! 它一共有 7 个引脚，其中
//! 1. 处于两侧的两个宽引脚，是机械固定引脚
//! 2. 其中有一边有三个引脚，其中中间的引脚是参考引脚，引脚名称为 C 引脚，也就是说，它需要接在 VCC 或 GND 上，
//!    中间引脚左右两侧的引脚为编码器的输出引脚，名称分别为 A 引脚和 B 引脚，这两个引脚需要接在 TIMx_CH1 和 TIM_CH2 对应的引脚上
//! 3. 最后的一个边上有两个引脚，因为我手上的这个旋转编码器的轴是可以按下的，相当于一个按钮，因此这个两个引脚就是这个按钮的引脚
//!
//! 其旋转模式为，旋转一周有 20 个可以被手感知的刻度，且每个刻度都对应 CNT 连续 +4 或连续 -4
//!
//! 在这里，我们会同时使用编码器功能，和按钮功能，按钮用于将编码器计数归回默认值
//!
//! 其连线为
//!
//! VCC -> 旋转编码器 C 引脚
//! 旋转编码器 A 引脚 -> PA0
//! 旋转编码器 B 引脚 -> PA1
//! VCC -> 按钮引脚 1
//! 按钮引脚 2 -> PA2
//!
//! 另外，我在使用面包板搭建电路的时候，遇到了一个小问题，就是我使用的旋转编码器，大小不是非常合适，很容易接触不良，
//! 导致我测试的时候，不得不一手按着编码器，另一个手旋转和按下编码器。

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use cortex_m_rt::exception;

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use embedded_hal::Direction;

use stm32f4xx_hal::{
    interrupt,
    pac::{CorePeripherals, Peripherals, TIM2},
    prelude::*,
    qei::Qei,
    timer::SysEvent,
};

// 使用 Qei 时，ARR 的值会被设置为底层 TIM 的最大可用 CNT 值
// 下面我们给出的是 TIM2，它是一个 32bit 的定时器，因此最大值为 0xF_FFF_FFF
// 因此其一半值为 (0xF_FFF_FFF + 1)/ 2 - 1 = 0x7_999_999
// 这样我们就尽量避免了 CNT 上下溢出的问题了
const START_CNT: u32 = 0x7_999_999;

static G_QEI: Mutex<RefCell<Option<Qei<TIM2>>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = Peripherals::take().unwrap();
    let cp = CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc.cfgr.use_hse(8.MHz()).hclk(48.MHz()).freeze();

    let gpioa = dp.GPIOA.split();

    // 依照 datasheet，GPIO PA2 处于 AF02 的时候，会作为 TIM5_CH3 存在
    // 我们开启 GPIO PA2 的内部下拉电阻
    gpioa.pa2.internal_pull_down(true).into_alternate::<2>();

    // 配置 TIM5，让其过滤来自旋转轴按钮的信号
    unsafe {
        // 由于 RCC 已经被 hal crate 移动走了，这里我们只能 unsafe{} + steal 了
        let dp = Peripherals::steal();
        dp.RCC.apb1enr.modify(|_, w| w.tim5en().enabled());
    }
    let tim5 = &dp.TIM5;
    let tim5_ccmr2 = tim5.ccmr2_input();
    // 启用 TIM3 的 CC3 的输入，并开启最大过滤
    tim5_ccmr2.modify(|_, w| {
        w.cc3s().ti3();
        w.ic3f().bits(15)
    });
    // 挂起 CC3 的中断标识
    tim5.dier.modify(|_, w| w.cc3ie().enabled());
    // 启用 CC3
    tim5.ccer.modify(|_, w| w.cc3e().set_bit());

    // 将 Qei 要捕获的两个引脚都启用其内部的下拉电阻
    let qei_pin0 = gpioa.pa0.internal_pull_down(true);
    let qei_pin1 = gpioa.pa1.internal_pull_down(true);
    // 然后将底层计数器和引脚都传递给 Qei
    let mut qei = Qei::new(dp.TIM2, (qei_pin0, qei_pin1));
    // 给出 QEI 的初始计数值（主要是为了避免底层 TIM 的 CNT 寄存器溢出）
    qei.set_count(START_CNT);
    cortex_m::interrupt::free(|cs| {
        G_QEI.borrow(cs).borrow_mut().replace(qei);
    });

    // 我们在配置好核心处理的内容之，再开启轴按下过滤器对应的 TIM5 的中断
    unsafe { NVIC::unmask(interrupt::TIM5) }

    // 启用 SysTick，并让它每秒触发 100 次中断
    let systick = cp.SYST;
    let mut counter = systick.counter_hz(&clocks);
    counter.listen(SysEvent::Update);
    counter.start(100.Hz()).unwrap();

    #[allow(clippy::empty_loop)]
    loop {}
}

// 每次 SysTick 触发中断，都读取一下 Qei 的计数值，以及当前的瞬间方向
#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|cs| {
        let qei_ref = G_QEI.borrow(cs).borrow();

        let qei = qei_ref.as_ref().unwrap();

        let dir = match qei.direction() {
            Direction::Downcounting => "D",
            Direction::Upcounting => "U",
        };

        // 下面打印的是 QEI 当前计数值与起始值之间的差值
        rprint!(
            "\x1b[2K\r{}, {}",
            qei.count() as i64 - START_CNT as i64,
            dir
        );
    })
}

// 每次 TIM5 触发中断，说明轴被按下，我们要重置一下计数器
#[interrupt]
fn TIM5() {
    cortex_m::interrupt::free(|cs| unsafe {
        let mut qei_mut = G_QEI.borrow(cs).borrow_mut();
        let qei = qei_mut.as_mut().unwrap();

        let dp = Peripherals::steal();
        let tim5 = &dp.TIM5;
        tim5.sr.modify(|_, w| w.cc3if().clear());

        qei.set_count(START_CNT);
    })
}
