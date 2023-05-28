#![allow(dead_code)]

use stm32f4xx_hal::pac;

pub fn setup_gpioa(dp: &pac::Peripherals) {
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

pub fn setup_gpiob(dp: &pac::Peripherals) {
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
