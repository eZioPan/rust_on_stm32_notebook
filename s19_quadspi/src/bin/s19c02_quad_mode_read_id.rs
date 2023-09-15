//! 在这个实验中，我们将通过 single mode、dual mode 和 quad mode 读取 W25Q32 的 Manufacturer ID 和 Device ID
//! 由本章节 c01 的实验结果我们知道，该实验返回的三个数据均应该为 0xEF15
//!
//! 这里需要注意的是，W25Q32 其实还有细分芯片版本，常见的有 W25Q32JV-IQ 和 W25Q32JV-IM，依照其 datasheet 的说法，IQ 版本是默认开启 quad mode 的，而 IM 版本是默认关闭 quad mode 的
//! 因此我们还需要检测 quad mode 是否被开启，如果没有开启，则还需要开启它，
//! 而开启 quad mode 则意味着我们要修改 flash 芯片的状态，那么我们还需要依照 datasheet 的要求，在写入类指令之后，跟随一些轮询指令，来检测 flash 自己是否已经完成操作
//! 注：我还真买了一些 W25Q32，虽然它们都标记为 IQ 版本，但是有些芯片默认就是没有开启 quad mode 的，而且有些芯片甚至无法开启 quad mode（感觉我买到的芯片里，有些就是有问题）
//!
//! 接线图同本章 c01 顶部的说明

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::pac::{self, Peripherals};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    let dp = Peripherals::take().unwrap();

    use_hse(&dp);
    setup_gpio(&dp);
    setup_systick(&dp);

    let rcc = &dp.RCC;
    let stk = &dp.STK;

    rcc.ahb3enr.modify(|_, w| w.qspien().disabled());
    rcc.ahb3rstr.modify(|_, w| w.qspirst().reset());
    rcc.ahb3rstr.modify(|_, w| w.qspirst().clear_bit());
    rcc.ahb3enr.modify(|_, w| w.qspien().enabled());

    let qspi = &dp.QUADSPI;

    qspi.cr.modify(|_, w| unsafe { w.prescaler().bits(24) });

    qspi.cr.modify(|_, w| w.sshift().set_bit());

    qspi.dcr.modify(|_, w| unsafe {
        w.fsize().bits(21);
        w.ckmode().set_bit();
        w
    });

    qspi.cr.modify(|_, w| w.en().set_bit());

    // 执行 0x66 0x99 的 W25Q32 重置命令
    reboot_w25q32(qspi, stk);

    // single mode 读取
    rprintln!("0x90 ID single mode");
    while qspi.sr.read().busy().bit_is_set() {}

    // 这里我们可以用另外一个思路来检测传输状态
    // 由于我们知道 0x90 指令后，flash 返回的字节数必然为 2，其必然不会填满 FIFO，且可以通过单次读取 DR 来获得所有数据
    // 因此我们可以通过检测 TCF 来确定传输是否已经完成
    //
    // 首先清理一下 TCF 位
    qspi.fcr.write(|w| w.ctcf().set_bit());

    qspi.dlr.write(|w| unsafe { w.dl().bits(2 - 1) });
    qspi.ccr.write(|w| unsafe {
        w.fmode().bits(0b01);
        w.imode().bits(0b01);
        w.admode().bits(0b01);
        w.adsize().bits(0b10);
        w.dmode().bits(0b01);
        w.instruction().bits(0x90);
        w
    });
    qspi.ar.write(|w| unsafe { w.address().bits(0x0) });

    // 之后我们等待 TCF 为被置 1，置 1 表示 QUADSPI 收发了足够数量的数据
    // 一旦我们读取到了其被置 1，就可以安心地从 DR 中读取我们要的数据了
    while qspi.sr.read().tcf().bit_is_clear() {}
    // 用完了还得清理一下 TCF 标识
    qspi.fcr.write(|w| w.ctcf().set_bit());

    // 由于我们可以保证，单次读取 DR 就读取了所有的数据，因此这个读取也没必要放在 while 循环里
    rprintln!(" {:X}", (qspi.dr.read().data().bits() as u16).swap_bytes());
    while qspi.sr.read().busy().bit_is_set() {}

    // dual mode 读取
    rprintln!("0x92 ID Dual I/O");
    while qspi.sr.read().busy().bit_is_set() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    qspi.dlr.write(|w| unsafe { w.dl().bits(2 - 1) });
    qspi.abr.write(|w| unsafe { w.alternate().bits(0xFF) });
    qspi.ccr.write(|w| unsafe {
        w.fmode().bits(0b01);
        w.imode().bits(0b01);
        w.admode().bits(0b10);
        w.adsize().bits(0b10);
        w.abmode().bits(0b10);
        w.absize().bits(0b00);
        w.dmode().bits(0b10);
        w.instruction().bits(0x92);
        w
    });
    qspi.ar.write(|w| unsafe { w.address().bits(0x0) });

    while qspi.sr.read().tcf().bit_is_clear() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    rprintln!(" {:X}", (qspi.dr.read().data().bits() as u16).swap_bytes());
    while qspi.sr.read().busy().bit_is_set() {}

    // 同最上面说的，测试 Quad Mode 是否开启，如果没有开启，则执行开启指令

    #[allow(unused_assignments)]
    let mut sr2_value = 0;

    // 0x35 读取 SR2 的状态，其中低 1 位为 Quad Enabled 位，当其为 1 时，表示 quad mode 已经启动了

    while qspi.sr.read().busy().bit_is_set() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    qspi.dlr.write(|w| unsafe { w.dl().bits(0) });
    qspi.ccr.write(|w| unsafe {
        w.fmode().bits(0b01);
        w.imode().bits(0b01);
        w.dmode().bits(0b01);
        w.instruction().bits(0x35);
        w
    });

    while qspi.sr.read().tcf().bit_is_clear() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    sr2_value = qspi.dr.read().bits();
    while qspi.sr.read().busy().bit_is_set() {}

    // 若 SR2 的 Quad Enabled 不为 1，则尝试启动 quad mode
    if sr2_value >> 1 & 1 == 0 {
        rprintln!("Quad Mode not enabled, will enable...");

        // 开启 quad 就是将 Status Register 2 的 Quad Mode 置 1
        // 这里是我们第一次遇见对 W25Q32 的写入操作
        // 除了检测 QUADSPI 的 BUSY 状态，还需要检测 flash 的 BUSY 状态

        // 在写入 Quad Mode 前，需要使用 0x50 Volatile SR Write Enable 启用 SR 的写入
        while qspi.sr.read().busy().bit_is_set() {}
        qspi.ccr.write(|w| unsafe {
            w.imode().bits(0b01);
            w.instruction().bits(0x50);
            w
        });
        while qspi.sr.read().busy().bit_is_set() {}
        // 一旦写入就检查 W25Q32 的 BUSY 位
        wait_w25q32_not_busy(qspi);

        // 最后就是通过 0x31 指令写入 Status Reigster 2
        while qspi.sr.read().busy().bit_is_set() {}
        qspi.dlr.write(|w| unsafe { w.dl().bits(0) });
        qspi.ccr.write(|w| unsafe {
            w.imode().bits(0b01);
            w.dmode().bits(0b01);
            w.instruction().bits(0x31);
            w
        });
        qspi.dr.write(|w| unsafe { w.data().bits(0b10) });
        while qspi.sr.read().busy().bit_is_set() {}
        // 一旦写入就检查 W25Q32 的 BUSY 位
        wait_w25q32_not_busy(qspi);

        // 写入后接着通过 0x35 检查 Status Register 2 的状态
        while qspi.sr.read().busy().bit_is_set() {}
        qspi.fcr.write(|w| w.ctcf().set_bit());
        qspi.dlr.write(|w| unsafe { w.dl().bits(0) });
        qspi.ccr.write(|w| unsafe {
            w.fmode().bits(0b01);
            w.imode().bits(0b01);
            w.dmode().bits(0b01);
            w.instruction().bits(0x35);
            w
        });

        while qspi.sr.read().tcf().bit_is_clear() {}
        qspi.fcr.write(|w| w.ctcf().set_bit());
        sr2_value = qspi.dr.read().bits();
        while qspi.sr.read().busy().bit_is_set() {}

        rprintln!("sr2:{:#010b}", sr2_value);

        match sr2_value >> 1 & 1 == 1 {
            true => rprintln!("Quad Mode enabled"),
            false => panic!("Quad Mode enable failed"), // 如果开启失败，直接 panic，反正后面要使用 quad mode，开启失败直接停止运行即可
        };
    } else {
        rprintln!("Quad Mode has already enabled");
    }

    rprintln!("0x94 ID Quad I/O");
    while qspi.sr.read().busy().bit_is_set() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    qspi.dlr.write(|w| unsafe { w.dl().bits(2 - 1) });
    qspi.abr.write(|w| unsafe { w.alternate().bits(0x0000) });
    qspi.ccr.write(|w| unsafe {
        w.fmode().bits(0b01);
        w.imode().bits(0b01);
        w.admode().bits(0b11);
        w.adsize().bits(0b10);
        w.abmode().bits(0b11);
        w.absize().bits(0b01);
        w.dmode().bits(0b11);
        w.dcyc().bits(8 + 2);
        w.instruction().bits(0x94);
        w
    });
    qspi.ar.write(|w| unsafe { w.address().bits(0x0) });

    while qspi.sr.read().tcf().bit_is_clear() {}
    qspi.fcr.write(|w| w.ctcf().set_bit());
    rprintln!(" {:X}", (qspi.dr.read().data().bits() as u16).swap_bytes());
    while qspi.sr.read().busy().bit_is_set() {}

    #[allow(clippy::empty_loop)]
    loop {}
}

fn reboot_w25q32(qspi: &pac::QUADSPI, stk: &pac::STK) {
    rprintln!("Reboting W25Q32");

    while qspi.sr.read().busy().bit_is_set() {}
    qspi.ccr.write(|w| unsafe {
        w.imode().bits(0b01);
        w.instruction().bits(0x66);
        w
    });

    while qspi.sr.read().busy().bit_is_set() {}
    qspi.ccr.write(|w| unsafe {
        w.imode().bits(0b01);
        w.instruction().bits(0x99);
        w
    });

    while qspi.sr.read().busy().bit_is_set() {}

    stk.ctrl.modify(|_, w| w.enable().set_bit());
    while stk.ctrl.read().countflag().bit_is_clear() {}
    stk.ctrl.modify(|_, w| {
        w.countflag().clear_bit();
        w.enable().clear_bit();
        w
    });
}

// flash 忙碌检测也比较简单，就是一直对 flash 芯片发送 0x05，并检测最低位是否为 1，
// 为 1 就表示其繁忙，那就接着轮询；否则就跳出循环
fn wait_w25q32_not_busy(qspi: &pac::QUADSPI) {
    qspi.dlr.write(|w| unsafe { w.dl().bits(0) });
    loop {
        while qspi.sr.read().busy().bit_is_set() {}
        qspi.fcr.write(|w| w.ctcf().set_bit());
        qspi.ccr.write(|w| unsafe {
            w.fmode().bits(0b01);
            w.imode().bits(0b01);
            w.dmode().bits(0b01);
            w.instruction().bits(0x05);
            w
        });

        while qspi.sr.read().tcf().bit_is_clear() {}
        qspi.fcr.write(|w| w.ctcf().set_bit());
        if qspi.dr.read().data().bits() & 1 == 0 {
            break;
        }
    }
}

fn use_hse(dp: &Peripherals) {
    let rcc = &dp.RCC;
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}
}

// 配置 quad mode 需要的 6 线 QuadSPI
fn setup_gpio(dp: &Peripherals) {
    let rcc = &dp.RCC;
    rcc.ahb1enr.modify(|_, w| {
        w.gpioaen().enabled();
        w.gpioben().enabled();
        w.gpiocen().enabled();
        w
    });

    let gpioa = &dp.GPIOA;
    gpioa.afrl.modify(|_, w| w.afrl1().af9()); // IO3 /HOLD /RESET
    gpioa.moder.modify(|_, w| w.moder1().alternate());

    let gpiob = &dp.GPIOB;
    gpiob.afrl.modify(|_, w| {
        w.afrl1().af9(); // CLK
        w.afrl6().af10(); // nCS
        w
    });
    gpiob.moder.modify(|_, w| {
        w.moder1().alternate();
        w.moder6().alternate();
        w
    });

    let gpioc = &dp.GPIOC;
    gpioc.afrh.modify(|_, w| {
        w.afrh8().af9(); // IO2 /WP
        w.afrh9().af9(); // IO0
        w.afrh10().af9(); // IO1
        w
    });
    gpioc.moder.modify(|_, w| {
        w.moder8().alternate();
        w.moder9().alternate();
        w.moder10().alternate();
        w
    });
}

fn setup_systick(dp: &Peripherals) {
    let systick = &dp.STK;

    systick.val.reset();

    systick.load.write(|w| unsafe { w.reload().bits(75 - 1) });
}
