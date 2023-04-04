//! 这部分代码演示的主要为中断的实际创建流程，所以芯片寄存器配置的流程讲的比较少
//! 若想了解中断的实现机制和作用流程，可以看一下 c12_s03_read_button_input_0pac.rs 这个文件

//! 这里我们要完成一个简单的操作，每当一个按钮被按下，就切换 LED 灯的亮灭，而且还会额外向 RTT 打印按钮被按下的总次数
//! 让处理器不断轮询 GPIO 自然是不合适的，因此这里我们尝试使用中断来处理

//! 在 stm32f411RET6 的 block diagram 的图中，APB2 总线上没有标注了 SYSCFG 模块，请注意

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::{
    gpio::{gpioa, gpioc, Edge, Input, Output, PinState},
    interrupt, pac,
    prelude::*,
    syscfg::SysCfgExt,
};

// 由于中断处理函数一定不在主函数里（如果在主函数里，函数就被顺序执行了，这违背了中断的设计）
// 就必然会有“线程”的切换，因此我们需要保证线程安全。
// cortex_m crate 在 interrupt 模块下提供了 Mutex 类型，可以用于在线程间安全地传递对象，
// （注意 Cortex-M4 是单核心的设计）这个 Mutex 能包裹一个全局 static 量，并使用“限制外部中断触发内核的方法”，保证处理器能不被干扰地完成对全局对象的访问，
// 从而达到让 static 量在线程间的安全访问。
// 不过由于 static 量是不可变的，因此我们还需要使用具有“内部可变性”的 Cell 和 RefCell 来包裹数据，以达到最终的效果
//
// G_BUTTON：按钮的 GPIO 量，是中断的触发器，在中断处理函数中主要用于清理 Pending Register 中对应的 bit
// 注意到，除了外层会使用 Mutex 和 RefCell 分别处理跨线程安全和内部可变性问题，
// 其内部还套了一层 Option，这是由于在声明这个 staic 量时，我们不可能完成这个量的初始化，
// 我们必须在主线程中经过操作才能完成其初始化，因此我们在这里使用 None 来“完成”初始化，之后在主线程完成实际的值注入。
// 这种设计方式被称为 lazy initialization
static G_BUTTON: Mutex<RefCell<Option<gpioa::PA0<Input>>>> = Mutex::new(RefCell::new(None));
// G_LED：LED 的 GPIO 量，是中断产生后我们要切换电平的 GPIO
// 同样使用了 lazy initialization 的设计方案
static G_LED: Mutex<RefCell<Option<gpioc::PC13<Output>>>> = Mutex::new(RefCell::new(None));
// G_COUNT：一个计数器，方便我们统计中断被触发了多少次，在中断处理函数中会有先读和后写的操作
// 1. 由于 u32 实现了 Copy，这里我们是不需要使用 RefCell 的，直接使用 Cell 即可
// 2. 由于我们只是包裹了一个 u32 类型的值，而且初始值我们是确定的，因此无需 lazy initialization，直接初始化即可
static G_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let Some(mut device_peripheral) = pac::Peripherals::take() {
        // 这里就直接启用了 AHB1 上的 GPIO Port A
        let gpioa = device_peripheral.GPIOA.split();

        // 这里就直接启用了 AHB1 上的 GPIO Port C
        let gpioc = device_peripheral.GPIOC.split();

        // 将 GPIO 设置为输入下拉模式，
        let mut button = gpioa.pa0.into_pull_down_input();

        // SYSCFG 控制了 EXTI，因此需要启用 SYSCFG
        let mut syscfg = device_peripheral.SYSCFG.constrain();

        // 将 EXTI0 关联到 Port A
        button.make_interrupt_source(&mut syscfg);

        // 启用上升沿触发输入线
        button.trigger_on_edge(&mut device_peripheral.EXTI, Edge::Rising);

        // 修改 中断请求遮罩，让 Line 0 的请求可以发送至 NVIC
        button.enable_interrupt(&mut device_peripheral.EXTI);

        // 同时 Cortex 处理器中的 NVIC 还需要接受 EXTI0 的中断
        // 由于这个操作会启动新的中断，所以该操作被认为是不安全的
        unsafe { NVIC::unmask(interrupt::EXTI0) };

        // 将核心板板载的 LED 灯的引脚 PC13 设置为推挽输出，然后将默认电平设置为高（让灯珠熄灭）
        let led = gpioc.pc13.into_push_pull_output_in_state(PinState::High);

        // 最后，由于我们需要将实际的值写入到 static 量中，
        // 我们必须要禁止任何外部中断再此期间干扰 Cortex 核心的运行
        // 因此我们要调用 cortex_m::interrupt::free 函数，
        // 暂时关闭所有的外部中断
        cortex_m::interrupt::free(|cs| {
            // 然后解开 Mutex，并替换 RefCell 中的 None 为实际的值
            G_BUTTON.borrow(cs).replace(Some(button));
            G_LED.borrow(cs).replace(Some(led));
        });
    };

    loop {}
}

// stm32f4xx_hal::interrput 这个过程宏，我们必须要导入到本地，才能使用
//
// 书写中断处理函数，函数的签名是固定的，见 stm32f4xx_hal::interrupt Enum
// 这里我们要处理的中断就是 EXTI0 产生的
// 这个函数的签名还可以添加 unsafe，处理会使用不安全的代码，不过这里我们没有；
// 函数的签名的结尾还可以添加 -> !，表示该函数永远不会返回，不过鉴于我们只是处理了一个按钮产生的中断，
// 最后还是要将控制流交还给主函数的，因此也不需要。
#[interrupt]
fn EXTI0() {
    // 中断处理函数里也不一定需要立刻暂停中断的产生
    rprintln!("Recived EXTI0, Start to Process");

    // 清除 Button 的 Pending Register，并修改 LED 的状态，最后还要从 rtt 打印一下触发的次数
    cortex_m::interrupt::free(|cs| {
        let mut button = G_BUTTON.borrow(cs).borrow_mut();
        button.as_mut().unwrap().clear_interrupt_pending_bit();
        let mut led = G_LED.borrow(cs).borrow_mut();
        led.as_mut().unwrap().toggle();

        let cur_count = G_COUNT.borrow(cs).get();
        rprintln!("Toggle LED, count: {}\r", cur_count);
        G_COUNT.borrow(cs).set(cur_count + 1);
    });
}
