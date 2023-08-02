//! USART: 通用同步异步收发器
//!
//! 通用同步异步收发器 Universal Synchronous / Asynchronous Receiver / Transmitter
//! 是一种以串行模式收发数据的设备，通常情况下，它承载的“串行端口（Serial Port）”的数据
//!
//! STM32 上的 USART 片上外设支持多种传输模式，
//! 包括 同步收发（USRT）、异步收发（UART）、区域互联网络（LIN）、红外通讯（IrDA）、智能卡模拟（Smartcard emulation）
//! 其中 异步收发 UART 是最常用的功能，而其它功能的则并不是那么常见，这里不做讨论

//! UART 最多需要 4 个接口，
//! Tx：Transmit 本机发送至远端的发送端口
//! Rx：Receive 接收来自远端数据的接收端口
//! RTS：Request To Send 接收端提示发送端可以发送数据的端口
//! CTS：Clear To Send 发送端用来接收来自接收端 RTS 信号的端口
//!
//! 【额外的】GND: 两个通信设备需要共地，以保证电平值的统一
//!
//! Note: Tx 和 Rx 其实来自摩尔斯码，分别为 - ..-... 和 .-. ..-... 表示发送和接收
//!
//! 从上面的说明大致可以猜到，一对 UART 设备的 4 对接口，刚好可以构成两对交叉连接
//!
//! UART1  Tx >---    ---< Tx  UART2
//!                \/
//!                /\
//! UART2  Rx <---    ---< Rx  UART2
//!
//! UART1 RTS >---    ---< RTS UART2
//!                \/
//!                /\
//! UART2 CTS <---    ---< CTS UART2
//!
//! 其中 Tx/Rx 对用于数据传输，RTS/CTS 用于流控制，而且，在最常见的 UART 通信中，只会用到 Tx/Rx 这个对，不会使用 RTS/CTS 对

//! 为什么叫异步收发呢，这是因为，异步收发不需要一条时钟线来同步收发两端的时钟，收发两端的“传输速率”是通过外部渠道传递的
//! 如果使用过串口，那么就可以发现，在使用前，我们需要手动配置串口的波特（Baud），这就一种通过外部渠道（人读说明书来配置）传递“传输速率”的方式
//! Note1: 准确来说，波特的单位是 Symbol/s，指的是单位时间内，发送的符号的数量（在电线里就是电平的变化次数），
//!       而在 UART 协议中，通信时只使用两个电平：高和低，因此 1 Baud = 1 Symbol/s = 1 * log2(2) bit/s = 1 bit/s，波特和比特率在数值上相等
//!       因此有时候我们会将波特的单位说成是 bit/s
//! Note2: 波特（Baud）就已经是一种速率了，因此“波特率”是一种不正确的称呼，不过这种错误非常常见，因此使用后者也无妨
//!
//! 由于异步收发的特点，会出现一个问题，那就是到底在何时采样电平、确定电平逻辑值，是完全由接收端自行判定的，
//! 因此 UART 必须要以比波特值高的频率采样信号线，才有可能确定电平的高低，以获得稳定的逻辑值
//! STM32F411RE 对 UART 电平的采样率为波特值的 16 倍（默认值）或 8 倍，以获得正确的电平值
//!
//! UART 的另一个问题是，由于是异步通信，波特值需要尽量精确，因此控制波特值的分频器的寄存器是有小数部分的，
//! 需要用 2 / 8 / 16 进制值来设置，与十进制小数的含义一样，小数位表示的是基数值的特定负指数的倍数

//! 这里我们要实现一个效果，那就是每秒向 UART 口输出一个 hello 字符串，并打印总计输出的次数
//! 需要启用 GPIO、USART1、TIM2，然后让 TIM2 每秒触发一个中断，在中断中我们通过 USART 输出一下我们想要的字符
//!
//! 电路连接方案：GPIO PA9 <-> DAPLink Rx

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::pac::{self, interrupt, Peripherals, NVIC};

// 将 Device Peripheral 存储在全局静态量中
static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

// 记录一下字符串的输出次数
static G_CNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let Some(dp) = pac::Peripherals::take() {
        // 切换到 HSE
        switch_to_hse(&dp);
        // 让 GPIO PA09 处于 USART1 的 Tx 模式
        set_gpio_in_alternate_mode(&dp);
        // 启动 USART1
        set_usart1_into_tx_mode(&dp);
        // 启动 TIM2
        set_tim2_1sec_trigger(&dp);

        // 这里有一个特殊的地方，为了防止 dp 还没有移动到全局变量前，TIM2 的中断就被触发了
        // 这里应该先确保 dp 的移出，在启动 TIM2 的计时器
        cortex_m::interrupt::free(|cs| {
            // 在这里，由于 G_DP 内部保存的值需要修改，因此需要使用 .borrow_mut() 获取可变借用
            G_DP.borrow(cs).borrow_mut().replace(dp);

            // 在 dp 移动到全局静态量中后，我们再用全局静态量来启动 TIM2 的计数器
            // 在 G_DP 注入完成之后，我们就不再需要修改 G_DP 这个变量了，因此这里的两次解引用都是不可变形式的
            let dp_ref = G_DP.borrow(cs).borrow();
            let dp = dp_ref.as_ref().unwrap();
            dp.TIM2.cr1.modify(|_, w| w.cen().enabled());
        });

        #[allow(clippy::empty_loop)]
        loop {}
    } else {
        panic!("Cannot Get Peripheral\r\n");
    }
}

fn switch_to_hse(dp: &Peripherals) {
    // 由于 UART 通信要求较为精准的时钟，这里我们尝试使用外部晶振作为 USART 模块的时钟来源
    let rcc = &dp.RCC;
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}
}

// 可以作为 USART1 Tx/Rx 引脚的有 AF07 模式下的 (PA9 PA15 PB6)/(PA10 PB3 PB7)
// 这里我们选择 PA9 作为 Tx，而且由于我们目前不需要接收，因此不用设置 Rx 引脚
fn set_gpio_in_alternate_mode(dp: &Peripherals) {
    dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    let gpioa = &dp.GPIOA;

    // 依照上面所说，将 PA09 的 AF 切换为 AF07
    gpioa.afrh.modify(|_, w| w.afrh9().af7());

    // 启用内部拉高电阻，这样在 USART 不传输时，Tx 也能保持高电平
    gpioa.pupdr.modify(|_, w| w.pupdr9().pull_up());

    // 启动 PA09 的 Alternate 模式
    gpioa.moder.modify(|_, w| w.moder9().alternate());
}

// 下面我们要将 USART1 配置为 UART 模式
// 并设置波特值 115200，停止位 1 位，单次发送 8 字节数据，不含奇偶校验
fn set_usart1_into_tx_mode(dp: &Peripherals) {
    // 开启 USART1 的时钟
    dp.RCC.apb2enr.modify(|_, w| w.usart1en().enabled());

    // 注意，由于 USART 支持较多的通信协议，导致了 USART 的配置寄存器较多，
    // 但是我们仅关注通信协议最简单的 UART，因此使用到的配置寄存器会比较少

    let serial1 = &dp.USART1;

    // 切换到 USART 的 UART 模式
    // 这样后面的设置能一次性调整对 USART 模块内部的状态
    serial1.cr1.modify(|_, w| w.ue().enabled());

    // 设置单次数据位 8 位
    serial1.cr1.modify(|_, w| w.m().m8());

    // 设置停止位为 1 位
    serial1.cr2.modify(|_, w| w.stop().stop1());

    // 设置波特分频器的值
    //
    // 由于 UART 是异步通信，所以波特值需要精确地调整
    // 因此 16 位 BBR 寄存器被分解为了 2 个部分，高 12 位是分频器的整数部分，低 4 位是分频器的小数部分
    // 高 12 位的整数部分比较好理解；低 4 位的小数部分，其实和整数位类似，表示的也是 2 的特定指数位的倍数，只不过小数是负指数
    // 这样说来，BBR 寄存器支持的最小小数为 2^(-4) = 0.0625，也就是说，在运算小数的时候，原始值最多取 5 位小数就足足有余了
    // 而且在计算小数部分的时候，由于 4 位 2 进制值刚好是 1 位 16 进制值，在转换时，可以直接乘 16，截取整数位即可
    //
    // 在设置波特分频器的值之前，我们首先要确认超采样模式的设置，也即是 USART_CR1 的 OVER8 的值的情况
    // 一般这个值为 0，也就是使用超采样为 16 次的模式
    //
    // 接着我们要计算一下，假设我们的目标波特值为 115200 Baud，根据以下公式
    // baud = f_CK / [ 8 * ( 2 - OVER8 ) * USARTDIV ]
    // 可知 USARTDIV = f_CK / [ 8 * (2 - OVER8) * baud ] = 8 MHz/[8*(2-1)*115200] ≈ 4.3402778
    //
    // 于是 USARTDIV 的整数位为 4，小数部分经过计算，其值为 0x5，也就是 5 了
    // 然后再反推一下，我们可以知道，此时 UART 的真实波特值为 115942 Baud，和目标值 115200 Baud 有大约 (115942-115200)/115200 = 0.644% 的误差
    // 其实上面的内容查一下 Reference Manual 中 USART 里的 Error rate 表也能看到，我们自己计算的话，在 USART 的时钟和波特值的选择会自由一些
    //
    // BBR: Baud Rate Register
    serial1.brr.write(|w| {
        w.div_mantissa().bits(4);
        w.div_fraction().bits(5);
        w
    });

    // 开启发送（但不用开启接收）
    serial1.cr1.modify(|_, w| {
        w.te().enabled();
        // 注意，这里不要开启 TXE 和 TC 的中断，
        // 在这里，我们是通过 TIM2 来控制 UART 的发送起始，通过 Cortex 核心轮询来判断发送完成的
        // w.txeie().enabled();
        // w.tc().enabled();
        w
    });
}

// 让 TIM2 定时器每 1 秒触发一个中断，在这个中断中我们会让 USART2 发送一些数据
fn set_tim2_1sec_trigger(dp: &Peripherals) {
    dp.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    let delay_timer = &dp.TIM2;

    delay_timer.cr1.modify(|_, w| w.dir().down());
    // 计算一下，8 MHz 的输入，1 Hz 的输出
    // PSC 为 7999，将输入时钟降频到 8 MHz / (7999 + 1) = 1000 Hz
    // ARR 为 999，将输出时钟降频到 1000 Hz / (999 + 1) = 1 Hz
    delay_timer.psc.write(|w| w.psc().bits(7999));
    delay_timer.arr.write(|w| w.arr().bits(999));

    delay_timer.cr1.modify(|_, w| w.urs().counter_only());
    delay_timer.dier.modify(|_, w| w.uie().enabled());
    delay_timer.sr.modify(|_, w| w.uif().clear());

    unsafe { NVIC::unmask(interrupt::TIM2) };

    // 在 G_DP 注入完成前，不应该启用定时器
    // delay_timer.cr1.modify(|_, w| w.cen().enabled());
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        let cur_cnt = G_CNT.borrow(cs).get();

        let dp_cell = G_DP.borrow(cs);

        if dp_cell.borrow().is_none() {
            NVIC::mask(interrupt::TIM2);
            panic!("Device Peripherals is not store in global static, will mask NVIC");
        }

        let dp_ref = dp_cell.borrow();
        let dp = dp_ref.as_ref().unwrap();

        let delay_timer = &dp.TIM2;

        // 先停止一下计时
        delay_timer.cr1.modify(|_, w| w.cen().disabled());

        // 清除一下 定时器的 Update Interrupt Event 位
        delay_timer.sr.modify(|_, w| w.uif().clear());

        let serial1 = &dp.USART1;

        // 打印 hello 字样
        for letter in *b"\x1b[2K\rhello " {
            // 打印前需要等待 TXE 表示位置高
            while serial1.sr.read().txe().bit_is_clear() {}
            // 注意，由于 DR 可以有 9 位，因此 DR 可以写入的数据必须扩展到 u16 的范围，但有效值并未覆盖全体 u16 的范围
            serial1.dr.write(|w| w.dr().bits(letter as u16));
        }

        // 通过外部库 itoa 将数字转换为字符串
        let mut buffer = itoa::Buffer::new();
        let num_str = buffer.format(cur_cnt);

        // 打印计数值
        for letter in num_str.as_bytes() {
            while serial1.sr.read().txe().bit_is_clear() {}
            serial1.dr.write(|w| w.dr().bits(*letter as u16));
        }

        while serial1.sr.read().txe().bit_is_clear() {}
        serial1.dr.write(|w| w.dr().bits('\r' as u16));

        // 在最后的发送结束后，应该等待 TC 位被置高，表示发送确实结束了
        // 这个位会在 停止位发送后，DR 没有新数据写入的情况下被置高
        // 图例见 Reference Manual 的 TC/TXE behavior when trasmitting
        while serial1.sr.read().tc().bit_is_clear() {}

        // 计数器值 +1
        G_CNT.borrow(cs).set(cur_cnt + 1);

        // 再次启动定时器，开启下一个计时周期
        delay_timer.cr1.modify(|_, w| w.cen().enabled());
    })
}
