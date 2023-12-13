//! bit-banding
//!
//! bit-banding 是 Cortex-M4 提供的一种访问地址数据的方式，使用它的效果为，读取/写入该地址（usize 大小）等价于读取/写入对应的 bit
//! Cortex-M4将 SRAM 地址和外设地址的高位部分配置为 bit-banding 模式，这样我们就可以方便地读取或修改某一位了
//!
//! 注意，bit-banding 的信息是记录在 PM0214 里的，具体请搜索 Bit-banding 字样，其中提供了转换公式
//!
//! 当前目标：
//! 通过修改 bit-banding 位置的内存，将 GPIO PA15 的 LED 灯点亮

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();

    // 为了简洁明了，这里的基础配置依旧使用 PAC 来实现

    // 启动 GPIOA 外设
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    // 将 GPIO PA15 的输出模式配置为推挽模式
    dp.GPIOA.otyper.modify(|_, w| w.ot15().push_pull());
    // 将 GPUIO PA15 的输出电平修改为低电平
    dp.GPIOA.odr.modify(|_, w| w.odr15().low());
    // 确实地将 GPIO PA15 切换到输出模式，确实地输出我们指定的电平
    dp.GPIOA.moder.modify(|_, w| w.moder15().output());

    // 此处开始，我们要通过 bit-banding 的方式，访问内存，
    // 将 GPIO PA15 的输出电平切换到高电平

    // 确定外设基地址
    let perihperal_start_addr = 0x4000_0000;
    // 计算 GPIOA 的 ODR 寄存器的偏移地址
    let gpioa_odr_offset = dp.GPIOA.odr.as_ptr() as usize - perihperal_start_addr;
    // 确定 ODR15 这个 bit 在 GPIOA ODR 寄存器的偏移量
    let bit_offset = 15usize;
    // 依照 PM0214，确定外设的 bit_banding 的起始地址
    let bit_banding_start_addr = 0x4200_0000;
    // 依照 PM0214 提供的公式，计算 GPIOA ODR15 的 bit-banding 地址
    let bit_band_addr = bit_banding_start_addr + gpioa_odr_offset * 32 + bit_offset * 4;
    // 对该地址写入 1，开启 GPIO PA15 的输出
    unsafe { (bit_band_addr as *mut usize).write_volatile(1) };

    #[allow(clippy::empty_loop)]
    loop {}
}
