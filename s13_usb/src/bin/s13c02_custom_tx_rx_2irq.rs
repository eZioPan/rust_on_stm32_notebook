//! 简单的数据发送与接收
//!
//! 本源码是中断版本的 custom_tx_rx，
//! 这里的中断指的是，在 USB OTG 模块收到数据之后，会产生中断，让 Cortex 核心读取数据的意思
//! 并非是说 USB 总线上有中断传输

//! 同 poll 版本的 custom_tx_rx，主机上的配套程序的源码在 .\host_side_app 路径下

#![no_std]
#![no_main]

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use defmt_rtt as _;
use panic_probe as _;

use stm32f4xx_hal::{
    otg_fs::{UsbBusType, USB},
    pac::{self, interrupt},
    prelude::*,
};
use usb_device::{class_prelude::*, prelude::*};

// 首先，为了整理代码，所有与 MyUSBClass struct 相关的代码，都移动到了 my_usb_class 这个 mod 里，在本文件的最下方
use crate::my_usb_class::MyUSBClass;

static COUNT: AtomicU32 = AtomicU32::new(0);
defmt::timestamp!("{}", COUNT.fetch_add(1, Ordering::Relaxed));

// 这里我们要在全局创建出两个会出现在中断中的静态量
// “量如其名”，就不再解释了
static G_USB_DEVICE: Mutex<RefCell<Option<UsbDevice<UsbBusType>>>> = Mutex::new(RefCell::new(None));
static G_MY_USB_CLASS: Mutex<RefCell<Option<MyUSBClass<UsbBusType>>>> =
    Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    // 特别注意，我们这里使用了一个 cortex_m_rt crate 提供的“语法糖”
    // 这个语法糖会将 #[entry] 内**顶部**的 static mut（必须是 mut 版本的 static）转换为 &'static mut
    // 这样我们就可以避免在 main 内部标注大量的 unsafe{} 块（逻辑上来说，memory safe 由 cortex_m_rt 保证了）
    //
    // 这个“语法糖”最常用的地方就是要创建一个 static mut 量，但这个量其实不用在多线程中传递，
    // 它的值会在程序运行的整个周期中持续存在，但在脱离 main 函数的范围时，无法通过变量名访问
    static mut EP_OUT_MEM: [u32; 10] = [0u32; 10];
    static mut USB_BUS_ALLOC: Option<UsbBusAllocator<UsbBusType>> = None;

    defmt::info!("program start");

    let dp = pac::Peripherals::take().unwrap();
    // 由于 Cortex 核心的调用是通过中断完成的，因此这里我们也不需要延时器了
    // let cp = pac::CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(96.MHz())
        .require_pll48clk()
        .freeze();

    // 由于 Cortex 核心的调用是通过中断完成的，因此这里我们也不需要延时器了
    // let mut delay = cp.SYST.delay(&clocks);

    let gpioa = dp.GPIOA.split();

    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    // 我们这里利用 &mut Option 的 .replace() 方法，将我们创建好的实际的值，替换掉 USB_BUS_ALLOC 里的 None
    USB_BUS_ALLOC.replace(UsbBusType::new(usb, EP_OUT_MEM));

    // 由于我们上面刚刚才注入实际的值，因此这里 .as_ref() 后直接调用 .unwrap() 也不会 panic
    let usb_bus_alloc = USB_BUS_ALLOC.as_ref().unwrap();
    let my_usb_class = MyUSBClass::new(usb_bus_alloc);
    let usb_device_builder = UsbDeviceBuilder::new(usb_bus_alloc, UsbVidPid(0x1209, 0x0001));
    let usb_dev = usb_device_builder
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial")
        .build();

    // 最后我们得将创建好的值注入到全局静态量中
    cortex_m::interrupt::free(|cs| {
        G_USB_DEVICE.borrow(cs).borrow_mut().replace(usb_dev);
        G_MY_USB_CLASS.borrow(cs).borrow_mut().replace(my_usb_class);
    });

    // 然后我们挂起 NVIC 中对应的中断
    // 注意，USB OTG 模块在 NVIC 中只有两个注册处理函数的位置
    // 其中一个就是 OTG_FS，它负责处理除了 USB 唤醒事件之外的其它所有 USB OTG 中断
    unsafe { NVIC::unmask(interrupt::OTG_FS) }

    // 主循环里我们什么都不用写
    #[allow(clippy::empty_loop)]
    loop {}
}

#[interrupt]
fn OTG_FS() {
    cortex_m::interrupt::free(|cs| {
        // 中断函数里，首先把两个全局静态量给“拆出来”
        let mut usb_device_mut = G_USB_DEVICE.borrow(cs).borrow_mut();
        let usb_device = usb_device_mut.as_mut().unwrap();
        let mut my_usb_class_mut = G_MY_USB_CLASS.borrow(cs).borrow_mut();
        let my_usb_class = my_usb_class_mut.as_mut().unwrap();

        // 常规操作，拉取一下数据
        // 这里其实不应该叫轮询了，因为我们并没有“轮询”，我们是等有中断才执行这步操作的
        if !usb_device.poll(&mut [my_usb_class]) {
            return;
        }

        // 这里稍稍有些特殊，由于每次中断后，必然做这个判定，因此
        // 这里的含义变成了，当前 UsbDevice 的状态是否为 Configured，如果不是 Configured，则不执行后面的代码
        if usb_device.state() != UsbDeviceState::Configured {
            return;
        }

        // 之后也没啥，就是一写，一读的常规操作了

        match my_usb_class.write(b"hello") {
            Ok(_) => defmt::info!("\"hello\" put into IN buf"),
            Err(UsbError::WouldBlock) => (),
            Err(e) => panic!("{:?}", e),
        };

        let mut rx_buf = [0u8; 64];

        match my_usb_class.read(&mut rx_buf) {
            Ok(count) => {
                defmt::println!(
                    "receive \"{}\"",
                    core::str::from_utf8(&rx_buf[0..count]).unwrap()
                );
            }
            Err(UsbError::WouldBlock) => (),
            Err(e) => panic!("{:?}", e),
        };
    })
}

// 关于 MyUSBClass 的设计，其实也没有什么改变
mod my_usb_class {
    use usb_device::{class_prelude::*, endpoint};

    pub(super) struct MyUSBClass<'a, B: UsbBus> {
        iface_index: InterfaceNumber,
        interrupt_in: EndpointIn<'a, B>,
        in_empty: bool,
        interrupt_out: EndpointOut<'a, B>,
        receive_buf: [u8; 64],
        receive_index: usize,
    }

    impl<'a, B: UsbBus> MyUSBClass<'a, B> {
        pub(super) fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
            Self {
                iface_index: alloc.interface(),
                interrupt_in: alloc.interrupt::<endpoint::In>(32, 1),
                in_empty: true,
                interrupt_out: alloc.interrupt::<endpoint::Out>(32, 1),
                receive_buf: [0u8; 64],
                receive_index: 0,
            }
        }

        pub(super) fn write(&mut self, bytes: &[u8]) -> Result<usize, UsbError> {
            match self.in_empty {
                true => {
                    let byte_written = self.interrupt_in.write(bytes)?;
                    if byte_written > 0 {
                        defmt::info!("IN byte written: {}", byte_written);
                        self.in_empty = false;
                        Ok(byte_written)
                    } else {
                        Err(UsbError::WouldBlock)
                    }
                }
                false => Err(UsbError::WouldBlock),
            }
        }

        pub(super) fn read(&mut self, buf: &mut [u8]) -> Result<usize, UsbError> {
            if self.receive_index > 0 {
                buf[0..self.receive_index]
                    .clone_from_slice(&self.receive_buf[0..self.receive_index]);
                let index = self.receive_index;
                self.receive_index = 0;
                Ok(index)
            } else {
                Err(UsbError::WouldBlock)
            }
        }
    }

    impl<'a, B: UsbBus> UsbClass<B> for MyUSBClass<'a, B> {
        fn get_configuration_descriptors(
            &self,
            writer: &mut DescriptorWriter,
        ) -> usb_device::Result<()> {
            writer.interface(self.iface_index, 0xFF, 0x00, 0x00)?;
            writer.endpoint(&self.interrupt_out)?;
            writer.endpoint(&self.interrupt_in)?;
            Ok(())
        }

        fn endpoint_out(&mut self, addr: EndpointAddress) {
            if addr != self.interrupt_out.address() {
                return;
            }
            let index = self.interrupt_out.read(&mut self.receive_buf).unwrap();
            self.receive_index += index;
        }

        fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
            if addr != self.interrupt_in.address() {
                return;
            }
            defmt::info!("IN buffer clear");
            self.in_empty = true;
        }
    }
}
