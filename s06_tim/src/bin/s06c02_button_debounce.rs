//! 在 read_button_input 这个系列的源码中，有一个问题
//! 那就是直接读取 GPIO 输入引脚的电平来判定按钮的状态，会由于实体按钮材料的弹性、按下的动作等因素，
//! 不可避免的出现，人体的感知是按下的一次按钮，但单片机给出的反馈似乎是按下又弹起了非常非常多次
//! 而恰恰好，TIM 定时器，除了核心的定时功能，它为了兼容具有噪音的外部输入时钟，其实还内含了一个过滤模块
//! 而这个过滤模块，就刚刚好可以被我们利用，再不加入任何外设（比如引脚外设置一个滤波电容这样的）的情况下，处理 bouncing / belling 的问题
//!
//! TIM2 之外的流程还是老样子，启用 HSE，并切换为系统时钟
//!
//! 然后是 TIM2 的外部引脚触发模式，需要一个触发引脚，而这个引脚并非任选的，它其实是 GPIO 的 Alternate Function
//! 然后我们查找 afternate function mapping，可以发现，AF01 模式下的 PA0 / PA5 / PA15 都可以作为 TIM2_ETR 出现
//! 这里，我们可以把 GPIO PA5 配置为 AF01 模式，让它成为 TIM2_ETR 的引脚
//!
//! 接着是 TIM2 内部的配置
//!
//! 观察 Reference Manual 的 General-purpose timer block diagram，我们可以发现，
//! TIMx_ETR 输入 ETR（ExTeRnal trigger）信号 会经过 Polarity selection & edge detector & prescaler 形成 ETRP（ExTeRnal trigger Prescaled）信号，
//! 然后 ETRP 信号会经过 Input Filter 形成 ETRF（ExTeRnal trigger Filtered）信号，ETRF 信号则有两个选择，
//! 第一个是直接输入给 Trigger Controller，
//! 另一个是输入到一个复用器中，并最终形成 TRGI（TRiGger Input）信号，而且可额外触发 TGI（TriGger Interrupt）中断
//! 而 TGI 这个中断就是我们想要的中断
//!
//! 我们接着阅读 Reference Manual，再 External clock source mode 2 段，以及 External trigger input block 图表中，
//! 我们可以看到 ETR 一路转化为 ETRF 的流程
//! 与之相关的有 TIMx_SMCR 寄存器控制的 ETP、ETPS、ETF 三个字段，分别设置的是：
//! ETP: 外部触发极性 External Trigger Polarity
//! ETPS: 外部触发预分频器 External Trigger PreScaler
//! ETF: 外部触发过滤器 External Trigger Filter
//! 另外，过滤器 还有另一个输入 f_DTS，在搜索 TIM2 的寄存器配置后，可以发现它与 TIMx_CR1 的 CKD（ClocK Division）相关。
//! 它将 TIM 的输入时钟进行分频之后，形成侦测外部信号的频率（DTS 表示 DeTect）。
//!
//! 不过我们上面读的 External trigger input block 这张图不能完全解决我们的问题
//! 因为这张图表示的是 ETRF 直接通过 External clock mode 2 形成 CK_PSC 的过程，而我们希望触发 TGI
//! 然后我们再看看附近，External clock source mode 1 段以及 TI2 external clock connection example 图表中
//! 中部偏右的复用器，其输出为 TRGI，是我们的输出目标，而且其底部有 ETRF 的输入源，那么这个复用器就是我们需要的关注的复用器了
//! 而且我们还能看到，控制它的寄存器为 TIMx_SMCR 的 TS 字段
//!
//! 还有一点需要注意，那就是，我们的确需要将 Slave Mode 设置为 TRGI 触发，才真的能产生 TGI 这个中断
//!
//! 到此为止，我们就大致拼凑出了要配置的寄存器
//!
//! PS: 这里我们会稍稍仔细地设置各种时钟频率，虽然我们不会使用 C/C++ 编写程序，
//! 但是我们依旧可以使用 ST 官方的 STM32CubeMX 软件来模拟时钟配置的流程，看看会不会出现什么错误

#![no_std]
#![no_main]

use core::cell::Cell;

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};
use stm32f4xx_hal::pac::{self, interrupt, NVIC};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let Some(dp) = pac::Peripherals::take() {
        // 老生常谈，HSE 作为 SYSCLK
        dp.RCC.cr.modify(|_, w| w.hseon().on());
        while dp.RCC.cr.read().hserdy().is_not_ready() {}
        dp.RCC.cfgr.modify(|_, w| w.sw().hse());
        while !dp.RCC.cfgr.read().sws().is_hse() {}

        // 从 Alternate Function table 上我们可以看到
        // 再 AF01 模式下的 GPIO PA0 PA5 PA15 都可以作为 TIM2_ETR 引脚出现
        // 这里我们选择 GPIO PA5 来执行这个功能
        dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
        dp.GPIOA.pupdr.modify(|_, w| w.pupdr5().pull_down());
        dp.GPIOA.afrl.modify(|_, w| w.afrl5().af1());
        dp.GPIOA.moder.modify(|_, w| w.moder5().alternate());

        // 然后，由于 APB1 的时钟频率和预分频器的取值会确定 TIM2 的输入时钟频率
        // 而我们又要对一个手按下的按钮 debouncing，因此这里可以故意将 APB1 的时钟频率设置的小一点
        // 让 debouncing 的时间值大一点
        // 将 APB1 的预分频器设置为 /8，此时 APB1 Timer Clock 会运行在 12 MHz / 8 * 2 = 3 MHz 的频率
        dp.RCC.cfgr.modify(|_, w| w.ppre1().div8());
        dp.RCC.dckcfgr.modify(|_, w| w.timpre().mul2());

        // 开启 TIM2 的时钟
        dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

        // 设置检测时钟的频率
        // 由于我们要检测的是手动按下一个按钮
        // 因此频率不需要非常高，这里挑一个最低的频率来使用
        // 将检测时钟的频率设为 TIM 输入时钟频率的 1/4
        // 也就是检测频率为 3 MHz / 4 = 750 KHz
        dp.TIM2.cr1.modify(|_, w| w.ckd().div4());

        dp.TIM2.smcr.modify(|_, w| {
            // 将 TRGI 的来源设置为 ETRF
            // TS: Trigger Selection
            w.ts().etrf();

            // 设置外部触发的极性
            // 此处使用反向设置，让外部触发在下降沿发生
            // 在我锁设计的按钮配置下，它表示仅当按钮松开时，才触发
            // ETP: External Trigger Polarity
            w.etp().inverted();

            // 接着是过滤器的设置
            // 我们当前设置为，在检测频率 1/32 的频率下，连续检测到 8 个低电平，才算触发成功
            // 也就是说最短的触发时长为：1 / (750 KHz / 32) * 8 = 0.3413 ms
            // 0.3413 ms 对于人手按按钮来说，还是很短的，不会带来太大的延迟感
            w.etf().fdts_div32_n8();

            // 虽然我们并不需要计数器计数，
            // 但是我们要在 slave mode 中启用这个模式，才能正确触发中断
            // 也就是要设置 CK_PSC 来自 TRGI 才可以触发中断
            w.sms().ext_clock_mode();
            w
        });

        // 懒得用寄存器法配置 NVIC 了，
        // 这里偷个懒，直接用 NVIC::unmask 这个函数吧
        unsafe { NVIC::unmask(interrupt::TIM2) };

        // 一切设置就绪，开启 TGI 中断
        dp.TIM2.dier.modify(|_, w| w.tie().enabled());

        rprint!("\x1b[2K\rDetect External Input: 0\r"); // 清理当前的行，并给出一个默认信息
    }

    #[allow(clippy::empty_loop)]
    loop {}
}

const START_COUNT: u32 = 1;
static G_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(START_COUNT));

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| unsafe {
        let dp = pac::Peripherals::steal();

        // 记得清理 TIMx_SR 的 TIF 位
        dp.TIM2.sr.modify(|_, w| w.tif().clear());

        let cur_cnt = G_COUNT.borrow(cs).get();

        rprint!("Detect External Input: {}\r", cur_cnt);

        G_COUNT.borrow(cs).set(cur_cnt + 1);
    })
}
