//! RTC 时钟跨 Reset 运行
//!
//! 这里要纠正一个错误，其实我手上的核心板是外接了 32.768 kHz 的晶振的，实际上 RTC 模块是可以跨 Reset 运行的

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

use stm32f4xx_hal::pac::{self, interrupt, Peripherals, NVIC};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot Get Peripherals");
    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 关闭 RTC 输入时钟的控制模块 Backup Domain 的锁定
    // 需要注意的是，这个操作在每次上电后都需要执行，因为即便是不用设置 RTC，我们也需要清理闹钟的 Flag
    unlock_pwr_dbp();

    // 初始化 RTC 模块
    // 只要 RTC 不断电，我们是不需要在重启之后再次配置 RTC 的
    init_rtc();

    // 启用 RTC 闹钟的中断从 EXTI 到 NVIC 的通路
    enable_alarm_interrupt();

    loop {}
}

// 解开第一道锁
// 关闭 RTC 输入时钟的控制模块 Backup Domain 的锁定
//
// 注意，解 PWR DPB 的操作在每次 Reset 之后，都需要执行
// 因为每次在闹钟中断处理函数中，我们都需要清理闹钟标识位
// 而设置任何 RTC 的寄存器，都需要解锁 PWR DPB 位
fn unlock_pwr_dbp() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 启动 PWR
        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());

        // 禁用后备域的保护
        dp.PWR.cr.modify(|_, w| w.dbp().set_bit());
    })
}

// 初始化 RTC
// 仅在 RTC 模块未初始化的情况下执行初始化
// 因此 SoC 在经过 Reset 后，RTC 一般是不需要初始化的
fn init_rtc() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 第一步就是读取一下 RTC_ISR 寄存器的 INITS 位
        // 仅当该位为未经初始化的状态，才执行 RTC 的初始化
        if dp.RTC.isr.read().inits().is_not_initalized() {
            // 启用 32.768 kHz 的 LSE
            // 从 datasheet 的 block diagram 我们可以知道
            // LSE 是直接接入到 RTC/AWU/Backup Register 模块的
            // 因此，只要 V_{BAT} 引脚有供电，即便主电源 V_{DD} 掉电也是无所谓的
            dp.RCC.bdcr.modify(|_, w| w.lseon().on());
            while dp.RCC.bdcr.read().lserdy().is_not_ready() {}

            // 设置好 RTCPRE 之后，切换 RTC 时钟的输入源，并启用 RTC 模块的时钟
            dp.RCC.bdcr.modify(|_, w| {
                w.rtcsel().lse();
                w.rtcen().enabled();
                w
            });

            // 通过 “magic number”，关闭 RTC 模块中可写寄存器的写保护
            dp.RTC.wpr.write(|w| w.key().bits(0xCA));
            dp.RTC.wpr.write(|w| w.key().bits(0x53));

            // 让 RTC 进入初始化状态
            dp.RTC.isr.modify(|_, w| w.init().init_mode());
            while dp.RTC.isr.read().initf().is_not_allowed() {}

            // 将最终输出的频率降至 1 Hz
            // 32.768 kHz/(1+127)/(1+255) = 1 Hz
            dp.RTC.prer.modify(|_, w| {
                w.prediv_s().bits(255);
                w.prediv_a().bits(127);
                w
            });

            // 初始化日历日期
            // 本案例中，我们将日期设置为 2023 年 4 月 6 日
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
            dp.RTC.tr.modify(|_, w| {
                w.ht().bits(1);
                w.hu().bits(6);
                w.mnt().bits(5);
                w.mnu().bits(0);
                w.st().bits(2);
                w.su().bits(5);
                w.pm().am();
                w
            });

            // 设置 12 小时 / 24 小时 显示模式
            dp.RTC.cr.modify(|_, w| w.fmt().twenty_four_hour());

            // 结束 RTC 的初始化状态
            dp.RTC.isr.modify(|_, w| w.init().free_running_mode());

            // 配置并启用 RTC 闹钟
            // 这里我们让 RTC 的 Alarm A 每秒钟都响一下

            // 首先禁用 Alarm A 和其中断
            dp.RTC.cr.modify(|_, w| {
                w.alraie().disabled();
                w.alrae().disabled();
                w
            });

            // 等待 Alarm A 的可写状态
            while dp.RTC.isr.read().alrawf().is_update_not_allowed() {}

            // 让 闹钟 Alarm A 每秒钟都发生
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
        }
    })
}

// 启用 RTC 闹钟的中断从 EXTI 到 NVIC 的通路
// 需要注意的是，由于 RTC 是跨越 Reset 的，因此在 Reset 过程中
// Alarm A 的中断标志位就已经被拉起来了，于是在这里我们还得手动检测一下
// 如果 Alarm A 已经有 Flag，则立刻手动触发一下中断
fn enable_alarm_interrupt() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // Alarm 的中断是发送给 EXTI17 的
        // 我们首先要配置 EXTI17
        dp.RCC.apb2enr.modify(|_, w| w.syscfgen().enabled());
        // 配置上升沿检测
        dp.EXTI.rtsr.modify(|_, w| w.tr17().enabled());
        dp.EXTI.imr.modify(|_, w| w.mr17().unmasked());

        // 启用 RTC_Alarm 中断
        unsafe { NVIC::unmask(interrupt::RTC_ALARM) };

        // 如果 RTC 的 Alarm A 的中断位已经被拉起来了，
        // 就立刻手动设置一下 Pending Register，触发一下中断处理
        if dp.RTC.isr.read().alraf().bit_is_set() {
            // 注意，手动触发 EXTI 的方案是写 SWIER 寄存器
            // 而非写 PR 寄存器，写 PR 寄存器表示清理 Pending Bit
            // SWIER: SoftWare Interrupt Event Register
            dp.EXTI.swier.modify(|_, w| w.swier17().pend());
        };
    });
}

#[interrupt]
fn RTC_ALARM() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 清理拉起来的中断标志
        dp.EXTI.pr.modify(|_, w| w.pr17().clear());
        dp.RTC.isr.modify(|_, w| w.alraf().clear());

        // 等待 RTC 影子寄存器与底层寄存器同步
        while dp.RTC.isr.read().rsf().is_not_synced() {}

        // 读取日期和时间
        let tr = dp.RTC.tr.read().bits();
        let dr = dp.RTC.dr.read().bits();

        // 解析日期和时间

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
        let du = dr >> 0 & 0b1111;

        let ht = tr >> 20 & 0b11;
        let hu = tr >> 16 & 0b1111;
        let mnt = tr >> 12 & 0b111;
        let mnu = tr >> 8 & 0b1111;
        let st = tr >> 4 & 0b111;
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
