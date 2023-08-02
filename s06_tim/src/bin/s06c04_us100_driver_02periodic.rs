//! 结合 TIM 的输入捕获和输出比较功能，以固定的间隔控制 US-100 超声波测距模块测量距离
//!
//! 由于我们需要等时间间隔测量距离，这里我们使用一个比较简易的方法，
//! 设置一个 TIM，启用该 TIM 的一个 输出比较 和两个 输入捕获
//! 其中 输出比较 用于在 TIM 计数器 的每轮循环的开始部分，拉高 US-100 的 Trig 引脚
//! 而两个 输入捕获，依旧是用于测量 US-100 的 Echo 高电平的时长

//! 这里和 01freerun 不同的地方在于，
//! 1. 这里的 TIM 不可以被捕获到的上升沿触发计数器重置，因为计数器还要负责稳定地周期触发 探测-接收 流程
//! 2. 也不可以在中断中重置 TIM 的计数器，理由同上
//!
//! 因此，TIM 的单个循环的时间要足够的长，以覆盖整个 US-100 的单次测量输出流程
//! 经过我实测，我手上的 US-100 从触发它工作，到它拉低 Echo 引脚，最长时间间隔大约是 155_000 多 us【注1】
//! 因此 TIM 的但个周期应该大于 155_000 us，而为了测量精度我们又选择了 1us 一个 tick，因此 ARR 的值应该大于 155_000
//! 因此 16 bit 的 TIM 就不合适了（ARR 最大 65535），得选择 32 bit 的 TIM 了
//! 这里我们选择了 TIM2 作为定时器使用
//!
//! 注1：其实这个时候 US-100 拉 Echo 引脚已经 66_000 多微秒了，对应的距离已经是 11 米多了，远远超过 US-100 的可测量范围，
//!      估计是 US-100 无法捕获到任何回波，然后被内置的看门狗拉低了电平

//! 接线图
//!
//! STM32 <-> US-100
//!  3.3V <-> VCC
//!   PA5 <-> Trig
//!  PB10 <-> Echo
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
        w.dbg_tim2_stop().set_bit();
        w
    });

    cortex_m::interrupt::free(|cs| {
        // 为了准确计量 US-100 Echo 引脚被拉高的时间，
        // 这里启用了外部晶振作为时钟源
        setup_hse(&dp);

        setup_gpio(&dp);

        setup_tim2(&dp);

        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

fn setup_hse(dp: &Peripherals) {
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}

    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}
}

fn setup_gpio(dp: &Peripherals) {
    // 切换 GPIO PA5 到 TIM2_CH1 上，作为拉高 US-100 的 Trig 引脚的输出比较端口
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    let gpioa = &dp.GPIOA;
    gpioa.afrl.modify(|_, w| w.afrl5().af1());
    gpioa.pupdr.modify(|_, w| w.pupdr5().pull_down());
    gpioa.moder.modify(|_, w| w.moder5().alternate());

    // 切换 GPIO PB10 到 TIM2_CH3 上，作为 US-100 的 Echo 引脚电平的输入捕获端口
    dp.RCC.ahb1enr.modify(|_, w| w.gpioben().enabled());
    let gpiob = &dp.GPIOB;
    gpiob.afrh.modify(|_, w| w.afrh10().af1());
    gpiob.pupdr.modify(|_, w| w.pupdr10().pull_down());
    gpiob.moder.modify(|_, w| w.moder10().alternate());
}

fn setup_tim2(dp: &Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    let measurer = &dp.TIM2;

    // 1 us CNT 产生一个 tick
    measurer.psc.write(|w| w.psc().bits(8 - 1));

    // 在 ARPE 关闭的情况下配置 ARR
    measurer.cr1.modify(|_, w| w.arpe().disabled());

    // 实测，从触发 US-100 测量开始，到 US-100 自行超时，大约需要 155_000 us
    // 因此这里我们取 200_000 一个周期，大概是一秒钟 5 次测量数据
    measurer.arr.write(|w| w.arr().bits(200_000 - 1));

    measurer.cnt.write(|w| w.cnt().bits(0));

    measurer.cr1.modify(|_, w| {
        w.arpe().enabled();
        w.dir().up();
        w
    });

    // 如果计数器溢出了，就挂起一个中断，在处理该中断时，软件应该打印 Out of Rnage
    measurer.dier.modify(|_, w| w.uie().enabled());

    // 启用 CC1 的 PWM 输出，以周期性触发 US-100 工作
    {
        let ccmr1_output = measurer.ccmr1_output();
        ccmr1_output.reset();
        ccmr1_output.modify(|_, w| {
            w.cc1s().output();
            w.oc1m().pwm_mode1();
            w
        });

        // 拉高 US-100 的 Trig 引脚 10 us，以触发 US-100 工作
        measurer.ccr1().write(|w| w.ccr().bits(10));

        measurer.ccer.modify(|_, w| w.cc1e().set_bit());
    }

    // 启动 CC3 和 CC4，以确定高电平的时间

    let ccmr2_input = measurer.ccmr2_input();

    ccmr2_input.reset();

    // 配置 TIM3 的 CC3，让它检测 Echo 线的上升沿
    // 并在 CC3 检测到上升沿的时候，在 CCR3 中保存计数器的值
    {
        ccmr2_input.modify(|_, w| {
            // 这里使用 TI3 作为输入源
            w.cc3s().ti3();
            w.ic3f().bits(0b11);
            w
        });

        // 让 CC3 捕获上升沿
        measurer.ccer.modify(|_, w| {
            w.cc3np().clear_bit();
            w.cc3p().clear_bit();
            // 这里我们不能随便重置计数器了，因为计数器还肩负周期性唤醒 US-100 的工作
            // 因此 CC3 触发捕获时，将 CNT 的值拷贝到 CCR3 中
            w.cc3e().set_bit();
            w
        });

        // 输入捕获的分频，不要分频，直出即可
        ccmr2_input.modify(|_, w| w.ic3psc().bits(0));
    }

    // 配置 TIM3 的 CC4，让它检测 Echo 线的下降沿
    // 当读取到下降沿的时候，触发中断，以便让软件访问 CCR4，并计算时长
    {
        // 类似 CC3，将 CC4 的输入设置为 TI3，并设置相同的采样过滤方式
        ccmr2_input.modify(|_, w| {
            w.cc4s().ti3();
            w.ic4f().bits(0b11);
            w
        });

        // 让 CC4 捕获下降沿
        measurer.ccer.modify(|_, w| {
            w.cc4np().clear_bit();
            w.cc4p().set_bit();
            // CC4 触发捕获时，将 CNT 的值拷贝到 CCR4 中
            w.cc4e().set_bit();
            w
        });

        // 输入捕获的分频，不要分频，直出即可
        ccmr2_input.modify(|_, w| w.ic4psc().bits(0));

        // 当 CC4 捕获到下降沿的时候，产生中断
        measurer.dier.modify(|_, w| w.cc4ie().enabled());

        // 启用 NVIC 中关于 TIM2 的中断处理函数
        unsafe { NVIC::unmask(interrupt::TIM2) };

        measurer.cr1.modify(|_, w| w.cen().enabled());
    }
}

static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let measurer = &dp.TIM2;

        let count = G_CNT.borrow(cs).get();

        let measurer_stat = measurer.sr.read();

        if measurer_stat.uif().is_update_pending() {
            // 若 UIF 被设置，就判定一下是 TIM 自然空转产生的，还是 CC3 触发了 但 CC4 还没触发产生的
            // 前者可以简单的忽略，但是后者就需要打印提醒一下了

            measurer.sr.modify(|_, w| w.uif().clear());

            // 自然空转，我们直接跳出处理函数即可
            if measurer_stat.cc3if().bit_is_clear() {
                return;
            }

            measurer.sr.modify(|_, w| w.cc3if().clear_bit());

            rprintln!("{}: Timer Overflow", count);
        } else if measurer_stat.cc4if().bit_is_set() {
            // 若 CC4IF 被设置，就计算一下距离，并顺道重置一下计数器里的值

            measurer.sr.modify(|_, w| w.cc4if().clear());

            let begin = measurer.ccr3().read().ccr().bits();
            let end = measurer.ccr4().read().ccr().bits();

            if begin > end {
                rprintln!("{}: begin: {}, end: {}", count, begin, end);
            } else {
                let time_interval = end - begin;

                let dist = ((end - begin) as f32 / 2.0 * 0.3314) as u16;

                // 在 release 模式下，如果计算得到的 dist 大于 4500 mm，就表示
                // US-100 是在自身的看门狗的触发下才拉低 Echo 的，可以直接忽略
                #[cfg(not(debug_assertions))]
                if dist > 4500 {
                    return;
                }

                #[cfg(debug_assertions)]
                rprintln!(
                    "{}: dist: {} mm, begin: {} us, end: {} us, time: {} us",
                    count,
                    dist,
                    begin,
                    end,
                    time_interval
                );

                #[cfg(not(debug_assertions))]
                rprint!("\x1b[2K\r{}: {} mm", count, dist);
            }

            /*
            // 这里不可以清零
            // 清零会导致 CC1 反复触发，就会不断让 US-100 进入工作模式
            measurer.cnt.write(|w| w.cnt().bits(0));
            */
        }

        G_CNT.borrow(cs).set(count + 1);
    });
}
