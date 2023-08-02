//! I²C
//!
//! I²C 又可以写为 IIC，或者简写为 I2C，是 Inter-Integrated Circuit Bus 的简称，中文可以称之为集成电路（间）总线，它是一种芯片与芯片通信的通信协议，
//! 它由 Philips Semiconductor（现在的 NXP Semiconductor）制定协议标准

//! I2C 是一种 两线、同步、串行、半双工、分主从、带响应的总线通信协议，让我们来逐个解释其意义
//!
//! 两线：I2C 通信需要两根线，分别为 SCL 时钟总线 和 SDA 数据总线，两根线就可以满足设备间的收发需求
//! 同步：从两线的说明我们就可以看出来，它需要一根独立的 SCL 时钟总线，而 I2C Bus 上的设备均需要这根线上的时钟信号来同步收/发的时序
//! 串行：从两线的说明我们就可以看出，I2C 的数据仅由一根 SDA 线缆传输，因此 I2C 必然是串行的协议
//! 半双工：由于 I2C 的数据传输线仅有一根，因此收发是无法同步进行的，因此 I2C 是一个半双工协议
//! 分主从：I2C 是一个分主机和从机的通信协议，其中 SCL 的时钟信号仅能由主机发出、从机接收（在某些情况下，主机会监测 SDA 数据线的情况来延迟 SCL 时钟线的信号），
//!         而且 SDA 上仅主机有主动权，仅有主机能发起通信，从机只能被动的接收数据，或者响应主机的要求发送数据
//!         不过这并不意味着主机和从机是永久固定的，STM32 的 I2C 硬件是支持主从切换的（准确来说是竞争主机状态，不竞争或竞争失败时退回到默认的从机状态）
//! 带响应：这一点很特殊，I2C 虽然是半双工通信，但是 I2C 设计了类似于“发送回执”功能的功能，在主机每字节发送/从机每字节发送之后，对端设备都可以简单给出一个“回复”，
//!         以告知发送方，自己是否接收到了这个字节
//! 总线通信协议：虽然 I2C 通信仅需要两根线，但这两根线实际上肩负了总线的功能，也就是说，这两根线理论上可以挂载无数的设备

//! 然后，我们依照上面给出的 I2C 的特点，看看 I2C 通信协议是怎么满足上面的特点的
//!
//! 首先是所有接入 I2C 的设备，他们连接在 SCL 和 SDA 总线上的端口，都必须处于开漏状态，并且 SCL 和 SDA 总线需要在外部分别加上一个上拉电阻，
//! 这样所有设备在操作两个总线的时候，可以这样理解：如果没有设备操作总线，则总线总是保持高电平，一旦有设备开始操作，则总线电平会被设备拉低，
//! 这样的设计有几个好处：
//! 1. 【开漏的好处】避免了在某些出错的情况下，有些设备想要拉低总线，而有些设备想要拉高总线，导致设备间产生短路状态
//! 2. 【开漏的好处】在 GPIO 的开漏输出模式下，同一个 GPIO 端口的逻辑输入可以同时开启，以不断监测总线的状态，确保总线总是处于自身希望的状态
//! 3. 【线与的好处】如果一个设备认为当前总线应该处于高位，但事实上总线处于低位，则表明有其它的设备拉低了总线，则当前设备可以认为连接上总线的设备产生了冲突，应该中断传输
//!
//! 由于总线处于高电平，表示“无人操作”、或者说是“空闲”状态，因此在实际传输数据过程中，设备对于 SDA 线上的有效电平的采样，总是在 SCL 高电平的时候发生的，
//! 而对 SDA 线的电平的修改，总是在 SCL 低电平（包含下降沿）时产生的，而且 SDA 电平必须在 SCL 上升沿到来之前保持稳定的有效值
//!
//! 由于 I2C 是支持多主机设备的，因此所有想抢占主机的设备必须有一种方式，能通知其它设备，当前是否有设备已经抢占了总线，
//! 因此 I2C 需要一种特殊的、区别于普通传输的信号来实现这个目标，这个信号就是：当 SCL 为高电平时（逻辑采样时间），在 SDA 上产生一个下降沿、或一个上升沿（通常情况下是不被允许的）
//! 其中下降沿表示，某个设备要抢占主机位置、并开始它的传输；而上升沿表示，当前主机已经结束传输、要释放主机的位置
//! 这个操作可以这么理解，在 SDA 原本不可变化的时间段里，有设备故意拉低了 SDA，这种主动性表示了对 I2C 总线的抢占，
//! 反过来说，在 SDA 本不可变的情况下，主设备故意释放了 SDA，让其电平回弹到高电平，则通知所有设备，主设备释放了 I2C 总线的控制
//! 上面这两个操作，在 I2C 的术语中，分别叫做 产生起始条件（START condition）和 产生终止条件（STOP condition）
//!
//! 通过起始条件和终止条件，在某个时间段内，I2C 的主从设备得以区分，主机是唯一，但从机却可以有很多，那么主机该如何确定要和哪一个从机进行通信呢？
//! 于是 I2C 还引入了另一个设计——I2C 总线地址（ADDRess）
//! I2C 总线地址是一个记录在每个 I2C 从机设备中的数据；当从机收到来自主机的起始信号后，所有的从机都需要确认即将接收到的下一个字节（某些配置下会是 2 字节，此处不讨论）的内容，
//! 该字节的发送方式与通常数据的发送方式无异，但其具有特殊意义，它表示的是主机想要通信的从机的 I2C 地址（以及一个其它的内容，下方会补充），
//! 所有的 I2C 从机设备都会拿着从 I2C 收到的地址，与自己的地址进行对比，若相同，则表示主机是想与自己通信，若不同，则表示主机并非想与自己通信（此时从机会产生一个动作，下方会补充）
//! 这样，每次产生了起始条件之后，通过主机端发送的地址信息，就确认了要通信的从机端（不太准确，下方会补充）
//! I2C 的设计中，设备的 I2C 地址类似于 MAC 地址，是在设备出厂时就确定的，一般一个厂家的某一型号的设备具有相同的 I2C 地址，不同厂家或不同型号的 I2C 设备具有不同的地址
//! 不过也可能会出现在同一个 I2C 总线上挂上同一个厂家的同一个型号的多个设备，为了防止地址的冲突，实际上几乎每个 I2C 设备上都有配置电阻/拨钮，可以修改该设备地址的后几位
//!
//! 在确认地址的流程中，由于 I2C 是通过一条 SDA 总线完成的收发操作的半双工模式，因此 I2C 协议还需要确认主从设备双方应该怎么协商，以确认后面的数据的发送方向（主发从收，还是从发主收）
//! 因此地址字节的最后一位，起始是本段传输的传输方向，该位置 0 表示主发从收（主机拉低 SDA，表示主机主动），1 则表示主收从发（主机未拉低 SDA，表示主机被动），
//! 因此，实际上 I2C 的有效 I2C 地址只有 7 位（某些配置下有 10 位，此处不讨论），且 I2C 从设备在收到地址的同时，也就知道了主机的意图（主机是读还是写），也同时会做好准备
//!
//! 然后我们还要解决一个问题，那就是主机怎么知道，被自己通过地址“叫到的”设备，的确存在于当前的总线上，而且的确处于可应答的状态的？
//! 这就涉及到 I2C 的响应机制，当主设备发送完地址后，会产生一个时钟周期，在该周期中，主设备不会操作 SDA 线，
//! 而对应的从机需要在该时钟周期内操作 SDA 线，在 SCL 低电平期间期间拉低 SDA 线，并在 SCL 高电平期间保持 SDA 的低电平，
//! 若主机在这个 SCL 周期的高电平中，监测到 SDA 保持低电平，则表示该地址被匹配上，且从机处于可应答状态，
//! 若主机在这个 SCL 周期的高电平中，监测到 SDA 保持高电平，则表示没有任何从机进行响应，主机就会认为地址匹配失败，从而需要执行其它的处理过程（比如生成一个结束状态，或者立刻再生成一个起始状态）
//! 这个使用一个额外的 SCL 周期，等待接收方响应的流程，就被称之为 ACK 过程，ACK 是 ACKnowledge 的简写，其中拉低电平就被称为 ACK，未拉低就被称为 NACK（Not ACK）
//! ACK 流程不仅出现在地址的发送中，还存在于每一个字节传输的结尾，只有接收端给出了 ACK，发送端才有必要发送下一个字节，否则发送端应该暂停/终止发送
//! 在 主发从收 的模式下，ACK 就只是从机的一个应答，但是在 从发主收 的模式下，主机发送的 NACK 还有另一个意义：主机不需要更多的数据了，从机结束发送吧。

//! 好了，让我们整理一下上面说的，看一看一个完成的 I2C 通信流程都有哪几个步骤：
//! 主设备想对着某个设备发送数据：设备产生 START condition -> 主设备发送 ADDR/W -> 从机响应 ACK -> 主设备发送（写入）第一个字节 -> 从机 ACK -> 主机写入第二个字节 -> 从机 ACK -> ... -> 主机发送最后一个字节 -> 从机 ACK -> 主机产生 STOP condition
//! 主设备想接收某个设备发来数据：设备产生 START condition -> 主设备发送 ADDR/R -> 从机响应 ACK -> 从设备发送（写入）第一个字节 -> 主机 ACK -> 从机写入第二个字节 -> 主机 ACK -> ... -> 从机发送第 N 个字节 -> 从机 NACK -> 主机产生 STOP condition
//!
//! 然后，这里有一个事件上的常态，那就是一般来说，I2C 通信会有一个这样的需求，主机首先对从机发送了一些指令，然后主机希望从机返回一些数据，于是流程（简化过）变成 主 START -> ADDR/W -> 主发... -> 主 STOP -> 主 START -> ADDR/R -> 主收... -> 主 STOP
//! 这里会有一个小小的问题，那就是在第一次主机产生 STOP condition 的时候，其它设备可能会争抢 I2C 总线的控制权，导致这个传输链中断
//! 为了避免这种情况发生，I2C 协议中还给出了一个 Repeated START condition，也就是说，在 主设备 应该发送 STOP 的时候，可以直接再产生一个普通的 START condition，然后接着执行后面的操作，而这个接着产生的 START condition，就被称为 Repeated START，在流程图中通常标记为 S_{r}
//! 于是流程就转化为 主 START -> ADDR/W -> 主发... -> 主 START -> ADDR/R -> 主收... -> 主 STOP
//! 另外，Repeated START 在一次通信中可以触发多个，也就是说，当一个设备成功抢占 I2C 总线之后，它是可以通过不断使用 Repeated START，和不同的设备进行多次通信的，但这个主设备在最后必须要使用一个 STOP condition 释放对 I2C 的控制

//! 最后的最后，还有一个小知识点，那就是，虽然在逻辑上，主设备对于 SCL 具有绝对的控制权，但实际上，I2C 还设计了一个 clock stretch 功能，它让从设备在 SCL 处于低电平的时候，也可以一同拉低 SCL，这样 SCL 就无法被释放而返回高电平
//! 当主设备发现 SCL 被其它设备保持在低位，而不能回弹的时候，主设备就要暂停 I2C 的收发流程，直到 SCL 正常回弹
//! 设计这个 stretch 功能，主要是用来处理下面两个情况：
//! 1. 主设备的 I2C 总线信号过快，从设备无法正确区分信号，从设备延长 SCL 低电平时间就等价于降低了 I2C 的传输速率
//! 2. 从设备需要一段时间处理主设备发来的请求，此时从设备可以一直保持 SCL 低电平，直到自身处理完成，再释放 SCL，让 I2C 总线继续运转

//! 好了，上面说了这么多关于 I2C 通信协议的事情，现在让我们看一看，在这个案例中我们要实现的效果
//!
//! I2C1 作为主机，向作为从机的 I2C2 发送一组数据
//! 注意到 I2C 是一个半双工的协议，因此我们不可能只使用一个 I2C 外设就完成传输工作（某一个时刻 I2C 要么发，要么收）

//! 接线图
//!
//!     I2C1 <-> I2C2
//! SCL  PB6 <-> PA8  SCL
//! SDA  PB7 <-> PC9  SDA

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};
use cortex_m::{interrupt::Mutex, peripheral::NVIC};

use panic_rtt_target as _;
use rtt_target::rtt_init_print;

use stm32f4xx_hal::{
    interrupt,
    pac::{CorePeripherals, Peripherals},
};

mod utils;
use utils::{
    printing::{master_rprintln, slave_rprintln},
    setup_pll,
};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

// 我们胡乱定义的 7 位 I2C 地址位
// 虽然是胡乱定义的，但绝对不可以将这 7 位设置为如下模式 11110XX
// 这个地址是留给 10 bit 地址模式使用的，7 位下绝对不可以设置
const I2C_SLAVE_ADDRESS: u8 = 0b1010101;

#[cortex_m_rt::entry]
fn main() -> ! {
    // 由于 I2C 的通信速度相较于 RTT 来说还是比较快的，因此我们需要扩大 RTT 的缓存容量，
    // 防止因为要打印的数据太多，导致数据被 RTT 丢弃或截断
    //
    //
    // 这里我们在初始化 RTT 的时候，给出两个参数，
    //
    // 第一个参数指的是，怎么处理即将导致 RTT 缓存溢出的数据；
    // 默认值是丢弃那个数据，这里我们设置为截断那个数据，
    // 这样我们就有机会分辨出，某行没有打印出来，是 I2C 没有动作导致的（整行都没有），还是 RTT 主动丢弃的（有可能显示出半行来）
    //
    // 第二个参数指的是，RTT 缓存空间的字节数，默认值为 1024，就我们的 I2C 通信内容来说稍稍有点小，这里改为 4096 会比较好
    rtt_init_print!(NoBlockTrim, 4096);

    let dp = Peripherals::take().expect("Cannot Get Peripherals");

    setup_pll::setup(&dp);

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);
    });

    // 由于 I2C 对于时序的要求较高，而我们为了实验，两个 I2C 又都是在同一块芯片里面
    // 因此这里有必要设置一下 I2C 的中断顺序

    let mut cp = CorePeripherals::take().expect("Cannot Get Core Peripherals");

    // 修改了优先级，I2C2 作为接收方，它的优先级需要高于 I2C1
    // 这样我们就保证了如果有输入输入，则优先处理接收操作，同时也阻止了发送的产生
    //
    // 优先级关系：
    // Slave_Error > Slave_Int > Master_Error > Master_Int
    unsafe {
        cp.NVIC.set_priority(interrupt::I2C2_ER, 2);
        cp.NVIC.set_priority(interrupt::I2C2_EV, 4);
        cp.NVIC.set_priority(interrupt::I2C1_ER, 8);
        cp.NVIC.set_priority(interrupt::I2C1_EV, 16);
    }

    // 为两个 I2C 设置 GPIO 引脚
    setup_gpio_for_i2c1();
    setup_gpio_for_i2c3();

    // 分别初始配置两个 I2C 外设
    setup_i2c_master();
    setup_i2c_slave();

    // 在我们完成了全部的初始化配置之后，我们需要手动触发一下 I2C1，让其产生 START condition
    // 以开始本流程的传输
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let master = &dp.I2C1;

        master_rprintln!("Main\ttrigger START condition");
        master.cr1.modify(|_, w| w.start().start());
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

fn setup_gpio_for_i2c1() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.ahb1enr.modify(|_, w| w.gpioben().enabled());

        let gpiob = &dp.GPIOB;

        gpiob.afrl.modify(|_, w| {
            w.afrl6().af4();
            w.afrl7().af4();
            w
        });

        // 依照 I2C 的说明，所有的输出状态必须处于开漏状态
        gpiob.otyper.modify(|_, w| {
            w.ot6().open_drain();
            w.ot7().open_drain();
            w
        });

        // 依照 I2C 的说明，SCL 线路和 SDA 线路必须处于弱上拉状态
        // 虽然 SCL 和 SDA 分别只需要一个上拉电阻就好了，这里我们还是启用了所有引脚的上拉电阻
        //
        // 讲句老实话，内置的上拉电阻太大了，生成的波形几乎都没法保持一个高电平
        // 但不得不说 STM32 的 I2C 电路的确很强，这么差劲的波形都能正确识别
        //
        // 具体上拉电阻应该使用什么值，应该根据电路的实际阻抗和实际工作电压来确定，
        // NXP 的 I2C 手册中有提到计算公式，最常见的上拉电阻阻值大约在 4.7KOhm 左右
        gpiob.pupdr.modify(|_, w| {
            w.pupdr6().pull_up();
            w.pupdr7().pull_up();
            w
        });

        gpiob.ospeedr.modify(|_, w| {
            w.ospeedr6().high_speed();
            w.ospeedr7().high_speed();
            w
        });

        gpiob.moder.modify(|_, w| {
            w.moder6().alternate();
            w.moder7().alternate();
            w
        });
    })
}

fn setup_gpio_for_i2c3() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        // 依照 I2C 的说明，所有的输出状态必须处于开漏状态
        // 依照 I2C 的说明，SCL 线路和 SDA 线路必须处于弱上拉状态
        // 虽然 SCL 和 SDA 分别只需要一个上拉电阻就好了，这里我们还是启用了所有引脚的上拉电阻

        // 讲句老实话，内置的上拉电阻太大了，生成的波形几乎都没法保持一个高电平
        // 但不得不说 STM32 的 I2C 电路的确很强，这么差劲的波形都能正确识别
        //
        // 具体上拉电阻应该使用什么值，应该根据电路的实际阻抗和实际工作电压来确定，
        // NXP 的 I2C 手册中有提到计算公式，最常见的上拉电阻阻值大约在 4.7 KOhm 左右

        dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
        let gpioa = &dp.GPIOA;
        gpioa.afrh.modify(|_, w| w.afrh8().af4());
        gpioa.otyper.modify(|_, w| w.ot8().open_drain());
        gpioa.pupdr.modify(|_, w| w.pupdr8().pull_up());
        gpioa.moder.modify(|_, w| w.moder8().alternate());

        dp.RCC.ahb1enr.modify(|_, w| w.gpiocen().enabled());
        let gpioc = &dp.GPIOC;
        gpioc.afrh.modify(|_, w| w.afrh9().af4());
        gpioc.otyper.modify(|_, w| w.ot9().open_drain());
        gpioc.pupdr.modify(|_, w| w.pupdr9().pull_up());
        gpioc.moder.modify(|_, w| w.moder9().alternate());
    })
}

fn setup_i2c_master() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.apb1enr.modify(|_, w| w.i2c1en().enabled());

        let master = &dp.I2C1;

        // I2C 模块的时钟（注意，不是 SCL 的频率），单位 MHz
        // 该值必须等于 I2C 所在总线的时钟频率，在这里是 APB1 的 32 MHz
        // 需要注意的是，
        // 如果设备处于标准模式（Sm: Standard Mode）则最小时钟为 2 MHz
        // 如果设备处于快速模式（Fm: Fast Mode），则最小时钟为 4 MHz
        master.cr2.modify(|_, w| unsafe { w.freq().bits(32) });

        // 【注意】依照 I2C 发送速率的不同，在加上我们调整过中断的优先级，以及 Cortex 处理中断的所需要的时间的不同
        // I2C 连续发送和连续接收的顺序可能不同

        // CCR: Clock Control Regsiter
        master.ccr.modify(|_, w| unsafe {
            // 实际控制的是，电平上升时间+高电平时间，或者电平下降时间+低电平时间，等于多少个 APB1 的时钟
            // 比如说，要达到 500 KHz 的 SCL，我们假定 上升+高电平 占一个 SCL 周期的一半的时间（Sm 模式下固定为一半的时间）
            // 那么 上升+高电平 的时长为 1/(500 KHz)/2 = 1 us，而且 APB1 的时钟周期为 1/(32 MHz) = 0.03125 us
            // 那么 CCR 应该设置的值为 (1 us) / (0.03125 us) = 32
            //
            // 该值仅在 I2C 设备处于主控模式时才有效
            //
            // 这里我们就设置值为 32
            w.ccr().bits(32)
        });

        // 实际上控制的是，为了确保 SCL 频率的稳定，I2C 模块电路应该假定的电平上升的最长时间，所对应的 APB1 时钟周期
        // 这个值必须参考 I2C 的数据手册，获得 I2C 的最大上升沿时长，计算出等待周期，并 +1
        //
        // 比如，从 STM92F411 的数据手册的 I2C characteristics 表中我们可以得知，
        // 在标准模式下，I2C 的 SDA 和 SCL 的上升时间 t_{r(SDA)} 和 t_{r(SCL)} 的最大值为 1000 ns，
        // 此处 APB1 的时钟频率为 32 MHz，则最大上升时间对应的 APB1 时钟周期为 (1 us / 0.03125 us) = 32，再 +1，就为 33
        //
        // 该值仅在 I2C 设备处于主控模式时才有效
        //
        // TRISE: maximum RISE Time
        //
        // 这里我们设置为 33 即可
        master.trise.write(|w| w.trise().bits(33));

        unsafe {
            NVIC::unmask(interrupt::I2C1_EV);
            NVIC::unmask(interrupt::I2C1_ER)
        };

        // 由于 I2C 是半双工运行的，导致了 I2C 的两个特性
        // 第一个是 I2C 的运行状态比较多，每个运行状态都需要对应一个 Interrupt Flag
        // 第二个是半双工导致一个 I2C 的外设的 发送空（TX_E）和 接收非空（RX_NE）不可能同时挂起，
        //         因此它们的 Interrupt Flag 是被同一个开关控制的
        master.cr2.modify(|_, w| {
            // 对应了大量的 I2C 事件，比如 START/STOP condition、ADDR，以及 TX_E 和 RX_NE
            // ITEVTEN: InterrupT EVenT ENable
            w.itevten().enabled();
            // 要让 TX_E 和 RX_NE 设置中断标识位，还需要下面这个开关
            // ITBUFEN: InterrupT BUFfer ENable
            w.itbufen().enabled();
            // 挂起与 I2C 通信错误相关的标识位
            // ITERREN: InterrupT ERRor ENable
            w.iterren().enabled();
            w
        });

        master.cr1.modify(|_, w| w.pe().enabled());

        // 如果 I2C 仅作为主控、且发送端的话，开启 ACK 其实没有啥意义
        // 因为 ACK 都是接收方发出的
        // master.cr1.modify(|_, w| w.ack().ack());
    });
}

fn setup_i2c_slave() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        dp.RCC.apb1enr.modify(|_, w| w.i2c3en().enabled());

        let slave = &dp.I2C3;

        // 让 I2C 的外设频率跟随 APB1
        slave.cr2.modify(|_, w| unsafe { w.freq().bits(32) });

        // 为处于 slave 模式下的 I2C3 设置自己的 I2C 地址
        //
        // OAR1: Own Address Register
        slave.oar1.modify(|_, w| {
            // 首先确认我们要使用的 ADD 为 7 位 I2C 地址
            w.addmode().add7();
            // 参考 Reference Manual，在 7 位模式下，我们设置的是 ADD 寄存器的 第 7 位到 第 1 位
            // 因此这里我们要左移一位，让地址对齐其要求
            w.add().bits((I2C_SLAVE_ADDRESS as u16) << 1);
            w
        });

        // 开启 I2C3 的中断
        unsafe {
            NVIC::unmask(interrupt::I2C3_EV);
            NVIC::unmask(interrupt::I2C3_ER)
        };

        // 启用 I2C 的中断标识位
        slave.cr2.modify(|_, w| {
            w.itevten().enabled();
            w.itbufen().enabled();
            w.iterren().enabled();
            w
        });

        // 启用 I2C 外设
        slave.cr1.modify(|_, w| w.pe().enabled());

        // 【重要】启用 ACK 响应，必须要在 I2C 外设启用的状态下设置才有效
        // 而且每次 I2C 外设关闭之后，下次再开启，则还需要再设置一遍 ACK 响应
        slave.cr1.modify(|_, w| w.ack().ack());
    });
}

// 实际将要发送的数据串
const OUT_LIST: [u8; 9] = [0x01, 0x02, 0x03, 0x04, 0x5, 0x6, 0x7, 0x8, 0x9];
// 然后我们为接收端设置一个 16 字节的 buf
static G_RECEIVE_BUF: Mutex<RefCell<[u8; 16]>> = Mutex::new(RefCell::new([0u8; 16]));

// 发送端和接收端各自的收发字节数计数
static G_SENDING_INDEX: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));
static G_RECEIVING_INDEX: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));

// 发送端和接收端各自触发中断的计数
static G_SENDING_INT_CNT: Mutex<Cell<usize>> = Mutex::new(Cell::new(1));
static G_RECEIVING_INT_CNT: Mutex<Cell<usize>> = Mutex::new(Cell::new(1));

// 主设备一直连续发送 hello 这 5 个字母
#[interrupt]
fn I2C1_EV() {
    cortex_m::interrupt::free(|cs| {
        // 记录中断次数用
        let interrupt_counter = G_SENDING_INT_CNT.borrow(cs);
        let interrupt_cnt = interrupt_counter.get();

        // 记录发送了多少字节用
        let sending_indexer = G_SENDING_INDEX.borrow(cs);
        let sending_idx = sending_indexer.get();

        let dp_cellref = G_DP.borrow(cs).borrow();
        let dp = dp_cellref.as_ref().unwrap();

        let master = &dp.I2C1;

        // 中断产生，第一步必然是获取一下当前状态寄存器 SR1 的数据
        let master_sr1 = master.sr1.read();

        // 【特别注意】
        // 我们并不可以假定在一个中断中，仅出现了一个标识位
        // 准确来说对于 I2C 而言，在一次中断中，出现了多少标识位，就要处理多少标识位
        // 因此我们并不能随便将多个判定标识位的 if 块用 else if 串联在一起
        // 一开始我在这里吃了大亏，触发了很多不应该触发的错误

        // 由于一个中断中要判定 I2C 外设的多个状态，因此我们并不能直接在流程的末尾确定，触发该中断的状态是否被处理了
        // 因此我们这里设置一个变量，只要下方任何的处理流程执行了处理，handled 就会被改写为 true
        // 我们最后只需要判定一下 handled 有没有被改写，就可以直到中断处理的情况
        let mut handled = false;

        // 【注意】与其它的片上外设不同，I2C 的标识位并非通过直接清理标识位本身来去除的
        //         清理它们需要通过特定顺寻地读写特定寄存器来实现

        // 判定 START 条件是否完成
        // 只有 START 条件完成，才能在 SDA 上发送 ADDR
        // SB: Start Bit
        if master_sr1.sb().is_start() {
            // 要清理 SB，
            // 需要先读一下 SR1，然后立刻写 DR 来实现
            master.sr1.read();

            // 在 SB 挂起的时候，写入 DR 就是 ADDR/W 或 ADDR/R 的内容了
            // I2C 从机的地址需要写在 DR 寄存器的高 7 位，
            // 最后一位表示的是，从现在开始，直到下一个 START condition（术语上称为 Repeated START）或 STOP condition 之间，
            // 主机是发送字节的状态（最后一位为 0），还是接收字节的状态（最后一位为 1）
            master
                .dr
                .write(|w| w.dr().bits(I2C_SLAVE_ADDRESS << 1 & !(1 << 0)));

            master_rprintln!(
                "Int {}\tSTART condition settled, sending ADDR/W",
                interrupt_cnt
            );

            handled = true;
        }

        // 判定 ADDR 是否被某个 Slave ACK 了
        // 仅当 ADDR 确实被某个 Slave ACK 了，才能进入正常的收发流程
        if master_sr1.addr().is_match() {
            // 清理 ADDR 位的操作顺序为
            // 读取 SR1，紧接着读取 SR2
            master.sr1.read();
            master.sr2.read();

            master_rprintln!("Int {}\treceive ARRD/W ACK, will send data", interrupt_cnt);

            handled = true;
        }

        // 这段稍稍有点绕，它与普通数据发送，以及产生 STOP condition 都相关
        //
        // 首先是 else if 的部分，它判定 TX_E 是否被设置，被设置说明我们可以向 DR 中写入新数据了，是正常发送流程
        //
        // 然后是 if 的部分，它判定的是 CR1（注意是 控制寄存器1 CR1 不是 状态寄存器1 SR1）的 STOP 位是否被人为设置
        // 如果被人为设置，说明发送已经完成，我们希望产生 STOP condition，
        // 而在 CR1 的 STOP 被设置，到 STOP condition 正真产生，中间可能会有时间差（比如 SCL 比较慢，或者 slave 拉低了 SCL）
        // 此期间 TX_E 依旧会触发中断，因此我们要先判定 CR1 的 STOP 是否被设置，
        // 如果 CR1 的 STOP 被设置，我们应该返回的是等待 STOP condition，而非 TX_E

        // 注意，这里检查的是 CR1 里的 STOP bit，表示的是我们有没有让 I2C 准备好产生 STOP condition
        // 不是检查 SR1 里的 STOPF bit
        if master.cr1.read().stop().bit_is_set() {
            // 这里必须要等待 STOP condition 实际建立，从而让 TX_E 的清空，
            // 不能直接关闭 ITBUFEN，否则 STOP condition 建立后，会额外触发一个中断，而这个中断触发时 SR1 和 SR2 均为 0
            // 导致我们无法处理最后那个中断

            master_rprintln!(
                "Int {}\tSTOP condition triggered, waiting it settled...",
                interrupt_cnt
            );

            handled = true;
        }
        // 如果 TX_E 为空，就表示我们可以向 DR 中写入新的数据了
        //
        // 注意 DR 为空并不意味着 I2C 不在传输，
        // 实际上 I2C 自己有一个寄存器是实际用来传输数据的，当它空了的时候，就会从 DR 拷贝新数据进来
        // 比如说，在刚开始发送数据的时候，很可能出现 DR 为空、且 I2C 内部的传输寄存器也为空的情况
        // 此时 TX_E 被挂起，我们向 DR 中写数据，然后 DR 中的数据会立刻拷贝至 I2C 发送寄存器里，
        // 此时由于 DR 为空，TX_E 就又被挂起了，接着我们就又能向 DR 中再写入一个字节了，
        // 而第二个写入的字节，会等待第一个写入的字节发送完毕之后，在进入 I2C 内部的发送寄存器中进行发送
        // 从上面的说明中我么可以看到，I2C 在传送第一个字节的时候，TX_E 是被挂起的
        else if master_sr1.tx_e().is_empty() {
            // 从整个源数据中拷贝出当前应该发送的字节
            let cur_byte = OUT_LIST[sending_idx];

            // 打印一下
            master_rprintln!("Int {}\tsending: {}", interrupt_cnt, cur_byte);

            // TX_E 挂起就表示 DR 为空，可以安全的写入 DR
            // 写入了 DR 就清理了 TX_E
            master.dr.write(|w| w.dr().bits(cur_byte));

            // 然后我们判定一下，当前发送的是否为源数据的最后一个
            // 如果是，我们就直接要求产生 STOP condition
            if sending_idx == OUT_LIST.len() - 1 {
                master_rprintln!(
                    "Int {}\tData sending finish, trigger STOP condition",
                    interrupt_cnt
                );
                master.cr1.modify(|_, w| w.stop().stop());
            }

            sending_indexer.set(sending_idx + 1);

            handled = true;
        }

        if !handled {
            master_rprintln!(
                "Int {}\tI2C1 Sending EVent not covered, master_sr1: {:014b}, master_sr2: {:08b}",
                interrupt_cnt,
                master_sr1.bits(),
                master.sr2.read().bits()
            );
        }

        interrupt_counter.set(interrupt_cnt + 1);
    });
}

#[interrupt]
fn I2C1_ER() {
    cortex_m::interrupt::free(|cs| {
        let dp_cellref = G_DP.borrow(cs).borrow();
        let dp = dp_cellref.as_ref().unwrap();

        let master = &dp.I2C1;
        slave_rprintln!(
            "I2C1 Sending Side Error SR1: 0b{:014b},\nSR2: 0b{:08b}",
            master.sr1.read().bits(),
            master.sr2.read().bits()
        );
    });
}

// 从设备不断接收，直到 STOP condition 产生
#[interrupt]
fn I2C3_EV() {
    cortex_m::interrupt::free(|cs| {
        let interrupt_counter = G_RECEIVING_INT_CNT.borrow(cs);
        let interrupt_cnt = interrupt_counter.get();

        // 获得接收 buf 的修改权，并取得已接收的索引号
        let mut receive_buf_mut = G_RECEIVE_BUF.borrow(cs).borrow_mut();
        let receiving_indexer = G_RECEIVING_INDEX.borrow(cs);
        let mut receiving_idx = receiving_indexer.get();

        let dp_cellref = G_DP.borrow(cs).borrow();
        let dp = dp_cellref.as_ref().unwrap();

        let slave = &dp.I2C3;

        let slave_sr1 = slave.sr1.read();

        // 同上，这里我们也要额外判定一下本次中断是否被处理过了
        let mut handled = false;

        // ADDR 被挂起，说明 SDA 上发来的 I2C 地址与自己的 I2C 地址匹配
        if slave_sr1.addr().is_match() {
            // 清理 ADDR 位的流程为，读 SR1 然后读 SR2
            slave.sr1.read();
            slave.sr2.read();
            // 由于我们为从设备设置了产生 ACK
            // 因此在我们清理了 ADDR 后，ACK 就自动从 SDA 线上发出去了

            slave_rprintln!("Int {}\tADDR/W received, ACKing", interrupt_cnt);

            handled = true;
        }

        // RX_NE 被挂起，说明我们可以从 DR 中读取新的数据了
        // 读取了我们就把数据放到接收 buf 里，并打印一下
        if slave_sr1.rx_ne().is_not_empty() {
            // 读 DR 就会清理 RX_NE 标识位
            let cur_char = slave.dr.read().dr().bits();

            receive_buf_mut[receiving_idx] = cur_char;
            receiving_indexer.set(receiving_idx + 1);

            slave_rprintln!("Int {}\treceived: {:?}", interrupt_cnt, cur_char);

            handled = true;
        }

        // 如果 STOPF 被挂起，说明 STOP condition 已经在 SCL 线和 SDA 线上产生
        // 我们需要清理该标识位，并打印一下全部获得的数据
        if slave_sr1.stopf().is_stop() {
            // 首先打印一下 STOP condition 已经被检测到
            slave_rprintln!("Int {}\tSTOP condition detected", interrupt_cnt);

            // 然后清理一下 STOPF
            //
            // 清理 STOPF 的步骤比较特殊，它需要读 SR1，并写一下 CR1
            // 不过这里我们并没有什么需要写 CR1 的，这里只需要调用一下 CR1 的 .modify() 方法即可
            slave.sr1.read();
            slave.cr1.modify(|_, w| w);

            // 最后我们打印一下全体数据
            // 这里需要注意的是，STOPF 可能和最后一个数据一同到来
            // 因此我们这里一定要刷新一下（重新获取一下）接收索引的值
            // 以正确打印整个收到的数据
            receiving_idx = receiving_indexer.get();
            slave_rprintln!(
                "Int {}\tprint all data: {:?}",
                interrupt_cnt,
                &receive_buf_mut[0..receiving_idx]
            );
            handled = true;
        }

        if !handled {
            slave_rprintln!(
                "Int {}\tI2C3 Receiving EVent not covered, slave_sr1: {:014b}, slave_sr2: {:08b}",
                interrupt_cnt,
                slave_sr1.bits(),
                slave.sr2.read().bits()
            );
        }

        interrupt_counter.set(interrupt_cnt + 1);
    });
}

#[interrupt]
fn I2C3_ER() {
    cortex_m::interrupt::free(|cs| {
        let dp_cellref = G_DP.borrow(cs).borrow();
        let dp = dp_cellref.as_ref().unwrap();

        let slave = &dp.I2C3;
        slave_rprintln!(
            "I2C3 Receiving Side Error SR1: 0b{:014b},\nSR2: 0b{:08b}",
            slave.sr1.read().bits(),
            slave.sr2.read().bits()
        );
    });
}
