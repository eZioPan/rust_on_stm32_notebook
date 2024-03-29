//! 本源码文件为了展示仅使用 pac 完成中断的效果，有部分代码是不安全的
//! 本源码文件仅做原理解析使用，实际应该使用的方法见 c12_s03_read_button_input_1hal.rs

//! 这里我们要完成一个简单的操作，每当一个按钮被按下，就切换 LED 灯的亮灭
//! 让处理器不断轮询 GPIO 自然是不合适的，因此这里我们尝试使用中断来处理

//! 在 STM32F413VGT6 的 block diagram 的图中，APB2 总线上没有标注了 SYSCFG 模块，请注意

#![no_std]
#![no_main]

use panic_rtt_target as _;

use rtt_target::rtt_init_print;

// 注意到我们并没有使用任何 hal 层级的库，我们只使用了 pac 层级（实际上是 stm32f4 crate）的库
use stm32f4xx_hal::pac::{self, interrupt};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let (Some(device_peripheral), Some(core_peripheral)) =
        (pac::Peripherals::take(), pac::CorePeripherals::take())
    {
        // 启动 GPIO Port A 的时钟
        // 它是我们选定的外部按钮所在的 Port
        device_peripheral
            .RCC
            .ahb1enr
            .modify(|_, w| w.gpioaen().enabled());

        // 将 PA0 设置为下拉模式，这样就启用了 PA0 自带的弱下拉电阻
        // 防止 Pin 在外部悬空时，捕获干扰而导致输入寄存器的值随机变动
        // 注意，输入寄存器的值是每个时钟都刷新一次的，所以采样频率是和时钟频率关联的
        device_peripheral
            .GPIOA
            .pupdr
            .modify(|_, w| w.pupdr0().pull_down());

        // 将按钮所在的 PA0 的模式设置为输入
        // 只有输入状态的 GPIO 才可以被设置为中断来源
        device_peripheral
            .GPIOA
            .moder
            .modify(|_, w| w.moder0().input());

        // 接着，我们要配置外部中断控制器，让它捕获来自按钮的信息
        //
        // 首先就是启用 SYSCFG 的时钟，SYSCFG 中就包含了外部中断控制器
        device_peripheral
            .RCC
            .apb2enr
            .modify(|_, w| w.syscfgen().enabled());

        // 见 Reference Manual 的 External interrupt/event GPIO mapping 图
        //
        // 在 STM32F413 上，外部中断控制器的寄存器有 4 个，分别为 EXTICR1 到 EXTICR4
        //（EXTernal Interrupt Controller Register 的缩写）
        // 与常规的寄存器先选 Port 再选 Pin 不同，EXTICR 是先选 Pin 再选 Port，
        // 也就是说假设我们想让 PD2 作为外部中断源，那我们先要找到 Pin 2 **所在的** EXTICR1，
        // 然后找到与 Pin 2 准确关联的 EXTI2 这个四位，接着才是将这四个位的值设置为 Port D 对应的值 0x3。
        // 上面我们提到，Pin 2 所在的 EXTICR1，这里我们需要注意的是
        // STM32F413 上总计只有 16 个外部中断
        // （4 个 EXTICR，每个 EXTICR 有 4 个分块，每个分块 4 位（每个 EXTICR 的高 16 位保留不用））
        // 不能同时覆盖所有的 Pin，于是 EXIT 控制器选择了一个方法，
        // 那就是所有 Port 中具有相同编号的 Pin（比如 PA2, PB2, PC2 ... PH2），同一时刻下，只能挑一个 Port 下的 Pin 作为外部中断的来源。
        // 这也正是一个 EXTI 寄存器的某四个位需要设置数字的意义——已知 Pin 编号，挑选 Port 编号。
        device_peripheral
            .SYSCFG
            .exticr1
            // 将 EXTI0 设置为监听 Port A
            .modify(|_, w| unsafe { w.exti0().bits(0) });

        // 注：EXTI 模块较为复杂，可以对照 Reference manual 的 External interrupt/event controller block diagram 来看这里的说明
        //
        // 在配置好了 SYSCFG 后，我们终于可以控制 EXTI 这个硬件模块了
        //
        // EXTI 这个硬件模块就像是硬件事件的收集器一样，会将硬件发生的变化，以修改寄存器的方式，告知 Cortex 核心（的 NVIC）
        //
        // 首先就是要设置触发的模式，也就是 上升沿触发、下降沿触发、两者皆触发 三种模式中选择一种
        // 这里我们启用 Trigger0（对应 Pin 0 的输入）的上升沿触发
        // rtsr 是 Rising trigger selection register 的缩写
        // 如果仔细观察的化，就会发现 Trigger 的数量不是 16 个，而是 22 个，
        // 这是由于，除了 GPIO 可以触发中断，还有 5 个额外的中断源，包含
        // EXTI16 的 PVD 输出、EXTI17 的 RTC Alarm 事件、EXTI18 的 USB OTG FS Wake 事件
        // EXTI21 的 RTC Tamper 和 TimeStamp 事件、EXTI22 的 RTC Wakeup 事件
        // 实际上，STM32F7 系列一共提供了 24 个 EXTI，除了上面提到的 15 个，还另有一些与那颗芯片上具有的额外功能对应的中断
        device_peripheral.EXTI.rtsr.modify(|_, w| w.tr0().enabled());
        // 在片上外设中，需要做的设置就只剩最后一步了，允许沿边检测电路把信号发到 Pending Register 上
        // imr 是 interrupt mask register 的缩写
        device_peripheral.EXTI.imr.modify(|_, w| w.mr0().unmasked());

        // 见 Reference manual 的 Vector table for STM32F413xx 表
        //
        // 好了，到此位置，片上外设的配置就全部完成了，但是，Cortex 的运算部分还不能直接处理这个信号
        //
        // 由于中断的发生不会顾及处理器的运行状态（要不然叫什么“中断”），让处理器即刻停下手里的工作转而处理中断是不现实的（会导致当前任务出现丢失）
        // 于是 Cortex 处理器中，有一个专门的模块，来检测中断信号，并执行初步的中断处理，那就是 NVIC（Nested Vectored Interrupt Controller），
        // NVIC 负责保存中断到达时，暂存核心的当前运行状态，同时从 Flash 里读取向量表，找到对应的中断处理函数的地址，配置好栈空间，接着让核心跳转到相应的位置开始执行中断处理代码
        // 当中断处理结束之后，它还负责弹出为中断处理而新建的栈，并还原处理器核心在处理中断前一刻的状态，以便处理器继续执行原来的任务。
        //
        // 由于 NVIC 要处理的内部异常/外部中断的数量远远多与 EXIT 的数量，因此为了缩减向量表大小，节省 FLASH，EXIT 与 向量表 实际上是 多对一 的关系，其中
        // EXTI0 到 EXTI4（也就是 Pin 0 到 Pin 4）在向量表中有 5 个单独的处理函数指针，
        // EXTI5 到 EXTI9（也就是 Pin 5 到 Pin 9）在向量表中合并至名为 EXTI9_5 的处理函数指针，
        // EXTI10 到 EXTI15（也就是 Pin 10 到 Pin 15）在向量表中合并至名为 EXTI15_10 的处理函数指针。
        // EXTI16 至 EXIT 24，由于是特殊功能的模块产生的中断，因此每个中断都有独立的处理函数指针。
        //
        // 在这里，我们要关掉 NVIC 中对应的触发掩码，以便让中断可以触发上面所说的中断处理流程
        // 如上所说，这个掩码是 Cortex 核心内部的，因此要使用的变量为 core_periperal
        // 这里我们就不能只参考 STM32F413 的手册了，还需要同时参考 Cortex-M4 Devices Generic User Guide 这本手册了
        // 依照 Cortex-M4 的手册，我们要设置的是名为 NVIC_ISER（NVIC Interrupt Set-Enable Reegisters）的寄存器
        // 然后依照 STM32F413 的 Reference Manual，EXTI0 处于向量表的 Position 6，
        // 而第 6 号 bit 处于编号为 0 的 ISER（Position 0~31 都属于 0 号 ISER），于是
        // 我们要将 ISER0 的索引为 6 的位设置位 1
        //
        // 由于 unmask 有大量的副作用，因此这个函数被认为是不安全的，所以此处需要使用 unsafe 标注
        unsafe {
            core_peripheral.NVIC.iser[0].modify(|d| d | 1 << 6);
        };
        // 到此一个按钮的中断初始化设置完成了
        // 其后还需要实际设置中断发生时，我们期望产生的效果，也就是创建（准确说是覆盖）特定的中断处理函数
        // 不过这得在另一个单独函数中配置了

        // 下面的事情就比较简单了，将 PA15 设置为高电平，最后开启推挽输出
        device_peripheral
            .GPIOA
            .moder
            .modify(|_, w| w.moder15().output());
        device_peripheral.GPIOA.odr.modify(|_, w| w.odr15().high());
        device_peripheral
            .GPIOA
            .otyper
            .modify(|_, w| w.ot15().push_pull());
    }

    #[allow(clippy::empty_loop)]
    loop {}
}

// 特别注意，这里的中断处理函数是不安全的，仅作为原理演示用
//
// 在这里，我们要覆盖用于处理 EXTI0 的中断处理函数
// 而这个函数名称是固定的，函数名称与中断名称的对应关系见 stm32f4xx_hal::pac::interrupt Enum
// 这里我们要创建名为 EXTI0 的函数
// 而且，由于 pac（实际为 stm32f4 这个 crate）并不能确定一个名为 EXIT0 的函数是否就是中断处理函数，
// 因此我们还需要引入 stm32f4xx_hal::pac::interrput 过程宏，用过程宏将这个函数签名“标记为”对应的中断处理函数
#[interrupt]
unsafe fn EXTI0() {
    // 在进入中断处理函数之后，首先要做的就是清理 EXTI 的 Pending Register 中 EXTI0 的 bit
    // 由于 pac::Peripherals 之前已经初始化过了，因此这里只能“偷取”它了。
    // 正是这一步引入了 unsafe 的操作
    let device_peripheral = pac::Peripherals::steal();
    // 清理 Pending bit
    // 这一步很重要，由于 Pending bit 不会自动清理，会导致我们一直陷在这个中断处理流程中（ISR - Interrupt Service Routine）
    device_peripheral.EXTI.pr.modify(|_, w| w.pr0().clear());

    // 切换 LED 的状态
    // 翻转是通过 XOR 实现的
    device_peripheral
        .GPIOC
        .odr
        .modify(|r, w| w.odr15().bit(r.odr15().bit() ^ true));
}
