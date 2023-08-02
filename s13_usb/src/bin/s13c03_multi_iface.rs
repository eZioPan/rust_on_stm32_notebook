//! 这里我们要实现具有 2 个自定义名称的 interface 的 device

#![no_std]
#![no_main]

mod my_usb_class {
    use usb_device::{class_prelude::*, device};

    // 要实现自定义 interface 名称，我们需要获取两个 STRING Desc 索引号
    // usb_device crate 为了防止我们误操作，对多个不同的 STRING Request 给出相同的 STRING Desc，
    // STRING Desc 索引号是需要通过 `UsbBusAllocator` 的 `.string()` 方法获取的
    //
    // 因此，我们在我们自己的 MyUSBClass 结构体里需要记录我们获取的 StringIndex 对象
    // 之后我们就可以在 `fn get_string()` 回调函数里通过判定 StringIndex 的值，来返回不同的字符串
    pub(super) struct MyUSBClass {
        iface0_index: InterfaceNumber,
        iface0_string: StringIndex,
        iface1_index: InterfaceNumber,
        iface1_string: StringIndex,
    }

    impl MyUSBClass {
        pub(super) fn new<B: UsbBus>(usb_bus_alloc: &UsbBusAllocator<B>) -> Self {
            Self {
                iface0_index: usb_bus_alloc.interface(),
                // 通过 `.string()` 方法获得 StringIndex，usb_device crate 会保证我们获得的 iString 号不重复
                iface0_string: usb_bus_alloc.string(),
                iface1_index: usb_bus_alloc.interface(),
                iface1_string: usb_bus_alloc.string(),
            }
        }
    }

    impl<B: UsbBus> UsbClass<B> for MyUSBClass {
        fn get_configuration_descriptors(
            &self,
            writer: &mut DescriptorWriter,
        ) -> usb_device::Result<()> {
            // 在 usb_device crate 中，`.interface()` 调用的其实是 `.interface_alt()`
            // 虽然逻辑上 `.interface_alt()` 是用来定义 Alternative Function 的，
            // 但我们可以仿照 `.interface()` 的实现方法，让 `.interface_alt()` 定义 interface 的默认行为
            // 最主要的是，我们可以通过 `.interface_alt()` 的 interface_string 参数，传递一个 StringIndex
            // 然后我们就可以在后面通过回复对应的 STRING Request，来为这个 interface 起名字了
            writer.interface_alt(
                self.iface0_index,
                // 在 `alternate_setting` 参数上给出 `DEFAULT_ALTERNATE_SETTING` 以设置 interface 的默认行为
                device::DEFAULT_ALTERNATE_SETTING,
                0xFF,
                0x00,
                0x00,
                // 传递对应的 StringIndex，在实际发送 GET CONFIGURATION Desc 的时候，会将其中隐含的数字写入 iInterface 字段
                Some(self.iface0_string),
            )?;

            writer.interface_alt(
                self.iface1_index,
                device::DEFAULT_ALTERNATE_SETTING,
                0xFF,
                0x00,
                0x00,
                Some(self.iface1_string),
            )?;

            Ok(())
        }

        // 然后，我们这里一定要实际响应 GET STRING Request，并返回对应的字符串传
        fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
            if index == self.iface0_string {
                Some("First Interface")
            } else if index == self.iface1_string {
                Some("Second Interface")
            } else {
                None
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
use my_usb_class::MyUSBClass;
use panic_probe as _;

use stm32f4xx_hal::{
    interrupt,
    otg_fs::{UsbBusType, USB},
    pac,
    prelude::*,
};

use usb_device::{class_prelude::*, prelude::*};

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

    let usb_dev = usb_device_builder
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial")
        .build();

    cortex_m::interrupt::free(|cs| {
        G_USB_DEVICE.borrow(cs).borrow_mut().replace(usb_dev);
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

        usb_device.poll(&mut [my_usb_class]);
    })
}
