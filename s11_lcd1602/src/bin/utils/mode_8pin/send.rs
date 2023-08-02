#![allow(dead_code)]

use stm32f4xx_hal::pac;

use super::super::common::delay;

pub fn send(dp: &pac::Peripherals, rs: u8, rw: u8, data: u8) {
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
        w.odr7().bit((data.checked_shr(7).unwrap()) & 1 == 1);
        w.odr6().bit((data.checked_shr(6).unwrap()) & 1 == 1);
        w.odr5().bit((data.checked_shr(5).unwrap()) & 1 == 1);
        w.odr4().bit((data.checked_shr(4).unwrap()) & 1 == 1);
        w.odr3().bit((data.checked_shr(3).unwrap()) & 1 == 1);
        w.odr2().bit((data.checked_shr(2).unwrap()) & 1 == 1);
        w.odr1().bit((data.checked_shr(1).unwrap()) & 1 == 1);
        w.odr0().bit((data.checked_shr(0).unwrap()) & 1 == 1);

        w
    });

    ctrl.odr.modify(|_, w| w.odr2().high());
    ctrl.odr.modify(|_, w| w.odr2().low());
}

pub fn read_busy_flag(dp: &pac::Peripherals) -> u8 {
    let ctrl = &dp.GPIOA;
    let dbus = &dp.GPIOB;

    ctrl.odr.modify(|_, w| w.odr2().low());

    // 由于是输入，这里需要将 PB0~PB7 切换到输入模式
    dbus.moder.modify(|_, w| {
        w.moder0().input();
        w.moder1().input();
        w.moder2().input();
        w.moder3().input();
        w.moder4().input();
        w.moder5().input();
        w.moder6().input();
        w.moder7().input();
        w
    });

    ctrl.odr.modify(|_, w| {
        w.odr0().low(); //RS
        w.odr1().high(); //RW
        w
    });

    ctrl.odr.modify(|_, w| w.odr2().high());

    let state = (dbus.idr.read().bits() & 0b11111111) as u8;

    ctrl.odr.modify(|_, w| w.odr2().low());

    dbus.moder.modify(|_, w| {
        w.moder0().output();
        w.moder1().output();
        w.moder2().output();
        w.moder3().output();
        w.moder4().output();
        w.moder5().output();
        w.moder6().output();
        w.moder7().output();
        w
    });

    state
}

pub fn wait_for_idle(dp: &pac::Peripherals, cp: &pac::CorePeripherals, poll_interval_ms: u32) {
    while read_busy_flag(dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(cp, poll_interval_ms);
    }
}

pub fn wait_and_send(
    dp: &pac::Peripherals,
    cp: &pac::CorePeripherals,
    rs: u8,
    rw: u8,
    data: u8,
    poll_interval_ms: u32,
) {
    wait_for_idle(dp, cp, poll_interval_ms);
    send(dp, rs, rw, data);
}
