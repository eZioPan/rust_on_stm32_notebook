//! ADC 模拟到数字转换器
//!
//! ADC Analog to Digital Converter 是一种采样模拟电压，并转换为数字信号的设备
//!
//!
//! 【重要】：虽然 STM32F412 的大部分引脚都是 FT（Five volt Tolerant）的，但在 analog 模式下，他们均不是 FT 的，
//!           切勿将高于 3.3V 的电压直接引入 analog 模式下的 GPIO 引脚
//!
//!
//! ADC channel：
//! ADC 的采样对象，主要是 GPIO 输入的电压值，另外该包含一些内部模拟数据，
//! 而且，一个 ADC 可以采样多个信号来源，因此必然有一个 ADC 中每个采样器编号与 GPIO 引脚编号对应的关系
//! 就 ADC 的部分来说，它被称为 ADC Channel，STM32F412RET6 这块芯片的 ADC 模块共有 19 个通道（编号 0-18），
//! 其中前 16 个分配给了 GPIO，另外 3 个分给了内部温度计、内部参考电压、V_{BAT} 电压值
//! 一旦涉及到 GPIO，就必然有一张表，将 GPIO 编号与 ADC channel 对应起来，
//! 这张表就是 datasheet 的 STM32F412xx pin definitions，
//! 它的最后一列 Additional functions 就记录了 GPIO 引脚对应的 ADC channel
//! 有一点需要注意的是，与 Reference Manual 中以 ADCx_INy 的形式来表示 ADC x 的 y 通道不同，
//! STM32F412xC/xE pin definitions 中的叫法为 ADCx_y，没有 IN 这两个字母
//!
//!
//! ADC sequence and Scan Mode：
//! 事实上，STM32 的 ADC 在 channel 的基础上还提供了一个额外的功能，那就是，它可以在收到一个启动命令之后，对一组（group）channel 中的每一个输入源进行采样
//! 因此，我们除了选定 channel 以外，我们还需要将 channel 排序到 sequence 里，并告诉 ADC sequence 实际的长度，然后才能让 ADC 正确运行
//! 而且，STM32 的 ADC 还分为了两个组：Regular Group 和 Inject Group，可以理解为常规组和插入组
//! Regular Group 的采样过程可以被 Inject Group 的采样过程中断
//!
//!
//! ADC 的工作原理简介：
//!
//! 1. 开始采样，接通 GPIO 口至 ADC 的通路，并在内部保存一下 GPIO 口的电压（比如通过一个小电容存储一下电压）
//! 2. 断开 GPIO 到 ADC 的通路，之后对电压进行量化的过程，全部是针对保存在 ADC 内部的电压进行的
//! NOTE: 这样做的好处是，ADC 在执行电压量化的过程中，ADC 量化的电压是固定的，防止 GPIO 输入的电压不断变化带来错误。
//! 3. 启用 ADC 内部的一个 DAC（数字到模拟转换器），通过不断修改 DAC 的值修改 DAC 输出的电压，最终找到一个最近似的值
//! NOTE1: DAC 并非使用 PWM 的形式输出等效电压，而是在内部使用了一个被称为 R-2R 梯形网络（R-2R Ladder Network）的电阻电路来，稳定的输出一个确定的电压（理论上来说，DAC 可以输出离散的模拟信号）
//! NOTE2：DAC 量化 ADC 采样到的电压，是通过二分法进行的，因此 STM32 的 ADC 才被称为 逐次逼近型（successive approximation）ADC
//! IMPORTANT：正是因为量化过程是二分逼近的，导致 ADC 转换电压需要花费多个 ADC 时钟周期
//! NOTE3：DAC 的 R-2R 梯形电阻网络的精度，决定了 ADC 的量化精度，也就是 ADC 的分辨率（resolution）指标的由来
//! 4. DAC 的值就作为 ADC 的值输出出去
//! IMPORTANT：注意 ADC 输出的值仅为 DAC 的值，这个值是相对于 V_{REF-} 和 V_{REF+} 这两个电压的值，我们需要在外部手动执行一些计算，才能将 ADC 寄存器的值对应上实际的电压值
//!
//!
//! ADC 的几个重要的输入电压
//!
//! V_{DDA} 和 V_{SSA}：
//! V_{DDA} 和 V_{SSA} 分别表示模拟电路正电源电压以及负极电压（A 表示 Analog），通过 datasheet 的 Power supply scheme 图表中我们可以知道
//! V_{DDA} 和 V_{SSA} 是为芯片上处理模拟信号的元件（比如 ADC、RC、PLL）供电的电源
//! V_{DDA} 是直接与 V_{DD} 相连的，且 V_{SSA} 是直接与 V_{SS} 相连的，且 V_{DDA} 和 V_{SSA} 之间连接了大量的去耦电容，以过滤来自数字电源的噪音
//!
//! V_{REF+} 和 V_{REF-}：
//! V_{REF+} 和 V_{REF-} 分别是 ADC 模块的参考正电压和参考负电压，
//! 通过 datasheet 的 Power supply scheme 图表，
//! 我们可以知道，V_{REF-} 是直接接在 V_{SSA} 上的，V_{REF+} 则可以选择直接接在 V_{DDA} 上，或者自行选择一个电压
//! 通过 datasheet 的 ADC characteristic 表，我们可以知道若自行选择 V_{REF+} 则，V_{DDA} - V_{REF+} 必须小于 1.2 V
//!
//!
//! ADC 的时钟
//!
//! 从 Reference Manual 的 ADC clock 节我们可以知道，
//! ADC 模块有两种独立的时钟，一个用来控制所有 ADC 的采样电路的频率，一组用来控制每个 ADC 的数字接口电路的频率
//!
//! ADC 采样电路的时钟：
//! 又称为 ADCCLK
//! 从 datasheet 的 ADC characteristic 图表中我们可以知道，在 V_{DDA} 不同时，具有不同的可选频率范围
//! 当 V_{DDA} 介于 1.7 V 到 2.4 V 时，ADCCLK 的频率介于 0.6 MHz 与 18 MHz 之间，其典型值为 15 MHz
//! 当 V_{DDA} 介于 2.4 V 到 3.6 V 时，ADCCLK 的频率介于 0.6 MHz 与 36 MHz 之间，其典型值为 30 MHz
//! ADCCLK 的实际值由 APB2 外设时钟分频而来，可用的分频值为 /2 /4 /6 /8
//!
//! ADC 数字界面时钟：
//! 就等于 APB2 外设时钟，该使用用来控制 ADC 寄存器的读写频率
//!
//!
//! 综上所述，在我们的设置中，
//! V_{REF+} = V_{DDA} = V_{DD} = 3.3 V，V_{REF-} = V_{SSA} = V_{SS} = 0 V
//! ADCCLK 的取值范围为 0.6 MHz 到 36 MHz
//! 在这里，如果我们让 ADCCLK 运行在 30 MHz 上，那么 APB2 的时钟频率的为 60 MHz（ADC 预分频器处于 /2 模式）
//! 要让 APB2 到达 60 MHz，使用 PLL 就是必不可少的了，而且由于系统时钟至少得 60 MHz，Cortex 读取 FLASH 的等待周期也必须要修改了

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};
use stm32f4xx_hal::pac::{interrupt, Peripherals, NVIC};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));
static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = Peripherals::take().expect("Cannot Get Peripherals");
    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 将 ADC 的采样时钟拉到 30 MHz，必须要使用 PLL
    setup_pll();

    // 我们将选用 GPIO PA6 作为 ADC 的采样引脚
    // GPIO PA6 对应的 ADC 通道为 6 号
    setup_gpio();

    // ADC 的采样触发选择了 TIM2 的 CC2 输出
    setup_adc();

    // 设置 TIM2 的 CC2 输出
    setup_tim2();

    #[allow(clippy::empty_loop)]
    loop {}
}

fn setup_pll() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        setup_hse();

        // 这里我们让 PLL 的输入时钟是 12 MHz 的 HSE，
        // 依照 PLLM 位 的说明，HSE 经过 PLLM 后最好得到 2 MHz，因此 PLLM 设置为 /6 模式
        // 接着是 PLLN 位，经过 PLLN 输出的频率需要在 100 ~ 432 MHz 之间，这里我们取 240 MHz，因此 PLLN 的倍率为 120
        // 最后我们要获得 60 MHz 的输出，因此我们要将 PLLP 设置为 /4 模式，将 240 MHz 降低到 60 MHz
        dp.RCC.pllcfgr.modify(|_, w| {
            w.pllsrc().hse();
            unsafe {
                w.pllm().bits(6);
                w.plln().bits(120);
            }
            w.pllp().div4();
            w
        });

        // 根据 Reference Manual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
        // 在系统时钟频率小于 64 MHz 的情况下，我们可以将 PWR 寄存器的 VOC 位设置为 0x01 也就是 Scale 3 mode，来稍微降低一些功耗
        lower_voc();

        // 根据 Reference Manual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
        // 在 V_{DD} 处于 2.7 V ~ 3.6 V 之间， 30 MHz < HCLK <= 64 MHz 时，Cortex 核心读取 FLASH 时，应该额外等待 1 个周期
        adjust_flash_wait();

        // HCLK 在 60 MHz 运行，略微超过了 APB1 的最高 50 MHz 的运行频率
        // 我们给 APB1 设置一个 /2 分频，这样 APB1 的时钟为 30 MHz
        dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

        // 等待 VOC 调整完成、等待 PLL 启动完成
        dp.RCC.cr.modify(|_, w| w.pllon().on());
        while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
        while dp.RCC.cr.read().pllrdy().is_not_ready() {}

        // 等待系统时钟切换为 PLL
        dp.RCC.cfgr.modify(|_, w| w.sw().pll());
        while !dp.RCC.cfgr.read().sws().is_pll() {}
    });
}

fn setup_hse() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.cr.modify(|_, w| w.hseon().on());
        while dp.RCC.cr.read().hserdy().is_not_ready() {}
        // 这里我们没有必要切换系统时钟来源为 HSE，因为我们最终是要使用 PLL 作为时钟源的
    })
}

fn lower_voc() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());

        // 设置为 Scale 3 Mode，降低功耗
        dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b01) });
    });
}

fn adjust_flash_wait() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 除了提高 Cortex 核心的读取等待周期，我们这里还想开启指令和数据的缓存
        // 这里我们先清除一下两个缓存
        dp.FLASH.acr.modify(|_, w| {
            w.dcrst().reset();
            w.icrst().reset();
            w
        });

        // 提高读取延迟、并开启 FLASH 指令和数据的缓存，以及预取功能
        dp.FLASH.acr.modify(|_, w| {
            w.latency().ws1();
            w.dcen().enabled();
            w.icen().enabled();
            w.prften().enabled();
            w
        });
    });
}

fn setup_gpio() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

        // GPIO_PA6 ADC1_6 对应的 GPIO 引脚
        dp.GPIOA.moder.modify(|_, w| w.moder6().analog());
    });
}

fn setup_adc() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 开启 ADC1 的时钟
        dp.RCC.apb2enr.modify(|_, w| w.adc1en().enabled());

        // 将 ADCCLK 的预分频器设置为 /2 模式，将 APB2 的 60 MHz 降低为 30 MHz
        // CCR: Common Control Register
        dp.ADC_COMMON.ccr.modify(|_, w| w.adcpre().div2());

        let voltage_sampler = &dp.ADC1;

        // 配置 ADC 的量化深度为 12 bit
        // 此为默认值
        // voltage_sampler.cr1.modify(|_, w| w.res().twelve_bit());

        // 配置 ADC 的写入 DR 寄存器的对齐方式为右对齐
        // 此为默认值
        // voltage_sampler.cr2.modify(|_, w| w.align().right());

        // 将 ADC 序列的第一个位置设置为 channel 6
        // SQR3：SeQuence Register 3
        // SQ1: SeQuence 1
        voltage_sampler
            .sqr3
            .modify(|_, w| unsafe { w.sq1().bits(6) });

        // 告诉 ADC，序列的总长度为 1
        voltage_sampler.sqr1.modify(|_, w| w.l().bits(0));

        // 采样通道 6 时，让 ADC 等待 480 个 ADCCLK 周期，再进入量化过程
        // SMPR2: ADC SaMPle time Register 2
        // SMP6: channel 6 SaMPling time selection
        voltage_sampler.smpr2.modify(|_, w| w.smp6().cycles480());

        // 使用外部触发源，触发 ADC 单次采样、量化
        voltage_sampler.cr2.modify(|_, w| {
            // TIM2 的 CC2 的电平变化来触发 ADC 动作
            // EXTSEL: EXTernal event SELect for regular group
            w.extsel().tim2cc2();
            // 启用外部触发源，并监测其上升沿作为 ADC 启动的信号
            // EXTernal trigger ENable for regular channels
            w.exten().rising_edge();
            w
        });

        // 挂起转换完成的中断
        voltage_sampler.cr1.modify(|_, w| {
            // EOCIE: Interrupt enable for EOC
            // EOC 指 regular channel End Of Conversion
            w.eocie().enabled();
            w
        });

        // 挂起 ADC 在 NVIC 中的中断
        unsafe { NVIC::unmask(interrupt::ADC) };

        // 实际启用 ADC 的转换模块，此时 ADC 会等待触发信号，以开始转换
        voltage_sampler.cr2.modify(|_, w| w.adon().enabled());
    })
}

fn setup_tim2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

        // 虽然 HCLK 是运行在 60 MHz，并导致 APB1 使用了 /2 分频
        // 但 APB1 的 TIM 时钟自动 x2，因此 TIM2 的时钟还是 60 MHz
        let delay_timer = &dp.TIM2;

        // 将 TIM2 自动重载触发的频率调整为 60 MHz / 6000 / 1000 = 10 Hz
        delay_timer.psc.write(|w| w.psc().bits(6000 - 1));
        delay_timer.arr.write(|w| w.arr().bits(1000 - 1));

        delay_timer.cr1.modify(|_, w| w.arpe().enabled());

        // 并设置 TIM2 的 CC2 为等于设定值时触发模式
        let delay_ccmr1 = delay_timer.ccmr1_output();
        delay_ccmr1.modify(|_, w| {
            w.cc2s().output();
            w.oc2pe().enabled();
            w.oc2m().pwm_mode1(); // 我们这里并没有真的使用 PWM 的内容，只不过这个模式下，CC2 对应的 OC2REF 能够周期性地产生上升沿
            w
        });

        let delay_ccr2 = delay_timer.ccr2();

        // 由于是等于时产生一个上升沿，因此只要 CCR 的值能被 CNT 取到即可，不需要什么额外的设置
        // 不过由于我们使用了默认的 counting-up 模式，这个值设置为 1 时，还真的就产生了一个有且仅有 1 个 TIM2 时钟刻的高电平
        delay_ccr2.write(|w| w.ccr().bits(1));

        // 启用 TIM2 的 CC2
        delay_timer.ccer.modify(|_, w| {
            w.cc2np().clear_bit();
            w.cc2p().set_bit();
            w.cc2e().set_bit();
            w
        });

        // 最后我们启动 TIM2 的 CNT
        delay_timer.cr1.modify(|_, w| w.cen().enabled());
    });
}

#[interrupt]
fn ADC() {
    cortex_m::interrupt::free(|cs| {
        let count = G_CNT.borrow(cs).get();

        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let voltage_sampler = &dp.ADC1;

        // ADC 中断触发时，读一下 EOC 位，
        // 若设置了 EOC 位，就读取一下 DR 中存储的数值，并转换为实际的电压值
        // 若是因为其它原因触发的 ADC 中断，就 panic
        let sr = voltage_sampler.sr.read();
        if sr.eoc().is_complete() {
            voltage_sampler.sr.modify(|_, w| w.eoc().clear_bit());

            let raw_value = voltage_sampler.dr.read().data().bits();

            // 计算一下 ADC 实际测量到的电压
            let voltage_value = raw_value as f32 / (2u32.pow(12) - 1) as f32 * 3.3;

            // 实际的电压值我们取三位小数
            rprint!("\x1b[2K\r{}: {:.3} V\r", count, voltage_value);

            G_CNT.borrow(cs).set(count + 1);
        } else {
            panic!("{:b}", sr.bits());
        }
    })
}
