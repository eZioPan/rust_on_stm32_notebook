//! TIM 定时器
//!
//! 虽然 TIM 被叫做“定时器”，但事实上，TIM 能实现的功能远多于“定时”这个范畴：
//! 输出比较（定时与延迟生成）、输出 PWM、单发模式、输入捕获（外部信号频率测量）、探测器接口（编码器、半探测器）等扽功能
//!
//! STM32 上的定时器有三大类，分别为 高级控制定时器（Advanced-control timer）、通用定时器（General-purpose timer）以及 基础定时器（Basic Timer）
//! 其功能依次递减
//!
//! 这里我们以 通用定时器 TIM2 作为例子，将其配置为基本的输出定时器模式
//!
//! 第一个问题，如果我们就想让 TIM2 做一个最简单的定时器，那么 TIM2 模块的输入时钟来自什么地方？
//! 可以通过这个流程图来解释
//!
//!           AHB PRE             APB2 PRE              TIM PRE
//! SYSCLK >------------> HCLK >------------> PCLK2 >------------> TIM2CLK（注意，这个时钟在 TIM 内部被称为 CK_INT）
//!
//! 其中 AHB PRE 和 APB2 PRE 这两个预分频器都比较好理解，都是纯粹手动指定的分频频率
//! 而 TIM PRE 分频器就稍显复杂，我们不能直接设置 TIM PRE 实际的倍频数值，转而我们设置的是 TIM PRE 的**最大**倍频值，
//! TIM PRE 的最大倍频值一共有 2 档，分别为默认的 2 倍频，以及 4 倍频
//! 而 TIM PRE 的**实际**倍频值，则要依据 APB2 PRE 分频器的数值来推断，
//! 当 APB2 PRE 的分频值（比如 /1 /2 /4）的分母没有超过 TIM PRE 设置的**最大**倍频数，则按照 APB2 PRE 分频值的分母设置倍频量，
//! 若超过，则按照 TIM PRE 设置的最大倍频数倍频 PCLK2
//!
//! 从 Reference Manual 的 General-purpose timer block diagram 的右侧中部，有通用定时器最核心的三个模块
//! 预分频器 PSC（PreSCaler）、计数器 CNT（CouNTer）以及自动重载寄存器 ARR（AutoReload Register），
//! 这三者合称 时基单元（Time-base unit）
//! 其中预分频器用于降低 TIM2 输入时钟的频率，以提供给计数器使用，
//! 计数器用于计数，当计数器上溢出或下溢出时，产生 更新事件（UEV: Update EVent，在定时器的 block diagram 中以单字母 U 表示），
//! 当产生更新事件的时候，还可以额外触发一个 更新中断（UI: Update Interrupt）
//! 而当计数器上溢或下溢后，自动重载寄存器会向计数器中载入设置好的数，并开始下一轮计数

#![no_std]
#![no_main]

use core::cell::Cell;
use cortex_m::interrupt::Mutex;

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};
use stm32f4xx_hal::pac::{self, interrupt};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprint!("\x1b[2K\r"); // clean current line

    if let (Some(dp), Some(cp)) = (pac::Peripherals::take(), pac::CorePeripherals::take()) {
        // 启用并等待 HSE 完成
        dp.RCC.cr.modify(|_, w| w.hseon().on());
        while dp.RCC.cr.read().hserdy().is_not_ready() {}

        // 由于我的核心板上的 HSE 是 8 MHz 的，比 HSI 的 16 MHz 要低
        // 在默认的配置下，先切换到 HSE 在配置分频器也没有什么问题
        dp.RCC.cfgr.modify(|_, w| w.sw().hse());
        while !dp.RCC.cfgr.read().sws().is_hse() {}

        // 将 AHB PRE 的值设置为 /8，
        // 这样 HCLK 的频率即为 1 MHz
        // 将 APB2 PRE 的值设置为 /1，
        // 这样 PCLK2 和 APB2 Timer Clock 的频率均为 1 MHz
        //
        // 注意，这里将 HCLK 降低为 1 MHz 并非处于节能的考量，
        // 只是为了让后面的计算简单一些而已
        //
        // HPRE: AHB PREscaler
        // PPRE2: APB PREscaler 2
        dp.RCC.cfgr.modify(|_, w| {
            w.hpre().div8();
            w.ppre2().div1();
            w
        });

        // 将 TIMPRE 的最大值设置为 2
        // DCKCFGR: Dedicated CLocks ConFiGuration Register
        dp.RCC.dckcfgr.modify(|_, w| w.timpre().mul2());

        // 好了，将要输入 TIM2 的时钟配置好了，现在就可以启动 TIM2 模块了
        dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

        // 关闭从模式，从而使用主模式，
        // 在主模式下，定时器的输入时钟为内部时钟（CK_INT）
        // SMCR: Slave Mode Control Register
        // SMS: Slave Mode Selection
        dp.TIM2.smcr.modify(|_, w| w.sms().disabled());

        // 之后我们要配置与 计数器 CNT 相关的配置
        dp.TIM2.cr1.modify(|_, w| {
            // 计数器将从指定的数字开始向下数（不断 -1）
            // DIR: DIRection
            w.dir().down();
            w
        });

        // 我们希望 TIM2 每 1 秒触发一个中断
        // 而 TIM2 的触发方式是，每当计数器（即将）向上溢出或向下溢出时，就可以触发事件/中断
        // 而计数器的计数用时钟则来自于上面选定的定时器输入时钟
        // 该时钟按照 TIM2_PSC 寄存器所设置的值进行降频之后，用于从 计数器 CNT 中 +1 或 -1
        // 而 计数器的溢出值/重载值，则由 TIM2_ARR 寄存器确定

        // TIM2 计数器的输入频率 f(CK_CNT) = f(CK_PSC)/(TIM2_PSC + 1)
        // 在本案例中，由于 TIM2 的计数器是仅向下计数的
        // 因此，最终中断的频率为 f(CK_CNT)/(TIM2_ARR +1)

        // 因为 1 MHz = 1000 Hz * 1000 Hz，因此这里我们直接让
        // PSC: PreSCaler
        dp.TIM2.psc.write(|w| w.psc().bits(999));
        // ARR: AutoReload Register
        dp.TIM2.arr.write(|w| w.arr().bits(999));

        // 之后是 ARR 以及触发模式的设置
        dp.TIM2.cr1.modify(|_, w| {
            // ARR 预载，值的是，在一个计数周期中，ARR 的值突然发生改变，计数器该如何处理
            // 开启 ARR 预载后，ARR 会等待计数器的本周期计数结束，在新一轮计数周期开启之时，才使用新设定的值
            // 若不使用 ARR 预载，则新设置的 ARR 值会在当前的周期就用于于 计数器 中的值进行比较，这样可能会引入一个小小问题
            //
            // 假设我们关闭了 ARR 预载，然后计数器为正数模式，先前 ARR 的值为 100，当前计数器的值为 50，
            // 接着，我们希望缩短计数时间，于是，我们将 ARR 的值设置为 30，接着就引入了一个问题
            // 由于当前计数器的值已经超过了 30，无法触发 ARR 重载计数器，
            // 于是我们只能等到计数器一路数到自身寄存器溢出，才能触发下一次时间/中断，并触发 ARR 重载
            // 而 TIM2 的 CNT 是 32 位的，也就是说，计数器要从 50 一直数到 2^32-1，才能触发下一次重载，会远超我们设计的 30 个计数周期
            //
            // ARR Preload Enable
            w.arpe().enabled();
            // 将 Update 中断或 DMA 请求的触发源头设置为
            // 仅由计数器上溢出或下溢出才产生
            w.urs().counter_only();
            w
        });

        // 接着，我们要让 TIM2 可以将 Update 中断发送给 Cortex 的 NVIC
        // DIER: DMA/Interrupt Enable Register
        // UIE: Update Interrupt Enable
        dp.TIM2.dier.modify(|_, w| w.uie().enabled());

        // 然后我们要设置 NVIC，在接收到来自 TIM2 的中断后产生动作
        unsafe {
            cp.NVIC.iser[0].modify(|d| d | 1 << 28);
        }

        // 最后我们需要启用定数器
        // CEN: Counter ENabled
        dp.TIM2.cr1.modify(|_, w| w.cen().enabled());
    }

    loop {}
}

const BEGIN_NUM: u32 = 1;
static G_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(BEGIN_NUM));

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| unsafe {
        let dp = pac::Peripherals::steal();
        // 一定要手动清理 TIM2_SR 寄存器的 UIF 位
        // 否则 TIM2 的中断会持续不断的产生
        // UIF: Update Interrupt Flag
        dp.TIM2.sr.modify(|_, w| w.uif().clear());
        let count_cell = G_COUNT.borrow(cs);

        let cur_val = count_cell.get();

        // 打印一下中断触发的次数
        if cur_val == BEGIN_NUM {
            rprint!("TIM2 Int Triggered: {}", cur_val);
        } else {
            // 为了让刷新的内容尽量少（只刷新数字）
            // 由于我们在没有 std / alloc 的环境下，
            // 不能很方便的用 .to_string().len() 这类函数取得整数的长度
            // 这里用了一个取巧的方式：使用 log 函数，且 log 的底数是打印数字的基数
            // 比如 log10(51) 的整数部分刚好是 2，也就是 51 这个数在十进制表示下的字符个数
            rprint!("\x1b[{}D{}", (cur_val - 1).ilog(10) + 1, cur_val);
        }

        count_cell.set(cur_val + 1);
    })
}
