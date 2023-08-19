//! IWDG
//!
//! 独立看门狗，一种超时就会触发 RESET 的机制，这里的超时指的是我们需要周期性的设置 IWDG 中的某个寄存器的某个字段，
//! 如果我们没能在周期中成功设置那个特定的寄存器，就算是超时了，就会触发 RESET
//!
//! IWDG 需要一个时钟来工作（废话……），这个时钟并非我们常见的 HSE 或 HSI，也不是 LSE，而是 LSI
//! 使用 LSI 的好处是，它的工作频率是比较低的（STM32F412 上的为 32kHz），这样它就比较容易计算出较长的时间间隔，
//! 而且它并不十分耗电，因此在低功耗模式下也能运行，而且不需要外接晶振，因此 LSI 总是处于可用的状态
//! 不过使用 LSI 也有坏处，那就是它其实不太准确，在我的测试中
//! 在室温大致是 26~28 摄氏度的情况下，我手上的 STM32F412 的 LSI 的 1 秒大概比晶振 HSE 提供的 1 秒多出 16% 来，
//! 的确是不太准确的
//!
//! 好了 LSI 就介绍到这里，我们接着说 IWDG
//! 由于 IWDG 是使用 LSI 的，因此 IWDG 也有能力在 MCU 低功耗模式下保持功能的特性
//! 因此其常用于一定要独立于其它设备独立“看门”的情况中
//! 而且 IWDG 还有一个特点，那就是，它也得输入“密钥”才能执行操作
//! 在配置 IWDG 时，需要输入一个“密钥”来解锁寄存器的写访问
//! 启动 IWDG 需要输入另一个“密钥”，而且重载计数器的操作，也是要输入一个“密钥”来进行的

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::interrupt::Mutex;
use cortex_m_rt::exception;
use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};

use stm32f4xx_hal::pac::Peripherals;

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));
static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("\nStart Program");

    let dp = Peripherals::take().unwrap();

    let rcc = &dp.RCC;

    // 开启 HSE 作为主时钟源
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}

    // 预先打开 LSI
    // 其实使用 IWDG 不需要预先打开 LSI
    // 我们这里为了避免 LSI 启动对于 IWDG 的计时延迟影响
    // 这里预先打开，并等待其稳定
    // 需要注意的是，LSI 在 RCC 的 CSR 寄存器里控制，而非在 CR 寄存器里控制
    rcc.csr.modify(|_, w| w.lsion().on());
    while rcc.csr.read().lsirdy().is_not_ready() {}

    // 设置 SysTick
    let stk = &dp.STK;
    stk.val.reset();
    // 这里我给出的是 1_000_000 个 Tick 触发中断
    // 不过这里也也可以额外的增加一些 tick，看看 IWDG 的超时极限在什么地方
    // 就可以间接推导出 LSI 的偏移量了
    stk.load.modify(|_, w| unsafe { w.reload().bits(999_999) });
    stk.ctrl.modify(|_, w| w.tickint().set_bit());

    // 先开启 Cortex 核心 halt 的时候，暂停 IWDG 的计数
    dp.DBGMCU.apb1_fz.modify(|_, w| w.dbg_iwdg_stop().set_bit());

    let iwdg = &dp.IWDG;

    // 解锁 IWDG 相关寄存器的写锁定
    // KR 是 Key 的缩写
    iwdg.kr.write(|w| w.key().enable());

    // 配置对 SLI 的分频，这里选择了 16 分频，也就是 IWDG 的计数频率为 32 kHz/ 16 = 2 kHz
    iwdg.pr.write(|w| w.pr().divide_by16());
    // 然后我们可以将重载器的值设置为 1999
    // 这样我们喂看门狗的周期就可以固定在 1 秒（左右）
    // 另，RLR 最大值仅为 0xFFF 也就是 4095
    iwdg.rlr.write(|w| w.rl().bits(1999));

    // 为了保证计时的准确性
    // 我们这里先注入 dp 到全局静态量中
    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 在这里我们连续打开 IWDG 的计数和 SysTick 的计数
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let iwdg = &dp.IWDG;
        // 启动 IWDG 也是向 KR 寄存器写入密钥
        iwdg.kr.write(|w| w.key().start());

        let stk = &dp.STK;
        stk.ctrl.modify(|_, w| w.enable().set_bit());
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let iwdg = &dp.IWDG;
        // 在中断中喂看门狗，也是向 KR 寄存器写入密钥
        iwdg.kr.write(|w| w.key().reset());

        let cnt_cell = G_CNT.borrow(cs);
        rprint!("\x1b[2K\rFeed IWDG: {}", cnt_cell.get());
        cnt_cell.set(cnt_cell.get() + 1);
    });
}
