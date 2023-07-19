//! 简单的数据发送与接收
//!
//! 本源码是使用 RTIC 版本的 custom_tx_rx，

//! 同 poll 版本的 custom_tx_rx，主机上的配套程序的源码在 .\host_side_app 路径下

//! 关于 RTIC 的说明，可以看一下 s02c01 的 2rtic 源码

#![no_std]
#![no_main]

#[rtic::app(device=stm32f4xx_hal::pac, peripherals=true)]
mod app {

    use defmt_rtt as _;
    use panic_probe as _;

    use crate::my_usb_class::MyUSBClass;
    use stm32f4xx_hal::{
        otg_fs::{UsbBusType, USB},
        prelude::*,
    };
    use usb_device::{class_prelude::*, prelude::*};

    // rx_buf 和 rx_buf_index 是放在 shared 结构体的
    // 因为我们会在中断处理 task 中写入数据，并在 idle task 里打印（并清空）数据
    #[shared]
    struct Shared {
        rx_buf: [u8; 64],
        rx_buf_index: usize,
    }

    #[local]
    struct Local {
        // 这两个量由于要在线程间传递，因此设置为 'static 是必然的
        usb_device: UsbDevice<'static, UsbBusType>,
        my_usb_class: MyUSBClass<'static, UsbBusType>,
    }

    // #[init(local=[])] 中**创建** init 的可变静态量（static mut）
    // 这里 init 这个 task 很特殊，它的 local=[] 只能创建 init 所要使用的可变静态量
    #[init(local=[
        ep_out_mem:[u32; 10] = [0u32; 10],
        usb_bus_alloc: Option<UsbBusAllocator<UsbBusType>> = None,
    ])]
    fn init(ctx: init::Context) -> (Shared, Local) {
        defmt::info!("program start");

        let dp = ctx.device;

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

        let usb_bus_alloc_option = ctx.local.usb_bus_alloc;
        let ep_out_mem = ctx.local.ep_out_mem;

        usb_bus_alloc_option.replace(UsbBusType::new(usb, ep_out_mem));

        let usb_bus_alloc = usb_bus_alloc_option.as_ref().unwrap();

        let my_usb_class = MyUSBClass::new(usb_bus_alloc);
        let usb_device_builder = UsbDeviceBuilder::new(usb_bus_alloc, UsbVidPid(0x1209, 0x0001));
        let usb_device = usb_device_builder
            .manufacturer("random manufacturer")
            .product("random product")
            .serial_number("random serial")
            .build();

        (
            Shared {
                rx_buf: [0u8; 64],
                rx_buf_index: 0,
            },
            Local {
                usb_device,
                my_usb_class,
            },
        )
    }

    // 空循环中负责监测 rx_buf_index 的状态，并打印数据
    #[idle(shared = [rx_buf, rx_buf_index])]
    fn idle(mut ctx: idle::Context) -> ! {
        loop {
            ctx.shared.rx_buf_index.lock(|rx_buf_index| {
                if *rx_buf_index > 0 {
                    ctx.shared.rx_buf.lock(|rx_buf| {
                        defmt::println!(
                            "receive \"{}\"",
                            core::str::from_utf8(&rx_buf[0..*rx_buf_index]).unwrap()
                        );
                        *rx_buf_index = 0;
                    })
                }
            });
        }
    }

    // 这个 task 想当于 OTG_FS 的中断处理函数
    #[task(binds = OTG_FS, local = [usb_device, my_usb_class], shared=[rx_buf, rx_buf_index])]
    fn otg_fs_handle(mut ctx: otg_fs_handle::Context) {
        let usb_device = ctx.local.usb_device;
        let my_usb_class = ctx.local.my_usb_class;

        if !usb_device.poll(&mut [my_usb_class]) {
            return;
        }

        if usb_device.state() != UsbDeviceState::Configured {
            return;
        }

        match my_usb_class.write(b"hello") {
            Ok(_) => defmt::info!("\"hello\" put into IN buf"),
            Err(UsbError::WouldBlock) => (),
            Err(e) => panic!("{:?}", e),
        };

        // 由于打印的工作交给 idle task 去做了，这里我们只负责修改 rx_buf
        ctx.shared.rx_buf.lock(|rx_buf| {
            match my_usb_class.read(rx_buf) {
                Ok(count) => {
                    ctx.shared.rx_buf_index.lock(|rx_buf_index| {
                        *rx_buf_index += count;
                    });
                }
                Err(UsbError::WouldBlock) => (),
                Err(e) => panic!("{:?}", e),
            };
        });
    }
}

mod my_usb_class {
    use usb_device::{class_prelude::*, endpoint};

    pub struct MyUSBClass<'a, B: UsbBus> {
        iface_index: InterfaceNumber,
        interrupt_in: EndpointIn<'a, B>,
        in_empty: bool,
        interrupt_out: EndpointOut<'a, B>,
        receive_buf: [u8; 64],
        receive_index: usize,
    }

    impl<'a, B: UsbBus> MyUSBClass<'a, B> {
        pub fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
            Self {
                iface_index: alloc.interface(),
                interrupt_in: alloc.interrupt::<endpoint::In>(32, 1),
                in_empty: true,
                interrupt_out: alloc.interrupt::<endpoint::Out>(32, 1),
                receive_buf: [0u8; 64],
                receive_index: 0,
            }
        }

        pub fn write(&mut self, bytes: &[u8]) -> Result<usize, UsbError> {
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

        pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, UsbError> {
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
