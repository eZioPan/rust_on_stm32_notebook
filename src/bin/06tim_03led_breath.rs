//! 通过 TIM 的 PWM 功能，在 GPIO PA05 上实现一个呼吸灯的效果
//!
//! 首先要介绍的是 TIM 的 PWM 功能
//! PWM 的特点是，它输出的是一个“周期函数”，在单个周期中，我们可以调节输出高低电平的时间占比，以极高速闪动，以达到模拟电平连续变化的效果
//!
//! TIM 实现的方法如下：
//! TIM 由于要计时，因此已经有了时基单元，其是通过计数器 CNT 的连续自增/自减 并搭配 上溢出/下溢出 和 自动重载寄存器 ARR 来实现的
//! 在 TIM 的实现中，可以发现两个特点，
//! 第一个是，时基单元自己可以构成周期输出，
//! 第二，在一个周期中，计数器是不断且均匀地增加/减少的，因此计数器的实时的值起始可以用来进行比较
//!
//! 由于 TIM 的时基单元具有上面的特点，因此实现 PWM 输出的方法也很简单
//! 我们设置一个固定值，然后启动时基单元，并做一个判断，若计数器当前的值大于（小于、等于、不等于）我们设置的固定值，就输出高电平，否则输出低电平
//!
//! 依照上面的推断，如果 PWM 要在 Cortex 核心不参与的情况下，实现 PWM 效果，则 TIM 还需要一个具有比较功能的模块
//! 于是 TIM 就有一个 捕获/比较寄存器 CCxR（Cpature/Compare x Register（其中 x 为 1 开头的索引值，一般一个通用定时器里有 4 个）），从名字就可以看出来，它具有比较的功能（也能直观看出来它具有捕获的功能，不过这里不做解释）
//! 而这个 CCxR 就可以实现我们上面的说的，记录一个固定的值，不断与计数器 CNT 的值进行比较，最后输出高低电平
//!
//! 不过要将 CCxR 输出的高低电平送到 GPIO PA5 上，还需要启用 CCxR 后端的 输出控制（Output Control）
//!
//! 好了，PWM 的配置流程大概就是这样的，然后是呼吸效果的配置，也就是 LED 的亮度需要有变化，因此我们可以启动另一个 TIM，每次后者触发 Update 时间的时候，就修改一下输出 PWM 的 TIM 的 CCxR 记录的比较值即可
//!
//! 这里我们需要搭建一个外围电路，大概的连接路线是
//! GPIO PA5 -> LED 正极 -- LED 负极 -> 220 欧电阻 -> 接地
//!
//! 后来我测试了一下，我手上的黄色草帽形 LED，在不烧毁的情况下，最大可以承受 2.1 V 的电压，通过的电流为 27 mA，在 3.3 V 的供电电压情况下，限流电阻的大小应为 44.8 欧
//! 不过 STM32F411RET6 的 GPIO 单口最大电流也就 25 mA，如果要用 GPIO 直接驱动 LED 的话，估计限流电阻至少得 50 欧以上了

#![no_std]
#![no_main]

use core::{
    cell::Cell,
    fmt::{Display, Formatter, Result},
};

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};
use stm32f4xx_hal::pac::{self, interrupt, NVIC};

// MAX_ARR_VALUE 确定了 TIM2 ARR 和 TIM3 ARR 能达到的最大值
const MAX_ARR_VALUE: u16 = 999;

// 设置 ARR 的最小值没有意义，因为 ARR 的下溢出一定是由于 CNT == 0 触发的
// const MIN_ARR_VALUE: u16 = 0;

// PWM 所在计时器 CCR 的变化步距，用来控制小灯泡亮度变化的速率（修改亮度的频率固定的情况下，使用步距调整亮度变化的速率）
const STEP: u16 = 50;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 由于我们这里需要设置 RCC、两个 TIM、以及 GPIO，内容较多，因此这里我们用独立的函数区分一下配置流程
    if let Some(dp) = pac::Peripherals::take() {
        // 让 SYSCLK 运行在 HSE 上
        config_hse(&dp);

        // 当 debug 时，TIM 的时钟应该被停止，方便我们查看问题
        dp.DBGMCU.apb1_fz.modify(|_, w| {
            w.dbg_tim2_stop().set_bit();
            w.dbg_tim3_stop().set_bit();
            w
        });

        // 将 GPIO PA5 配置为 AF01 模式
        gpio_pa5_af1(&dp);

        // 将 TIM2 配置为 PWM 输出
        tim2_pwm_init(&dp);

        // 使用 TIM3 以 50 Hz 的频率修改 TIM2 输出 PWM 的占空比
        tim3_timer(&dp);
    }

    loop {}
}

// 让 SYSCLK 运行在 HSE 上
fn config_hse(dp: &pac::Peripherals) {
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}
    dp.RCC.cfgr.modify(|_, w| w.sw().hse());
    while !dp.RCC.cfgr.read().sws().is_hse() {}
}

// 查表后可知，TIM2 的 TIM2_CH1 的端口可以是 GPIO PA5
// 这里将 GPIO PA5 切换到 AF01 上
fn gpio_pa5_af1(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    dp.GPIOA.afrl.modify(|_, w| w.afrl5().af1());
    // 就下方 TIM2 时基模块的输出频率 800 Hz 来说，是没有必要修改 GPIO 的输出速率的
    // 不过其它情况下就不一定了
    // dp.GPIOA.ospeedr.modify(|_, w| w.ospeedr5().high_speed());
    dp.GPIOA.moder.modify(|_, w| w.moder5().alternate());
}

// 这里我们要正式配置 PWM 模式的 TIM2 了
fn tim2_pwm_init(dp: &pac::Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    // 由于我们要设置两个定时器
    // 因此还是给当前需要设置的定时器一个别名，省的写错了
    let pwm_timer = &dp.TIM2;

    pwm_timer.cr1.modify(|_, w| {
        w.arpe().enabled();
        w.dir().down();
        w
    });

    // 推荐 PWM 所在的 TIM 的 ARR 的值应该和 输出比较寄存器 能达到的最大值相同
    pwm_timer.arr.write(|w| w.bits(MAX_ARR_VALUE as u32));
    // 由于 PWM 的循环周期是由 f_TIMCLK/[(PSC+1)*(ARR+1)] 确定的
    // 因此这个乘积不应该太大，防止闪动
    pwm_timer.psc.write(|w| w.psc().bits(9));

    // CCMR: Capture / Compare Mode Register
    // 这里看起来非常奇怪，是由于 TIMx_CCMR1 和 TIMx_CCMR2 是**功能复用的地址**
    // 嗯，你没有看错，能复用的不仅仅是复用器，还有内存地址
    // Referencing Manual 上说，TIMx_CCMR1 和 TIMx_CCMR2 的位的含义，将随着该寄存器中对应的 CCxS 位的不同而不同
    // CCxS: Capture / Compare x Selection
    // 当 CCxS 处于输出模式时，则对应的其它位就是输出相关的位，若处于输入模式时，则对应的其它位就是输入相关的位
    // 不过 Rust 的结构体并不支持可变字段，因此我们得**预先**选择好**结构体的状态**，再进行实际的配置
    //
    // 这里我们要让 TIM2_CH1 对应的 CCR1 设置为 OC（输出比较）模式
    // 因此我们先要通过 .ccr1_output() 告诉 Rust，将准备好用于输出模式的 CCMR1 结构体
    // 然后再通过 .modify() 确实让 CC1S 位处于 Output 模式
    let ccmr1_output = pwm_timer.ccmr1_output();

    // 这里我们最好 Reset 一下这个寄存器，因为这个寄存器是复用的，防止以前模式的值的干扰
    ccmr1_output.reset();
    // 开始配置输出比较相关的内容
    ccmr1_output.modify(|_, w| {
        // 确实让 CC1S 位处于 Output 模式
        w.cc1s().output();
        // 启用比较寄存器的预载
        w.oc1pe().enabled();
        // OC1M: Output Compare 1 Mode
        // 000 Frozen: CCR 与 CNT 的比较不会影响输出（OC1REF 的值）
        // 001 Active Level on Match: 当 CNT 等于 CCR 时，OC1REF 置为高电平，其余时刻置于低电平
        // 010 Inactive Level on Match: 当 CNT 等于 CCR 时，OC1REF 置为低电平，其余时刻置于高电平
        // 011 Toggle: 当 CNT 等于 CCR 时，OC1REF 的电平翻转
        // 100 Force Inactive:  OC1REF 强制置低
        // 101 Force Active: OC1REF 强制置高
        // 110 PWM Mode 1: CNT < CCR 高电平，CNT > CCR 低电平，CNT == CCR 时，切换为 CNT 继续向后数之后应该在的状态
        // 111 PWM Mode 2: CNT > CCR 高电平，CNT < CCR 低电平，CNT == CCR 时，切换为 CNT 继续向后数之后应该在的状态
        //
        // PWM Mode 1 简单理解为 CCR 压着 CNT 么？压着就高电平，不压着就低电平
        // PWM Mode 2 简单理解为 CNT 飞跃了 CCR 么？飞跃了就高电平，没飞跃就低电平
        w.oc1m().pwm_mode2();
        w
    });

    // 并不是说我们前面将 CCMR1 结构体配置为输出模式后，
    // CC2S 也只能配置为输出模式了，这里 CC2S 还是可以被配置为输入模式的
    // pwm_timer.ccmr1_input().modify(|_, w| w.cc2s().ti2());
    // 我们可以观察到 CC1S 是处于输出模式的，而 CC2S 是处于 TI2 输入模式的

    // 这里也是一样，准备一下配置 ccr1 的结构体
    // CCR1: Capture / Compare Register 1
    // 然后我们初始化一下 CCR 的值
    let ccr1 = pwm_timer.ccr1();
    ccr1.write(|w| w.ccr().bits(MAX_ARR_VALUE as u32));

    // 然后我们需要通过 CC1E 启动 CC1R
    // CCER: Capture / Compare Enable Register
    pwm_timer.ccer.modify(|_, w| {
        // 在输出模式下，将 CC1 的输出极性设置为默认值
        // CC1P: Capture / Comapre 1 Polarity
        // 这样激活状态的输出为高电平，非激活状态输出的为低电平
        //
        // 不过由于我们前面已经 reset 过整个寄存器了，这里其实没有必要设置这个位
        // w.cc1p().clear_bit();

        // 根据 Reference Manual，当我们要启用 CC1E 时，
        // 我们必须保证 CC1NP 为默认状态
        // CC1NP: Capture / Comapre 1 Negate Polarity
        //
        // 不过由于我们前面已经 reset 过整个寄存器了，这里其实没有必要设置这个位
        // w.cc1np().clear_bit();

        // 最后，设置 CC1E 以启用 CC1 的输出
        // CC1E: Capture / Compare 1 Enable
        w.cc1e().set_bit();
        w
    });

    // 最后启动 TIM2 的计数器
    pwm_timer.cr1.modify(|_, w| w.cen().enabled());
}

fn tim3_timer(dp: &pac::Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim3en().enabled());

    // 由于我们要设置两个定时器
    // 因此还是给当前需要设置的定时器一个别名，省的写错了
    let shift_timer = &dp.TIM3;

    // 人眼对于光强度的抖动的识别在 24 Hz 下就不易辨识
    // 不过我们设置到 50 Hz 好了，好算一点
    shift_timer.psc.write(|w| w.psc().bits(9999));
    shift_timer.arr.write(|w| w.arr().bits(15));
    shift_timer.cr1.modify(|_, w| {
        w.arpe().enabled();
        // 当 TIM3 的 CNT 溢出的时候，就触发一个中断
        // 中断产生时，修改 TIM2 CCR1 的值，来调节 TIM2 PWM 输出的“平均功率”
        w.urs().counter_only();
        w
    });

    // 当 TIM3 的 CNT 溢出的时候，就触发一个中断
    // 中断产生时，修改 TIM2 CCR1 的值，来调节 TIM2 PWM 输出的“平均功率”
    unsafe {
        NVIC::unmask(interrupt::TIM3);
    }

    // 当 TIM3 的 CNT 溢出的时候，就触发一个中断
    // 中断产生时，修改 TIM2 CCR1 的值，来调节 TIM2 PWM 输出的“平均功率”
    shift_timer.dier.modify(|_, w| w.uie().enabled());

    // 最后启动计数器
    shift_timer.cr1.modify(|_, w| w.cen().enabled());
}

// 由于我们要制作呼吸灯效果，这里需要手动记录一下，GPIO PA5 上的灯是在变亮还是在变暗
#[derive(Clone, Copy)]
enum Direction {
    Lighting,
    Dimming,
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(match self {
            Direction::Lighting => "Lighting",
            Direction::Dimming => "Dimming",
        })
    }
}

// 由于我们要制作呼吸灯效果，这里需要手动记录一下，GPIO PA5 上的灯是在变亮还是在变暗
// 由于上面的初始化过程中，TIM2 的 CCR1 的值被设置为 MAX_ARR_VALUE，因此初始化该值时，必然是处于变暗的过程中
static CUR_DIR: Mutex<Cell<Direction>> = Mutex::new(Cell::new(Direction::Dimming));
// 注意，当前 GPIO PA5 上灯珠的亮度是不需要额外记录的，因为亮度等价于 TIM2 CCR1 的值

// 当 TIM3 的计数器溢出触发中断的时候，修改 TIM2 PWM 的 CCR1 的值，来修改 PWM 输出的“功率”
#[interrupt]
fn TIM3() {
    cortex_m::interrupt::free(|cs| unsafe {
        let dp = pac::Peripherals::steal();

        let (pwm_timer, shift_timer) = (&dp.TIM2, &dp.TIM3);

        // 还是一样，先清理 TIM3 的 状态寄存器的 UIF 位
        shift_timer.sr.modify(|_, w| w.uif().clear());

        // 读取一下 TIM2 CCR1 的当前值
        let tim2_ccr1 = pwm_timer.ccr1();
        let last_value = tim2_ccr1.read().ccr().bits() as u16;

        let g_dir = CUR_DIR.borrow(cs);

        // 读取一下当前灯泡处于变亮中还是变暗中的状态
        let last_dir = g_dir.get();

        // 判定一下需要怎么设置 TIM2 的 CCR1 的值
        // 基本方案就是：在变亮区，且还能变亮，则变量，否则切换变化方向；在变暗区则反之
        let cur_value = match last_dir {
            Direction::Lighting => {
                if last_value <= MAX_ARR_VALUE - STEP {
                    last_value + STEP
                } else {
                    g_dir.set(Direction::Dimming);
                    // 在切换变化方向的时候，得用预设的最大值减去步长，不要用 last_value 减去步长
                    // 防止 TIM2 CCR1 的值，在每次切换中累计偏移
                    MAX_ARR_VALUE - STEP
                }
            }
            Direction::Dimming => {
                if last_value >= 0 + STEP {
                    last_value - STEP
                } else {
                    g_dir.set(Direction::Lighting);
                    // 在切换变化方向的时候，得用预设的最小值加上步长，不要用 last_value 加上步长
                    // 防止 TIM2 CCR1 的值，在每次切换中累计偏移
                    0 + STEP
                }
            }
        };

        tim2_ccr1.write(|w| w.ccr().bits(cur_value as u32));

        // 清空本行并打印变化方向和 CCR 的当前值
        rprint!("\x1b[2K\r{}: {}\r", g_dir.get(), cur_value);
    });
}
