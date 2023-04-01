//! SPI
//!
//! SPI 是 串行外设接口 serial peripheral interface 的简称，
//! 它是一种最多需要 4 根连线，最少需要 3 根连线的 有主从关系、同步、高速 通信协议
//! SPI 可以使用的引脚的名称和含义如下
//! SPI_SCK: SPI 时钟引脚，主机（master）和从机（slave）通过该时钟同步发送&接收数据
//! SPI_MOSI: SPI 主发从收引脚，主机通过该线向从机发送数据，从机通过该线接收来自主机的数据
//! SPI_MISO: SPI 主收从发引脚，从机通过该线向主机发送数据，主机通过该线接收来自从机的数据
//! SPI_NSS: SPI 片选引脚，当 SPI 作为主机存在时，该引脚应该被拉高，当 SPI 作为从机存在时，该引脚应该被拉低
//!          有一种做法是，主机的 SPI_NSS 引脚直接拉高，从机的 SPI_NSS 引脚接到主机的一个 GPIO 上，
//!          若主机需要与该从机通信，则直接拉低这个引脚
//!          还有一种做法是，若 SPI 仅作为单个主机和单个从机的通信手段，
//!          那么可以直接将主机的 SPI_NSS 连接至从机的 SPI_NSS，并启用主机的 SSOE: SS Output Enabled，
//!          这样主机就可以通过自己的 SPI_NSS 控制 从机的 SPI_NSS 了

//! SPI 回环测试
//! 将 SPI2 的 MISO 和 MOSI 通过导线连接在一起，并向 SPI2 发送一次数据，当数据接收完成之后，关闭 SPI2

//! 从 datasheet 上我们可以看出，STM32F411RET6 的引脚中，
//! 除了电源相关的 VCC、VCCA、VDD、VDD、VBAT、VSS、VSSA、VCAP1，以及与启动模式与重置相关的 BOOT0、NRST 之外，所有的引脚都是 GPIO
//! 这里引入了一个问题，那就是，SPI 都没有专用的引脚，是怎么和外界通信的
//!
//! 这里就不得不提到 Cortex 核心、片上外设模块和 GPIO 的关系了
//!
//! 简单来说，
//! Cortex 核心是通用计算单元，是大脑，复杂逻辑都是由它执行的
//! 片上外设模块（比如 I2C SPI USART）是“小脑”，它们可以被 Cortex 核心这个大脑配置，但实际上是**独立运行的**，可以通过总线（以及 NVIC）与 Cortex 这个大脑通信
//! 而 GPIO 则是四肢，Cortex 核心可以直接控制，也可以交由片上外设模块控制，
//! Cortex 核心直接控制引脚就是普通的 GPIO，若交给片上外设模块控制，此时 GPIO 则被称为 GPIO Alternate Function，可以简称 AF
//! 而且每个引脚能承载的 Alternate Function 是不相同的，它们的说明在 datasheet 的 Alternate function mapping 表中有所说明

#![no_std]
#![no_main]

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::pac::{self, interrupt};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let (Some(dp), Some(cp)) = (pac::Peripherals::take(), pac::CorePeripherals::take()) {
        // 首先我们要启用 SPI2 控制器的时钟
        dp.RCC.apb1enr.modify(|_, w| w.spi2en().enabled());

        dp.SPI2.cr1.modify(|_, w| {
            // 设置 SPI 为主机模式
            // 主机可以主动发起一个会话
            // MSTR: MaSTER selection
            w.mstr().master();
            // 单次发送和接收 16 bit 的数据
            // 注意在 STM32F411RE 上，SPI 的收缓存与发缓存使用同一个寄存器
            // 而 SPI 的收发又是同步的，因此该寄存器（共 32 bit）一分为二，形成了单次最大 16 bit 的收发容量
            // 还可以将缓存设置为 8 bit，这样收发就刚好为 1 个 byte
            // DFF: Data Frame Format
            w.dff().sixteen_bit();
            w
        });

        dp.SPI2.cr2.modify(|_, w| {
            // 当发送缓存为空时，触发中断
            // 发送缓存为空，表示我们可以发送数据了
            // TXEIE: TX buffer Empty Interrupt Enable
            w.txeie().not_masked();
            // 当接收缓存不为空时，触发中断
            // 接收缓存不为空，表示我们可以从接收缓存中读取数据了
            // RXNEIE: RX buffer Not Empty Interrupt Enable
            w.rxneie().not_masked();
            // 这里有一点要注意，无论是 TXE 还是 RXNE，
            // 到达 NVIC 后，对应的处理函数就只有一个名为 SPI2 的函数
            // 因此需要在处理函数中判定是那个寄存器触发的
            w
        });

        // 启用 SPI2 的中断
        unsafe {
            cp.NVIC.iser[1].modify(|d| d | 1 << (36 - 32));
        };

        // 查找 Alternate function mapping 表，
        // 发现 GPIO PB12 至 PB15 可以作为 SPI2 的通信引脚
        dp.RCC.ahb1enr.modify(|_, w| w.gpioben().enabled());

        // 将 PB12 至 PB15 配置为 Alternate function 5
        // 引脚有如下对应关系
        // PB12 SPI2_NSS
        // PB13 SPI2_SCK
        // PB14 SPI2_MISO
        // PB15 SPI2_MOSI
        dp.GPIOB.afrh.modify(|_, w| {
            w.afrh12().af5();
            w.afrh13().af5();
            w.afrh14().af5();
            w.afrh15().af5();
            w
        });

        // 并将 SPI2_NSS 对应的引脚 PB12 拉高
        // 防止 SPI2 进入 slave 模式
        dp.GPIOB.pupdr.modify(|_, w| w.pupdr12().pull_up());

        // 最后开启 GPIO 的 Alternate 模式
        // 注意，GPIO 模式的切换必须先于 SPI 的正式启动
        dp.GPIOB.moder.modify(|_, w| {
            w.moder12().alternate();
            w.moder13().alternate();
            w.moder14().alternate();
            w.moder15().alternate();
            w
        });

        // 在配置好 SPI、并启动所需的 GPIO 之后
        // 启动 SPI2
        // SPE: SPI Enabled
        dp.SPI2.cr1.modify(|_, w| w.spe().enabled());
    }
    loop {
        // 如果不处于 debug 编译模式，则启用 WFI: Wait For Interrupt
        // 在该模式下，CPU 计算单元的时钟会被挂起，直到发生了中断，才会启动 CPU 时钟，并处理中断
        // 不过 debug 模式不能用，由于 CPU 计算单元的时钟被挂起，导致 DAPLink 无法正常与本芯片通信
        #[cfg(not(debug_assertions))]
        cortex_m::asm::wfi()
    }
}

// SPI2 中断处理
// 需要做的为
// 检查中断来源，首先判定是否接收非空，
// 1. 接收非空，则读取数据寄存器 DR 中的数据，并打印出来
//    之后等待 SPI2 停工（BSY 位置低），并关闭 SPI2、关闭 NVIC 中对应的中断、最后关闭 GPIOB
// 2. 接收为空，则判断发送是否为空，
// 2.1 若发送为空，则直将准备好的数据写入数据寄存器中，准备好发送
// 2.2 若发送不为空，则表示当前的中断即不是由 RXNE 触发，也不是由 TXE 触发
//     此时，打印 SPI1 状态寄存器的内容至 panic

#[interrupt]
fn SPI2() {
    unsafe {
        let dp = pac::Peripherals::steal();
        let cp = pac::CorePeripherals::steal();

        // 接收代码
        if dp.SPI2.sr.read().rxne().is_not_empty() {
            // 打印接收的数据
            rprintln!("Recieved:  {:X}\r", dp.SPI2.dr.read().dr().bits());
            // 等待 SPI2 停工
            while dp.SPI2.sr.read().bsy().is_busy() {}
            // 关闭 SPI2 时钟
            dp.RCC.apb1enr.modify(|_, w| w.spi2en().disabled());
            // 掩蔽 NVIC 中 SPI2 的中断
            cp.NVIC.icer[1].modify(|d| d | 1 << (36 - 32));
            // 关闭 GPIOB 的时钟
            dp.RCC.ahb1enr.modify(|_, w| w.gpioben().disabled());
            return;
        }

        // 发送代码
        if dp.SPI2.sr.read().txe().is_empty() {
            // 打印要发送的字节
            rprintln!("Will Send: FFAA\r");
            // 将要发送的字节写入发送缓存中
            dp.SPI2.dr.modify(|_, w| w.dr().bits(0xFFAA));
            return;
        }

        // 奇怪的 SPI2 中断触发缘由，产生 panic
        unreachable!(
            "SPI2 interrupt Unhandled, should not reach here, SPI2.SR code: {}\r",
            dp.SPI2.sr.read().bits()
        );
    }
}
