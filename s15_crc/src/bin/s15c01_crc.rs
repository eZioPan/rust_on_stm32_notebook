//! CRC
//!
//! 循环冗余校验，一种简易的校验机制
//!
//! STM32 硬件上内置了一种 CRC32 的校验算法
//!
//! 需要注意的是 STM32 实现的 CRC32 版本，通常被称为 CRC-32/MPEG-2 其特点为
//! 宽度 Width 为 32bit
//! 权值 Poly 为 0x04C11DB7
//! 初值 Init 为 0xFFFFFFFF
//! 异或值 XorOut 为 0x00000000
//! 无输入反射 RefIn 为 false，无输出反射 RefOut 为 false
//!
//! 且在连续计算中，上一个 CRC32 的输出值，会参与下一个 CRC32 计算，因此，在计算某一个单值的 CRC32 值时，需要将 CRC 模块的数据重置为 0xFFFFFFFF
//!
//! 如果你想使用网络上的各种 CRC32 计算器校验 STM32 CRC 模块的结果，除了要选择对应的 CRC32 算法之外，还有一点需要注意
//! 那就是你给出的原始值，必须是 32 bit 的，比如 0x1，则必须写成 0x00000001，不可以简写为 0x1，因为 8 位的 0x1 和 32 位的 0x00000001 的 CRC32 值并不相同

#![no_std]
#![no_main]

use cortex_m::asm;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::pac::Peripherals;

const SOURCE_NUMBER: u32 = 0x1;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = Peripherals::take().unwrap();

    // 注意 CRC 模块是挂载在 AHB1 上的
    dp.RCC.ahb1enr.modify(|_, w| w.crcen().enabled());

    let crc = &dp.CRC;

    // 在每次计算前，需要重置 CRC 模块的计算单元，并将 DR 寄存器的值设置为 0xFFFFFFFF
    crc.cr.write(|w| w.reset().reset());

    // 然后我们向 DR 寄存器写入我们想要计算 CRC 的原始数值
    crc.dr.write(|w| w.dr().bits(SOURCE_NUMBER));

    // 依照 Reference Manual 的说明，等待 4 个 AHB 时钟周期
    asm::delay(4);

    // 然后从 DR 中读取一下结果
    rprintln!("{:#10X}", crc.dr.read().dr().bits());

    // 最后我们可以再重置一下 CRC 模块的计算单元
    crc.cr.write(|w| w.reset().reset());

    #[allow(clippy::empty_loop)]
    loop {}
}
