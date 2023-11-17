//! 用 DMA 实现 DAC 余弦波的输出
//!
//! DAC 要使用 DMA，需要一个触发源，触发源可选的有：部分 TIM 的 TRGO 输出，或者软件触发，或者外部引脚触发
//! 在我们的案例中，选用的是 TIM 触发，触发的流程为：
//! TIM 发生 Update Event，该 Event 通过 TRGO 传播到 DAC 模块，DAC 模块触发 DMA 请求，DMA 将数据从 Flash 中转运到 DHR 寄存器里
//!
//! 在一个余弦波需要 100 个采样点，HCLK 100 MHz TIM2 100 MHz 每 10 个 tick 触发一个 TRGO 的情况下，输出的余弦波的频率为 100 kHz

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    interrupt,
    pac::{self, NVIC},
};

mod wave_data;
use wave_data::COS_WAVE_100 as COS_WAVE;

static G_DP: Mutex<RefCell<Option<pac::Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    let dp = pac::Peripherals::take().unwrap();

    setup_rcc(&dp);
    setup_gpio(&dp);
    setup_dma(&dp);
    setup_dac(&dp);
    setup_tim(&dp);

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.DMA1.st[5].cr.modify(|_, w| w.en().enabled());
        dp.DAC.cr.modify(|_, w| w.en1().enabled());
        dp.TIM2.cr1.modify(|_, w| w.cen().enabled());
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

// 将 STM32F413 的 HCLK 拉到 100 MHz
fn setup_rcc(dp: &pac::Peripherals) {
    // 启动 HSE
    dp.RCC.cr.modify(|_, w| w.hseon().on());
    while dp.RCC.cr.read().hserdy().is_not_ready() {}

    // 设置 PLL
    dp.RCC.pllcfgr.modify(|_, w| {
        w.pllsrc().hse();
        unsafe {
            w.pllm().bits(6);
            w.plln().bits(100)
        };
        w.pllp().div2();
        w
    });

    // 提高供电电压
    dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
    dp.PWR.cr.modify(|_, w| unsafe { w.vos().bits(0b01) });

    // 启动 PLL
    dp.RCC.cr.modify(|_, w| w.pllon().on());
    while dp.PWR.csr.read().vosrdy().bit_is_clear() {}
    while dp.RCC.cr.read().pllrdy().is_not_ready() {}

    // 设置 Flash 读取延迟
    dp.FLASH.acr.modify(|_, w| {
        w.latency().ws3();
        w.dcen().enabled();
        w.icen().enabled();
        w.prften().enabled();
        w
    });

    // 配置 APB 分频
    dp.RCC.cfgr.modify(|_, w| w.ppre1().div2());

    // 切换系统时钟源
    dp.RCC.cfgr.modify(|_, w| w.sw().pll());
    while !dp.RCC.cfgr.read().sws().is_pll() {}
}

// 将 GPIO PA4 切换到 analog
fn setup_gpio(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    dp.GPIOA.moder.modify(|_, w| w.moder4().analog());
}

// 设置 DMA
// 查询 DMA request mapping 可知，DAC 的 channel 1 发出的 dma request 处于 DMA1 的 Stream 5 Channel 7 上
fn setup_dma(dp: &pac::Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.dma1en().enabled());

    let dma1 = &dp.DMA1;

    let dma1_st5 = &dma1.st[5];

    if dma1_st5.cr.read().en().is_enabled() {
        dma1_st5.cr.modify(|_, w| w.en().disabled());
        while dma1_st5.cr.read().en().is_enabled() {}
    }

    dma1_st5.cr.modify(|_, w| {
        // 将 Stream 5 切换到 Channel 7
        w.chsel().bits(7);
        // 运输方向为内存到外设模式
        w.dir().memory_to_peripheral();
        // 由于我们要不断输出余弦波，因此要启动循环模式
        w.circ().enabled();
        // 然后我们将 memory 端的单次读取长度设置到 16 bit
        w.msize().bits16();
        // 按照 FIFO threshold configurations 表，当我们以 16 bit（也就是 Half-word）读取，
        // FIFO 全开的时候，MBURST 可以给到 INCR8
        w.mburst().incr8();
        // 内存端的访问是自增的，毕竟我们要顺次读取整个余弦波的采样数据
        w.minc().incremented();
        // 外设端则因为我们要使用 12 bit 精度的 DAC，因此我们必须使用 16 bit 宽度的 DMA
        w.psize().bits16();
        // 外设端的地址不需要自增，因为 DHR 的地址是固定的
        w.pinc().fixed();
        w
    });

    // 启用 FIFO
    // 尽量降低 DMA 通过 AHB 访问 Flash 的次数
    // 可以略微提高最终输出的频率
    dma1_st5.fcr.modify(|_, w| {
        w.dmdis().disabled();
        w.fth().full();
        w
    });

    // 给出余弦波的实际内存地址（准确说是 AHB 地址）
    dma1_st5
        .m0ar
        .write(|w| unsafe { w.bits(&COS_WAVE as *const _ as u32) });

    // 给出 DHR 的实际内存地址（准确说是 AHB 地址）
    dma1_st5
        .par
        .write(|w| unsafe { w.pa().bits(dp.DAC.dhr12r1.as_ptr() as u32) });

    // 查询 Packing/unpacking and endian behavior 表可知，在内存端 32 bit，外设端 16 bit 的情况下
    // 转移计数是按照外设端计算的，因此这里我们给出的是 COS_WAVE 列表的长度（COS_WAVE 就是 u16 存储的）
    dma1_st5.ndtr.write(|w| w.ndt().bits(COS_WAVE.len() as u16));

    // 清理相关的错误中断标志
    dma1.hifcr.write(|w| {
        w.cteif5().clear();
        w.cfeif5().clear();
        w
    });

    // 开启传输错误和 FIFO 错误中断，但不要开启 Half Transfer 和 Transfer Complete 的中断
    dma1_st5.cr.modify(|_, w| w.teie().enabled());
    dma1_st5.fcr.modify(|_, w| w.feie().enabled());

    // 开启 DMA1 STREAM5 的中断
    unsafe { NVIC::unmask(interrupt::DMA1_STREAM5) }

    // 注意，这里没有实际开启 DMA1，我们会最后统一开启
}

// 配置 DAC
// 主要是配置 DAC 的触发，以及发送 DMA 请求的部分
fn setup_dac(dp: &pac::Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.dacen().enabled());

    let dac = &dp.DAC;

    dac.cr.modify(|_, w| {
        // 让 TIM2 发出的 TRGO 作为触发 DHR 转运到 DOR 的条件
        w.tsel1().tim2_trgo();
        // 让 DAC 接受触发
        w.ten1().enabled();
        // DAC 被 TRGO 触发的时候，顺便触发 DMA 请求
        w.dmaen1().enabled();
        // 如果 DAC 检测到了错误（比如 DMA 运行的速度不够），就挂起中断标识符
        w.dmaudrie1().enabled();
        w
    });

    // 开启 DAC1 的中断
    unsafe { NVIC::unmask(interrupt::TIM6_GLB_IT_DAC1_DAC2) }

    // 同样的，这里也没有启动 DAC 的输出，后面会统一开启
}

fn setup_tim(dp: &pac::Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    let tim2 = &dp.TIM2;

    // TIM 的主框图里没有指出 MMS 的用法，不过在 Master/Slave timer example 图中指出 UEV（Update Event）是可以触发 TRGO 的
    tim2.cr2.modify(|_, w| w.mms().update());

    // 为了防止 DMA 转运不过来，导致 DAC 触发中断，该值实测的最小值为 10
    tim2.arr.write(|w| w.arr().bits(10 - 1));
}

// 中断处理，主要是打印出现的错误
#[interrupt]
fn TIM6_GLB_IT_DAC1_DAC2() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();
        dp.DAC.sr.modify(|_, w| w.dmaudr1().no_underrun());
    });
    rprintln!("DMA under-run");
}

// 中断处理，主要是打印出现的错误
#[interrupt]
fn DMA1_STREAM5() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let dma1 = &dp.DMA1;

        let dma1_hisr_reader = dma1.hisr.read();

        if dma1_hisr_reader.teif5().is_error() {
            dma1.hifcr.write(|w| w.cteif5().clear());
            rprintln!("DMA Transfer error");
        }

        if dma1_hisr_reader.feif5().is_error() {
            dma1.hifcr.write(|w| w.cfeif5().clear());
            rprintln!("DMA FIFO error");
        }
    });
}
