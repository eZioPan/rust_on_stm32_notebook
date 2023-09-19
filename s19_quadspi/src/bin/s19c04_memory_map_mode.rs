#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    pac::{CorePeripherals, Peripherals},
    prelude::*,
    qspi::{
        AddressSize, Bank1, FlashSize, Qspi, QspiConfig, QspiMemoryMappedConfig, QspiMode,
        QspiReadCommand, QspiWriteCommand,
    },
    timer::SysDelay,
};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("Program Start");

    let dp = Peripherals::take().unwrap();
    let cp = CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.use_hse(12.MHz()).hclk(48.MHz()).freeze();

    let mut delay = cp.SYST.delay(&clocks);

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();
    let gpioc = dp.GPIOC.split();

    let mut qspi = Qspi::bank1(
        dp.QUADSPI,
        (
            gpiob.pb6, gpioc.pc9, gpioc.pc10, gpioc.pc8, gpioa.pa1, gpiob.pb1,
        ),
        QspiConfig::default()
            // 这里我们用 QUADSPI 模块的分频器，将该模块的适中频率降低到 24 MHz
            // 这么做其实也没啥原因，可能是因为我手上的烧录夹的线太长了，更高的频率会导致读写失败
            .clock_prescaler(2 - 1)
            .address_size(AddressSize::Addr24Bit)
            .fifo_threshold(4)
            .flash_size(FlashSize::from_megabytes(4)),
    );

    reboot_w25q32(&mut qspi, &mut delay);
    check_w25q32_id(&mut qspi);
    enable_quad_mode(&mut qspi, &mut delay);

    enable_write(&mut qspi, &mut delay);
    qspi.indirect_write(
        QspiWriteCommand::default()
            .instruction(0x20, QspiMode::SingleChannel)
            .address(0x0, QspiMode::SingleChannel),
    )
    .unwrap();
    wait_w25q32_not_busy(&mut qspi, &mut delay);

    enable_write(&mut qspi, &mut delay);
    qspi.indirect_write(
        QspiWriteCommand::default()
            .instruction(0x32, QspiMode::SingleChannel)
            .address(0x0, QspiMode::SingleChannel)
            .data("hello, world!".as_bytes(), QspiMode::QuadChannel),
    )
    .unwrap();
    wait_w25q32_not_busy(&mut qspi, &mut delay);

    // 内存映射模式
    // 在结果上看来，当 QUADSPI 处于内存映射模式时，Cortex 核心可以像读取内存一样读取 flash 的内容
    // 实现方法应该是我们预先给出了读取 flash 的指令，然后在我们实际访问 QUADSPI 映射的空间的时候
    // AHB 或 QUADSPI 会将 AHB 地址转换为 flash 地址，然后通过我们前面指定的 flash 指令从 flash 读取相应的数据
    // 而且，为了降低再次读取数据的延迟，QUADSPI 会尝试继续读取数据，直到 FIFO 被塞满（也就是预载）
    let memory_mapped = qspi
        .memory_mapped(
            QspiMemoryMappedConfig::default()
                .instruction(0xEB, QspiMode::SingleChannel)
                .address_mode(QspiMode::QuadChannel)
                .data_mode(QspiMode::QuadChannel)
                .alternate_bytes(&[0xFF], stm32f4xx_hal::qspi::QspiMode::QuadChannel)
                .dummy_cycles(4),
        )
        .unwrap();

    // 通过 .buffer() 获得一个 &[u8]，我们可以通过索引简单地访问 flash 中的数据
    //
    // 它的实现方法也很有意思，由于我们不可能（也没有意义）真的将整个 flash 中的数据读取到 SRAM 中
    // 因此这个 &[u8]，是通过 core::slice::from_raw_paers() 这个 unsafe 函数，将 QUADSPI 映射的内存标记为 &[u8]
    // 当我们通过索引访问数据中的数据的时候，实际上是 AHB 和 QUADSPI 在帮我们（Cortex 核心）获取数据的
    let memory = memory_mapped.buffer();

    rprintln!(
        "memory map read: {}",
        core::str::from_utf8(&memory[0..13]).unwrap()
    );

    #[allow(clippy::empty_loop)]
    loop {}
}

fn reboot_w25q32(qspi: &mut Qspi<Bank1>, delay: &mut SysDelay) {
    rprintln!("reboot w25q32");
    // 这里用了一个 .and_then() 链式调用，也算是表明了 0x66 和 0x99 必须连续输入才算成功
    qspi.indirect_write(QspiWriteCommand::default().instruction(0x66, QspiMode::SingleChannel))
        .and_then(|_| {
            qspi.indirect_write(
                QspiWriteCommand::default().instruction(0x99, QspiMode::SingleChannel),
            )
        })
        .unwrap();

    delay.delay_ms(50u8);
}

// 读取 flash id，若非 W25Q32 则直接 panic
fn check_w25q32_id(qspi: &mut Qspi<Bank1>) {
    rprintln!("check flash id");

    let mut buf = [0u8; 2];

    qspi.indirect_read(
        QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
            .instruction(0x90, QspiMode::SingleChannel)
            .address(0x0, QspiMode::SingleChannel),
    )
    .unwrap();

    if (buf[0] as u16).checked_shl(8).unwrap() + buf[1] as u16 != 0xEF15 {
        panic!("Not a W25Q32 flash chip");
    }
}

// 通过轮询 W25Q32 的 SR1，等待 flash 处于空闲状态
fn wait_w25q32_not_busy(qspi: &mut Qspi<Bank1>, delay: &mut SysDelay) {
    let mut buf = [0u8; 1];
    loop {
        delay.delay_ms(1u8);
        qspi.indirect_read(
            QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
                .instruction(0x05, QspiMode::SingleChannel),
        )
        .unwrap();

        if buf[0] & 1 == 0 {
            break;
        }
    }
}

// 检查并启用 W25Q32 的 quad mode
fn enable_quad_mode(qspi: &mut Qspi<Bank1>, delay: &mut SysDelay) {
    let mut buf = [0u8; 1];
    qspi.indirect_read(
        QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
            .instruction(0x35, QspiMode::SingleChannel),
    )
    .unwrap();

    if buf[0] >> 1 & 1 == 0 {
        rprintln!("quad mode not enabled");

        // 0x50 启用对 flash 的状态寄存器的易失性写入
        qspi.indirect_write(QspiWriteCommand::default().instruction(0x50, QspiMode::SingleChannel))
            .unwrap();

        wait_w25q32_not_busy(qspi, delay);

        // 然后将 Quad Enable 位置 1
        qspi.indirect_write(
            QspiWriteCommand::default()
                .instruction(0x31, QspiMode::SingleChannel)
                .data(&[buf[0] | 0b10], QspiMode::SingleChannel),
        )
        .unwrap();

        wait_w25q32_not_busy(qspi, delay);

        // 最后再检测一下 quad enable 的状态
        qspi.indirect_read(
            QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
                .instruction(0x35, QspiMode::SingleChannel),
        )
        .unwrap();

        match buf[0] >> 1 & 1 == 1 {
            true => rprintln!("Quad mode Enabled"),
            false => panic!("Unable activate Quad mode"),
        }
    } else {
        rprintln!("quad mode already enabled");
    }
}

// 启用写入
// 准确来说，这个命令的作用是，下一个对 flash 的写类型的操作，是非易失性写入
// 这里说的“写类型的操作”包含对存储区的写入和擦除、写入状态寄存器、写入或清除安全寄存器
fn enable_write(qspi: &mut Qspi<Bank1>, delay: &mut SysDelay) {
    let mut buf = [0u8; 1];

    // 读取 SR1，来判定写使能状态
    qspi.indirect_read(
        QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
            .instruction(0x05, QspiMode::SingleChannel),
    )
    .unwrap();

    if buf[0] >> 1 == 0 {
        rprintln!("Write not enable, enabling...");

        // 通过 Write Enable 命令，让 W25Q32 开启写使能
        qspi.indirect_write(QspiWriteCommand::default().instruction(0x06, QspiMode::SingleChannel))
            .unwrap();

        wait_w25q32_not_busy(qspi, delay);

        // 开启之后，我们需要再次检测 SR1 的状态
        qspi.indirect_read(
            QspiReadCommand::new(&mut buf, QspiMode::SingleChannel)
                .instruction(0x05, QspiMode::SingleChannel),
        )
        .unwrap();

        match buf[0] >> 1 == 1 {
            true => rprintln!("Write Enabled"),
            false => panic!("Unable enable write"),
        }
    }
}
