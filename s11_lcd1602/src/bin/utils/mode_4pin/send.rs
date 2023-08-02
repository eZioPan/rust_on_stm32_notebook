#![allow(dead_code)]

use stm32f4xx_hal::pac;

use super::super::common::delay;

pub fn send_8bit(dp: &pac::Peripherals, rs: u8, rw: u8, data: u8) {
    send_4bit(dp, rs, rw, data.checked_shr(4).unwrap());
    send_4bit(dp, rs, rw, data & 0b1111);
}

pub fn send_4bit(dp: &pac::Peripherals, rs: u8, rw: u8, data: u8) {
    assert!(data < 2u8.pow(4), "Data overflow, 4 bit only");

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

pub fn read_busy_flag(dp: &pac::Peripherals) -> u8 {
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

    state_high.checked_shl(4).unwrap() + state_low
}

pub fn wait_for_idle(dp: &pac::Peripherals, cp: &pac::CorePeripherals, poll_interval_ms: u32) {
    while read_busy_flag(dp).checked_shr(7).unwrap() & 1 == 1 {
        delay(cp, poll_interval_ms);
    }
}

pub fn wait_and_send_8bit(
    dp: &pac::Peripherals,
    cp: &pac::CorePeripherals,
    rs: u8,
    rw: u8,
    data: u8,
    poll_interval_ms: u32,
) {
    wait_for_idle(dp, cp, poll_interval_ms);
    send_8bit(dp, rs, rw, data);
}

pub fn wait_and_send_4bit(
    dp: &pac::Peripherals,
    cp: &pac::CorePeripherals,
    rs: u8,
    rw: u8,
    data: u8,
    poll_interval_ms: u32,
) {
    wait_for_idle(dp, cp, poll_interval_ms);
    send_4bit(dp, rs, rw, data);
}
