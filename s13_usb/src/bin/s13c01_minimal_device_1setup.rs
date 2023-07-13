//! 最小化 USB 设备
//!
//! 这个 USB 设备完全没有任何功能，但是可以正常被 Linux 和 Windows 枚举
//! 需要注意的是，Windows 上，正确被枚举，但是没有在 Windows 上正确配置驱动的 USB 设备
//! 会出现在 设备管理器 的 其它设备 列表中，而且设备的图标上会有黄色三角叹号，这是正常的
//! 如果一个 USB 设备连枚举都没有通过，则会出现在 通用串行总线控制器 这一栏，而且设备的标注是 未知的 USB 设备

//! USB 通信协议是比较复杂的，它的传输方式与 TCP/IP 协议有几分相似
//! 学习 USB 协议的时候，建议找一个教程，了解一下 USB 的中非常重要的一些概念
//! 然后找一个中文版的 USB 2.0 Specification，快速了解一下 USB Specification 中的内容

//! 这里我们假设读者已经了解了 USB 的基本的概念了，下面不会对 USB 的概念做详细的说明

//! 在写代码之前，需要稍稍检查一下开发板的硬件配置
//! 注意查一下开发板的 schematic，看一下 USB 的 D+ 和 D- 引脚上有没有上拉电阻，如果有的话，记得把它们去掉，
//! 因为 STM32F4 的 USB OTG 模块内置了必要的上下拉电阻，它们会在 USB 模块工作的时候，自动执行上拉/释放操作，
//! 外部的上拉电阻，反而会影响 USB 模块内置的一些功能

//! 如果你通过 Wireshark 抓包来观察 USB 传输，那么要注意的是，Wireshark 提供的是操作系统内核提供的 URB（USB Request Block）
//! URB 是一种介于 USB Spec 中 transaction 和 transfer 之间的一种概念，它并非 USB Spec 中最常见的 packet 层级的一种描述
//! 要准确对应上 USB Spec 层级的 packet，需要通过其它软件，或者是逻辑分析仪等硬件，才能正确捕获到 packet 层级的信息

//! 还有一点要注意的是，（为了节省硬件成本）USB Spec 中，Host 和 Device 之间是没有线路传递中断的，每个 packet 的开始都是通过轮询实现的
//! 因此 USB 对于响应超时是比较敏感的，所以我们可能不能通过随手打断点的方式调试 USB 程序了
//! 在这里我们使用了 defmt 这个 crate 来生成 log
//! 并用 defmt-rtt 将 defmt 通过 RTT 传输到 Host 上
//! 然后我们还用 panic-probe 将 panic!() 重定向到 defmt 上
//! 最后我们还开启了所有支持 defmt 的 crate 的 `defmt` feature，方便我们 debug

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

// UsbClass 大致对应 USB 2.0 Specification 中的 Configuration Descriptor 层级
// 一个 UsbClass 可以理解为具有特定功能的一个 Configuration
//
// 注意，在 usb_device crate 的 class 模块里，有说明，实现 UsbClass trait 的对象不应该直接捕获 UsbBus 对象，而应该获得 UsbBusAllocator 的临时性引用
// 并从这个临时引用中产生 endpoint/interface 等等，然后让对象捕获这些生成的对象。
struct MyUSBClass {
    // 除了 EndPoint 0，最小 USB deivce 不需要其它的 Endpoint，
    // 但必须启动一个 Interface，才能让 Linux 和 Windows 的操作系统将其识别为一个有效的 USB 设备
    //
    // 这里记录的是当前获得的 interface 的编号
    // 我们在后面，为这个 struct 实现 UsbClass trait 的时候，需要使用到这个编号
    iface_index: InterfaceNumber,
}

// 为 MyUSBClass 实现一些特有的关联函数/方法
//
// 特别要注意的是，USB Spec 中的 Configuration Description 中的 Configuration，是一个抽象概念，或者说是一个逻辑上的概念
// 一个 Configuration 并非在硬件上真实存在，因此它不会从 UsbBusAllocator 生成，而是在实现 UsbClass trait 时，“动态地”生成
impl MyUSBClass {
    // 这里我们为 MyUSBClass 实现一个额外的 new() 关联函数
    // 它就符合 usb_device crate 的 class 模块的说明，使用了一个 UsbBusAllocator 的临时的引用，
    // 生成了一个 interface，然后让 MyUSBClass 去捕获这个新生成的 interface
    //
    // 注意，我们这里仅生成了一个 interface，没有生成其它的东西，
    // 但一般来说，一个“有用”的 USB 设备，还需要生成至少一个 endpoint
    fn new<B: UsbBus>(usb_bus_alloc: &UsbBusAllocator<B>) -> Self {
        Self {
            iface_index: usb_bus_alloc.interface(),
        }
    }
}

// 接着我们要为 MyUSBClass 实现 UsbClass trait
// 这个 trait 的内容，更像是在写回调函数一般，
// 内容是，当 Host 问起 Device 一些问题的时候，Device 应该如何回答
//
// 理论上来说，UsbClass 的每个方法，都有默认的实现，因此不需要实现任何一个方法
// 但是 UsbClass 的默认实现，均是空实现，因此要产生实际的效果，还是需要手动实现一些方法的
impl<B: UsbBus> UsbClass<B> for MyUSBClass {
    // 我们这里就实现了 get_configuration_descriptors 这个方法
    // 当 Device 收到来自 Host 的对 CONFIGURATION 的 GET DESCRIPTOR 请求时，
    // 就会使用该函数的来生成要回复的内容
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        // 由于我们已经从 UsbBusAllocator 中分配了一个 interface
        // 因此我们把这个 interface 的信息写到回复中
        //
        // 我们传入的参数中 interface_class 的值 0xFF 表示该这个 interface 是“厂商自定义 interface”，
        // 也就是说，这个 interface 的通信不（保证）属于任何 USB IF 预定义的通信规范
        // 其中承载的数据流是由厂商自行规定的
        writer.interface(self.iface_index, 0xFF, 0x00, 0x00)?;
        Ok(())
    }
}

// 开辟一些内存
// 我猜可能是作为 Endpoint 的收发 buffer 准备的？
static mut EP_MEM: [u32; 1024] = [0u32; 1024];

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("program start");

    let dp = pac::Peripherals::take().unwrap();
    let cp = pac::CorePeripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    // 配置 RCC 的时候需要注意，
    // 我们至少需要将 SYSCLK 拉到 12 MHz 或以上（正常情况应该远高于 12 MHz），并单独使用 .require_pll48clk() 方法
    // 让 USB OTG 模块能正常启动并与 USB Host 通信
    //
    // 另外，USB 也没有单独的时钟线，device 和 host 间的总线时钟同步是靠数据线上的“特定电平变化”实现的
    // 因此，我们这里启用外部晶振，尽量保持 device 端的总线时钟的精确
    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(12.MHz())
        .require_pll48clk()
        .freeze();

    // STM32F411RET6 上，USB 的 D+ 和 D- 引脚对应的是 PA12 和 PA11
    let gpioa = dp.GPIOA.split();

    // 使用 stm32f4xx_hal 提供的 USB struct
    // stm32f4xx_hal 的 USB struct 实现了 synopsys_usb_otg crate 的 UsbPeripheral trait
    // 因此在之后，它可以作为 UsbBus::new() 的参数，一同构建 UsbBusAllocator
    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &clocks,
    );

    // 在 synopsys_ubs_otg crate 内部，UsbBusType struct 实现了 usb_device crate 里的 UsbBus trait
    // 由于 UsbBusType 实现了 UsbBus，因此 UsbBusType 可以塞进 usb_device 的 UsbBusAllocator struct 里
    // 于是乎，我们在这里看到的总效果就是，我们使用 UsbBusType 的 new() 关联函数，生成了 UsbBusAllocator
    //
    // 注意到是，不同的，标记为“实现了 usb_device crate”的 crate，他们实现 UsbBus trait 的方法可能各不相同
    // 这里 UsbBusType 要求传递一个实现了 usb_device crate 的 UsbPeripheral trait 的对象，以及一个静态 u32 数组
    // 其它的实现方法可能需要其它传递的对象和参数
    //
    // TIP: 实际上，UsbBusType 是 UsbBus<USB> 的别名，不过由于 UsbBus 已经是与之关联的 trait 的名称了
    // 因此 synopsys_ubs_otg crate 定义了一个别名，方便我们使用
    let usb_bus_alloc = UsbBusType::new(usb, unsafe { &mut EP_MEM });

    // 在生成了 UsbBusAllocator 之后，我们就可以通过它来构建我们自己的 MyUSBClass 实例对象了
    //
    // 特别注意，我们必须在创建 UsbDevice 对象之前创建好 MyUSBClass
    // 下方我们使用了 UsbDeviceBuilder 创建 UsbDevice，因此这里实际上是在对 UsbDeviceBuilder 的实例调用 `.build()` 方法前，创建 MyUSBClass
    let mut my_usb = MyUSBClass::new(&usb_bus_alloc);

    // 接着，我们需要创建 UsbDevice 的实例
    // 这个实例依旧需要 UsbBusAllocator 来获得 Endpoint 0 IN 和 Endpoint 0 OUT 的控制
    // 因此，我们是需要传入 usb_bus_alloc 的引用的
    //
    // 注意到 UsbVidPid 这个 struct，它表示的是 USB 设备的 厂商编号（VendorID）和设备编号（ProductID）
    // 正常情况下，VID 是需要从 USB IF 手上购买的，不过有好心人将自己不用的（USB IF 认为是作废的）VID 贡献了出来，
    // 于是在开发和测试阶段，我们是可以使用这个 VID 的，而且这个 VID 下的前几个 PID 都是给 Test 使用的，因此我们也可以安全的使用前几个 PID 来测试效果
    //
    // 另外就是，我们可以在这里设置字符串类型的厂商名称、产品名称和产品序列号（注意序列号是字符串类型的，并非整数型的）
    let usb_device_builder = UsbDeviceBuilder::new(&usb_bus_alloc, UsbVidPid(0x1209, 0x0001))
        .manufacturer("random manufacturer")
        .product("random product")
        .serial_number("random serial");

    // 另外，在开发和测试阶段，我们的 MCU 几乎必然是外部供电的（比如通过 DAPLink 供电），因为还是需要 debug 的
    // 因此在开发测试阶段，我们得告知 USB Host，MCU 具有外部供电，并支持（和启用了）远程唤醒
    // 依照成品实际的供电状态，这两个参数在实际的成品设备中不一定要出现
    #[cfg(debug_assertions)]
    let usb_device_builder = usb_device_builder
        .self_powered(true)
        .supports_remote_wakeup(true);

    // 调用 `.build()` 方法，构建实际的 UsbDevice 实例
    // 构建好后，UsbDevice 实例会处于 UsbDeviceState::Default 状态
    // 之后，我们可以不断轮询 UsbDevice 实例，看看 Host 有没有发送数据，在做出响应的响应
    // 注意到
    // 1. 这里我们为了方便，是通过轮询 USB OTG 模块的方式，看 USB OTG 模块的状态的
    // 2. USB 上的每个 transaction 仅能由 Host 发起，因此我们还是得等 Host 发过来的信息
    //
    // 注意，在构建 UsbDevice 之前，必须要构建好所有需要的 UsbClass 实例
    // 因为在 `.build()` 之后，hal 库会一直可变借用 UsbBusAllocator，导致我们创建的 UsbClass 无法可变借用后者。
    let mut usb_dev = usb_device_builder.build();

    // 为了方便理解，下面我们会通过轮询 USB 模块的方法获取数据
    // 不过鉴于 USB 上的包是有发送间隔的，因此我们启动 Cortex 自带的时钟
    // 这样一次轮询之后，可以让 CPU 等待一段时间，再轮询
    let mut delay = cp.SYST.delay(&clocks);

    // 然后就是轮询 USB OTG 模块了
    // 准确来说，是轮询 UsbDevice 实例，有没有特定的实现了 UsbClass 的实例需要处理的内容
    // 没有的话就等待 500 us，然后执行下一个询问
    // 有的话就执行后面的代码，让那些实例有空处理内容
    loop {
        if !usb_dev.poll(&mut [&mut my_usb]) {
            delay.delay_us(500u16);
            continue;
        }

        // 如果 USB Host 发来了数据，那我们得稍微等一下，等回复发送出去了，再接着 poll USB Bus，否则会直接枚举失败
        //
        // 正常情况下，我们并不需要额外等待这个时间，因为一般来说，在有数据来了之后，我们都会进一步进行处理，
        // 这样 USB OTG 模块自然就有时间发送回执了，也就不需要这个 delay 了
        // delay.delay_us(1u8);

        // 在这里，由于我们的 MyUSBClass 没有什么功能，
        // 因此我们这里随便打印一下 UsbDevice 的状态好了
        defmt::info!("{:?}", usb_dev.state());
    }
}
