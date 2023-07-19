//! 实现 Microsoft OS 2.0 Descriptor 配置，以在 Windows 上自动适配 WinUSB 驱动
//!
//! 首先，非常感谢 https://github.com/newaetech/naeusb/blob/main/wcid.md 提供的信息
//! 这篇代码内容几乎就是上面这篇博客的 rust usb_device 的版本

//! 这里主要涉及两方面的知识，
//! 第一个是来自 USB 3.2 Spec 的 BOS 描述符
//! 第二个是来自 Microsoft OS 2.0 Descriptors Specification（下简称 MS OS 2.0 Desc Spec）的 Vendor 特定的描述符
//!
//! 在枚举的过程中，
//! Host 设备会先发出 BOS 请求，此时 Device 需要发送全部的 BOS 描述符，
//! 然后 Host 依照返回的 BOS 描述符的内容，发出特定的 Vendor 请求，此时设备需要返回相应的 Vendor 特定的描述符

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
    use usb_device::{class_prelude::*, control::RequestType};

    use crate::my_usb_class::{
        bos_desc::MS_OS_20_DESC_PLAT_CAP_DESC, ms_os_20_desc_set::MS_OS_20_DESC_SET,
    };

    mod bos_desc {
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
        // MS 对这里的这个特别的 16 位的值
        // 有特殊的称呼 MS_OS_20_Platform_Capability_ID
        // 另外，我们在 MS OS 2.0 Desc Spec 中看到的，以字符串表示的 UUID
        // 其前三个字段都是大端序表示的，因此这里需要翻转，
        // 而最后两个字段是小端序表示的，是不需要再次翻转的
        #[repr(C)]
        struct PlatCapUUID {
            g0: [u8; 4],
            g1: [u8; 2],
            g2: [u8; 2],
            g4: [u8; 2],
            g5: [u8; 6],
        }

        // 注意到，除了 UUID 的部分字段，如果是多字节字段值，则需要以**小端序**的顺序排列
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
                // dwWinVersion 共 4 位
                dw_win_version: [0x00, 0x00, 0x03, 0x06],
                // MS_OS_DESC_SET 的回复实际总长度
                w_ms_os_desc_set_total_length: [156, 0x00],
                // 请求 MS_OS_20_DESC_SET 实际所需的 Vendor Code
                b_ms_vendor_code: 0x20,
                // Alternative Enumerator 号，0x00 表示不存在
                b_alt_enum_code: 0x00,
            };
    }

    mod ms_os_20_desc_set {

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
            feat_desc_comp_id: FeatDescCompatID,
            reg_prop_desc: RegPropDesc,
        }

        // MS OS 2.0 feature desc 类型的描述符中
        // 名为 Microsoft OS 2.0 compatible ID descriptor 的描述符
        #[repr(C)]
        struct FeatDescCompatID {
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

        // 确定了一个 Windows 注册表的键和值
        // 这个设备会注册在 HKLM\SYSTEM\CurrentControlSet\Enum\USB\VID_<VID>&PID_<PID>\<产品名> 下
        #[repr(C)]
        struct RegPropDesc {
            // 该描述符的总长度
            w_length: [u8; 2],
            // 描述符类型
            w_desc_type: [u8; 2],
            // 注册表属性的类型
            w_prop_data_type: [u8; 2],
            // 注册表键名长度
            w_prop_name_length: [u8; 2],
            // 注册表键名（UTF16LE 编码）
            prop_name: [u8; 38],
            // 注册表值长度
            w_prop_data_length: [u8; 2],
            // 注册表值
            prop_data: PropData,
        }

        // 注册表值是一个非常长的 GUID
        // 它是一个用 UTF16LE 表示的字符串
        // 其包含了一对大括号，32 个十六进制字符，以及 4 个横线字符，还有尾部的两个 Null
        #[repr(C)]
        struct PropData {
            bracket_left: [u8; 2],
            group0: [u8; 16],
            dash0: [u8; 2],
            group1: [u8; 8],
            dash1: [u8; 2],
            group2: [u8; 8],
            dash2: [u8; 2],
            group3: [u8; 8],
            dash3: [u8; 2],
            group4: [u8; 24],
            bracket_right: [u8; 2],
            blank: [u8; 2],
        }

        pub(super) const MS_OS_20_DESC_SET: MsOs20DescSet = MsOs20DescSet {
            // 头部长度，长度应该为 10 byte
            w_length: [10, 0x00],
            // 其值的名称固定为 MSOS20_SET_HEADER_DESCRIPTOR，数值固定为 0x00
            w_desc_type: [0x00, 0x00],
            dw_win_version: [0x00, 0x00, 0x03, 0x06],
            w_total_length: [156, 0x00],
            feat_desc_comp_id: FeatDescCompatID {
                w_length: [20, 0x00],
                // 其值的名称固定为 MS_OS_20_FEATURE_COMPATBLE_ID，数值固定为 0x03
                w_desc_type: [0x03, 0x00],
                // 这个 Compatible ID 的值是微软定义的，其值参见：
                // https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/automatic-installation-of-winusb#setting-the-compatible-id
                // 如果一个设备兼容 WinUSB，则此处必须这样设置
                compat_id: [b'W', b'I', b'N', b'U', b'S', b'B', b'\0', 0x00],
                sub_compat_id: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            },
            reg_prop_desc: RegPropDesc {
                w_length: [126, 0x00],
                // 其值的名称固定为 MS_OS_20_FEATURE_REG_PROPERTY，数值固定为 0x04
                w_desc_type: [0x04, 0x00],
                w_prop_data_type: [0x07, 0x00],
                w_prop_name_length: [38, 0x00],
                // 注册表中，该键的键名是固定的，就是 DeviceInterfaceGUID
                prop_name: [
                    b'D', 0x00, b'e', 0x00, b'v', 0x00, b'i', 0x00, b'c', 0x00, b'e',
                    0x00, // "Device"
                    b'I', 0x00, b'n', 0x00, b't', 0x00, b'e', 0x00, b'r', 0x00, // "Inter
                    b'f', 0x00, b'a', 0x00, b'c', 0x00, b'e', 0x00, // face"
                    b'G', 0x00, b'U', 0x00, b'I', 0x00, b'D', 0x00, // "GUID"
                ],
                w_prop_data_length: [78, 0x00],
                // 不过注册表的键值我是乱起的，正常情况下应该生成一个 GUID
                prop_data: PropData {
                    bracket_left: [b'{', 0x00],
                    group0: [
                        b'0', 0x00, b'1', 0x00, b'2', 0x00, b'3', 0x00, //
                        b'4', 0x00, b'5', 0x00, b'6', 0x00, b'7', 0x00, //
                    ],
                    dash0: [b'-', 0x00],
                    group1: [b'8', 0x00, b'9', 0x00, b'A', 0x00, b'B', 0x00],
                    dash1: [b'-', 0x00],
                    group2: [b'C', 0x00, b'D', 0x00, b'E', 0x00, b'F', 0x00],
                    dash2: [b'-', 0x00],
                    group3: [b'0', 0x00, b'1', 0x00, b'2', 0x00, b'3', 0x00],
                    dash3: [b'-', 0x00],
                    group4: [
                        b'4', 0x00, b'5', 0x00, b'6', 0x00, b'7', 0x00, b'8', 0x00, b'9',
                        0x00, //
                        b'A', 0x00, b'B', 0x00, b'C', 0x00, b'D', 0x00, b'E', 0x00, b'F',
                        0x00, //
                    ],
                    bracket_right: [b'}', 0x00],
                    blank: [0x00, 0x00],
                },
            },
        };
    }

    // 参考 https://stackoverflow.com/a/42186553
    // 将任何一个指针指向的内存，直接拷贝为一个 byte array
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

        fn control_out(&mut self, xfer: ControlOut<B>) {
            defmt::info!("{:#04X}", xfer.request());
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

    let usb_dev = usb_device_builder
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial")
        .supports_remote_wakeup(true)
        .self_powered(true)
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
