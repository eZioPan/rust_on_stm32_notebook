//! 多 interface 的 WinUSB 配置
//!
//! 这里我们的 USB Deivce 具有 3 个 interface，其中
//! interface 0 是一个 function
//! interface 1 和 interface 2 通过 IAD 关联为一个 function（这里使用 IAD，仅是为了展示出 function 与 interface、iad 之间的关系）
//!
//! 在这种情况下，就 WinUSB 而言，我们需要为每个 function 都关联上 Compatible ID
//! 那么 Vendor 描述符的结构
//! 就从简单的 MS_OS_20_DESC_SET -> MS_OS_20_FEAT_DESC
//! 变为 MS_OS_20_DESC_SET -> MS_OS_20_CONF_SUBSET -> [MS_OS_20_FUNC_SUBSET_0 -> MS_OS_20_FEAT_DESC_0, MS_OS_20_FUNC_SUBSET_1 -> MS_OS_20_FEAT_DESC_1]
//!
//! 特别的，在 Windows 的 设备管理器 中，一个 function 就会形成一个独立的设备

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
    interrupt,
    otg_fs::{self, UsbBusType},
    pac,
    prelude::*,
};
use usb_device::{class_prelude::*, prelude::*};

mod my_usb_class {
    use crate::my_usb_class::{
        bos_desc::MS_OS_20_DESC_PLAT_CAP_DESC, ms_os_20_desc_set::MS_OS_20_DESC_SET,
    };
    use usb_device::{class_prelude::*, control::RequestType};

    pub(super) struct MyUSBClass {
        iface0_index: InterfaceNumber,
        iad0_iface0_index: InterfaceNumber,
        iad0_iface1_index: InterfaceNumber,
    }

    impl MyUSBClass {
        pub(super) fn new<B: UsbBus>(usb_bus_alloc: &UsbBusAllocator<B>) -> Self {
            Self {
                iface0_index: usb_bus_alloc.interface(),
                iad0_iface0_index: usb_bus_alloc.interface(),
                iad0_iface1_index: usb_bus_alloc.interface(),
            }
        }
    }

    impl<B: UsbBus> UsbClass<B> for MyUSBClass {
        fn get_bos_descriptors(&self, writer: &mut BosWriter) -> usb_device::Result<()> {
            defmt::info!("write BOS desc");
            writer.capability(0x5, unsafe {
                any_as_u8_slice(&MS_OS_20_DESC_PLAT_CAP_DESC)
            })
        }

        fn get_configuration_descriptors(
            &self,
            writer: &mut DescriptorWriter,
        ) -> usb_device::Result<()> {
            defmt::info!("write config desc");
            writer
                .interface(
                    self.iface0_index,
                    0xFF,
                    0x00,
                    0x00,
                )
                .unwrap();

            // 注意：IAD interface association descriptor 是 USB 3.2 Spec 里的内容
            // 这里是我们第一次接触到将多个 interface 合并为一个 function
            // 我们是通过 IAD 将多个 interface 合并为一个 function 的
            // 注意 IAD 的配置必须紧邻将要关联的 interface 的前面
            writer
                .iad(
                    self.iad0_iface0_index,
                    2,
                    0xFF,
                    0x00,
                    0x00,
                )
                .unwrap();
            writer
                .interface(self.iad0_iface0_index, 0xFF, 0x00, 0x00)
                .unwrap();
            writer
                .interface(self.iad0_iface1_index, 0xFF, 0x00, 0x00)
                .unwrap();
            Ok(())
        }

        fn control_in(&mut self, xfer: ControlIn<B>) {
            let req = xfer.request();

            if req.request_type == RequestType::Vendor
                && req.request == 0x20
                && req.index == 0x7
                && req.value == 0x0
            {
                defmt::println!("Sending MS_OS_20_DESC_SET");
                let winusb_desc = unsafe { any_as_u8_slice(&MS_OS_20_DESC_SET) };
                let req_length = req.length as usize;
                let desc_length = winusb_desc.len();

                let output_len = usize::min(req_length, desc_length);

                xfer.accept_with_static(&winusb_desc[0..output_len])
                    .unwrap();
            }
        }
    }

    mod bos_desc {
        #[repr(C)]
        pub(super) struct MsOs20DescPlatCapDesc {
            b_reserved: u8,
            plat_cap_uuid: PlatCapUUID,
            dw_win_version: [u8; 4],
            w_ms_os_desc_set_total_length: [u8; 2],
            b_ms_vendor_code: u8,
            b_alt_enum_code: u8,
        }

        #[repr(C)]
        struct PlatCapUUID {
            g0: [u8; 4],
            g1: [u8; 2],
            g2: [u8; 2],
            g4: [u8; 2],
            g5: [u8; 6],
        }

        pub(super) const MS_OS_20_DESC_PLAT_CAP_DESC: MsOs20DescPlatCapDesc =
            MsOs20DescPlatCapDesc {
                b_reserved: 0x00,
                plat_cap_uuid: PlatCapUUID {
                    g0: [0xDF, 0x60, 0xDD, 0xD8],
                    g1: [0x89, 0x45],
                    g2: [0xC7, 0x4C],
                    g4: [0x9C, 0xD2],
                    g5: [0x65, 0x9D, 0x9E, 0x64, 0x8A, 0x9F],
                },
                dw_win_version: [0x00, 0x00, 0x03, 0x06],
                // 在编写好 MsOs20DescSet 之后，记得把总字节数填写回来
                w_ms_os_desc_set_total_length: [10 + 8 + 2 * (20 + 8), 0x00],
                b_ms_vendor_code: 0x20,
                b_alt_enum_code: 0x00,
            };
    }

    // 在我们实际编写 Vendor Descriptor 的时候
    // struct 是由顶至底编写的
    // struct 的大部分数值也是由顶自底填写的
    // 但是所有的 w_total_length 是由底至顶填写的
    // 最后的最后，顶部的 w_total_length 需要填写回 BOS 描述符的 w_ms_os_desc_set_total_length 字段
    mod ms_os_20_desc_set {

        #[repr(C)]
        pub(super) struct MsOs20DescSet {
            w_length: [u8; 2],
            w_desc_type: [u8; 2],
            dw_win_version: [u8; 4],
            w_total_length: [u8; 2],
            conf_subset: [ConfSubset; 1],
        }

        #[repr(C)]
        struct ConfSubset {
            w_length: [u8; 2],
            w_desc_type: [u8; 2],
            b_conf_value: u8,
            b_reserved: u8,
            w_total_length: [u8; 2],
            func_subset: [FuncSubset; 2],
        }

        #[repr(C)]
        struct FuncSubset {
            w_length: [u8; 2],
            w_desc_type: [u8; 2],
            b_first_iface: u8,
            b_reserved: u8,
            w_subset_length: [u8; 2],
            comp_id: CompatID,
        }

        #[repr(C)]
        struct CompatID {
            w_length: [u8; 2],
            w_desc_type: [u8; 2],
            compat_id: [u8; 8],
            sub_compat_id: [u8; 8],
        }

        // 可以注意看一下这里的结构，是与我们最上面说的层级结构相映照的
        pub(super) const MS_OS_20_DESC_SET: MsOs20DescSet = MsOs20DescSet {
            w_length: [10, 0x00],
            w_desc_type: [0x00, 0x00],
            dw_win_version: [0x00, 0x00, 0x03, 0x06],
            w_total_length: [10 + 8 + 2 * (20 + 8), 0x00],
            conf_subset: [ConfSubset {
                w_length: [8, 0x00],
                // 该描述符类型被称为 MS_OS_20_SUBSET_HEADER_CONFIGURATION
                w_desc_type: [0x01, 0x00],
                b_conf_value: 0,
                b_reserved: 0x00,
                w_total_length: [8 + 2 * (20 + 8), 0x00],
                func_subset: [
                    FuncSubset {
                        w_length: [8, 0x00],
                        // 该描述符类型被称为 MS_OS_20_SUBSET_HEADER_FUNCTION
                        w_desc_type: [0x02, 0x00],
                        b_first_iface: 0,
                        b_reserved: 0x00,
                        w_subset_length: [8 + 20, 0x00],
                        comp_id: CompatID {
                            w_length: [20, 0x00],
                            w_desc_type: [0x03, 0x00],
                            compat_id: [b'W', b'I', b'N', b'U', b'S', b'B', b'\0', 0x00],
                            sub_compat_id: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                        },
                    },
                    FuncSubset {
                        w_length: [8, 0x00],
                        w_desc_type: [0x02, 0x00],
                        b_first_iface: 1,
                        b_reserved: 0x00,
                        w_subset_length: [8 + 20, 0x00],
                        comp_id: CompatID {
                            w_length: [20, 0x00],
                            w_desc_type: [0x03, 0x00],
                            compat_id: [b'W', b'I', b'N', b'U', b'S', b'B', b'\0', 0x00],
                            sub_compat_id: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                        },
                    },
                ],
            }],
        };
    }

    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        core::slice::from_raw_parts((p as *const T) as *const u8, core::mem::size_of::<T>())
    }
}

use crate::my_usb_class::MyUSBClass;

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

    let usb = otg_fs::USB::new(
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
        .composite_with_iads()
        .build();

    cortex_m::interrupt::free(|cs| {
        G_USB_DEVICE.borrow(cs).borrow_mut().replace(usb_dev);
        G_MY_USB_CLASS.borrow(cs).borrow_mut().replace(my_usb_class);
    });

    unsafe { NVIC::unmask(interrupt::OTG_FS) }

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
