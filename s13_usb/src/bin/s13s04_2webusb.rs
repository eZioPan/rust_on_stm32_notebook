//! 实现 WebUSB 的 Device Capacibility Descriptor，以及对应的 Vendor Descriptor
//!
//! WebUSB 的 Spec 可以在 https://wicg.github.io/webusb/ 上找到

//! 实现了 WebUSB 以后，浏览器就可以通过新的 API 访问 USB 设备了

#![no_std]
#![no_main]
mod webusb_desc {
    use core::mem::size_of;

    use usb_device::{class_prelude::*, control::RequestType};

    #[repr(C)]
    struct WebUsbPlatCapDesc {
        // bLength: u8,
        // bDescType: u8,
        // bDevCapType: u8,
        b_reserved: u8,
        plat_cap_uuid: PlatCapUUID,
        // 支持的 WebUSB 协议的版本
        bcd_version: [u8; 2],
        // 要获得 WebUSB 相关的 Vendor Desc，应该请求的 bRequest 号
        b_vendor_code: u8,
        // 支持 WebUSB 的 device，在设备插入后，可以告知用户，一个开始使用设备的网址
        // 这里就是 Host 的 USB 栈，向设备请求该网址的地址时，所应该输入的 Value 值
        i_landing_page: u8,
    }

    #[repr(C)]
    struct PlatCapUUID {
        g0: [u8; 4],
        g1: [u8; 2],
        g2: [u8; 2],
        g3: [u8; 2],
        g4: [u8; 6],
    }

    const WEBUSB_PLAT_CAP_DESC: WebUsbPlatCapDesc = WebUsbPlatCapDesc {
        b_reserved: 0x00,
        // WebUSB 特殊的 UUID
        plat_cap_uuid: PlatCapUUID {
            g0: 0x3408B638u32.to_le_bytes(),
            g1: 0x09A9u16.to_le_bytes(),
            g2: 0x47A0u16.to_le_bytes(),
            g3: 0x8BFDu16.to_be_bytes(),
            g4: [0xA0, 0x76, 0x88, 0x15, 0xB6, 0x65],
        },
        bcd_version: 0x0100u16.to_le_bytes(),
        b_vendor_code: 0x30,
        i_landing_page: 0x31,
    };

    #[repr(C)]
    struct WebUsbVendorDesc<const URL_LENGTH: usize> {
        b_length: u8,
        b_desc_type: u8,
        b_scheme: u8,
        url: [u8; URL_LENGTH],
    }

    const WEBUSB_VENDOR_DESC: WebUsbVendorDesc<9> = WebUsbVendorDesc {
        b_length: size_of::<WebUsbVendorDesc<9>>() as u8,
        b_desc_type: 0x03,
        b_scheme: 0x00,
        // 这里我们的网址是随便乱写的，在实际的使用过程中，应该指向正确的地址
        url: *b"127.0.0.1",
    };

    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        core::slice::from_raw_parts((p as *const T) as *const u8, core::mem::size_of::<T>())
    }

    pub(super) struct MyUSBClass {
        iface_index: InterfaceNumber,
    }

    impl MyUSBClass {
        pub(super) fn new<B: UsbBus>(usb_bus_alloc: &UsbBusAllocator<B>) -> Self {
            Self {
                iface_index: usb_bus_alloc.interface(),
            }
        }
    }

    impl<B: UsbBus> UsbClass<B> for MyUSBClass {
        fn get_bos_descriptors(&self, writer: &mut BosWriter) -> usb_device::Result<()> {
            defmt::info!("write BOS desc");
            writer.capability(0x5, unsafe { any_as_u8_slice(&WEBUSB_PLAT_CAP_DESC) })
        }

        fn get_configuration_descriptors(
            &self,
            writer: &mut DescriptorWriter,
        ) -> usb_device::Result<()> {
            defmt::info!("write config desc");
            writer
                .interface(self.iface_index, 0xFF, 0x00, 0x00)
                .unwrap();
            Ok(())
        }

        fn control_in(&mut self, xfer: ControlIn<B>) {
            let req = xfer.request();

            // 依照我们在 PlatCapDesc 中写的内容，在这里过滤一下
            if req.request_type == RequestType::Vendor
                && req.request == 0x30
                && req.value == 0x31
                && req.index == 0x02
            {
                defmt::println!("Sending WebUSB_DESC");
                xfer.accept_with_static(unsafe { any_as_u8_slice(&WEBUSB_VENDOR_DESC) })
                    .unwrap();
            }
        }
    }
}

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use defmt_rtt as _;
use panic_probe as _;

use stm32f4xx_hal::{
    interrupt,
    otg_fs::{UsbBusType, USB},
    pac,
    prelude::*,
};
use usb_device::{class_prelude::*, prelude::*};

use crate::webusb_desc::MyUSBClass;

static COUNT: AtomicU32 = AtomicU32::new(0);
defmt::timestamp!("{}", COUNT.fetch_add(1, Ordering::Relaxed));

static G_USB_DEVICE: Mutex<RefCell<Option<UsbDevice<UsbBusType>>>> = Mutex::new(RefCell::new(None));
static G_MY_USB_CLASS: Mutex<RefCell<Option<MyUSBClass>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    static mut EP_OUT_MEM: [u32; 2] = [0u32; 2];
    static mut USB_BUS_ALLOC: Option<UsbBusAllocator<UsbBusType>> = None;

    defmt::info!("program start");

    let dp = pac::Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(96.MHz())
        .require_pll48clk()
        .freeze();

    let gpioa = dp.GPIOA.split();

    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    USB_BUS_ALLOC.replace(UsbBusType::new(usb, EP_OUT_MEM));
    let usb_bus_alloc = USB_BUS_ALLOC.as_ref().unwrap();

    let my_usb_class = MyUSBClass::new(usb_bus_alloc);

    let usb_device_builder = UsbDeviceBuilder::new(usb_bus_alloc, UsbVidPid(0x1209, 0x0001));

    let usb_device = usb_device_builder
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial")
        .build();

    cortex_m::interrupt::free(|cs| {
        G_USB_DEVICE.borrow(cs).borrow_mut().replace(usb_device);
        G_MY_USB_CLASS.borrow(cs).borrow_mut().replace(my_usb_class);
    });

    unsafe { NVIC::unmask(interrupt::OTG_FS) }

    #[allow(clippy::empty_loop)]
    loop {}
}

#[interrupt]
fn OTG_FS() {
    cortex_m::interrupt::free(|cs| {
        let mut usb_device_mut = G_USB_DEVICE.borrow(cs).borrow_mut();
        let usb_device = usb_device_mut.as_mut().unwrap();
        let mut my_usb_class_mut = G_MY_USB_CLASS.borrow(cs).borrow_mut();
        let my_usb_class = my_usb_class_mut.as_mut().unwrap();

        usb_device.poll(&mut [my_usb_class])
    });
}
