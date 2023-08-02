//! 实现 Microsoft OS 2.0 Descriptor 配置，以在 Windows 上自动适配 WinUSB 驱动
//!
//! 首先，非常感谢 https://github.com/newaetech/naeusb/blob/main/wcid.md 提供的信息
//! 这篇代码正是简化自上面这篇博客，并使用 rust usb_device 实现的的版本

//! 这里主要涉及两方面的知识，
//! 第一个是来自 USB 3.2 Spec 的 BOS 描述符
//! 第二个是来自 Microsoft OS 2.0 Descriptors Specification（下简称 MS OS 2.0 Desc Spec）的 Vendor 特定的描述符
//!
//! 在枚举的过程中，
//! Host 设备会先发出 BOS 请求，此时 Device 需要发送全部的 BOS 描述符，
//! 然后 Host 依照返回的 BOS 描述符的内容，发出特定的 Vendor 请求，此时设备需要返回相应的 Vendor 特定的描述符

//! 本篇给出的是最简单的设备，所使用的最简单的配置
//! 在这里，这个 device 仅有一个 interface，那么我们可以直接在 Vendor 描述符的顶层，写入 WinUSB 的 Compatible ID
//!
//! 如果一个 device 有两个或以上的 interface，那么我们就需要书写更加完整的 Vendor 描述符，这点后面的代码会有所涉及

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

// 在这个案例中，USBClass 的构成更加重要，因此放到了最前面
mod my_usb_class {
    use core::mem::size_of;

    use usb_device::{class_prelude::*, control::RequestType};

    use crate::my_usb_class::{
        bos_desc::MS_OS_20_DESC_PLAT_CAP_DESC, ms_os_20_desc_set::MS_OS_20_DESC_SET,
    };

    mod bos_desc {
        use core::mem::size_of;

        use super::ms_os_20_desc_set::MsOs20DescSet;

        // BOS 回复结构体
        // 注意到，除了 UUID 的部分字段，如果是多字节字段值，则需要以**小端序**的顺序排列
        #[repr(C)]
        pub(super) struct MsOs20DescPlatCapDesc {
            /*
            // 前三个字段的值由 usb_device crate 负责，我们不需要填写
            b_length: u8
            b_desc_type: u8
            b_dev_cap_type: u8
            */
            // 之后是 MS OS 2.0 Desc Spec 特有的部分，需要逐个填写
            b_reserved: u8,
            plat_cap_uuid: PlatCapUUID,
            // dwWindowsVersion
            // https://learn.microsoft.com/zh-cn/cpp/porting/modifying-winver-and-win32-winnt
            // 这个字段其实对应 Windows SDK 中的 sdkddkver.h 文件中的 NTDDI_* 常量的值
            // 如果你安装了 Windows SDK，那么它应该会出现在
            // C:\Program Files (x86)\Windows Kits\<<版本号>>\Include\<版本号>\shared\sdkddkver.h 文件中
            dw_win_version: [u8; 4],
            // 响应 Vendor 请求时，返回的描述符的总长度
            // 必须与 MsOs20DescSet 结构体的实际长度对应
            w_ms_os_desc_set_total_length: [u8; 2],
            // 在 Host 请求 MS_OS_DESC_SET 时，应该使用的 Request Code
            b_ms_vendor_code: u8,
            // Alternative Enumeration 所使用的 Request Code
            b_alt_enum_code: u8,
        }

        // PlatformCapabilityUUID
        // 在 USB 3.2 Spec 中，该 UUID 用于确定一个特定的
        #[repr(C)]
        struct PlatCapUUID {
            g0: [u8; 4],
            g1: [u8; 2],
            g2: [u8; 2],
            g3: [u8; 2],
            g4: [u8; 6],
        }

        // 注意到，除了 UUID 的部分字段，如果是多字节字段值，则需要以**小端序**的顺序排列
        pub(super) const MS_OS_20_DESC_PLAT_CAP_DESC: MsOs20DescPlatCapDesc =
            MsOs20DescPlatCapDesc {
                b_reserved: 0x00,
                // MS 对这里的这个特别的 16 位的值
                // 有特殊的称呼 MS_OS_20_Platform_Capability_ID
                // 另外，我们在 MS OS 2.0 Desc Spec 中看到的，以字符串表示的 UUID
                // 其前三个字段都是大端序表示的，因此这里需要翻转，
                // 而最后两个字段是小端序表示的，是不需要再次翻转的
                plat_cap_uuid: PlatCapUUID {
                    g0: 0xD8DD60DFu32.to_le_bytes(),
                    g1: 0x4589u16.to_le_bytes(),
                    g2: 0x4CC7u16.to_le_bytes(),
                    g3: 0x9CD2u16.to_be_bytes(), // 注意我们这里用的是转大端序，不是转小端序
                    // 最后一节没有写成某个整数的原因，是因为 6 个 byte，是不能恰好填充某个内置的数据类型的
                    // 因此就单独写成数组的形式了
                    g4: [0x65, 0x9D, 0x9E, 0x64, 0x8A, 0x9F],
                },
                // dwWinVersion 共 4 位
                dw_win_version: 0x06030000u32.to_le_bytes(),
                // MS_OS_DESC_SET 的回复实际总长度
                // 由于 NsOS20DescSet 的结构体的长度是固定的，且不含指针，因此我们可以直接使用 core::mem::size_of() 函数来取得结构体的大小
                w_ms_os_desc_set_total_length: (size_of::<MsOs20DescSet>() as u16).to_le_bytes(),
                // 请求 MS_OS_20_DESC_SET 实际所需的 Vendor Code
                b_ms_vendor_code: 0x20,
                // Alternative Enumerator 号，0x00 表示不存在
                b_alt_enum_code: 0x00,
            };
    }

    mod ms_os_20_desc_set {
        use core::mem::size_of;

        // 当 Host 发来 MS_OS_20_DESC_SET 相关的请求时
        // 我们应该回复给 Host 的 Desc
        #[repr(C)]
        pub(super) struct MsOs20DescSet {
            // 头部长度
            w_length: [u8; 2],
            // Microsoft OS 2.0 descriptor types
            w_desc_type: [u8; 2],
            // Windows 版本号，见上面 BOS 描述符里的说明
            dw_win_version: [u8; 4],
            // 整个 MS OS 2.0 Desc Set 的总长度
            //（在这里就是 MsOs20DescSet 这个结构体的总大小）
            w_total_length: [u8; 2],
            comp_id: CompatID,
        }

        // MS OS 2.0 feature desc 类型的描述符中
        // 名为 Microsoft OS 2.0 compatible ID descriptor 的描述符
        #[repr(C)]
        struct CompatID {
            // 一个 feature desc 的长度
            w_length: [u8; 2],
            // Microsoft OS 2.0 descriptor types
            w_desc_type: [u8; 2],
            // Compatible ID
            // https://learn.microsoft.com/en-us/windows-hardware/drivers/install/compatible-ids
            // 一种厂商自定义的序列，
            // 若一个硬件设备使用了相同的序列值，那么 Windows 就会用厂商提供的特定驱动包来驱动这个硬件
            compat_id: [u8; 8],
            // Sub Compatible ID
            sub_compat_id: [u8; 8],
        }

        pub(super) const MS_OS_20_DESC_SET: MsOs20DescSet = MsOs20DescSet {
            // 头部长度，在 MS OS 2.0 Spec 中，这个长度是固定的，
            // 不过我们可以这么计算，结构体的总长度 减去 载荷的长度 就是 头部的长度
            w_length: ((size_of::<MsOs20DescSet>() - size_of::<CompatID>()) as u16).to_le_bytes(),
            // 其值的名称固定为 MSOS20_SET_HEADER_DESCRIPTOR，数值固定为 0x00
            w_desc_type: 0x00u16.to_le_bytes(),
            dw_win_version: 0x06030000u32.to_le_bytes(),
            w_total_length: (size_of::<MsOs20DescSet>() as u16).to_le_bytes(),
            comp_id: CompatID {
                w_length: (size_of::<CompatID>() as u16).to_le_bytes(),
                // 其值的名称固定为 MS_OS_20_FEATURE_COMPATBLE_ID，数值固定为 0x03
                w_desc_type: 0x03u16.to_le_bytes(),
                // 这个 Compatible ID 的值是微软定义的，其值参见：
                // https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/automatic-installation-of-winusb#setting-the-compatible-id
                // 如果一个设备兼容 WinUSB，则此处必须这样设置
                compat_id: *b"WINUSB\0\0",
                sub_compat_id: [0x00; 8],
            },
        };
    }

    // 参考 https://stackoverflow.com/a/42186553
    // 将任何一个指针指向的内存，直接拷贝为一个 byte array
    unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        core::slice::from_raw_parts((p as *const T) as *const u8, size_of::<T>())
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
        // BOS 描述符是从这个函数里返回的
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
                .interface(self.iface_index, 0xFF, 0x00, 0x00)
                .unwrap();
            Ok(())
        }

        // 请求 Vendor 描述符时，我们要修改这个函数
        fn control_in(&mut self, xfer: ControlIn<B>) {
            let req = xfer.request();

            // 需要对请求做一个判定，来看看请求是不是 MS OS 2.0 Vendor 请求
            // 它的 bRequest 是我们在 BOS 描述符中定义的 bMS_VendorCode 的值
            // wValue 固定为 0x00，wIndex 固定为 0x07
            // wIndex 的 0x07 在 MS OS 2.0 Spec 里称为 MS_OS_20_DESCRIPTOR_INDEX
            // 与之对应的还有 0x08，称为 MS_OS_20_SET_ALT_ENUMERATION，这里暂且不表
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

        usb_device.poll(&mut [my_usb_class]);
    })
}
