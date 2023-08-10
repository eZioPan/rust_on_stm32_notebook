//! WWDG
//!
//! 窗口看门狗，这里的“窗口”可以理解为，喂狗需要在特定的窗口期才能执行，在这个窗口期之外的时间喂狗和不喂狗的效果一致
//!
//! 参照 Reference Manual 的 Watchdog block diagram 图，
//!
//! WWDG 的最终效果是触发一个 RESET，它由 WWDG_CR 的 WDGA 位作为总控，只要 WDGA 不设置，就不触发 RESET
//! 然后 WWDG 的另一路触发条件，又一份为二，而且两路中任何一路满足条件，都会（与 WWDG 一道）触发 RESET（上限约束+下限约束）
//! 首先看下面一路，这里路直接连接到了 WWDG_CR 的 T 字段的 6 号位，而且这路最终是有一个取反的，
//! 也就是说，当 T 的第 6 位保持为 1，下路就不触发；这表示，要想不触发，T 字段的最小值为 0b1000000，用十六进制表示即为 0x40（下限约束）
//! 然后是上一路，上一路稍微复杂一些，看到的是这样一个表述 “Write WWDG_CR”。它的含义是，“当我们写入 WWDG_CR 的 T 字段的**时候**”，
//! 在这个时候我们要执行一个判断，这个判断为：此时，WWDG_CR 的 T 字段的值，大于 WWDG_CFR 的 W 字段的值么？如果大于则触发 RESET，否则不触发
//! 实际上这里给出的是上限约束，因为它要求 WWDG_CR 的 T 字段小于某个指定的数字，
//! 特别注意的是，这里说的 T 字段的值，指的是在我们装载新数值到 T 字段的前一刻，T 字段本来所具有的值，而非我们装载到 T 字段的新值
//! 也就是说，这个比较不会关注我们写入 T 字段的值到底有多大
//!
//! 从上面的规则中，我们可以看到，WWDG 的 WWDG_CR 的 T 字段的下限是固定的，就是 0x40，上限是我们可以调整的，在 WWDG_CFR 的 W 字段中设置
//! 然后是 WWDG 的初始计数值是我们可以调整的，因为每次轮计数的初始值，都是我们手动给出的
//! 从图表里我们可以看出，T 字段最大计数为 2^{7} - 1，用十六进制表示为 0x7F，不触发 RESET 的最小值为 0x40，
//! W 字段“有效”的最小值为 0x40，最大值为 0x7F，关于 0x40 这个最小值，我们可以这么理解，由于 W 卡的是上限，因此上限不能贴着下限，
//! 否则就没有窗口留给我们喂看门狗了
//!
//! 这样 WWDG 的核心寄存器就分析完了，剩下的还有两个部分，第一个是 WWDG 的预分频器 WDGTB，可选值有 1/2/4/8 四个分频，
//! 然后还有一个固定的分频数 4096，它总是要把 APB1 总线时钟 PCLK1 降低到原来的 1/4096
//! 你可能发现我上面对于 T 和 W 字段的描述，总是使用十六进制，这是由于 4096 这个分频数，写作十六进制，就是 0x1000，
//! 也就是说，**在十六进制下**，计算喂狗间隔，可以快速计算 T 值与 W 值的差，以及 T 值与 0x40 的差，然后乘上 WDGTB 的分频数，
//! 接着直接在这个数之后添上 3 个 0，就能得到 PCLK1 的计数，方便我们快速得到我们喂狗的频率与时间点
//!
//! 嗯，你看的没有错我说的就是得到 PCLK1 的计数，与 IWDG 不同，WWDG 是挂载在 APB1 上的，而且每次 RESET 之后，WWDG 的所有值都会被重置

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::interrupt::Mutex;
use cortex_m_rt::exception;
use rtt_target::{rprint, rprintln, rtt_init_print};
use stm32f4xx_hal::pac::Peripherals;

use panic_rtt_target as _;

// 让我们预定义几个常数，我们试着使用上面我给出的速算方法，算出所有我们需要的数据

// 这里我们胡乱设置一个不小于 0x40，不大于 0x7F 的值，作为我们的 T 字段的重载值
// 这里我们选择的是 0x70
const WWDG_RELOAD_VALUE: u8 = 0x70;

// 然后我们要设计一个 W 字段表示的上限
// 这个值应该不小于 0x40，而且为了让这个上限有意义，它也不应该大于 WWDG_RELOAD_VALUE
// 这里我们随便选一个，比如 0x50
const WWDG_UPPER_BOUND: u8 = 0x50;

// 然后我们还得挑一个预分频
// 这里我们也胡乱选一个，比如 4 分频
const WWDG_PRESCALE: u8 = 0b11;

// 接着，我们假设我们使用 SysTick 喂狗、PCLK1 = HCLK = 8 * SysTick
// 则我们可以获得 SysTick 喂狗的间隔数据
// 这里我们挑选的喂狗位置，应该介于 0x50 和 0x40 之间，比如我们选择 0x48
// 有 WWDG T 字段计数差 0x70 - 0x48 + 1
// WWDG 预分频前的计数为 (0x70 - 0x48 + 1) * 4
// 乘以 1/4096 带来的 0x1000 倍率 (0x70-0x48+1)*4*0x1000
// 最后 /8 获得 SysTick 喂狗的计数值 (0x70-0x48+1)*4*0x1000/8
const SYSTICK_FEED: u32 =
    (WWDG_RELOAD_VALUE as u32 - 0x48 + 1) * 2u32.pow(WWDG_PRESCALE as u32) * 0x1000 / 8;

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));
static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("\nProgram Start");

    let dp = Peripherals::take().unwrap();

    // 与 IWDG 一样，这里我们配置 WWDG 在我们调试的时候，暂停 WWDG 内部的计数
    dp.DBGMCU.apb1_fz.modify(|_, w| w.dbg_wwdg_stop().set_bit());

    dp.RCC.apb1enr.modify(|_, w| w.wwdgen().enabled());

    let wwdg = &dp.WWDG;

    // 设置 WWDG 分频与窗口上限
    wwdg.cfr.modify(|_, w| {
        // 为了展示我们运算没有问题，
        // 这里我故意没有使用 stm32f4 crate 提供的 variant
        w.wdgtb().bits(WWDG_PRESCALE);
        w.w().bits(WWDG_UPPER_BOUND);
        w
    });
    // 并载入一个重载值
    wwdg.cr.modify(|_, w| w.t().bits(WWDG_RELOAD_VALUE));

    // 初始化 SysTick，载入重载值，并启动 SysTick 的中断
    // 让我们可以在中断中喂狗
    let stk = &dp.STK;
    stk.val.reset();
    stk.load.write(|w| unsafe { w.reload().bits(SYSTICK_FEED) });
    stk.ctrl.modify(|_, w| w.tickint().set_bit());

    // 为了保证我们喂狗的及时性，这里先将 dp 注入 G_DP 中
    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 然后在同时启动 WWDG 和 SysTick
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let wwdg = &dp.WWDG;
        let stk = &dp.STK;

        wwdg.cr.modify(|_, w| w.wdga().enabled());
        stk.ctrl.modify(|_, w| w.enable().set_bit());
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

// 在 SysTick 中中断出发时喂狗，并计数
#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 为了安全，这里先喂狗，再计数
        let wwdg = &dp.WWDG;
        wwdg.cr.modify(|_, w| w.t().bits(WWDG_RELOAD_VALUE));

        let cnt_cell = G_CNT.borrow(cs);
        rprint!("\x1b[2K\rFeed watchdog {}", cnt_cell.get());
        cnt_cell.set(cnt_cell.get() + 1);
    })
}
