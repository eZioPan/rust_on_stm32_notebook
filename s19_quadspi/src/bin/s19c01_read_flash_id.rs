//! 通过 QUADSPI 外设读取 Winbond W25Q32JV 的 ID 号
//!
//! 这里其实有三个大的概念
//!
//! 1. QuadSPI 是什么
//! 2. STM32F412 的 QUADSPI 模块怎么用
//! 3. W25Q32JV 要接收什么样的信息
//!
//! 让我们一个一个说
//!
//! 首先 QuadSPI 是什么
//!
//! 它其实是 SPI 通信的一个变种
//!
//! 它可以模拟常规的 4 线 SPI（CLK/CS/SI/SO, 后两个端口对饮 MISO 和 MOSI），此模式称为 single mode
//! 也可以将 SI 和 SO 改成双向通信端口，这称为 dual mode，而这两个端口则称为 IO0 和 IO1
//! 还可以在 dual mode 的基础上，再加上一对数据线，IO3 和 IO4，这样同时就有 4 根线通信了，它就称为 quad mode
//! 而 QuadSPI 也因 quad mode 而命名
//!
//! 然后 STM32 上的 QuadSPI 还有一个特别之处，它支持同时与两路 quad mode 通信，比如说，你可以用 11 根线（共用了时钟线）接入两个支持 QuadSPI 的 Flash
//! 这样你就可以一下写入 8 bit 的数据，并自动分配到两个 Flash 上
//!
//! 题外话，还有一种 OctoSPI，是一种通过 8 个数据线，将 8 bit 的数据一次性写入到支持的 Flash 上模块/通信方案，这里暂且不表。
//!
//!
//! QuadSPI 在协议层面，相较于 SPI 有额外的规定
//!
//! QuadSPI 上的数据，都是以“命令”（command）作为最小单位的
//! 一个命令可以包含 5 个阶段（phase）
//! Instruction（指令）阶段、Address（地址）阶段、Alternate-byte（交替字节）阶段、Dummy-cycles（空指令）阶段、Data（数据）阶段
//! 一个命令至少要包含指令、地址、交替字节或数据阶段中的一个，而且这 5 个阶段是顺次执行的
//! 注：有些外部设备会要求空指令之后再输入地址，由于 STM32 的 QUADSPI 模块不支持这种操作，因此我们可能需要通过两个命令来模拟它
//!
//! 指令阶段，一般用于确定当前这个命令的类型，当然这个“类型”是外部设备定义的
//! 地址阶段，一般用于确定要访问的外部设备的内存地址，注意，这个地址**不是外部设备的地址**，而是外部设备自己的内存的某个地址（不要和 I2C 的总线地址搞混）
//! 交替字节阶段，额外指定一些字节（不知道有啥用）
//! 空指令阶段，额外留空数个 QuadSPI 时钟周期，让外部设备有时间处理数据，而且，在有单线双向通信的模式下（dual mode 和 quad mode），必须至少留出一个空指令，因为引脚需要一个周期来切换输入输出方向
//! 数据阶段，用于传输或接收数据
//!
//! 然后，需要指出的是，QuadSPI 中的 single mode/dual mode/quad mode 是针对阶段（phase）说的，也就是说，一个指令的不同阶段可以使用不同的 mode
//! 比如指令阶段是 single mode，地址和数据阶段是 dual mode，等等
//!
//!
//! 就 QuadSPI 的访问外部设备的模式来说，它还分为三种
//!
//! 1. indirect（间接）模式
//! 2. status flag polling（状态标志轮询）模式
//! 3. memory-mapped（内存映射）模式
//!
//! 间接模式很好理解，和大多数外设一样，对于外部设备存储的访问，都是通过向外部存储发送特定的指令来实现的
//! 状态标志轮询模式，其实就是让 QUADSPI 模块自动轮询外部设备的状态，当达到特定状态时，通知 CPU 的一种模式
//! 内存映射模式，这种模式非常特殊，在这种模式下，外部设备的内存会被直接映射到 Cortex 核心中，也就是说，Cortex 核心可以像访问自己内部的内存一样，访问外部设备的内存
//!
//!
//! 好了第一个问题我们就有了一个大致的概念了，至于后面两个问题，我们边写代码边说明
//! 这里需要预先说明的关于 QUADSPI 模块的一个非常重要的概念
//!
//! 那就是，QUADSPI 模块是自动判定一个 transfer 是否要发送的，并不存在什么“启动发送”的位
//! 也就是说，一旦寄存器的状态达到了启动 transfer 的状态，则 QUADSPI 就一定会开始一个指令
//! 这就会导致一个问题，那就是，如果配置 QUADSPI 寄存器的顺序，或者操作不正确，那么就很可能导致 QUADSPI 错误地向外部设备发送了命令
//! QUADSPI 开始执行命令的判定条件为
//!
//! 1. 若写入了 CCR 寄存器的 INSTRUCTION 字段，且不需要地址阶段，而且没有要发送出去的数据（当前指令为读指令，或不需要数据阶段），那么直接开始命令
//! 2. 若写入了 AR 寄存器的 ADDRESS 字段，而且没有要发送出去的数据（当前指令为读指令，或不需要数据阶段），那么直接开始命令
//! 3. 若写入了 DR 寄存器的 DATA 字段，且需要地址阶段，且有数据需要发送出去（当前指令为写指令，且需要数据阶段，），那么开始指令
//!
//! 后面我们会举例说明，在这些条件的限制下，我们按怎样的顺序和方式写入寄存器

//! 接线图
//!
//!                  STM32 <-> W25Qxx
//!                    VCC <-> VCC              (脚 8)
//!                    GND <-> GND              (脚 4)
//!        CLK  PB1 (AF 9) <-> CLK              (脚 6)
//! BK1_IO0/SO  PC9 (AF 9) <-> DI IO0           (脚 5)
//! BK1_IO1/SI PC10 (AF 9) <-> DO IO1           (脚 2)
//!     BK1_IO2 PC8 (AF 9) <-> /WP IO2          (脚 3) (注：single mode/dual mode 中忽略)
//!     BK1_IO3 PA1 (AF 9) <-> /HOLD /RESET IO3 (脚 7) (注：single mode/dual mode 中忽略)
//!    BK1_nCS PB6 (AF 10) <-> /CS              (脚 1）
//!
//! 要接的线还是挺多的，至少要接 6 根线，最多要接上 8 根线
//! 我这里是用了一个 Flash 烧录夹，将 8 根线全部连上了

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::pac::Peripherals;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    let dp = Peripherals::take().unwrap();

    // 用 HSE
    use_hse(&dp);
    // 启用所使用的引脚，并调整到对应的 Alternate Function 上
    setup_gpio(&dp);
    // 设置一个 50 us 的定时器，后面的操作中会用到
    setup_systick(&dp);

    // 启动 QUADSPI 的外设时钟
    // 需要注意的是，QUADSPI 是挂载在 AHB3 上的
    let rcc = &dp.RCC;

    // 由于 QUADSPI 会继承前序操作的寄存器状态
    // 因此，我们在使用 QUADSPI 之前，最好来一次彻底的外设寄存器重置操作
    // 这一点在调试程序时会很有用
    rcc.ahb3enr.modify(|_, w| w.qspien().disabled());
    rcc.ahb3rstr.modify(|_, w| w.qspirst().reset());
    rcc.ahb3rstr.modify(|_, w| w.qspirst().clear_bit());

    // 启动 QUADSPI 模块的输入时钟
    rcc.ahb3enr.modify(|_, w| w.qspien().enabled());

    let qspi = &dp.QUADSPI;

    // 首先我们要告诉 QUADSPI 模块的信息，就是外部存储的大小
    // 我手上这颗 W25Q32，其总容量为 32 Mbit，也就是 4 MB，也就是 2^22 byte
    // 于是这里要填上 22 - 1 = 21
    qspi.dcr.modify(|_, w| unsafe { w.fsize().bits(21) });

    // 启动 QUADSPI 外设，准备传输
    qspi.cr.modify(|_, w| w.en().set_bit());

    // 之后我们就要逐步依照 W25Q32 的说明
    // 逐步向 W32Q25 发送指令了

    // 首先我们要做的就是 重启一下 W25Q32
    // 由于在设备上电到 QUADSPI 可以执行命令的期间，Flash 芯片可能已经收到了一些干扰，导致运行状态不确定
    // 我们这里先发送指令，让其回到刚上电的初始状态
    //
    // 该操作由两条命令共同完成，在 W25Q32 上称为 Enable Reset（指令号 0x66）和 Reset Device（指令号 0x99）
    // 这两条命令均为仅有指令阶段的命令，且命令必须连续发出

    // 首先我们发送 0x66 指令
    //
    // 特别要注意，如果要修改 CCR 寄存器，建议一次性修改完成，不要将单个命令的配置，分多次写入 CCR 寄存器
    // 因为，我们是按整个寄存器写入的方式，写入寄存器的，因此，即便我们没有写入 INSTRUCTION 的目标，该字段也会被写入
    // 从上面我们知道，一旦 INSTRUCTION 字段被写入，QUADSPI 就会检测启动条件，只要符合判定条件，传输就会自动启动
    // 比如这里，我们要发送的命令刚好是没有地址阶段的，它就会直接开始命令
    // 鉴于此，我建议，在单次命令中，总是一次性就写好整个 CCR 所需的配置
    //
    // 在默认情况下，CCR 寄存器的值全部为 0，也就是说，QUADSPI 默认处于写模式，且没有配置任何阶段
    // 于是我们这里仅需要配置指令阶段为 single mode，并给出指令号即可
    // 注意，各个阶段到底使用哪种 mode，是需要看外设的 datasheet 的，我们并不能胡乱指定
    qspi.ccr.modify(|_, w| unsafe {
        // 依照 W25Q16 的说明
        // 指令阶段使用 single spi mode
        w.imode().bits(0b01);
        // 指令号为 0x66
        w.instruction().bits(0x66);
        w
    });

    // 然后我们等待上一个命令发送结束
    // 这里我们靠轮询 BUSY 位，来确定发送是否已经完成
    while qspi.sr.read().busy().bit_is_set() {}
    // 接着立刻执行下一个命令 0x99
    qspi.ccr.modify(|_, w| unsafe {
        // 这里我们直接写入指令号 0x99
        //
        // 一般不推荐这样操作，因为一般情况下，我们并不能确定上一次使用的 CCR 寄存器的状态
        // 这里由于我们知道上一个操作是写模式，且用 single mode 写了一个 0x66，CCR 的状态是符合这里的要求的
        // 因此这里我们才能省略配置，仅修改 INSTRUCTION 字段的值
        w.instruction().bits(0x99);
        w
    });

    // 依照 W25Q32 的说明，在触发 Reset 之后，Flash 芯片会有大约 30 us 的时间不会响应任何指令
    // 这里我们就等个 50 us 的时间
    dp.STK.ctrl.modify(|_, w| w.enable().set_bit());
    while dp.STK.ctrl.read().countflag().bit_is_clear() {}
    // 用完了记得清理 Flag，并关闭 SysTick
    dp.STK.ctrl.modify(|_, w| {
        w.countflag().clear_bit();
        w.enable().clear_bit();
        w
    });

    // 在重置了 W25Q32 之后，就是正式读取 W25Q32 的信息了

    // 首先我们要读取的信息，被称为 JEDEC ID，依照 W25Q32 的说明，
    // 它会返回三个 byte，分别表示 Flash 的生产厂商、Flash 的存储类型和 Flash 的容量

    // 老规矩，发命令之前，先确认 QUADSPI 是否空闲
    while qspi.sr.read().busy().bit_is_set() {}

    // 修改 DLR 的值来确定来回传输的数据总量
    // 依照 Winbond 的说明 JEDEC ID 共返回 3 个字节，
    // 这里的值应该是需要传输的 byte 数 -1，也就是 2
    qspi.dlr.write(|w| unsafe { w.dl().bits(2) });

    // 读 JEDEC ID 的命令的指令号为 0x9F，且没有地址阶段
    // 下面还是使用了简略写法，仅做了必要的修改
    qspi.ccr.modify(|_, w| unsafe {
        // indirect 读模式（注意是读模式了）
        w.fmode().bits(0b01);
        // 数据阶段为 single spi modes（有数据阶段了，且为 single mode）
        w.dmode().bits(0b01);
        // 写对应的指令
        w.instruction().bits(0x9F);
        w
    });

    // 这里又体现出 QUADSPI 模块的一个特性
    // 那就是 BUSY 状态与读取 DR 寄存器是相关联的
    // 由于 QuadSPI 上来回传送的数据的量不是固定的，
    // 因此 QUADSPI 模块会一直挂 BUSY 位，直到收发了指定字节数的数据
    // 因此在读取的过程中，我们必须一直读取 DR 位，直到 BUSY 位被清空
    while qspi.sr.read().busy().bit_is_set() {
        // 不过，这里我们已经直到返回的总字节数为 3，因此一次读取返回的 4 byte 数据肯定就包含了所有我们需要的数据了
        // 因此这里一个循环也就完成了读取了
        rprintln!("JEDEC ID: {:#X}", qspi.dr.read().data().bits());
    }
    // 就我手上的 W25Q32JV 来说，从 DR 读取到的值为 0x1640EF
    // 也就是说，就多 byte 接收来说，QUADSPI 以 byte 为单位，将每个 byte，按照接收的顺序，从低 byte 到高 byte 填充 DR 寄存器

    // 之后我们来读取一个真的需要拉取两次的数据
    // 通过 0x4B 读取设备的 UID

    qspi.dlr.write(|w| unsafe { w.dl().bits(8 - 1) });
    qspi.ccr.modify(|_, w| unsafe {
        w.dcyc().bits(4 * 8);
        w.instruction().bits(0x4B);
        w
    });

    let mut uid_list: [u32; 2] = [0; 2];
    let mut cnt = 0;
    while qspi.sr.read().busy().bit_is_set() {
        uid_list[cnt] = qspi.dr.read().data().bits();
        cnt += 1;
    }

    // 这里我们可以以正确的顺序显示 UID
    let uid = ((uid_list[0].swap_bytes() as u64) << 32) + uid_list[1].swap_bytes() as u64;

    rprintln!("UID: {:#X}", uid);

    // 然后我们再读取一个需要给出内存地址的指令
    // 通过 0x90 读取设备的生产厂商和设备 ID

    qspi.dlr.write(|w| unsafe { w.dl().bits(2 - 1) });

    // 这里我实在是记不清前面的配置了，因此直接使用 .write() 方法，从寄存器的初始值开始配置
    //
    // 另外，就 0x90 这个指令来说，为啥我觉得 W25Q32 的指令表和下面的命令详解说的都不太对
    // 就我测试来说，地址阶段是接在空指令阶段之前的，而且是需要 8 个 bit 的空指令周期
    qspi.ccr.write(|w| unsafe {
        w.fmode().bits(0b01);
        w.imode().bits(0b01);
        w.admode().bits(0b01);
        w.adsize().bits(0b01);
        w.dcyc().bits(8);
        w.dmode().bits(0b01);
        w.instruction().bits(0x90);
        w
    });

    // 由于是有地址阶段的，因此传送的触发是在写入地址寄存器之后
    qspi.ar.write(|w| unsafe { w.address().bits(0x0) });

    while qspi.sr.read().busy().bit_is_set() {
        rprintln!(
            "Manufacturer/Device ID: {:#X}",
            (qspi.dr.read().data().bits() as u16).swap_bytes()
        );
    }

    #[allow(clippy::empty_loop)]
    loop {}
}

fn use_hse(dp: &Peripherals) {
    let rcc = &dp.RCC;
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}
}

fn setup_gpio(dp: &Peripherals) {
    let rcc = &dp.RCC;
    rcc.ahb1enr.modify(|_, w| {
        w.gpioaen().enabled();
        w.gpioben().enabled();
        w.gpiocen().enabled();
        w
    });

    let gpioa = &dp.GPIOA;
    gpioa.afrl.modify(|_, w| w.afrl1().af9()); // IO3 /HOLD /RESET
    gpioa.moder.modify(|_, w| w.moder1().alternate());

    let gpiob = &dp.GPIOB;
    gpiob.afrl.modify(|_, w| {
        w.afrl1().af9(); // CLK
        w.afrl6().af10(); // nCS
        w
    });
    gpiob.moder.modify(|_, w| {
        w.moder1().alternate();
        w.moder6().alternate();
        w
    });

    let gpioc = &dp.GPIOC;
    gpioc.afrh.modify(|_, w| {
        w.afrh8().af9(); // IO2 /WP
        w.afrh9().af9(); // IO0
        w.afrh10().af9(); // IO1
        w
    });
    gpioc.moder.modify(|_, w| {
        w.moder8().alternate();
        w.moder9().alternate();
        w.moder10().alternate();
        w
    });
}

fn setup_systick(dp: &Peripherals) {
    let systick = &dp.STK;

    systick.val.reset();

    // 由于 HSE 的频率是 12 MHz，除以 8 是 1.5 MHz，也就是一个 tick 是 2/3 us
    // 50 us 是 75 个 tick
    systick.load.write(|w| unsafe { w.reload().bits(75 - 1) });
}
