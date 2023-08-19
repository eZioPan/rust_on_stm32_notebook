//! RTC 实时时钟
//!
//! RTC Real-Time Clock 实时时钟，是一种能计算真实时间和日期的时钟，
//! 这里说的真实，指的是它能以年月日（以及星期）时分秒的方式给出时间，而且能自动依照月份不同给出不一样的天数，并自动推断闰年，还能给出夏令时时间
//! 除了给出真实的时间，RTC 还能设置两个闹钟（Alarm A 以及 Alarm B），一个周期唤醒计时器（WakeUp Timer），一个时间戳（Timestamp），以及非常多其它的辅助功能
//! 而且，就硬件层面来说，RTC 还有一个非常特殊的属性，那就是
//! 一旦 RTC 被启动，除非供电电压不足（比如芯片断电）导致 RTC 模块失效，RTC 模块是不会关闭的
//! 也就是说 RTC 是**有能力**跨 Sleep Mode 甚至是跨系统 Reset 工作的。
//!
//! 需要注意的是，RTC 跨 Sleep Mode 以及 跨系统 Reset 工作，是需要满足一定的外部条件的
//! 若我们希望 RTC 真的能跨系统 Reset 运行，那么为该模块接入一个 32.768 kHz 的晶振，对应的引脚是 PC13-OSC32_IN 和 PC14-OSC32_OUT
//! 很可惜我的核心板并没有这个东西，所以目前我没法实验 RTC 跨系统 Reset 运行

//! RTC 模块的简单配置与应用
//! 在这里，我们最终需要达成的一个效果是，通过 RTT 显示一个时钟，每秒刷新一下
//! 需要注意的是，由于没有实际的外部授时机制介入，因此这里我们不能真正实现与本地时同步的效果，只是做一个 RTC 的配置演示
//!
//! RTC 模块的配置较为复杂，会牵涉 RCC 下多个寄存器以及 RTC 下多个寄存器的配置
//!
//! 下面的配置流程参考 Reference Manual 的 “RTC initialization and configuration”
//!
//! RTC 模块具有一个特殊的性质，就是它的寄存器受到两层“写保护”，因此修改 RTC 寄存器的前提是解除这两层“写保护”
//!
//! RTC 的第一层写保护来自 PWR 模块，每次系统 Reset 后，PWR_CR 寄存器的 DBP 字段都会置 0，以锁定整个 RTC 的修改权限
//! 因此要操作 RTC 首先要做的就是从 APB1 上启动 PWR 的时钟，并通过向 PWR_CR 的 DBP 字段写入 1 来关闭 RTC 模块的保护
//! 之后我们要选择 RTC 模块的输入时钟（RTCCLK），这里我们选择了 HSE（可以让 RTC 更精确一些），于是我们就要启动 HSE，并等待 HSE 稳定
//! 之后是，RTCCLK 的频率必须不大于 1 MHz，这里我们就取 1 MHz，而 HSE 为 12 MHz，因此我们还需要配置 RCC_CFGR 的 RTCPRE 字段，让 RTCCLK 降低到 1 MHz
//! 在配置好 RTCPRE 之后才能通过 RCC_BDCR 的 RTCSEL 将时钟切换到 HSE，并通过 RTCEN 启动 RTC 模块，注意，一旦启用了 RTC，那么在断电之前，RTC 都无法被关闭
//!
//! 之后就是 RTC 模块内部的寄存器配置了
//!
//! 接着我们就遇见了第二层“写保护”：RTC 模块内部的“写保护”，这层保护会让本被标记为可写的寄存器全部只读，而解开这个保护的方法则非常特殊
//! 它需要向 RTC_WPR 寄存器顺序写入下面两个字节：0xCA 以及 0x53
//! 这两个字节是人为指定的，相当于 magic number，在输入了这两个字节之后，RTC 的“全部只读”就解开了
//! 需要注意的是，这个保护状态不会随着系统 reset 而发生变化（RTC 掉电还是会重新锁上的）
//!
//! 在 RTC 寄存器可写入之后，我们是否就可以“配置” RTC 了呢？
//! 答案是否定的，因为 RTC 默认是处于自由运行模式的（free running mode），我们要让 RTC 进入初始化模式（initialization mode），才可以配置 RTC
//! 要让 RTC 进入初始化模式，需要将 RTC_ISR 的 INIT 位设置为 1，接着轮询 INITF 位，直到该位为 1，表示 RTC 进入初始化模式
//!
//! 到此为止，我们才正式进入 RTC 的配置流程
//!
//! 首先要配置的就是 RTC 的两个预分频器，PREDIV_A 异步预分频器，以及 PREDIV_S 同步预分频器
//! RTCCLK 会首先经过 异步预分频器，形成 ck_apre 频率，该频率用于触发 RTC_SSR 亚秒寄存器 倒数，而 RTC_SSR 倒数到 0 时，就触发 RTC_TR 时间寄存器 计数，进而触发 RTC_DR 日期寄存器 计数
//! 而 RTC_SSR 自动重装的值，则由 同步预分频器 的值**推导**出来，其与 异步预分频器 协同工作，让 RTC_SSR 重装的频率（ck_spre）刚好是 1 Hz
//! 而且，Reference Manual 指出，出于节能的考虑，PREDIV_A 的值应该尽量大，鉴于其只有 7 位，最大值为 127，因此这里我们设置为 124，让 异步分频器 输出的频率为 f(ck_apre) = f(RTCCLK)/(124 + 1) = 8000 Hz
//! 之后，依照这个频率，我们就需要将 PREDIV_S 同步预分频器设置为 7999，让同步预分频器 输出的频率为 f(ck_spre) = f(ck_apre) / (7999 + 1) = 1 Hz
//!
//! 接着我们要配置与日历相关的两个计时器 RTC_DR 日期寄存器 以及 RTC_TR 时间寄存器，这两个寄存器分别设置年月日星期、以及时分秒和输入时间是上午的时间/24小时制的时间，还是下午的时间
//! 所设置的时刻，将作为 RTC 自由运行的起始时刻
//!
//! 最后我们还可以设置一下 RTC 输出的时间应该是 12 小时制的还是 24 小时制的：RTC_CR 的 FMT 位就可以
//!
//! 到目前为止，RTC 的基础配置就完成了，下面我们就可以反向操作，让 RTC 自由运行
//!
//! 首先是退出 RTC 的初始化模式，将 RTC_CR 的 INIT 设置为 0（此时 RTC 就已经处于可以自由运行的状态了）
//! 接着是启用 RTC 的写保护，向 RTC_WPR 写入 0xFF（实际上只要不是 0xCA，就都能启用写保护）
//!
//! 到此为止，RTC 的基础配置就完成了
//!
//!
//! 接着我们还可以配置一下 RTC 的 Alarm A，让 Alarm A 每秒钟产生一个中断
//!
//! 第一步依旧是解开 RTC_WPR
//! 第二步是禁用 Alarm A 的中断和启用，将 RTC_CR 的 ALRAIE 和 ALRAE 这两个位置 0，并通过轮询 RTC_ISR 的 ALRAWF 来确认 Alarm A 进入了可修改模式
//! 第三步是设置闹钟应该启用的时间点，设置 RTC_ALRMAR 寄存器，这里有一个小小的特殊点，
//! 那就是在 stm32-rs 中，Alarm A 的配置寄存器和 Alarm B 的配置寄存器合并为了一个长度为 2 的数组，而数组的名称为 alarmr，因此
//! 访问 Alarm A 的配置寄存器的方法就变成了 dp.RTC.alarmr[0]
//! 接着，默认情况下，Alarm 会在指定月份、日期（或星期）、时分秒响铃，换句话，说响铃周期为年，不过这并不符合我们期望的每秒都产生一个中断的
//! 而这个寄存器还额外的提提供了 4 个掩码位，可以分别控制 月份&天数（或星期）、小时、分钟、秒数是否要参与响铃对比，
//! 由于我们希望每秒都响铃，换句话说，没有任何一位应该参与对比，因此这里我们需要将所有的掩码位全部设置为忽略对比
//! 之后就是反向操作：启用 Alarm A 和 Alarm A 的中断，并启用 RTC_WPR
//!
//! 需要注意的是，Alarm A 是通过 EXTI17 传递给 Cortex 的 NVIC 的，因此我们还需要启用 SYSCFG，并配置上升沿触发模式的 EXTI17，还得启用 NVIC 关于 RTC_ALARM 的中断处理
//!
//!
//! 最后的最后，是中断函数的书写
//!
//! 这里需要注意的是，在进入中断函数之后，
//! 1. 除了清理 EXTI17 对应的 pending bit，还要同时清理 RTC_ISR 寄存器的 ALRAF 位（“按掉闹钟”），否则不会产生下次闹钟
//! 2. 读取寄存器的时候，先读取 RTC_TR 再读取 RTC_DR，这样在访问 RTC_TR 的时候，就会锁上 RTC_DR 寄存器的值（不影响底层 RTC 运行），
//!    这样，虽然读取 RTC_TR 和 RTC_DR 需要至少两个步骤，但能保持读取到的时间点的统一性

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use stm32f4xx_hal::pac::{self, interrupt};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    if let (Some(dp), Some(cp)) = (pac::Peripherals::take(), pac::CorePeripherals::take()) {
        // 初始化 RTC 设置
        {
            // 把 12 MHz 的 HSE 拉起来
            dp.RCC.cr.modify(|_, w| w.hseon().on());
            while dp.RCC.cr.read().hserdy().is_not_ready() {}

            // 解开第一道锁
            // 关闭 RTC 输入时钟的控制模块 Backup Domain 的锁定
            // 启动 PWR
            dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
            // 设置 DBP 位为 1
            // DBP: Disable Backup domain write Protection
            dp.PWR.cr.modify(|_, w| w.dbp().set_bit());

            // 依照 Reference Manual 的说明，在使用 HSE 作为 RTC 模块的输入源的时候
            // 必须将 HSE 频率降低为 1 MHz 才能输入 RTC 模块
            // 由于我手上的外部晶振的频率是 12 MHz，因此这里 RTCPRE 的值选择 /12
            // RTCPRE: 特别用于 HSE 的预分频器分频比
            dp.RCC.cfgr.modify(|_, w| w.rtcpre().bits(8));
            // 设置好 RTCPRE 之后，切换 RTC 时钟的输入源，并启用 RTC 模块的时钟
            // 注意，这一步之后，除非 RTC 掉电，RTC 都不可能被关闭
            // BDCR: Backup Domain Control Register
            // 这里的 Backup 指的是直流电源 V_{DD} 不存在时，使用电池供电的 V_{BAT} **后备**电源模式
            // RTCSEL: RTC source SELection
            // RTCEN: RTC ENable
            dp.RCC.bdcr.modify(|_, w| {
                w.rtcsel().hse();
                w.rtcen().enabled();
                w
            });

            // 关闭 RTC 模块中可写寄存器的写保护
            // 这个是 “magic number”，人为设置的
            // WPR: Write Protection Register
            dp.RTC.wpr.write(|w| w.key().bits(0xCA));
            dp.RTC.wpr.write(|w| w.key().bits(0x53));

            // 让 RTC 进入初始化状态
            // ISR: Initialization and Status Register
            dp.RTC.isr.modify(|_, w| w.init().init_mode());
            // 等待 RTC 模式切换完成
            // INITF: Initialization Flag
            while dp.RTC.isr.read().initf().is_not_allowed() {}

            // 配置同步预分频器和异步预分频器
            // 将最终输出的频率降至 1 Hz
            // 公式如下
            // RTC 输入时钟频率 / (异步预分频器寄存器值 + 1) = 异步预分频频率
            // 异步预分频频率 / (同步预分频器寄存器值 + 1) = 同步预分频频率
            // 而 同步预分频频率 将会输入给 RTC 的日历模块，所以该频率应为 1 Hz。
            //
            // RTC 内部实际用于计时的寄存器为 RTC_SSR（RTC 亚秒寄存器），其中异步预分频频率用于驱动 RTC_SSR 倒数，
            // 而 RTC_SSR 倒数到 0 时，会将 RTC_SSR 的值重载为 同步预分频器寄存器 的值，然后执行下一次倒数循环
            // 若两个预分频器配置正确，则每当 RTC_SSR 倒数到 0，就表示过去了 1 秒
            //
            // 依照 Reference Manual 的说明，
            // 先写同步预分频器的频率，再写异步预分频器的频率，且无论是否用到了两个分频器，两个分频器寄存器的值都必须写一遍
            // 而且为了降低能耗，应该尽量让 异步分频器寄存器 的值大一些
            // PRER: PREscaler Register
            dp.RTC.prer.modify(|_, w| {
                // 设置同步预分频器寄存器的值，
                // 共 15 位，寄存器值范围 0 ~ 32767，分频范围 1 ~ 32768
                // RTC_SSR 倒数 8000 下
                w.prediv_s().bits(7999);
                // 设置异步预分频器寄存器的值，
                // 共 7 位，寄存器值范围 0 ~ 127，分频范围 1 ~ 128
                // RTC_SSR 每 125 个 RTC 时钟周期倒数一下
                w.prediv_a().bits(124);
                w
            });

            // 初始化日历日期
            // 本案例中，我们将日期设置为 2023 年 4 月 6 日
            // 注意：该寄存器使用 BCD 码进行日期编码
            // 注意：本芯片（STM32F412RET6）的 RTC 不计算日历的前两位（千年和百年）
            // DR: Date Register
            // 位名称中的 t 表示 tens，十位；u 表示 unit，个位
            dp.RTC.dr.modify(|_, w| {
                w.yt().bits(2);
                w.yu().bits(3);
                // 由于 MT 只有 1 位，svd2rs 的时候当成了 bool 值处理
                // 在本案例中，这一位为 0，所以先用 false 替代
                // issues: https://github.com/stm32-rs/stm32-rs/issues/828
                w.mt().bit(false);
                w.mu().bits(4);
                w.dt().bits(0);
                w.du().bits(6);
                unsafe {
                    // WDU: WeekDay Unit 星期
                    w.wdu().bits(4);
                }
                w
            });
            // 初始化日历时间
            // 本案例中，我们将时间设置为 16 时 50 分 25 秒
            // 注意：该寄存器使用 BCD 码进行日期编码
            dp.RTC.tr.modify(|_, w| {
                w.ht().bits(1);
                w.hu().bits(6);
                w.mnt().bits(5);
                w.mnu().bits(0);
                w.st().bits(2);
                w.su().bits(5);
                // PM 指出指定的时间是否为 12 小时制的下午
                // 若指定为 AM，则表示为上午，或者为 24 小时计时制
                w.pm().am();
                w
            });
            // 设置 12 小时 / 24 小时 显示模式
            dp.RTC.cr.modify(|_, w| w.fmt().twenty_four_hour());

            // 结束 RTC 的初始化状态
            dp.RTC.isr.modify(|_, w| w.init().free_running_mode());

            // 启用 RTC 寄存器的保护
            dp.RTC.wpr.write(|w| w.key().bits(0xFF));
        }

        // 配置并启用 RTC 闹钟
        // 这里我们让 RTC 的 Alarm A 每秒钟都响一下
        // 我们这里要启用的是 Alarm A
        {
            // 关闭 RTC 中大部分寄存器的写保护
            // 这个是 “magic number”，人为设置的
            dp.RTC.wpr.write(|w| w.key().bits(0xCA));
            dp.RTC.wpr.write(|w| w.key().bits(0x53));

            // 首先禁用 Alarm A 和其中断
            dp.RTC.cr.modify(|_, w| {
                // ALRAIE: ALaRm A Interrupt Enabled
                w.alraie().disabled();
                // ALRAE: ALaRm A Enabled
                w.alrae().disabled();
                w
            });

            // 等待 Alarm A 的可写状态
            // ALRAWF: ALaRm A Write Flag
            while dp.RTC.isr.read().alrawf().is_update_not_allowed() {}

            // 与 Reference Manual 的名称不同，
            // Alarm A 与 Alarm B 的寄存器都放在字段 alarmr 中，并被编排为数组
            // 因此访问 Alarm A 的配置寄存器就变成了 dp.RTC.alrmr[0]
            // 此处我们选择 Alarm A，并忽略遮蔽所有的位，让闹钟每秒钟都发生
            // alrmr: ALaRM Rigster
            dp.RTC.alrmr[0].modify(|_, w| {
                w.msk1().not_mask();
                w.msk2().not_mask();
                w.msk3().not_mask();
                w.msk4().not_mask();
                w
            });

            // 启用闹钟，并启用闹钟的中断
            dp.RTC.cr.modify(|_, w| {
                w.alrae().enabled();
                w.alraie().enabled();
                w
            });

            // 启用 RTC 寄存器的保护
            dp.RTC.wpr.write(|w| w.key().bits(0xFF));

            // Alarm 的中断是发送给 EXTI17 的
            // 我们首先要配置 EXTI17
            dp.RCC.apb2enr.modify(|_, w| w.syscfgen().enabled());
            dp.EXTI.rtsr.modify(|_, w| w.tr17().enabled());
            dp.EXTI.imr.modify(|_, w| w.mr17().unmasked());

            // 启用 RTC_Alarm 中断
            unsafe { cp.NVIC.iser[1].modify(|d| d | 1 << (41 - 32)) };
        }

        // 读取 RTC 的配置
        {
            // 按照 Reference Manual 的说法，在读取 RTC 输出的时间时，需要注意：
            // 1. APB1 时钟的频率必须不小于 RTC 时钟频率
            // 2. 为了保证单次读取既能获得正确时间，APB1 时钟的频率应该不小于 7 倍 RTC 时钟的频率
            // 3. 若 APB1 时钟的频率小于等于 7 倍 RTC 时钟的频率，则应该执行至少两次读取，若相邻的两次读取的值相同，才能保证读取的正确性
            //
            // 在这个案例中，我们的 RTC 时钟的频率为 1 MHz，为了方便，我们直接将 HSE 设置为 SYSCLK，默认情况下，APB1 就运行在 12 MHz 了。是大于 7 MHz 的。
            dp.RCC.cfgr.modify(|_, w| w.sw().hse());
            while !dp.RCC.cfgr.read().sws().is_hse() {}
        }
    }

    #[allow(clippy::empty_loop)]
    loop {}
}

#[interrupt]
fn RTC_ALARM() {
    cortex_m::interrupt::free(|_cs| unsafe {
        let dp = pac::Peripherals::steal();
        dp.EXTI.pr.modify(|_, w| w.pr17().clear());
        // 除了要清理 EXTI，还得清理 RTC_ISR 的 ALRAF 位
        // ALRAF: ALaRm A Flag
        dp.RTC.isr.modify(|_, w| w.alraf().clear());

        // 由于我们没有忽略 RTC 的影子寄存器（RTC_CR 的 BYPSHAD 未设置为 1）
        // 于是每次读取 RTC 计时器时，都需要等待 RTC 影子寄存器与底层寄存器同步
        // RSF: Registers Synchronization Flags
        while dp.RTC.isr.read().rsf().is_not_synced() {}

        // 读取时先读取小单位的寄存器 SSR -> TR -> DR
        // 这样 DR 会在读取 SSR 和 TR 时被锁上，直到读取了 DR 后才解锁，保持了数据的一致性
        // 另外，这里我没有选择逐个读取字段，是因为每次读取操作都会浪费一次操作的时间
        // 不如单次读取整个寄存器，然后再手动进行解析
        let tr = dp.RTC.tr.read().bits();
        let dr = dp.RTC.dr.read().bits();

        let yt = dr >> 20 & 0b1111;
        let yu = dr >> 16 & 0b1111;
        let wdu = dr >> 13 & 0b111;
        let weekday = match wdu {
            1 => "Mon",
            2 => "Tue",
            3 => "Wed",
            4 => "Thu",
            5 => "Fri",
            6 => "Sat",
            7 => "Sun",
            _ => "Err",
        };

        let mt = dr >> 12 & 0b1;
        let mu = dr >> 8 & 0b1111;

        let month = match mt * 10 + mu {
            1 => "Jan",
            2 => "Feb",
            3 => "Mar",
            4 => "Apr",
            5 => "May",
            6 => "Jun",
            7 => "Jul",
            8 => "Aug",
            9 => "Sep",
            10 => "Oct",
            11 => "Nov",
            12 => "Dec",
            _ => "Err",
        };

        let dt = dr >> 4 & 0b11;
        #[allow(clippy::identity_op)]
        let du = dr >> 0 & 0b1111;

        let ht = tr >> 20 & 0b11;
        let hu = tr >> 16 & 0b1111;
        let mnt = tr >> 12 & 0b111;
        let mnu = tr >> 8 & 0b1111;
        let st = tr >> 4 & 0b111;
        #[allow(clippy::identity_op)]
        let su = tr >> 0 & 0b1111;

        // 第二行打印结束之后通过 ESC[A 向上移动一行，然后回到行首
        // 这样就可以覆写两行的内容了
        rprint!(
            "20{}{}/{}/{}{}/{}\n\r{}{}:{}{}:{}{}\x1b[A\r",
            yt,
            yu,
            month,
            dt,
            du,
            weekday,
            ht,
            hu,
            mnt,
            mnu,
            st,
            su
        );
    });
}
