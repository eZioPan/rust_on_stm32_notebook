//! 最小 USB 设备，Request 捕获
//!
//! 在 最小 USB 设备 的源码上，修改 class 的代码，在 defmt 中打印出每个 host 发给 device 的 request 的内容
//!
//! 我这里捕获的 request、以及其解析，放在了 ./note/minimal_usb_request.adoc 文件里

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use stm32f4xx_hal::{
    otg_fs::{UsbBusType, USB},
    pac,
    prelude::*,
};
use usb_device::{
    class_prelude::*,
    prelude::{UsbDeviceBuilder, UsbVidPid},
};

struct MyUSBClass {
    iface_index: InterfaceNumber,
}

impl MyUSBClass {
    fn new<B: UsbBus>(usb_bus_alloc: &UsbBusAllocator<B>) -> Self {
        Self {
            iface_index: usb_bus_alloc.interface(),
        }
    }
}

impl<B: UsbBus> UsbClass<B> for MyUSBClass {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(self.iface_index, 0xFF, 0x00, 0x00)?;
        Ok(())
    }

    // 打印一下输入到 CONTROL IN 的 Request 的内容
    // xfer 变量包含 request 的内容，也可以操作这个 xfer 来发送反馈
    // 不过我们这里就只打印一下 request，不需要覆盖回复
    fn control_in(&mut self, xfer: ControlIn<B>) {
        defmt::info!("{:#04X}", xfer.request());
    }

    // 打印一下输入到 CONTROL OUT 的 Request 的内容
    fn control_out(&mut self, xfer: ControlOut<B>) {
        defmt::info!("{:#04X}", xfer.request());
    }
}

static mut EP_MEM: [u32; 1024] = [0u32; 1024];

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("program start");

    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(96.MHz()) // 这里我们还是将 SYSCLK 的频率设置的高一些
        .require_pll48clk()
        .freeze();

    let gpioa = dp.GPIOA.split();

    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    let usb_bus_alloc = UsbBusType::new(usb, unsafe { &mut EP_MEM });

    let mut my_usb = MyUSBClass::new(&usb_bus_alloc);

    let usb_device_builder = UsbDeviceBuilder::new(&usb_bus_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial");

    #[cfg(debug_assertions)]
    let usb_device_builder = usb_device_builder
        .self_powered(true)
        .supports_remote_wakeup(true);

    let mut usb_dev = usb_device_builder.build();

    let mut delay = cp.SYST.delay(&clocks);

    loop {
        if !usb_dev.poll(&mut [&mut my_usb]) {
            delay.delay_us(500u16);
            continue;
        }

        // 最后，我们不要每次轮询到有数据的时候都打印状态了，直接等一下就好了
        delay.delay_us(5u8);
    }
}
