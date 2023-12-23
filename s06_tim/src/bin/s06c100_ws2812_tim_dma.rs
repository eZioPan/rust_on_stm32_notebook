//! 点亮一个 ws2812 灯珠
//!
//! ws2812 是一个自带逻辑电路的三色灯珠，与其说点亮一个 ws2812 灯珠，不如说是与 ws2812 通信，让其控制灯珠的显示状态。
//! ws2812 的通信协议也比较有趣，首先它的通信频率是固定的，最标准的码率为 800 bit/s，而每个 bit 是通过高低电平的比例（占空比）来表示的，
//! ws2812 的 datasheet 称这种编码形式为 Non-Return-to-Zero（NRZ）编码。
//! 具体来说，一个标准的 Bit 0 应该是 0.4 us 的高电平 + 0.85 us 的低电平，一个标准的 Bit 1 则是 0.8 us 的高电平 + 0.45 us 的低电平
//! 假设我们使用 0.05 us 作为单位，则 Bit 0 和 Bit 1 的高低电平需要的单位数量分别为 8/17 和 16/9（这四个数据将用来确定 TIM 的各种参数）
//! 而一个 ws2812 上又有绿红蓝（注意颜色的排序顺序）三个发光二极管，每个发光二极管又需要 8 bit 的特数据，
//! 因此每个 ws2812 需要接收 3 * 8 = 24 bit 的数据，而且每个 byte 的数据都是按照 MSB 先发送的顺序发送的
//! 而且 ws2812 具有一个很好的特新，就是多个 ws2812 可以简单串联起来统一控制，
//! 每个 ws2812 在收到第一组 24 bit 的数据，都会放到自己的寄存器里，如果之后再收到其它的数据，则会通过输出引脚输出到下一个 ws2812 中
//! 这样我们就可以通过一个引脚连续数据多个 bit，来控制多个 ws2812，大大减少了引脚和连线的使用。
//!
//! 另外，在每轮数据传输完成之后（也就是某一时刻下所有 ws2812 应该显示的颜色），我们需要保持 ws2812 的数据线至少有连续 50 us 的低电平，
//! 让所有的 ws2812 可以确认数据输出已经完成，并让自己寄存器内存储的数据修改三色 LED 实际的颜色
//!
//! 在实现上，我们将使用 TIM 的 PWM 输出功能，搭配 DMA 输出数据流。注意到 800 kHz 对于 中断 + Cortex CPU 改写寄存器来说，频率还是太高了，
//! 因此，使用 DMA 就是必然的了。另外，我们还开启了另一个 TIM 来实现闪烁效果，并使用 WFI 和 Sleep on Exit，节省少许能源消耗。
//!
//! 接线图：
//!
//! 第一颗 ws2812 的 DIN 引脚接入 GPIO PB4，VCC 接入 3.3V 或 5V 电源，GND 接地即可

#![no_std]
#![no_main]

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
};

use cortex_m::{asm, interrupt::Mutex, peripheral::NVIC};
use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};
use stm32f4xx_hal::{interrupt, pac};

// 颜色表，具体的数值写在代码末尾
// 可以注意到这里颜色表本身是 static，而且它是一个数组切片，且其中的元素也是多个数据切片
// 这样有两个好处，第一个是，相较于使用 const + 数组的组合，我们可以节省大量的存储空间
// 第二个是我们后面要使用 DMA 传输，构建 DMA 的时候需要给出指针，而切片本来就是指针，
// 这样我们可以少些几个借用符号
static COLOR_LIST: &[&[u16]] = &[
    TURN_OFF,
    DIM_GREEN,
    DIM_RED,
    DIM_BLUE,
    DIM_YELLOW,
    DIM_CRAN,
    DIM_MAGENTA,
    DIM_WHITE,
];

// 记录下一次要展示的颜色的在 COLOR_LIST 中的索引
static COLOR_INDEX: AtomicU8 = AtomicU8::new(1);

static G_DP: Mutex<RefCell<Option<pac::Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("\nProgram Start");

    let cp = pac::CorePeripherals::take().unwrap();
    let dp = pac::Peripherals::take().unwrap();

    setup_rcc(&dp);
    setup_low_power(&cp, &dp);
    setup_gpio(&dp);
    setup_dma(&dp);
    setup_pwm(&dp);
    setup_delay(&dp);

    cortex_m::interrupt::free(|cs| {
        let mut dp_mut = G_DP.borrow(cs).borrow_mut();
        dp_mut.replace(dp);

        let dp = dp_mut.as_ref().unwrap();

        enable(dp);
    });

    // 搭配 Sleep on Exit，我们并不需要手动使用 loop {} 防止 main 退出
    asm::wfi();
    unreachable!("Do Not Forget to set SleepOnExit");
}

// 将 SYSCLK/HCLK/PCLK 全部设置为 20 MHz，这样一个 tick 就是 0.05 us
// 正好，标准的 ws2812 的时序，都是 0.05 us 的整数倍
fn setup_rcc(dp: &pac::Peripherals) {
    let rcc = &dp.RCC;

    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}

    rcc.pllcfgr.modify(|_, w| {
        w.pllsrc().hse();
        unsafe {
            w.pllm().bits(6);
            w.plln().bits(80);
        }
        w.pllp().div8();
        w
    });

    rcc.cr.modify(|_, w| w.pllon().on());
    while rcc.cr.read().pllrdy().is_not_ready() {}

    rcc.cfgr.modify(|_, w| w.sw().pll());

    while !rcc.cfgr.read().sws().is_pll() {}
}

// 低功率相关设置
// 主要是开启 Sleep on Exit，因为在配置完成之后，整个程序运行都是由各种中断触发的
// 中断处理结束就可以直接进入睡眠
fn setup_low_power(cp: &pac::CorePeripherals, dp: &pac::Peripherals) {
    // 在 Cortex 核心配置中开启 Sleep on Exit
    unsafe { cp.SCB.scr.modify(|v| v | 1 << 1) };

    let dbgmcu = &dp.DBGMCU;
    dbgmcu.cr.reset();
    #[cfg(debug_assertions)]
    dbgmcu.cr.modify(|_, w| w.dbg_sleep().set_bit());
}

// 开启 GPIO PB4 的 alternate 输出，让其输出 TIM3 的 CC1 的输出
fn setup_gpio(dp: &pac::Peripherals) {
    let rcc = &dp.RCC;
    rcc.ahb1enr.modify(|_, w| w.gpioben().enabled());

    let gpiob = &dp.GPIOB;
    gpiob.ospeedr.modify(|_, w| w.ospeedr4().medium_speed());
    // 开启下拉电阻，因为会有一段时间，TIM3 是关闭的，通过下拉电阻，我们可以保持引脚处于低电平
    gpiob.pupdr.modify(|_, w| w.pupdr4().pull_down());
    gpiob.afrl.modify(|_, w| w.afrl4().af2());
    gpiob.moder.modify(|_, w| w.moder4().alternate());
}

fn setup_dma(dp: &pac::Peripherals) {
    let rcc = &dp.RCC;

    rcc.ahb1enr.modify(|_, w| w.dma1en().enabled());

    let pwm_dma = &dp.DMA1;

    let pwm_st = &pwm_dma.st[4];

    // 在配置 DMA 之前，总是确保 DMA Stream 处于停止的状态
    if pwm_st.cr.read().en().is_enabled() {
        pwm_st.cr.modify(|_, w| w.en().disabled());
        while pwm_st.cr.read().en().is_enabled() {}
    }

    pwm_st.cr.modify(|_, w| {
        w.chsel().bits(5);
        // 使用 MBURST，搭配 FIFO，减少对系统总线的使用
        w.mburst().incr8();
        w.pl().high();
        w.msize().bits16();
        w.psize().bits16();
        w.minc().incremented();
        w.dir().memory_to_peripheral();
        // 虽然我们希望 ws2812 不断闪动，但其并非用 DMA 的 circular 模式实现的
        // w.circ().disabled();
        // 开启 DMA 传输完成触发中断，我们需要在中断中执行一些“清理工作”
        w.tcie().enabled();
        // 当然 DMA 传输错误我们也是要处理的
        w.teie().enabled();
        w
    });

    let cur_color = COLOR_LIST[0];

    pwm_st.ndtr.write(|w| w.ndt().bits(cur_color.len() as u16));

    // 占空比的数据直接输入到 CCR 寄存器里
    pwm_st
        .par
        .write(|w| unsafe { w.pa().bits(dp.TIM3.ccr1().as_ptr() as u32) });
    pwm_st
        .m0ar
        .write(|w| unsafe { w.m0a().bits(cur_color.as_ptr() as u32) });

    // 开启 FIFO 的全部容量
    pwm_st.fcr.modify(|_, w| {
        w.dmdis().disabled();
        w.feie().enabled(); // 如果产生了 FIFO 错误则触发中断
        w.fth().full();
        w
    });

    // 最后一步，清理 DMA Stream 的半传输完成标识和传输完成标识
    pwm_dma.hifcr.write(|w| {
        w.chtif4().clear();
        w.ctcif4().clear();
        w
    });

    // 记得 unmask NVIC 中对应的中断位
    unsafe { NVIC::unmask(interrupt::DMA1_STREAM4) }

    // 注意，这里我们并没有启动 DMA Stream
    // 因为有多个外设需要近乎同时启动
}

fn setup_pwm(dp: &pac::Peripherals) {
    let rcc = &dp.RCC;

    rcc.apb1enr.modify(|_, w| w.tim3en().enabled());

    let pwm_tim = &dp.TIM3;

    // 设置为 25 个 tick 一个溢出，这样在 APB2 为 20 MHz 的时候，
    // TIM 的溢出频率为 800 kHz
    pwm_tim.arr.write(|w| w.arr().bits(25 - 1));

    // 上计数模式，搭配 PWM_MODE1，让每个 TIM 溢出周期中，首先产生高电平，之后产生低电平
    // 以符合 ws2812 对于 Bit 时序的要求
    pwm_tim.cr1.modify(|_, w| w.dir().up());

    // 这里的设置稍稍有点特殊，我们将触发 CC DMA 请求的来源，从 CC Event 改为了 Update Event
    // 这样，在我们这个案例中，我们就可以，在 TIM3 记数上溢出的时候，让 TIM 通过 TIM3_CH1 这个 Stream & Channel 来触发 DMA 修改 CCR 寄存器的值
    pwm_tim.cr2.modify(|_, w| w.ccds().on_update());

    // 启用 CC1 的 DMA Request，配合上面对于 CCDS 的修改，达成我们的需求
    pwm_tim.dier.modify(|_, w| w.cc1de().enabled());

    // 设置 PWM_MODE1，搭配 DIR 设置，达成我们的需求
    let pwm_ccmr1 = pwm_tim.ccmr1_output();
    pwm_ccmr1.modify(|_, w| {
        w.cc1s().output();
        w.oc1m().pwm_mode1();
        // 强烈建议设置 CCR 寄存器的预载功能
        // 使用之后会略微增加启动输出的延迟（需要与上溢出同步）
        // 但能保证输出波形的完整性
        w.oc1pe().enabled();
        w
    });

    // 最后开启 CC1 的输出
    pwm_tim.ccer.modify(|_, w| w.cc1e().set_bit());

    // 注意，这里我们并没有启动 TIM3，也没有设置任何中断
    // 因为有多个外设需要近乎同时启动，
    // 而且 TIM3 除了触发 DMA 转运，也不需要其它的中断了，它是一个几乎被动的设备
}

// 使用另外一个定时器，设置一个延时，这个延时有两个作用，
// 第一个是在一轮传输完成后保持 50 us 的低电平，让 ws2812 可以确认收到的数据，并修改好 LED 的状态
// 第二个是我们需要让灯的某个状态保持一段时间，让我们可以观察到灯的变化
// 不过就目前我们的设置来说，第一个时间可以包含在第二个时间里，因此这里我们直接使用单一的 TIM 完成两个延时功能
fn setup_delay(dp: &pac::Peripherals) {
    let rcc = &dp.RCC;

    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());

    let delay_tim = &dp.TIM2;

    // 将 TIM2 的 tick 调整为 1 ms 一次
    delay_tim.psc.write(|w| w.psc().bits(20_000 - 1));
    // 将 TIM2 溢出调整为 0.5 s 一次
    delay_tim.arr.write(|w| w.arr().bits(500 - 1));

    // 注意，这里我们拉起的是中断，而非 DMA，
    // 因为在 TIM2 溢出的时候，我们还是需要稍稍处理一些逻辑的
    delay_tim.dier.modify(|_, w| w.uie().enabled());

    unsafe { NVIC::unmask(interrupt::TIM2) };

    // 此处依旧不开启 TIM2，依旧是等待最后统一开启所有的外设
}

// 开启三大外设
fn enable(dp: &pac::Peripherals) {
    dp.DMA1.st[4].cr.modify(|_, w| w.en().enabled());
    dp.TIM2.cr1.modify(|_, w| w.cen().enabled());
    dp.TIM3.cr1.modify(|_, w| w.cen().enabled());
}

// DMA 的中断处理函数，大致要处理两种情况
// 第一种情况是 DMA 报错，此时我们打印一下错误信息，并 panic 即可
// 第二种情况是 DMA 成功完成了一轮传输，那么我们就需要处理一些标识位，并可以关掉不必要的外设，以稍稍降低功耗
#[interrupt]
fn DMA1_STREAM4() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let dma1 = &dp.DMA1;

        let hifcr = &dma1.hifcr;
        let hisr = dma1.hisr.read();

        // 这里处理 DMA 错误所需要的工作
        // 这里的处理方法也非常简单，就是打印一下错误，然后清理掉对应的标识位，并终止所有外设的运行
        let mut teif = false;
        let mut feif = false;
        {
            if hisr.teif4().is_error() {
                hifcr.write(|w| w.cteif4().clear());
                teif = true;
            }

            if hisr.feif4().is_error() {
                hifcr.write(|w| w.cfeif4().clear());
                feif = true;
            }

            if teif || feif {
                dma1.st[4].cr.modify(|_, w| w.en().disabled());
                if teif {
                    rprintln!("DMA1 STEAM4 FIFO Error");
                }
                if feif {
                    rprintln!("DMA1 STEAM4 FIFO Error");
                }

                // 清理工作，三大外设的关闭和重置

                let rcc = &dp.RCC;

                rcc.apb1enr.modify(|_, w| {
                    w.tim2en().disabled();
                    w.tim3en().disabled();
                    w
                });
                rcc.apb1rstr.modify(|_, w| {
                    w.tim2rst().reset();
                    w.tim3rst().reset();
                    w
                });
                rcc.apb1rstr.modify(|_, w| {
                    w.tim2rst().clear_bit();
                    w.tim3rst().clear_bit();
                    w
                });
                rcc.ahb1enr.modify(|_, w| {
                    w.gpioben().disabled();
                    w.dma1en().disabled();
                    w
                });
                rcc.ahb1rstr.modify(|_, w| {
                    w.gpiobrst().reset();
                    w.dma1rst().reset();
                    w
                });
                rcc.ahb1rstr.modify(|_, w| {
                    w.gpiobrst().clear_bit();
                    w.dma1rst().clear_bit();
                    w
                });

                panic!("Stop here");
            }
        }

        // 这里是处理正常完成传输所需要的额外操作
        // 主要就是清理半传输完成和全传输完成标识位，并打印一下信息
        // 注意，清理两个标识位非常重要，如果不清理，则下次 DMA 传输是无法开始的
        if hisr.tcif4().is_complete() {
            hifcr.write(|w| {
                w.chtif4().clear();
                w.ctcif4().clear();
                w
            });
            rprint!(
                "\x1b[2K\rDMA1 STREAM4 Transfer Completed: {}",
                G_CNT.fetch_add(1, Ordering::AcqRel)
            );

            // 注意，这里我们必须关闭 TIM 的 CC 的 DMA 请求
            // 如果我们不关闭，DMA 会在 Stream 关闭之后依旧收到 DMA 请求，从而导致 FIFO 错误
            dp.TIM3.dier.modify(|_, w| w.cc1de().disabled());

            // 关闭计数，并重置 CNT 寄存器，因为我们不能确定执行到这里的时候，CNT 寄存器的状态
            // 因此我们要停止 TIM3 的计数，并清零 CNT 寄存器，让下一次启动 TIM3/PWM 的时候是一个初始化的状态
            //
            // 注意：
            // 这里其实有一个小小的问题，那就是我们在停止计数器的时候，是无法确定最后一个从 DMA 读取到的 CCR 数据，是否已经完成了输出
            // 不过这里有一点很巧妙，那就是我们必然知道，倒数第二个波形必然是输出完成了的，因此当前 CC1 必然是处于底电平输出的，
            // 因此我们即便关闭了计数器，也不会导致 CC1 输出的电平变化，因此这里我们可以安全地关闭计数器功能
            dp.TIM3.cr1.modify(|_, w| w.cen().disabled());
            dp.TIM3.cnt.reset();

            // 为了节省一些能量，我们进一步关闭了 TIM3 外设
            dp.RCC.apb1enr.modify(|_, w| w.tim3en().disabled());

            // 如果你需要 RTT，则不要关掉 DMA
            // dp.RCC.ahb1enr.modify(|_, w| w.dma1en().disabled());
        }
    })
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.TIM2.sr.modify(|_, w| w.uif().clear());

        let pwm_dma = &dp.DMA1;

        // 如果你需要 RTT，从而没有关掉 DMA，则这里也不需要开启它
        // dp.RCC.ahb1enr.modify(|_, w| w.dma1en().enabled());

        let pwm_st = &pwm_dma.st[4];

        // 此处无需检查 DMA Stream 的运行状态，因为我们在 DMA 完成中断中已经处理过了

        // 在尽可能短的时间里，获取当前颜色索引，计算下一次的颜色索引，并存储下一次的颜色索引
        let color_index = COLOR_INDEX.load(Ordering::Acquire);
        let next_index = if color_index < COLOR_LIST.len() as u8 - 1 {
            color_index + 1
        } else {
            0
        };
        COLOR_INDEX.store(next_index, Ordering::Release);

        // 配置 DMA，让其在 TIM 下次通过 TIMx_CCy 发出 DMA 请求的时候，发送正确的数据
        let cur_color = COLOR_LIST[color_index as usize];
        pwm_st.ndtr.write(|w| w.ndt().bits(cur_color.len() as u16));
        pwm_st
            .m0ar
            .write(|w| unsafe { w.m0a().bits(cur_color.as_ptr() as u32) });

        // 此处无需清理 DMA 的 ISR，因为它已经在 DMA 的中断中被清理了

        pwm_st.cr.modify(|_, w| w.en().enabled());

        // 由于我们为了节省能量，每次数据输出完成，我们都关闭了 TIM3，
        // 因此这里我们还需要开启 TIM 的 DMA 请求和 TIM 时钟
        dp.RCC.apb1enr.modify(|_, w| w.tim3en().enabled());
        dp.TIM3.dier.modify(|_, w| w.cc1de().enabled());
        dp.TIM3.cr1.modify(|_, w| w.cen().enabled());
    });
}

static G_CNT: AtomicU32 = AtomicU32::new(1);

// ws2812 使用频率固定，但占空比不同的 PWM 信号当作 bit 0 和 bit 1

const N0: u16 = 8;
const N1: u16 = 16;

// 注意 ws2812 特殊的颜色排序
static TURN_OFF: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N0, N0, // G
    N0, N0, N0, N0, N0, N0, N0, N0, // R
    N0, N0, N0, N0, N0, N0, N0, N0, // B
    // 在一轮颜色数据发送完成之后，需要将 TIM 的 CC 的输出置 0，并保持一段时间，让 ws2812 刷新自己的显示
    // 由于这里我们仅控制 1 个 ws2812，于是我们在 24 bit 之后就可以直接输出这 50 us 的低电平了
    0,
];

static DIM_WHITE: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N1, N0, // G
    N0, N0, N0, N0, N0, N0, N1, N0, // R
    N0, N0, N0, N0, N0, N0, N1, N0, // B
    0,
];

static DIM_GREEN: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N1, N0, // G
    N0, N0, N0, N0, N0, N0, N0, N0, // R
    N0, N0, N0, N0, N0, N0, N0, N0, // B
    0,
];

static DIM_RED: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N0, N0, // G
    N0, N0, N0, N0, N0, N0, N1, N0, // R
    N0, N0, N0, N0, N0, N0, N0, N0, // B
    0,
];

static DIM_BLUE: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N0, N0, // G
    N0, N0, N0, N0, N0, N0, N0, N0, // R
    N0, N0, N0, N0, N0, N0, N1, N0, // B
    0,
];

static DIM_YELLOW: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N1, N0, // G
    N0, N0, N0, N0, N0, N0, N1, N0, // R
    N0, N0, N0, N0, N0, N0, N0, N0, // B
    0,
];

static DIM_CRAN: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N1, N0, // G
    N0, N0, N0, N0, N0, N0, N0, N0, // R
    N0, N0, N0, N0, N0, N0, N1, N0, // B
    0,
];
static DIM_MAGENTA: &[u16] = &[
    N0, N0, N0, N0, N0, N0, N0, N0, // G
    N0, N0, N0, N0, N0, N0, N1, N0, // R
    N0, N0, N0, N0, N0, N0, N1, N0, // B
    0,
];
