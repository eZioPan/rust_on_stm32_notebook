//! 编码器接口模式
//!
//! 编码器接口模式 Encoder interface mode 其实是定时器的一个功能
//! 它是我们遇见的，第一个同时使用了两个输入输入比较端口、以及计数核心的一个功能
//!
//! 编码器接口最常用的模式是正交编码器接口模式（Quadrature Encoder Interface），
//! 这里的“正交”指的是两个具有相同周期的周期函数，其相位相差 1/4 个周期的状态，
//! 为什么说这叫“正交”呢？因为在数学上，分析向量的时候，可以将一个极坐标表示的向量 [A, φ] 分解到直角坐标中 [A*cos(φ), A*sin(φ)]，
//! cos(x) 和 sin(x) 刚好是相位相差 π，且刚好能做正交分解，于是相位相差 1/4 个周期的两个等周期的周期函数就被称为“正交”状态了。
//!
//! 正交编码的状态是什么样的？
//! 想象一下，你面前有一把小凳子，现在要求你利用这个小凳子做“踏台阶”的运动，
//! 也就是说，让你站在地面上，然后双脚站到凳子上，然后再从凳子上退回到地面上，并一直保持这个循环动作，一般人会选择这样一个流程
//! 左脚踏台阶 -> 右脚踏台阶 -> 左脚下台阶 -> 右脚下台阶 -> ... 然后就可以直至循环下去（当然你想先迈右腿也是可以的，流程上左右互换即可）
//! 好了，正交编码中，两个输入端口检测到的电平状态也是如此变化的，A 低 B 低 -> A 高 B 低 -> A 高 B 高 -> A 低 B 高 -> ...
//!
//! 正交编码有什么用？
//! 首先，它可以计数（啊，这不是废话么……），只要电平有高低变化就可以计数，不过这还不够，更重要的，在计数的同时，它还可以表示方向
//! 怎么个表示方向法呢，让我们回想一下刚才说的“踏台阶”运动，假设我们规定左脚先踏上为正方向，那么右脚先上的过程，其实就是左脚先上的逆向流程，不信你可以试一试
//! 于是乎，我们就仅通过一对电平的交替变化，获得了计数以及方向
//!
//! 正交编码有什么实际的用途么？
//! 有，让我们思考以下情况，假设我想设计一个音量旋钮，我们会怎么做
//! 最简单的做法，直接使用一个电位器，单片机读取电位，让后用这个数值来确定音量，这个方案看起来不错，但是在实现上有一些难度
//! 第一个是我们要将模拟信号电压值转换为数字信号，这里就得使用 ADC 之类的设备/模块，增加成本，而且不一定准确
//! 第二个是一般旋转电位器一般设计为 1 圈到头，也就是说，用户在操作的时候，最多在一圈的范围之内就得将音量从最小调整至最大，细分度可能不够
//! 第三个是电位器是有端头的，也就是说，用户是可能将其拧过头的
//! 但如果我们使用旋转编码器，我们就可以解决上面的问题
//!
//! 旋转编码器是一种将旋转转换为正交电平的元件，它的工作原理可以这样理解
//! 想象一个圆环，我们沿圆周的方向，将其等分为多份，然后交替将其填充上黑白颜色，然后我们再沿着圆环的中线剪开这个圆环，这样我们就获得了一大一小两个圆环
//! 然后我们再将内侧的小圆环逆时针旋转**半个**白块的距离，形成了一种外侧的黑白块边界对上了内侧黑白块中间的效果
//! 然后我们再从圆环的圆心拉出一条射线来，这条射线会绕着圆心转动，那么每时每刻，射线与两个圆环相交的颜色，就代表了旋转编码器读取到的电平值
//!
//! 旋转编码器相对于电位器的优势在于
//! 输出的是数字信号，免去了模数转换的问题；可以旋转多圈，不存在拧过头的问题；旋转编码器的绝对位置和控制的量直接没有必然关系，分度可以自行控制
//!
//!
//! 上面介绍了半天正交编码器，那么正交编码器的信号应该怎么样被 STM32 的定时器识别呢？
//! 最开始我们说，STM32 的定时器的编码器接口模式会同时使用两个输入比较端口和计数核心
//! 从 Reference Manual 的 General-purpose timer block diagram 框图我们可以看到
//! Encoder Interface 处于框图的右上角，属于 Trigger controller 的一部分，它的输入为 TI1FP1 和 TI2FP2，分别属于左下角 TIMx_CH1 和 TIMx_CH2 的 Input filter & edge detector 的输出
//! 然后 Encoder Interface 输出的信号会进入到 PSC/CNT/ARR 中进行计数。
//!
//! 这样，我们大体的配置思路为
//!
//! 1. 配置 TIMx_CH1 和 TIMx_CH2 的输入过滤和边沿检测部分
//! 2. 将 Trigger controller（也就是 SMS - Slave Mode Selection）配置为编码器相关的模式
//! 3. 配置 PSC/CNT/ARR
//! 4. 最后，这里我们为了看清编码器的工作状态，采用轮询的方式来显示 Encoder Interface 的状态
//!
//!
//! 在电路上，我们这里选择了使用两个按钮充当编码器的两个输入，这样我们就可以手动感受编码器的“踏台阶”的工作模式了
//! 电路连接
//!
//! VCC -> 按钮1 -> PA0
//! VCC -> 按钮2 -> PA1
//!
//! 在烧录好程序之后，想象两个按钮就是我们上面说的两只脚，按下按钮就表示踏上一只脚，松开按钮就表示后退一只脚

#![no_std]
#![no_main]

use stm32f4xx_hal::pac;

use panic_rtt_target as _;
use rtt_target::{rprint, rtt_init_print};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();

    // 常规操作，使用 HSE 作为主时钟源
    let rcc = &dp.RCC;
    rcc.cr.modify(|_, w| w.hseon().on());
    while rcc.cr.read().hserdy().is_not_ready() {}
    rcc.cfgr.modify(|_, w| w.sw().hse());
    while !rcc.cfgr.read().sws().is_hse() {}

    // 然后将 GPIO 的 PA0 和 PA1 开启内部下拉，并切换到 TIM2 AF 上
    rcc.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    let gpioa = &dp.GPIOA;
    gpioa.pupdr.modify(|_, w| {
        w.pupdr0().pull_down();
        w.pupdr1().pull_down();
        w
    });
    gpioa.afrl.modify(|_, w| {
        w.afrl0().af1();
        w.afrl1().af1();
        w
    });
    gpioa.moder.modify(|_, w| {
        w.moder0().alternate();
        w.moder1().alternate();
        w
    });

    // 然后开启 TIM2 的时钟
    rcc.apb1enr.modify(|_, w| w.tim2en().enabled());

    let tim2 = &dp.TIM2;

    // 然后是构建 CCMR1 的 input 模式结构体
    let ccmr1 = tim2.ccmr1_input();
    // 在 input 结构体模式下，将 CC1S 选择为 TI1 输入，CC2S 选择为 TI2 输入
    // 然后由于我们这里使用了按钮模拟编码器的效果，因此把输入过滤拉到最大
    ccmr1.modify(|_, w| {
        w.cc1s().ti1();
        w.ic1f().fdts_div32_n8();
        w.cc2s().ti2();
        w.ic2f().variant(15);
        w
    });
    // 将 TI1 和 TI2 的输入触发都设置为上升沿触发
    tim2.ccer.modify(|_, w| {
        w.cc1p().clear_bit();
        w.cc1np().clear_bit();
        w.cc2p().clear_bit();
        w.cc2np().clear_bit();
        w
    });

    // TIM 的 Encoder Interface 实际上有三种计数模式，
    // 具体的可以看 Reference Manual 中的 Counting direction versus encoder signals 这张表
    // 上面我们介绍的，和这里我们使用的都是三个模式中最复杂的模式，也是最常用的模式
    // 其它两种模式我们也就不过多介绍了
    tim2.smcr.modify(|_, w| {
        // 将编码器接口设置为在 TI1 和 TI2 上计数
        w.sms().encoder_mode_3();
        w
    });

    // 设置一下 ARR 的值
    // 从 Example of counter operation in encoder interface mode 这样图我们可以看出
    // 每个完整踏步循环，计数器一共会增加 4，这里我们可以将 ARR 的值设置的大一些，方便我们观察
    tim2.arr.write(|w| w.arr().bits(15));
    // 然后我们可以将计数器的初始值调整一下，比如设置到一个居中的位置，这样可以尽量防止 CNT 上溢出或下溢出
    tim2.cnt.write(|w| w.cnt().bits(7));

    // 最后，我们开启计数器
    tim2.cr1.modify(|_, w| w.cen().enabled());

    loop {
        rprint!(
            "\x1b[2K\r{} {}",
            tim2.cnt.read().bits(),
            // 注意，在编码器模式下，TIM 的 CR1 寄存器下的 DIR 位就成为了只读寄存器了
            // 我们可以通过读取该值，来确认上次计数器变动时，计数器变动的方向
            match tim2.cr1.read().dir().bit() {
                true => "D",
                false => "U",
            }
        );
    }
}
