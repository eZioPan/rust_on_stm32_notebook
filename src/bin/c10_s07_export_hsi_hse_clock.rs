//! 将 HSI HSE 的时钟信号输出到外部引脚上
//!
//! 在 Reference Manual 的 Clock tree 图中，我们可以看见两个特别的输出端口 MCO1 和 MCO2
//! MCO 是 Microcontroller Clock Out 的缩写，也就是说，我们可以输出时钟信号到外部
//! 接着查询 Datasheet 可以发现
//! MCO1 是 GPIO_A8 的 Alternate function 00（简称 AF00，又称为 SYS_AF）
//! MCO2 是 GPIO_C9 的 Alternate function 00（简称 AF00，又称为 SYS_AF）
//! 于是，我们只要启动这两个 GPIO 口，就可以在两个 Pin 上检测到时钟信号
//!
//! 首先，我们打开 GPIOA 和 GPIOC 的时钟，这两个设备是挂载在 AHB1 上的
//! 之后，我们需要分别将 GPIO_A8 和 GPIO_C9 切换到 Alternate function 模式，
//! 并分别将它们的 Alternate function 切换到 00
//! 最后，我们还需要将 MCO1 和 MCO2 分别配置为输出 HSI 和 HSE
//!
//! 后记：
//! 在实际的操作，16 MHz 的 HSI 的波形稍稍有些尖锐，不过 FFT 显示的 16 MHz 的频率还是很明显的
//! 而 8 MHz 的 HSE 则产生了一个比较大的 24 MHz 的额外波形，波形虽然圆润，但是变形的比较厉害

#![no_main]
#![no_std]

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal as hal;

use hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let Some(device_peripheral) = pac::Peripherals::take() {
        let mut wait_cnt: u32 = 1;

        // 启用 HSE
        device_peripheral.RCC.cr.modify(|_, w| w.hseon().on());

        // 等待 HSE 稳定
        while device_peripheral.RCC.cr.read().hserdy().is_not_ready() {
            wait_cnt += 1;
        }

        rprintln!("HSE READY, wait loop: {}\r", wait_cnt);

        // 启动 AHB1 上 GPIOA 和 GPIOC 的时钟
        device_peripheral.RCC.ahb1enr.modify(|_, w| {
            w.gpioaen().enabled();
            w.gpiocen().enabled();
            w
        });

        // 定位到 GPIO_A
        let gpio_a = device_peripheral.GPIOA;
        // 把 GPIOP_A8 的输出模式改到 alternate function
        gpio_a.moder.modify(|_, w| w.moder8().alternate());
        // 并将 GPIO_A8 的 alternate function 切换到 AF00
        gpio_a.afrh.modify(|_, w| w.afrh8().af0());

        // 同理，我们还可以将 GPIO_PC9 也切换到 MCO_2 模式
        let gpio_c = device_peripheral.GPIOC;
        gpio_c.moder.modify(|_, w| w.moder9().alternate());
        gpio_c.afrh.modify(|_, w| w.afrh9().af0());

        // 最后，选择 MCO 正确的输出源
        device_peripheral.RCC.cfgr.modify(|_, w| {
            w.mco1().hsi();
            w.mco2().hse();
            w
        });
        rprintln!("MC1&2_READY\r");
    } else {
        rprintln!("MC1&2_NOT_OUT\r");
    }

    loop {}
}
