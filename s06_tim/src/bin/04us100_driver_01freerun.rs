//! 结合 TIM 的输入捕获和输出比较功能，控制 US-100 超声波测距模块测量距离

//! 关于 US-100 超声波测距模块的说明：
//!
//! US-100 通过发射超声波、接收反射波，并依照发射和接收的间隔来确定自身到物体的间距
//!
//! US-100 有两种工作模式，UART 模式 和 类 HC-SR04 模式，UART 模式相对简单，这里我们要使用的是“更原始”的 类 HC-SR04 模式
//! 在 类 HC-SR04 模式下，US-100 超声波模块需要在 Trig 引脚接收 10 us 以上的高电平以启动测量，并通过拉高 Echo 引脚电平的时长告知去波+来波总时长
//!
//! 因此，我们只需给 Trig 引脚输出一个高电平，并测量 Echo 引脚每次被拉高电平的时间，就可以知道声波来回的总时长，之后乘以声波的速度，并除以 2，就可以知道模块距离待测物体的距离了
//! 注：US-100 的工作模式的切换，是通过其背部的跳线帽确定的，若跳线帽连接了两个针脚，US-100 处于 UART 模式，否则处于 类 HC-SR04 模式

//! 拉高 Trig 引脚，我们可以将 Trig 引脚接在 3.3V 电源端实现
//! 而测量 Echo 拉高电平的时间，则可以通过 TIM 的 输入捕获（IC: Input Capture）功能实现

//! 下面是一些计算方面的说明：
//!
//! 标准状况下（0 摄氏度，1 个标准大气压），空气中的声速为 331.4 m/s，但是空气声速受温度的影响较大，
//! 不过 US-100 自带的芯片会测量当前的环境温度，并抵消温度变化带来的变化
//!
//! US-100 可测量的范围为 2 cm - 450 cm，再由于 US-100 的测量精度为 0.3cm ± 1%
//! 此处我们将距离单位设置为 mm，就完全可以覆盖 US-100 能达到的最大分度
//! 换算下来，高电平的可能范围为
//! (20mm ~ 4500mm * 2)/(0.3314mm/us)  = (120.7us ~ 27157.5us) 稍稍扩大范围为 120 us 至 27158 us
//! 注意：距离 * 2 是因为测量的时长为声波来回的总时长，因此距离要 * 2
//! 由于 27158 还远未超过 16 bit 的最大值 65535，因此我们可以任选 16 bit 或 32 bit 的 TIM 进行输入比较处理

//! 对于 US-100 工作电压的说明：
//!
//! US-100 可兼容 3V ~ 5V 的工作电压，电压低于 5 V 时，最大测量距离会变短

//! 上面提到，我们会使用 TIM 的 输入捕获 功能，输入捕获 的基础原理：
//!
//! 和 输出比较 功能类似，输入捕获 这个功能的核心依旧是 CCR 寄存器，只不过在输入捕获的功能中，对于我们来说，CCR 是只读寄存器，
//! 当输入捕获触发的时候，会在对应的 CCR 中记录当时的 计数器 CNT 的值，借此我们就可以推算出输入捕获（在当前 CNT 计数周期中）发生的时刻
//! 如果我们能在一个 CNT 计数周期中，记录下两个时刻，那么这两个时刻的差值，就能确定一段时间的时长了
//!
//! 再分析一下 US-100 的工作状态：通过拉高电平，并保持一段时间，来表示超声波返回和发射的时间差
//! 也就是说，我们要确定的时长，即为 US-100 的 Echo 引脚上一个上升沿到下降沿的时间
//! 而 输入捕获 恰好能通过上升沿触发，或通过下降沿触发，而且由于 TIM 的特殊设计，这两个捕获可以在同一个 TIM 中完成，这样就保证了同时性

//! 在这里案例中，我们使用 TIM3 的 CC1 和 CC2 记录 US-100 Echo 引脚的高电平的时间，
//! 其中 CC1 用来捕获 Echo 的上升沿，而 CC2 用来捕获 Echo 的下降沿
//!
//! 选择 TIM3 的这两个功能，有两个原因
//!
//! 1. 我们希望读取一个 CCR 寄存器的值，就可以直接计算时间间隔了，而不用读取两个再相减、求差值的方法
//!    这个要求我们可以通过在 CC1 捕获到上升沿的时候，通过某种方法将计数器的值归 0，这样 CCR2 记录的就已经是时间差的值了
//!    这样做还有一个额外的好处，就是不会出现因为 US-100 拉高 Echo 的时间过晚，或时间过长，导致计数器溢出，而需要额外处理这种情况
//!    而 TIM 有一个 从模式 Slave Mode，我们就可以配置为：当 CC1 模块的产生上升沿的时候，重置计数器的值，
//!    且这个功能只有 CC1 和 CC2 的输出才有，CC3 和 CC4 是不行的，因此，可用的引脚就限定为连接到 TIMx_CH1 或 TIM_CH2 的引脚了
//! 2. 还有一个原因就是，由于我们是临时用杜邦线连接的设备，我希望所使用的引脚尽量集中，且靠近电源引脚
//!
//! 基于上面两个理由，我选择了 GPIO PA6 作为输入引脚，因为在我的核心板上，它足够靠近 3.3 V 的电源引脚，且它可以切换到 TIM3_CH1 这个通道

//! 接线图
//!
//! STM32 <-> US-100
//!  3.3V <-> VCC
//!  3.3V <-> Trig
//!   PA6 <-> Echo
//!   GND <-> GND

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};
use cortex_m::interrupt::Mutex;

use rtt_target::rtt_init_print;

#[cfg(debug_assertions)]
use rtt_target::rprintln;

#[cfg(not(debug_assertions))]
use rtt_target::rprint;

use stm32f4xx_hal::pac::{interrupt, Peripherals, NVIC};

use panic_rtt_target as _;

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = Peripherals::take().expect("Cannot take device peripherals");

    dp.DBGMCU.apb1_fz.modify(|_, w| {
        w.dbg_tim3_stop().set_bit();
        w
    });

    cortex_m::interrupt::free(|cs| {
        // 为了准确计量 US-100 Echo 引脚被拉高的时间，
        // 这里启用了外部晶振作为时钟源
        setup_hse(&dp);

        setup_gpio(&dp);

        setup_tim3(&dp);

        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    loop {}
}

fn setup_hse(dp: &Peripherals) {
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}

    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}
}

fn setup_gpio(dp: &Peripherals) {
    // 切换 GPIO PA6 到 TIM3_CH1 上，作为 US-100 输出电平的输入捕获的端口
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    let gpioa = &dp.GPIOA;
    gpioa.afrl.modify(|_, w| w.afrl6().af2());
    gpioa.pupdr.modify(|_, w| w.pupdr6().pull_down());
    gpioa.moder.modify(|_, w| w.moder6().alternate());
}

fn setup_tim3(dp: &Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim3en().enabled());

    let measurer = &dp.TIM3;

    // 8 MHz 输入，预分频为 8 时，输出的频率为 1 MHz，也就是 1 us CNT 产生一个 tick
    measurer.psc.write(|w| w.psc().bits(8 - 1));

    // 【重要】如果要配置 ARR，一定要在 ARPE 关闭的情况下配置，否则第一个循环能等死人
    measurer.cr1.modify(|_, w| w.arpe().disabled());
    // 我们记录的值不应该超过 27158，这里我们扩展到 30000，如果还溢出了就算是检测失败了
    measurer.arr.write(|w| w.arr().bits(30000 - 1));

    measurer.cnt.write(|w| w.cnt().bits(0));

    measurer.cr1.modify(|_, w| {
        w.arpe().enabled();
        w.dir().up();
        w
    });

    // 如果计数器溢出了，就挂起一个中断，在处理该中断时，软件应该打印 Out of Rnage
    measurer.dier.modify(|_, w| w.uie().enabled());

    // 启动两个 Input Capture Channel，以确定高电平的时间
    // 如上面所说，我们这里要执行两个设置
    // 第一个是让 CC1 捕获上升沿，并在上升沿触发的时候，通过 TIM 的从模式重置计数器的值
    // 第二个时让 CC2 捕获下降沿，并在下降沿触发的时候，将计数器的值拷贝到 CCR2 中，并挂起一个中断
    // 在这个中断处理函数中，软件会计算出实际的距离值

    // CC1 和 CC2 均被 CCMR1 控制，且两者均处于输入捕获模式
    // 因此这里我们构建一次 CCMR1 的表示结构体即可
    let ccmr1_input = measurer.ccmr1_input();

    // 重置一下 CCMR1 可能以及存在的数据
    ccmr1_input.reset();

    // 配置 TIM3 的 CC1，让它检测 Echo 线的上升沿
    // 并让该 TIM 读取该上升沿，以重置计数器 CNT
    {
        ccmr1_input.modify(|_, w| {
            // 设置 CC1 为输入模式，且输入源为 TI1
            // 这里的 TI 实际上为 Timer Input 的缩写
            w.cc1s().ti1();
            // 设置输入滤波模式为使用 TIM 时钟作为滤波时钟
            // 且连续采样到 8 个有效的电平才算触发成功
            // 注意：只要 CC1 的输入滤波模式和 CC2 的输入滤波模式一致
            //       就不会额外添加采样到的时长的偏移
            //
            // IC1F: Input Capture 1 Filter
            w.ic1f().bits(0b11);
            w
        });

        // 让 CC1 捕获上升沿
        //
        // 从 Reference Manual 的 Capture/comapre channel(example: channel 1 input stage) 图我们可以知道
        // 在输入捕获模式下，CC1NP 和 CC1P 两个位会联合控制捕获的模式
        // 在 CC1P 位的说明中，我们可以知道，在输入捕获模式下，两个位的排列顺序为：CN1NP-CN1P，
        // 其中 0-0 表示捕获上升沿，0-1 表示捕获下降沿，1-1 表示捕获上升沿和下降沿
        //
        // 这里可能会遇见比如 TI1FP1、TI2FP2 等字样
        // 这里的 TI1 指的是从输入 1 输入的信号，FP1 指的是经过 CC1 滤波、并检测边沿后的信号
        // 连起来就是 输入 1 输入的信号，经过 CC1 处理之后产生的信号，其它的标记也是类似
        measurer.ccer.modify(|_, w| {
            w.cc1np().clear_bit();
            w.cc1p().clear_bit();
            // 这里我们并不需要 CC1 检测到上升沿后，将计数器的值写到 CCR1 中
            // 它只需要将触发 TI1FP1 这个信号即可
            // w.cc1e().set_bit();
            w
        });

        // 输入捕获的分频，不要分频，直出即可
        // 注意 ICxPSC 的值有特殊的含义
        // 其中 0 为不分频、1 为 2 分频、2 为 4 分频、3 为 8 分频
        //
        // IC1PSC: Input Capture 1 PreSCaler
        ccmr1_input.modify(|_, w| unsafe { w.ic1psc().bits(0) });

        // 当 CC1 触发的时候（检测到输入有上升沿），重置计数器的值
        // 这样，CC2 触发的时候（检测到输入有下降沿）时
        // 计数器中的值，就是 CC1 触发到 CC2 触发的间隔值
        //
        // 这里对应的其实是 Reference Manual 的 TI2 external clock connection example 这张图
        // 这张图看起来是在选择一个信号作为 CK_PSC，输入给 TIM 的时基单元使用的，
        // 但实际上，通过修改 SMS 的值，它还可以有其它的作用
        // 依照这张图，我们还需要配置的有三个寄存器 TS / ECE / SMS
        measurer.smcr.modify(|_, w| {
            // 设置从模式的触发源
            // TI1FP1 指的是从 TI1 输入，经过滤波之后，捕获到的上升沿信号
            //
            // TS: Trigger Selection
            w.ts().ti1fp1();

            // 是否启动外部时钟源 2
            // 如果启用了外部时钟源 2，则会覆盖 SMS 位的设置，而锁定使用 ETRF 作为输入源
            // 此处我们保证它处于关闭模式即可
            //
            // ECE: External Clock 2 Enable
            w.ece().disabled();

            // 从模式设为：来自输入源（统称为 TRGI）的上升沿会触发计数器重置
            //
            // SMS: Slave Mode Selection
            w.sms().reset_mode();
            w
        });
    }

    // 配置 TIM3 的 CC2，让它检测 Echo 线的下降沿
    // 当读取到下降沿的时候，触发中断，以便让软件访问 CCR2，并计算时长
    {
        // 类似 CC1，将 CC2 的输入设置为 TI1，并设置相同的采样过滤方式
        ccmr1_input.modify(|_, w| {
            w.cc2s().ti1();
            w.ic2f().bits(0b11);
            w
        });

        // // 让 CC2 捕获下降沿
        // 关于输入捕获模式的 CCxP 和 CCxNP 的含义，参见上面
        measurer.ccer.modify(|_, w| {
            w.cc2np().clear_bit();
            w.cc2p().set_bit();
            // 这里我们的确希望 CC2 触发捕获时，确实将 CNT 的值拷贝到 CCR2 中
            // 这样，在后面我们为 CC2 触发挂起一个中断后，就可以通过读取 CCR2 的值来
            // 计算时间和距离了
            w.cc2e().set_bit();
            w
        });

        // 输入捕获的分频，不要分频，直出即可
        ccmr1_input.modify(|_, w| unsafe { w.ic2psc().bits(0) });

        // 启用 CC2 的中断
        // 当 CC2 捕获到下降沿的时候，产生中断
        // 在中断处理函数中，我们就可以通过读取 CCR2 的值，来计算时间和距离了。
        measurer.dier.modify(|_, w| w.cc2ie().enabled());

        // 启用 NVIC 中关于 TIM3 的中断处理函数
        unsafe { NVIC::unmask(interrupt::TIM3) };

        measurer.cr1.modify(|_, w| w.cen().enabled());
    }
}

static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

// 在我们的设置中 TIM3 的中断被触发，主要有两大类

// 1. CC2I 导致的中断，这种情况下，我们应该通过公式计算一下测量到的距离，
//    而且为了防止无意义的计数器重载，而触发中断，这里我们可以手动重置一下计数器的值
// 2. UIF 导致的中断，这种情况需要分别讨论
//    如果 UIF 触发时 CC1IF 没有被设置过，说明这一轮 US-100 没有拉高 Echo 引脚，属于 TIM 空转了，这是正常现象，忽略即可
//    如果 UIF 触发时 CC1IF 已经设置，说明这一轮 US-100 拉高了 Echo 引脚，但还没有拉低 Echo 引脚，TIM 就溢出了，这是错误的情况，应该报告一下
#[interrupt]
fn TIM3() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let measurer = &dp.TIM3;

        let count = G_CNT.borrow(cs).get();

        let measurer_stat = measurer.sr.read();

        if measurer_stat.uif().is_update_pending() {
            // 若 UIF 被设置，就判定一下是 TIM 自然空转产生的，还是 CC1 触发了 但 CC2 还没触发产生的
            // 前者可以简单的忽略，但是后者就需要打印提醒一下了

            measurer.sr.modify(|_, w| w.uif().clear());

            if measurer_stat.cc1if().bit_is_set() {
                measurer.sr.modify(|_, w| w.cc1if().clear_bit());

                rprintln!("{}: Timer Overflow", count);

                G_CNT.borrow(cs).set(count + 1);
            }
        } else if measurer_stat.cc2if().bit_is_set() {
            // 若 CC2IF 被设置，就计算一下距离，并顺道重置一下计数器里的值

            measurer.sr.modify(|_, w| {
                w.cc1if().clear();
                w.cc2if().clear();
                w
            });

            let end = measurer.ccr2().read().ccr().bits();

            // 打印距离的时候，
            // 如果是 debug 模式，就每个数据占一行；如果不是 debug 模式，就用覆写模式输出在同一行
            {
                #[cfg(debug_assertions)]
                rprintln!("{}: {} mm", count, ((end as f32 / 2.0 * 0.3314) as u16));

                #[cfg(not(debug_assertions))]
                rprint!(
                    "\x1b[2K\r{}: {} mm",
                    count,
                    ((end as f32 / 2.0 * 0.3314) as u16)
                );
            }

            measurer.cnt.write(|w| w.cnt().bits(0));

            G_CNT.borrow(cs).set(count + 1);
        }
    });
}
