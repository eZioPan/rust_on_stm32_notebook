use stm32f4xx_hal::pac;

pub fn delay(cp: &pac::CorePeripherals, micro_sec: u32) {
    unsafe {
        cp.SYST.rvr.write(micro_sec);
        cp.SYST.csr.modify(|_data| 1);

        while cp.SYST.csr.read().checked_shr(16).unwrap() & 1 == 0 {}
    };
}
