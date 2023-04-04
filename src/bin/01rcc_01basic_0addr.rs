//! RCC - Reset and Clock Control
//! STM32 重置与时钟控制
//! 时钟可以说是任何一个单片机的核心功能，时钟是一种有稳定周期的脉冲信号
//! 这个脉冲信号用于协调整个 SoC，甚至是 SoC 外部的部件的协调工作
//! 特别要注意的是 RCC，也就是产生时钟的硬件，不在 Cortex Core 里面，而是由 STM32 的片上外设提供的（具体可以看 STM32F411RET6 的 datasheet 的 Block Diagram）
//!
//! 特别注意，不同芯片的 RCC 的内部结构可能不同，这里仅讨论 STM32F411RE 的 RCC 的构造
//!
//! RCC 本身的结构，可以看 Reference Manual 的 Clock Tree 这张图
//! 图的中心位置有一个 HSI/HSE/PLLCLK 复用器，它表示了时钟产生的三个（可切换的）源头
//!
//! HSI：High Speed Iternal，高速内部时钟源，是一个 16 MHz 的 RS 震荡电路（图中中心偏左上的 16 MHz HSI RC），
//! 也是单片机上电之后默认的时钟源
//!
//! HSE：High Speed External，高速外部时钟源（图中左边缘中下 4-26 MHz HSE OSC），有外部引脚（同位置左侧的 OSC_OUT/OSC_IN），
//! 依照 HSE clock 的说明，可以通过 OSC_IN 接入外部时钟源（有一些板子上会让 JLINK 发送时钟信号给 STM32 芯片用），
//! 或者在 OSC_IN 和 OSC_OUT 之间接入 晶体振荡器/陶瓷谐振器 来获得较高精度的时钟信号
//!
//! PLLCLK：Phase Lock Loop CLocK，锁相环时钟，这个时钟比较特殊，它实际上是一个时钟修改器，能通过内部的电路倍增输入的时钟的频率
//!
//! HSI/HSE/PLLCLK 复用器输出的信号叫做 SYSCLK（SYStem CLocK），它会经过一系列电路的处理，为整个 SoC 提供“不同”的频率的时钟
//! 比如
//! SYSCLK 经过 AHB PRESC 的降频处理，后会直接输入到 Cortex 中，这路时钟被称为 FCLK（Free running CLocK）用于采样中断以及供应 cortex 的 debug 模块使用
//! AHB PRESC 之后的时钟，经由不同的“启动寄存器”的控制，会输出给 AHB 总线、Cortex 内核、内存、DMA 等元件，作为时钟使用，这一路被称为 HCLK（其中的 H 来自 AHB，表示 AHB 总线时钟）
//! AHB PRESC 之后的时钟，还会直接降低到原频率的 1/8 作为 Cortex 核心的倒计时器的时钟使用（但该计时器也可以被配置为直接使用 HCLK）
//! AHB PRESC 之后的时钟，还会经过 APBx PRESC 的处理，为挂载在 APB 总线上的片上外设提供时钟（**实际上开启则还需要启用对应的时钟**）

//! 当前的源代码是通过 直接修改内存地址 的方式，执行下面一个操作：
//! 启动 ADC1 的 SCAN 模式
//!
//! 在启动这个模式之前，我们还需要执行一些额外的操作
//!
//! 1. 我们要让 HSE 作为 SYSCLK 的来源
//! 2. 由于使用晶振作为 HSE 源，我们需要等待晶振频率稳定在 8MHz
//! 3. 由于 ADC1 是一个（片上）外设，为了省电，它默认是不开启的，我们需要手动启动它的时钟，来启动它
//! 4. 最后，我们才可以启动 ADC1 的 SCAN 模式

#![no_std]
#![no_main]

use panic_rtt_target as _;

use rtt_target::{rprintln, rtt_init_print};

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 仿照 C，直接修改寄存器来实现目标
    // 所有的内存地址均来自 stm32f411RET6 的 reference manual

    // 由于要等待外部震荡源的稳定
    // 这里我们可以看看我们要等待多少次循环才能完成这个操作
    let mut wait_count: u32 = 1;

    unsafe {
        // RCC 相关的寄存器在内存中的地址的起始位置
        // 在 reference manual 中，这些寄存器的实际地址都会标记为基于该地址的偏移
        const RCC__BASE: u32 = 0x4002_3800;

        // 启用外部时钟源
        // CR 是 Control Register 的简称
        const RCC_CR__OFFSET: u32 = 0x00;
        const RCC_CR__ADDRESS: u32 = RCC__BASE + RCC_CR__OFFSET;
        // 注意到 Cortex-M4 是 32 位的处理器，也就表示处理器一次性会读取 32 位的数据
        // 因此将一个 bit 编码为一个地址是不合适的，所以，有很大概率一个地址上会保存多个寄存器的信息
        // 因此我们必须用左右移操作配合 且/或/异或 操作来修改
        //
        // 在这里我们要访问取两个寄存器
        // 第一个是 HSEON 表示 HSE ON，表示我们要启用外部时钟源
        const RCC_CR__HSEON__BIT: u32 = 16;
        // 第二个是 HSERDY 表示 HSE ReaDY，这是一个只读位置（仅由硬件写入的位置）
        // 由于外部震荡源的启动和稳定都需要一段时间，因此我们必须监看这个位置，直到它返回 1
        // 才能继续执行下面的操作
        const RCC_CR__HSERDY__BIT: u32 = 17;

        // 实际修改内存，以启动 HSE
        *(RCC_CR__ADDRESS as *mut u32) |= 1 << RCC_CR__HSEON__BIT;

        // 这里我们让核心空转来等待震荡源稳定（到 8MHz）
        // 而且可以记录一下等待的圈数
        while *(RCC_CR__ADDRESS as *const u32) & 1 << RCC_CR__HSERDY__BIT == 0 {
            wait_count += 1;
        }

        // 将外部时钟源设置为系统时钟
        // CFGR 为 ConFiGuration Register 的缩写
        const RCC_CFGR__OFFSET: u32 = 0x08;
        const RCC_CFGR__ADDRESS: u32 = RCC__BASE + RCC_CFGR__OFFSET;
        // SW 为 SWitch 的缩写，这两个 bit 用来切换 SYSCLK 的来源
        const RCC_CFGR__SW__BIT: u32 = 0;
        // 注意 SW 是两位的
        *(RCC_CFGR__ADDRESS as *mut u32) |= 01 << RCC_CFGR__SW__BIT;

        const RCC_CFGR__SWS_BIT: u32 = 2;
        // 等待系统时钟切换完成
        while *(RCC_CFGR__ADDRESS as *const u32) & 01 << RCC_CFGR__SWS_BIT != 01 {}

        // 启用 APB2 总线上 ADC1 的时钟
        // APB2ENR 为 APB2 ENable Register 的缩写
        const RCC_APB2ENR__OFFSET: u32 = 0x44;
        const RCC_APB2ENR__ADDRESS: u32 = RCC__BASE + RCC_APB2ENR__OFFSET;
        // ADC1EN 为 ADC1 ENable 的缩写
        const RCC_APB2ENR__ADC1EN__BIT: u32 = 8;
        *(RCC_APB2ENR__ADDRESS as *mut u32) |= 1 << RCC_APB2ENR__ADC1EN__BIT;

        // 启用 ADC1 的 SCAN 模式
        const ADC1__BASE_ADDRESS: u32 = 0x4001_2000;
        // 特别注意，由于每个 ADC 的寄存器的配置都是相同的，因此文档里不会区分 ADC 的名称
        // 也就是说文档里寄存器的名字会是 ADC_CR1 而非 ADC1_CR1
        // 其次，由于 ADC 的配置参数比较多，因此 Control Register 也有两个：CR1 和 CR2
        const ADC1_CR1__OFFSET: u32 = 0x04;
        const ADC1_CR1__ADDRESS: u32 = ADC1__BASE_ADDRESS + ADC1_CR1__OFFSET;
        const ADC1_CR1__SCAN__BIT: u32 = 8;
        *(ADC1_CR1__ADDRESS as *mut u32) |= 1 << ADC1_CR1__SCAN__BIT;

        // 返回等待外部震荡器稳定的循环次数
        wait_count
    };

    // 最后我们来读取一下时钟的实际状态
    unsafe {
        const RCC__BASE: u32 = 0x4002_3800;
        const RCC_CFGR__OFFSET: u32 = 0x08;
        const RCC_CFGR__ADDRESS: u32 = RCC__BASE + RCC_CFGR__OFFSET;
        // SWS 为 SWitch State 的缩写
        const RCC_CFGR__SWS__BIT: u32 = 2;
        // 这里还需要这组 bit 的长度
        const RCC_CFGR__SWS__LEN: u32 = 2;
        // 生成全是长度符合 RCC_CFGR_SWS_LEN，且每个 bit 都是 1 的数字，作为 Mask 使用
        const RCC_CFGR__SWS__MASK: u32 = u32::pow(2, RCC_CFGR__SWS__LEN + 1) - 1;

        rprintln!("Raw Register Modify\r");

        // 0b01 是使用 HSE 的寄存器状态
        if ((*(RCC_CFGR__ADDRESS as *const u32) >> RCC_CFGR__SWS__BIT) & RCC_CFGR__SWS__MASK)
            == 0b01
        {
            rprintln!("SYSCLK generated by HSE\r\nwait count: {}\r", wait_count);
        } else {
            rprintln!(
                "SYSCLK NOT generated by HSE\r\nwait count: {}\r",
                wait_count
            );
        }
    }

    loop {}
}
