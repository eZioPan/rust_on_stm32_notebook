//! USB Host 端的程序
//!
//! 由于 USB 总线完全是由 Host 控制的，因此我们必须要写一个 Host 端程序，才能正常与 Device 通信
//!
//! 需要注意的是，在一个有操作系统的环境下，USB Device 与 USB Host 之间的初始化配置，是交由 kernel，以及 kernel 中的 driver 处理的
//! 且作为运行在操作系统上的应用程序，我们仅能使用 kernel 暴露出来的 API 来与 USB 设备通信
//!
//! 这里我们更进一步，不仅不调用 kernel 的 API，我们这里实际上使用的是 libusb 这个 C 库暴露的 API
//! libusb 这个库兼容了[多个操作系统](https://github.com/libusb/libusb)，并抽象为统一的 API，极大的降低了我们在 Host 端编写 USB 应用的难度
//!
//! 另外，由于我们是使用的是 rust 编写程序，因此我们还需要一个 libusb 的 rust binding
//! 这里我使用的是 rusb 这个 crate

use std::{process, str, time::Duration};

// 一般来说，一个 USB Hub 上会有不少 USB Device，因此我们要通过 VID / PID 过滤出我们想要通信的 device
const VID: u16 = 0x1209;
const PID: u16 = 0x0001;

// 而且，由于我们使用的是测试用 VID/PID，可能有不少设备也使用了同一对 VID & PID
// 因此我们最好还是过滤一下生产商名和产品名
const MANUFACTURER_NAME: &'static str = "random manufacturer";
const PRODUCT_NAME: &'static str = "random product";

// 而且，这里我们还额外指定了特定的序列号来通信
// 不过一般来说，没有必要预先指定序列号，我们可以搜集所有具有相同 VID&PID 的 device 的序列号，并将序列号信息上报给用户，让用户挑选一个设备进行通信
// 不过这里为了简化演示，我们直接指定了一个序列号
const SERIAL: &'static str = "random serial";

fn main() {
    // 首先是取得所有挂载的 USB 设备
    let usb_devices = rusb::devices().unwrap();

    // 然后我们通过 VID、PID、Serial 定位我们要找的 device
    let mut device_list: Vec<_> = usb_devices
        .iter()
        .filter(|cur_device| {
            // 先过滤 VID/PID
            let device_desc = cur_device.device_descriptor().unwrap();
            let (cur_vid, cur_pid) = (device_desc.vendor_id(), device_desc.product_id());
            if cur_vid != VID || cur_pid != PID {
                return false;
            }

            // 然后过滤 manuafacture name、product name 和 serial name
            // 这里我们特别要求返回的必须是 ASCII 字符表示的三个名称
            // 这个信息在初始化 USB Device 时，会保存在 kernel 中，无需再从 USB Device 上访问得到
            //
            // 我们告诉操作系统，我们要打开这个 USB Device 的 handle
            // 这个操作仅通知了 Host 的 kernel，不会对 USB 总线做任何操作
            let my_dev_handle = cur_device.open().unwrap();

            let cur_manufacture = my_dev_handle
                .read_manufacturer_string_ascii(&device_desc)
                .unwrap();
            if cur_manufacture != MANUFACTURER_NAME {
                return false;
            }

            let cur_product = my_dev_handle
                .read_product_string_ascii(&device_desc)
                .unwrap();
            if cur_product != PRODUCT_NAME {
                return false;
            }

            let cur_serial = my_dev_handle
                .read_serial_number_string_ascii(&device_desc)
                .unwrap();
            if cur_serial != SERIAL {
                return false;
            }

            true
        })
        .collect();

    match device_list.len() {
        0 => {
            println!("No matched USB device found, exit");
            process::exit(1);
        }
        1 => (),
        _ => {
            println!("multiple USB devices with sample name found, unplug other.\nexit");
            process::exit(1);
        }
    }

    let my_device = device_list.pop().unwrap();

    // 然后我们逐步检查一下当前设备的所有的 configuration、每个 configuration 下的 interface、每个 interface 下的 endpoint 的信息

    let device_desc = my_device.device_descriptor().unwrap();

    println!("device: VIP & PID: 0x{:04x} & 0x{:04x}", VID, PID);

    (0..device_desc.num_configurations()).for_each(|config_num| {
        println!(" └─configure number: {}", config_num);

        let conf_desc = my_device.config_descriptor(config_num).unwrap();

        let iface_list = conf_desc.interfaces();

        iface_list.for_each(|iface| {
            println!("    └─interface number {}", iface.number());
            iface.descriptors().for_each(|iface_desc| {
                iface_desc.endpoint_descriptors().for_each(|ep_desc| {
                    println!(
                        "       └─endpoint addr: 0x{:02x}, endpoint dir: {:?}",
                        ep_desc.address(),
                        ep_desc.direction()
                    );
                })
            })
        });
    });

    println!("");

    // 之后我们就可以激活正确的 configuration，申请特定的 interface，并通过其中的 endpoint 产生通信了

    my_device.config_descriptor(0).unwrap();

    let mut my_dev_handle = my_device.open().unwrap();

    // 然后我们得选择一个 interface
    // 由于我们只有一个 interface
    my_dev_handle.claim_interface(0).unwrap();

    // libusb 在 windows 上有点 bug，这里不能给出 0 长度（length）的 Vec，
    // 所使用的 Vec 必须具有足够容纳单次数据的长度
    // 所以这里用 .resize() 方法直接将整个预留空间（capacity）填充 0
    let mut buf = Vec::with_capacity(32);
    buf.resize(buf.capacity(), 0);

    // 接收来自 Device 的数据
    // 出错时，先手动释放对 interface 的占用，再 panic，下同
    //
    // 注意，endpoint 地址，In 方向的是从 0x80 开始的
    let byte_read = my_dev_handle
        .read_interrupt(0x81, &mut buf, Duration::from_millis(500))
        .or_else(|e| {
            my_dev_handle.release_interface(0).unwrap();
            Err(e)
        })
        .unwrap();

    // 打印一下收到的数据
    println!(
        "receive \"{}\"",
        str::from_utf8(&buf[0..byte_read]).unwrap()
    );

    // 并发送一个数据到 Device 上
    let byte_send = my_dev_handle
        .write_interrupt(0x01, b"hi", Duration::from_millis(500))
        .or_else(|e| {
            my_dev_handle.release_interface(0).unwrap();
            Err(e)
        })
        .unwrap();

    if byte_send == b"hi".len() {
        println!("\"hi\" send");
    } else {
        println!("error occurred, when sending \"hi\"");
    }

    // 最后我们可以手动释放一下对 interface 的占用
    my_dev_handle.release_interface(0).unwrap();
}
