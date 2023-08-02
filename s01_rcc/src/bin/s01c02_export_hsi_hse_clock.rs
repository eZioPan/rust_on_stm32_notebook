//! 将 HSI HSE 的时钟信号输出到外部引脚上

//! 在 Reference Manual 的 Clock tree 图中，我们可以看见两个特别的输出端口 MCO1 和 MCO2
//! MCO 是 Microcontroller Clock Out 的缩写，也就是说，我们可以输出时钟信号到外部
//! 接着查询 Datasheet 可以发现
//! MCO1 是 GPIO PA8 的 Alternate function 00（简称 AF00，又称为 SYS_AF）
//! MCO2 是 GPIO PC9 的 Alternate function 00（简称 AF00，又称为 SYS_AF）
//! 于是，我们只要启动这两个 GPIO 口，就可以在两个 Pin 上检测到时钟信号

//! 首先，我们配置 MCO1 和 MCO2，让它们分别切换到 HSI 和 HSE
//! 接着，我们打开 GPIOA 和 GPIOC 的时钟，这两个设备是挂载在 AHB1 上的
//! 并配置 GPIO_PA8 和 GPIO_PC9 的 Alternate function 模式
//! 分别将它们的 Alternate function 切换到 00
//! 最后，我们需要分别将 GPIO_PA8 和 GPIO_PC9 切换到 Alternate function 模式，

//! 后记：
//! 在实际的操作，16 MHz 的 HSI 的波形稍稍有些尖锐，不过 FFT 显示的 16 MHz 的频率还是很明显的
//! 而 8 MHz 的 HSE 则产生了一个比较大的 24 MHz 的额外波形，波形虽然圆润，但是变形的比较厉害

#![no_main]
#![no_std]

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

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

        // 将 MCO 切换为正确的输入源
        device_peripheral.RCC.cfgr.modify(|_, w| {
            w.mco1().hsi();
            w.mco2().hse();
            w
        });

        // 启动 AHB1 上 GPIOA 和 GPIOC 的时钟
        device_peripheral.RCC.ahb1enr.modify(|_, w| {
            w.gpioaen().enabled();
            w.gpiocen().enabled();
            w
        });

        // 定位到 GPIO Port A
        let gpio_a = device_peripheral.GPIOA;
        // 并将 GPIO PA8 的 alternate function 切换到 AF00
        gpio_a.afrh.modify(|_, w| w.afrh8().af0());

        // 同理，我们还可以将 GPIO PC9 也切换到 MCO_2 模式
        let gpio_c = device_peripheral.GPIOC;
        gpio_c.afrh.modify(|_, w| w.afrh9().af0());

        // 最后将 GPIO PA8 以及 GPIO PC9 的模式切换到 alternate，
        // 开启 MCO 的输出功能
        gpio_a.moder.modify(|_, w| w.moder8().alternate());
        gpio_c.moder.modify(|_, w| w.moder9().alternate());

        rprintln!("MC1&2_READY\r");
    } else {
        rprintln!("MC1&2_NOT_OUT\r");
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
