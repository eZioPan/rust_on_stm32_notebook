#![no_std]
#![no_main]

// A0/A1/A2 RS/RW/E
// B4~B7 D4~D7

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();

    setup_gpioa(&dp);
    setup_gpiob(&dp);

    // 初始化流程
    //
    // 这里其实还缺少了一步，就是给 LCD1602 的 Vcc 断电再供电
    // 可能在 8 bit 模式下，这个问题还不明显，但在 4 bit 模式下，
    // 如果给一个已经初始化过的 ST7066U 再次执行 4 bit 的初始化，会立刻让 LCD1602 变成不可用状态
    // 需要再次进行一次初始化，才能正确运行，
    // 这种效果看起来就是：4 bit 模式下，除了上电那次一次初始化就成功，其他情况下都得重置两次，才能成功
    // 所以其实应该 Vcc 加一个三极管或 mos 管，然后每次初始化的第一步就是给 Vcc 断电再供电
    //
    // 第二个是，应该给 E 引脚一个外部的下拉电阻，因为我们可能会修改单片机的程序，在我们重置单片机的过程中，E 引脚必然是悬空的，
    // 而 LCD1602 又已经被我们初始化过了，因此 LCD1602 会随意捕获到混乱的数据，导致显示出错，因此给 E 一个外部下拉电阻，就可以避免这个问题

    delay(&cp, 40_000);

    send_4bit(&dp, 0, 0, 0b0010);
    delay(&cp, 40);

    send_8bit(&dp, 0, 0, 0b0010_1000);
    delay(&cp, 40);

    send_8bit(&dp, 0, 0, 0b0010_1000);
    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }

    send_8bit(&dp, 0, 0, 0b0000_1111);
    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }

    send_8bit(&dp, 0, 0, 0b0000_0001);
    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }

    send_8bit(&dp, 0, 0, 0b0000_0110);

    //init end

    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }
    send_8bit(&dp, 0, 0, 0b1000_0000);

    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }
    send_8bit(&dp, 1, 0, 0b0100_0001);

    while read_busy_flag(&dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(&cp, 10);
    }
    send_8bit(&dp, 1, 0, 0b0100_0010);

    loop {}
}

fn setup_gpioa(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    let gpioa = &dp.GPIOA;

    gpioa.pupdr.modify(|_, w| {
        w.pupdr0().pull_down();
        w.pupdr1().pull_down();
        w.pupdr2().pull_down();
        w
    });

    gpioa.otyper.modify(|_, w| {
        w.ot0().push_pull();
        w.ot1().push_pull();
        w.ot2().push_pull();
        w
    });

    gpioa.odr.modify(|_, w| {
        w.odr0().low();
        w.odr1().low();
        w.odr2().low();
        w
    });

    gpioa.moder.modify(|_, w| {
        w.moder0().output();
        w.moder1().output();
        w.moder2().output();
        w
    })
}

fn setup_gpiob(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioben().enabled());

    let gpiob = &dp.GPIOB;

    gpiob.pupdr.modify(|_, w| {
        w.pupdr4().pull_down();
        w.pupdr5().pull_down();
        w.pupdr6().pull_down();
        w.pupdr7().pull_down();
        w
    });

    gpiob.otyper.modify(|_, w| {
        w.ot4().push_pull();
        w.ot5().push_pull();
        w.ot6().push_pull();
        w.ot7().push_pull();
        w
    });

    gpiob.odr.modify(|_, w| {
        w.odr4().low();
        w.odr5().low();
        w.odr6().low();
        w.odr7().low();
        w
    });

    gpiob.moder.modify(|_, w| {
        w.moder4().output();
        w.moder5().output();
        w.moder6().output();
        w.moder7().output();
        w
    })
}

fn delay(cp: &pac::CorePeripherals, micro_sec: u32) {
    unsafe {
        cp.SYST.rvr.write(micro_sec);
        cp.SYST.csr.modify(|_data| 1);

        while cp.SYST.csr.read().checked_shr(16).unwrap() & 1 == 0 {}
    };
}

fn send_8bit(dp: &pac::Peripherals, rs: u8, rw: u8, data: u8) {
    send_4bit(dp, rs, rw, data.checked_shr(4).unwrap());
    send_4bit(dp, rs, rw, data & 0b1111);
}

fn send_4bit(dp: &pac::Peripherals, rs: u8, rw: u8, data: u8) {
    if data > 0b1111 {
        panic!("Data overflow, 4 bit only");
    }

    let ctrl = &dp.GPIOA;
    let dbus = &dp.GPIOB;

    ctrl.odr.modify(|_, w| w.odr2().low());

    match rs {
        0 => ctrl.odr.modify(|_, w| w.odr0().low()),
        1 => ctrl.odr.modify(|_, w| w.odr0().high()),
        _ => panic!("RS value Error"),
    }

    match rw {
        0 => ctrl.odr.modify(|_, w| w.odr1().low()),
        1 => ctrl.odr.modify(|_, w| w.odr1().high()),
        _ => panic!("RW value Error"),
    }

    dbus.odr.modify(|_, w| {
        w.odr7().bit((data.checked_shr(3).unwrap()) & 1 == 1);
        w.odr6().bit((data.checked_shr(2).unwrap()) & 1 == 1);
        w.odr5().bit((data.checked_shr(1).unwrap()) & 1 == 1);
        w.odr4().bit((data.checked_shr(0).unwrap()) & 1 == 1);
        w
    });
    ctrl.odr.modify(|_, w| w.odr2().high());

    ctrl.odr.modify(|_, w| w.odr2().low());
}

fn read_busy_flag(dp: &pac::Peripherals) -> u8 {
    let ctrl = &dp.GPIOA;
    let dbus = &dp.GPIOB;

    ctrl.odr.modify(|_, w| w.odr2().low());

    // 由于是输入，这里需要将 PB0~PB7 切换到输入模式
    dbus.moder.modify(|_, w| {
        w.moder7().input();
        w.moder6().input();
        w.moder5().input();
        w.moder4().input();
        w
    });

    ctrl.odr.modify(|_, w| {
        w.odr0().low(); //RS
        w.odr1().high(); //RW
        w
    });

    ctrl.odr.modify(|_, w| w.odr2().high());

    let state_high = dbus.idr.read().bits().checked_shr(4).unwrap() as u8;

    ctrl.odr.modify(|_, w| w.odr2().low());

    ctrl.odr.modify(|_, w| w.odr2().high());

    let state_low = dbus.idr.read().bits().checked_shr(4).unwrap() as u8;

    ctrl.odr.modify(|_, w| w.odr2().low());

    dbus.moder.modify(|_, w| {
        w.moder7().output();
        w.moder6().output();
        w.moder5().output();
        w.moder4().output();
        w
    });

    state_high.checked_shl(4).unwrap() as u8 + state_low
}
