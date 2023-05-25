use stm32f4xx_hal::pac::Peripherals;

pub fn setup(dp: &Peripherals) {
    setup_hse(dp);

    // 这里我们让 PLL 的输入时钟是 8 MHz 的 HSE，
    // 依照 PLLM 位 的说明，HSE 经过 PLLM 后最好得到 2 MHz，因此 PLLM 设置为 /4 模式
    // 接着是 PLLN 位，经过 PLLN 输出的频率需要在 100 ~ 432 MHz 之间，这里我们取 256 MHz，因此 PLLN 的倍率为 128
    // 最后我们要获得 64 MHz 的输出，因此我们要将 PLLP 设置为 /4 模式，将 256 MHz 降低到 64 MHz
    dp.RCC.pllcfgr.modify(|_, w| {
        w.pllsrc().hse();
        unsafe {
            w.pllm().bits(4);
            w.plln().bits(128);
            w.pllp().div4();
        }
        w
    });

    // 根据 Reference Manual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
    // 在系统时钟频率等于大于 64 MHz 的情况下，我们可以将 PWR 寄存器的 VOS 位设置为 0x11 也就是 Power Scale2
    adjust_vos(dp);

    // 根据 Reference Manual 中 Relation between CPU clock frequency and Flash memory read time 节的说明，
    // 在 V_{DD} 处于 2.7 V ~ 3.6 V 之间， 30 MHz < HCLK <= 64 MHz 时，Cortex 核心读取 FLASH 时，应该额外等待 1 个周期
    adjust_flash_wait(dp);

    // 等待 VOC 调整完成、等待 PLL 启动完成
    dp.RCC.cr.modify(|_, w| w.pllon().on());
    while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
    while dp.RCC.cr.read().pllrdy().is_not_ready() {}

    // 这里由于 HCLK 运行在 64 MHz，而 APB1 最大也就 50 MHz，因此这里还得提前将 APB1 的分频切换到 div2
    // 这样在 PLL 被设置为系统时钟之后，APB1 才能运行在 32 MHz 下正常的工作
    dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

    // 等待系统时钟切换为 PLL
    dp.RCC.cfgr.modify(|_, w| w.sw().pll());
    while !dp.RCC.cfgr.read().sws().is_pll() {}
}

fn setup_hse(dp: &Peripherals) {
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}
    // 这里我们没有必要切换系统时钟来源为 HSE，因为我们最终是要使用 PLL 作为时钟源的
}

fn adjust_vos(dp: &Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());

    // 设置为 Power Scale2，让最大 AHB 时钟可到 84 MHz
    dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b10) });
}

fn adjust_flash_wait(dp: &Peripherals) {
    // 除了提高 Cortex 核心的读取等待周期，我们这里还想开启指令和数据的缓存
    // 这里我们先清除一下两个缓存
    dp.FLASH.acr.modify(|_, w| {
        w.dcrst().reset();
        w.icrst().reset();
        w
    });

    // 提高读取延迟、并开启 FLASH 指令和数据的缓存，以及预取功能
    dp.FLASH.acr.modify(|_, w| {
        w.latency().ws1();
        w.dcen().enabled();
        w.icen().enabled();
        w.prften().enabled();
        w
    });
}
