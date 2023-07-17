//! 简单的数据发送与接收
//!
//! 本章节我们需要实现这样一个效果，将 USB deivce 连接上 Host 后，从 MCU 上拉取 "hello" 字符串，然后从 Host 发送 "hi" 字符串到 MCU 上

//! 注意，该 MCU 程序需要与运行在主机上的程序配合才能看到效果，主机上的配套程序的源码在 .\host_side_app 路径下

//! 注意，对于 Windows 用户来说，需要手动为该 USB 设备配置一个驱动程序，具体方法是
//! 1. 插入 USB 设备
//! 2. 在 设备管理器 - 其它设备 中找到名为 random product 的设备
//! 3. 在 random product 上右键
//!    - 更新驱动程序 - 浏览我的电脑以查找驱动程序
//!    - 让我从计算机上的可用驱动程序列表中选取
//!    - 找到 通用串行总线设备
//!    - 厂商选 WinUSB 设备，型号选 WinUSB 设备
//!    - 确认弹出的警告提示
//!
//! 另：在 s13c03_1winusb.rs 中，我们会实现一种 Windows 可以自动识别，并匹配 WinUSB 驱动的 usb device

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use defmt_rtt as _;
use panic_probe as _;

use stm32f4xx_hal::{
    otg_fs::{UsbBusType, USB},
    pac,
    prelude::*,
};
use usb_device::{
    class_prelude::*,
    endpoint,
    prelude::{UsbDeviceBuilder, UsbDeviceState, UsbVidPid},
};

// 为了更好的标识 MCU 处理的内容
// 这里为 defmt 添加了一个编号，这样我们就能方便地观察 log 的序号
static COUNT: AtomicU32 = AtomicU32::new(0);
defmt::timestamp!("{}", COUNT.fetch_add(1, Ordering::Relaxed));

// 这里我们自定义的 USB Class 需要具有收发功能，因此其内容也要增加一些
struct MyUSBClass<'a, B: UsbBus> {
    // 这里还是一样，负责相关数据收发的 Endpoint 需要归类在一个 Interface 下
    iface_index: InterfaceNumber,
    // 然后我们需要额外开辟一个 IN Endpoint，而且在后面实际创建的时候，它会是一个 Interrupt IN Endpoint
    interrupt_in: EndpointIn<'a, B>,
    // 这里我们仿照 STM32 SPI/UART/I2C 的 TXE 标识位设计，也给出一个 IN endpoint Empty 标识位
    // 搭配上 UsbClass trait 的 endpoint_in_complete 函数，我们就可以确保上一个数据发送完之后，再发送下一个数据
    in_empty: bool,
    // 这里我们还需要开辟一个 OUT Endpoint，与上面的 IN Endpoint 类似，实际上它也会是一个 Interrupt OUT Endpoint
    interrupt_out: EndpointOut<'a, B>,
    // 当前，我们给出一个 buffer，以及一个索引号，以便从底层的库中提取 MCU 收到的数据
    // 这里我们要给出 buffer 的原因是，底层 USB 总线收发数据的频率和上层调用者读取数据的频率不能保证相同
    // 因此我们必须要维护一个 buffer，在底层 USB 收到数据的时候，第一时间保存下来，
    // 等到上层要读取数据的时候，在从我们的 buffer 里返回所存储的数据
    receive_buf: [u8; 32],
    receive_index: usize,
}

// 根据 usb_device crate 的建议，我们最好还要为我们自定义的 UsbClass 结构体实现额外的新建、读写函数
//
// PS: 其实就实验来说，这两个函数直接写在主函数的 loop{} 也是没有问题的，不过这里我们就额外封装一下好了
impl<'a, B: UsbBus> MyUSBClass<'a, B> {
    fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            iface_index: alloc.interface(),
            // 我们这里有意标注了 `.interrupt()` 方法所需要的范型参数，来表示生成的是特定方向的 interrupt 类 Endpoint
            // 但在实际使用时，它是没有必要标注的，因为编译器可以从字段类型里自动推断
            //
            // max_packet_size 表示每个 packet 的最大字节数，就 Full-Speed 的 Interrupt 来说，它应该不大于 64 byte
            // interval 表示这个 endpoint 应该拉取的间隔，就 Full-Speed 的 Interrupt 来说，这个值表示的是毫秒数，取值范围为 1 ~ 255
            // 关于这两个参数的含义，参见 USB 2.0 Spec 的 9.6.6 Endpoint
            interrupt_in: alloc.interrupt::<endpoint::In>(32, 1),
            in_empty: true,
            interrupt_out: alloc.interrupt::<endpoint::Out>(32, 1),
            receive_buf: [0u8; 32],
            receive_index: 0,
        }
    }

    // 为我们自定义的 UsbClass 结构体是实现一个写函数
    // 写函数的功能也很简单，读取调用者传入的一个字节切片，然后发送出去
    // 这里我参考了 usbd_serial crate 的 `.write()` 的实现方法
    // 它仅执行单次的、对底层 buf 的写入，并把写入的结果回传给调用者
    fn write(&mut self, bytes: &[u8]) -> Result<usize, UsbError> {
        // 这里的 in_empty 字段，是要配合 UsbClass trait 的 `endpoint_in_complete` 回调函数一同使用的
        // 我们这里将其设置为 IN 非空，而 `endpoint_in_complete` 会将 IN 设置为空
        match self.in_empty {
            true => {
                let byte_written = self.interrupt_in.write(bytes)?;
                // 这里我借鉴了 usbd_serial crate 的 `.write()` 的实现方法
                // 如果写入的字节数等于 0，那么我们也还是应该返回 Err(UsbError::WouldBlock)
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

    // 自定义了一个读函数
    // 将自身结构体中保存的数据提交给调用者的 buffer 里
    // 同上，这里也是单次读取，但与 usbd_serial crate 不同的是，当没有数据可读的时候，我们会返回 Err(UsbError::WouldBlock)
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, UsbError> {
        if self.receive_index > 0 {
            buf[0..self.receive_index].clone_from_slice(&self.receive_buf[0..self.receive_index]);
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
        // 先注册一个 interface，接着注册 Interrupt IN 和 Interrupt OUT
        writer.interface(self.iface_index, 0xFF, 0x00, 0x00)?;
        writer.endpoint(&self.interrupt_out)?;
        writer.endpoint(&self.interrupt_in)?;
        Ok(())
    }

    // 如果非 control 的 OUT 端口发来的数据，我们就收一下，放到我们自己的 struct 的 buffer 里面
    // 等着什么时候上层的函数从 struct 的 buffer 里拿数据
    fn endpoint_out(&mut self, addr: EndpointAddress) {
        // 不过，首先我们要比对一下 OUT 端口的地址，如果不是我们的 UsbClass 对应的地址，就忽略
        if addr != self.interrupt_out.address() {
            return;
        }
        let index = self.interrupt_out.read(&mut self.receive_buf).unwrap();
        // 注意，这里使用了 +=，因为我们不确定 Host 发来的信息到底有没有分段
        self.receive_index += index;
    }

    // usb_device 还提供了一个回调函数，它会在 Endpoint IN 发送完毕的时候被调用
    // 我们可以利用这个函数，修改我们的 UsbClass 的 struct 里的 in_empty 字段，来标记发送已经完成
    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        // 同上，我们也是要先检查一下地址是否匹配
        if addr != self.interrupt_in.address() {
            return;
        }
        defmt::info!("IN buffer clear");
        self.in_empty = true;
    }
}

// 参考 s13c01 的说法
// 我们这里有 CONTROL OUT 0 和 INTERRUPT OUT 1
// 其中 CONTROL OUT 0 的 max_packet_size 为 8 byte
// INTERRUPT OUT 1，从上面的代码中，可以看到为 32 byte
// 因此，该数组的长度为 (8+3)/4+(32+3)/4 = 10
static mut EP_OUT_MEM: [u32; 10] = [0u32; 10];

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("program start");

    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(96.MHz())
        .require_pll48clk()
        .freeze();

    let mut delay = cp.SYST.delay(&clocks);

    let gpioa = dp.GPIOA.split();

    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    let usb_bus_alloc = UsbBusType::new(usb, unsafe { &mut EP_OUT_MEM });

    let mut my_usb_class = MyUSBClass::new(&usb_bus_alloc);

    let usb_device_builder = UsbDeviceBuilder::new(&usb_bus_alloc, UsbVidPid(0x1209, 0x0001));

    let mut usb_dev = usb_device_builder
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial")
        .build();

    // 下方，我将循环切分为了两个部分，
    // 第一个 loop{} 块为 USB 枚举所在的循环，
    // 它的终止为 UsbDevice 的状态达到 Configured
    // 第二个 loop{} 块为实际应用程序所在的循环
    // 它会依照 Host 的要求，收发数据

    defmt::info!("USB Device Enumerating");
    loop {
        // 这里展示的代码的模式，是非常常见的模式
        // 首先还是对 UsbDevice 和 UsbClass 的轮询
        // UsbDevice 的 `.pool()` 方法，会返回一个 bool 值
        // 当轮询发现任何 UsbClass 具有可读或可写的状态，UsbDevice 的 `.pool()` 就会返回 true
        // 此时我们就可以执行后面的代码，比如从 UsbClass 中拉取数据，等等
        // 如果 UsbDevice 的 `.pool()` 返回 false，我们就可以等待一段时间，再次 poll UsbDevice
        if !usb_dev.poll(&mut [&mut my_usb_class]) {
            // 进入这个“短路”分支，表示当前 UsbDevice 中没有可以被任何 UsbClass 读/写的数据
            // 此时，我们就可以等待一段时间，再询问 UsbDevice
            //
            // 这里等待的毫秒数，等价于 s13c01 中的等待时间
            delay.delay_ms(10u8);
            continue;
        };

        // 如果代码跳转到此处，说明 UsbClass 有可取的数据，或者有空余的空间以供发送
        // 不过我们这里还在 USB 设备的枚举阶段，也没什么数据需要额外处理的，因此就i检测一下 UsbDevice 的 state
        // 如果 UsbDevice 进入 Configured 的状态，说明 UsbDevice 枚举完成，那我们就跳出这个循环，去下一个循环里执行实际收发数据的逻辑代码
        if usb_dev.state() == UsbDeviceState::Configured {
            break;
        }

        // 这里我们要稍稍等一下（比如这里为 10 us），让 Host 和 Device 完成剩下的工作，
        // 再跳转回本 loop{} 的头部，开始下一次的询问
        // 不等待一段时间的话，USB 设备的枚举过程会失败
        delay.delay_us(10u8);
    }

    // 在上面的枚举完成之后，就是实际执行“业务代码”的循环了
    defmt::info!("USB Device Configured");
    let mut receive_buf = [0u8; 16];
    loop {
        if !usb_dev.poll(&mut [&mut my_usb_class]) {
            // 这里我们将空循环的等待时间缩短了，这样我们可以稍稍提高设备的平均响应速度
            delay.delay_us(100u16);
            continue;
        };

        // 由于我们上面封装了 `.read()` 和 `.write()` 方法，这里我们要做的就比较简单了，直接就是读一个数据、然后写一个数据

        match my_usb_class.read(&mut receive_buf) {
            Ok(count) => {
                defmt::println!(
                    "receive \"{}\"",
                    core::str::from_utf8(&receive_buf[0..count]).unwrap()
                );
            }
            Err(UsbError::WouldBlock) => (),
            Err(e) => panic!("{:?}", e),
        };

        match my_usb_class.write(b"hello") {
            Ok(_) => defmt::info!("\"hello\" put into IN buf"),
            Err(UsbError::WouldBlock) => (),
            Err(e) => panic!("{:?}", e),
        };
    }
}
