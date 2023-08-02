//! 前面两个源码，我们分别在 pac 和 hal 层级上实现了 GPIO 的中断效果
//! 这里我们使用 cortex-m-rtic 这个 crate 提供的 rtic 框架，实现并扩充之前的效果

//! 注意，在使用 cortex-m-rtic 时，需要启用 hal 库对于 rtic 的支持，比如 stm32f4xx_hal 就有一个 rtic 的 feature 需要启用

//! 实现这样一个效果：
//! 按钮按下，板载 LED 灯由不亮变闪亮，或从闪亮变不亮，并发送按钮的按下次数

#![no_std]
#![no_main]

// rtic::app 是一个过程宏，必须施加在一个 mod 上，而且必须含有一个 device 参数，该参数必须指向一个 pac 库或 pac mod 的路径
// 它可以被认为是 cortex_m_rt::entry 的替代
#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {

    use panic_rtt_target as _;

    use rtt_target::{rprintln, rtt_init_print};

    use stm32f4xx_hal::{
        gpio::{self, Edge, Input, Output, PinState},
        pac::TIM2,
        prelude::*,
        timer::{CounterMs, Event},
    };

    // 在我们的案例中，由于灯需要闪动，所以灯的亮灭并不等价于灯在逻辑上的开和关
    // 因此这里我们额外创建一个枚举类型，来记录灯在逻辑上的开关
    #[derive(Clone, Copy)]
    pub enum LEDLogicState {
        Off, // 关模式，表示此时灯应该持续不亮
        On,  // 开模式，表示此时灯应该闪动
    }

    // rtic 特有的 shared 属性，它表示，被它标记的结构体（本例中的名称为 Shared）
    // 是一个“资源仓库”，该仓库中的每个值都可以被多个 task 访问
    #[shared]
    struct Shared {
        // 表示 GPIO PC13，在我们的核心板上与 LED 灯相连
        // 该对象会被 `button_pressed` task 和 `blink_led` task 访问
        led: gpio::Pin<'C', 13, Output>,
        // LED 的逻辑上的开关状态
        // 该对象会被 `button_pressed` task 和 `blink_led` task 访问
        led_state: LEDLogicState,
        // 闪动计时器，确定了当 LED 打开时，闪动的频率
        // 该对象会被 `button_pressed` task 和 `blink_led` task 访问
        timer: CounterMs<TIM2>,
    }

    // rtic 特有的 shared 属性，它表示，被它标记的结构体（本例中的名称为 Local）
    // 是一个“资源仓库”，该仓库中的任意一个值都仅可以被一个 task 访问
    #[local]
    struct Local {
        // 表示 GPIO PA0，是按钮所在的 GPIO 口
        // 仅被 `button_pressed` 访问
        button: gpio::Pin<'A', 0, Input>,
        // 按钮被按下的计数
        // 仅被 `button_pressed` 访问
        trigger_count: u16,
    }

    // rtic 特有的属性，表示 init task
    // 这个 task 会在芯片 reset 后被执行，执行该 task 时，
    // 会禁用所有的外部中断，并获得处理器的完全控制权
    // 如其名，它一般用于初始化系统
    #[init]
    // 有一个值得注意的地方，init::Context 中的 init 来自函数名 init，
    // 而这个函数名是任取的，因此这个前缀也并非固定的，在后面的 task 定义中就可以发现这个特点
    fn init(mut ctx: init::Context) -> (Shared, Local) {
        rtt_init_print!();

        // ctx.device 就是 stm32f4xx_hal::pac::Peripheral::take().unwrap() 的结果
        let gpio_port_a = ctx.device.GPIOA.split();
        let mut button = gpio_port_a.pa0.into_pull_down_input();
        let mut syscfg = ctx.device.SYSCFG.constrain();
        button.make_interrupt_source(&mut syscfg);
        button.trigger_on_edge(&mut ctx.device.EXTI, Edge::Falling);
        button.enable_interrupt(&mut ctx.device.EXTI);

        // 注意，使用 rtic 后，就不再需要使用 NVIC::unmask 了

        let gpio_port_c = ctx.device.GPIOC.split();
        let led = gpio_port_c
            .pc13
            .into_push_pull_output_in_state(PinState::High);

        let clocks = ctx
            .device
            .RCC
            .constrain()
            .cfgr
            .use_hse(8.MHz())
            .sysclk(48.MHz())
            .freeze();

        // 用 RCC 配置一个定时器
        // 注意，这里仅是关联了 RCC，没有修改与计时器相关的任何寄存器
        let timer = ctx.device.TIM2.counter_ms(&clocks);

        (
            Shared {
                led,
                led_state: LEDLogicState::Off,
                timer,
            },
            Local {
                button,
                trigger_count: 0,
            },
        )
    }

    // rtic 特有的属性，idle task 是运行在启用了外部中断的环境下的，而且该函数永远不能返回
    #[idle(local = [], shared = [])]
    fn idle(_ctx: idle::Context) -> ! {
        #[allow(clippy::empty_loop)]
        loop {
            // WFI: Wait For Interrupt
            // 是一种 cortex-m 的低功耗模式，
            // 在这种模式下，处理器的时钟会时不时暂停，
            // 导致芯片无法与 DAPLink 的时钟同步，也无法正确接受 halt 的命令，让芯片停机
            // 因此这个命令仅在 release 模式下启用，debug 时需要关闭
            #[cfg(not(debug_assertions))]
            rtic::export::wfi(); // 等价于 cortex::asm::wfi()
        }
    }

    // 自定义的一个 task，如果其具有一个 bind 参数，则表示该 task 是中断处理函数
    // 在这里，是发生 EXTI0 中断后会执行的函数
    // 其后还有两个参数 local 和 shared，表示要访问的 local 和 shared 资源
    #[task(binds = EXTI0, local = [button, trigger_count], shared = [led, led_state, timer])]
    fn button_pressed(mut ctx: button_pressed::Context) {
        // ctx.local 能访问的参数是本 task attribute 的 local 参数中的资源
        // 从下面的代码中我们可以发现，local 中的资源可以简单的直接访问
        ctx.local.button.clear_interrupt_pending_bit();

        // ctx.shared 能访问的参数是本 task attribute 的 shared 参数中的资源
        // 从下面的代码中我们可以发现，shared 中的资源需要锁定，或者说需要解开 Mutex，
        // 而且需要使用类似回调函数的形式在 .lock() 方法中传入要执行的函数
        ctx.shared.led_state.lock(|state| match state {
            // 当按钮被按下，且 LED 当前的状态为 Off 时，就启动计时器，并切换 LED 状态为 On
            LEDLogicState::Off => {
                ctx.shared.led.lock(|led| led.set_low());
                ctx.shared.timer.lock(|timer| {
                    timer.start(1000.millis()).unwrap();
                    timer.listen(Event::Update);
                });
                *state = LEDLogicState::On;
            }
            // 当按钮被按下，且 LED 当前的状态为 On 时，关闭 LED，关闭计时器，并切换 LED 状态为 Off
            LEDLogicState::On => {
                ctx.shared.led.lock(|led| led.set_high());
                ctx.shared.timer.lock(|timer| timer.cancel().unwrap());
                *state = LEDLogicState::Off;
            }
        });
        *(ctx.local.trigger_count) += 1;
        rprintln!("Trigger Count: {}\r", ctx.local.trigger_count);
    }

    // 当倒计时结束，TIM2 中断会被触发，
    // 此时我们清除 TIM2 的 Pending bit，并 toggle LED 的亮灭
    #[task(binds = TIM2, local = [], shared = [timer, led, led_state])]
    fn blink_led(mut ctx: blink_led::Context) {
        ctx.shared
            .timer
            .lock(|timer| timer.clear_interrupt(Event::Update));
        ctx.shared.led_state.lock(|state| match state {
            LEDLogicState::Off => unreachable!("Timer isn't shut down properly\r"),
            LEDLogicState::On => ctx.shared.led.lock(|led| led.toggle()),
        });
    }
}
