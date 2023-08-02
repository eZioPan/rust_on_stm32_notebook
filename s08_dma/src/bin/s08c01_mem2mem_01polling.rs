//! DMA 直接内存访问
//!
//! DMA Direct Memory Access
//! 直接内存访问，之所以叫“直接”，是由于该模块可以独立于 Cortex 内核读写内存、读写外设寄存器以及读取 Flash，
//! DMA 模块如同一个专职的搬运工一样，当 Cortex 核心配置并启用 DMA 后，DMA 就可以在没有 Cortex 核心参与的条件下，自行运输数据
//!
//! STM32F411RE 有两个 DMA 模块，每个模块有 8 个 stream（流），每个流可以从 8 个 channel（通道）中选择一个使用
//! 这里的 steam 和 文件系统 / 网络系统 中所说的 流 的该概念很类似，都是一个功能包含两个端口（port），当向其中一个端口写入数据时，该数据会从另一个端口流出

#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 我们在 main 函数的栈上开辟两个数组
    // 其中 src_list 是源数组，DMA 要拷贝的数据就来自于这里
    // dst_list 是目标数组，DMA 的目标位置就是这里
    let src_list: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

    // 有一个稍稍显反直觉的地方，那就是 dst_list 明明在程序中被修改了内容，但我们却没有将该变量标记为 mut
    // 原因其实也很简单，dst_list 的内容变化，是由 DMA 实现的，不是由 Cortex 核心的运算单元实现的，Cortex 核心自然不会认为 dst_list 值的变化是自己直接完成的
    // 而 rust 直接控制仅为 Cortex 核心的运行，因此也就没有必要为 dst_list 标记 mut
    let dst_list: [u8; 8] = [0u8; 8];

    // 这里我们打印一下 src_list 和 dst_list 指针的地址
    // 在运行时应该可以观察到它们均处于内置 SRAM 的地址范围（STM32F411RET6 上为 0x2000_0000 之上的 128KB 的范围）
    rprintln!(
        "src_list addr {:010p}, dst_list addr {:010p}\r",
        &src_list,
        &dst_list
    );

    rprintln!("dst_list start value: {:?}", &dst_list);

    if let Some(dp) = pac::Peripherals::take() {
        // 由于我们要执行的是 内存到内存的 DMA，而只有 DMA2 具有该项功能，因此我们这里启动 DMA2 的时钟
        dp.RCC.ahb1enr.modify(|_, w| w.dma2en().enabled());

        // 我们简化一下下面书写的名称
        let dma2 = &dp.DMA2;

        // 这里比较特殊，Reference Manual 上并没有 st 这个寄存器，st 这个名字是 stm32f4 crate 给出的名称
        // st 其实是 STream 的简写，表示 DMA 流，而 DMA2 一共支持 8 个流，因此 dp.DMA2.st 是个长度为 8 的列表，我们需要从中选择一个流来执行 DMA
        // 由于这里我们只是内存与内存之间的数据对拷，因此，我们可以任选一个流，这里我们选择了 DMA2 的 STREAM0 来执行这个操作
        let dma2_st0 = &dma2.st[0];

        // 下面的操作内容参考了 Reference Manual 的说明
        // 涉及的操作基本与 Reference Manual 相同，但顺序进行了重组，以方便理解
        // 可能有些步骤在这里是非必要的，但这里我们一并给写了

        // 下面所有的寄存器名，在 Reference Manual 中的实际名称，在前面均需添加 DMA_Sx
        // 举例，下方的 CR 寄存器，在 Reference Manual 中的名称为 DMA_SxCR

        // 【重要步骤】检测要配置的流是否关闭，若未关闭，则关闭，并等待关闭状态
        // 这是由于，部分配置寄存器，在流运行的时候，是不可以被配置的
        if dma2_st0.cr.read().en().is_enabled() {
            dma2_st0.cr.modify(|_, w| w.en().disabled());
            while dma2_st0.cr.read().en().is_enabled() {}
        }

        // 【此处非必要】接着我们要设置 DMA 请求通道（REG_STRx_CHy），以及当前流的软件优先级（Priorities）
        // DMA 请求通道用于 外设到内存的 DMA 中，由外设发出的请求 DMA 开始运行的通道，其行为类似中断，
        // 由于我们是 内存到内存的 DMA，因此这里不需要关心
        // DMA 流的软件优先级，当设置了多个流，在多个流同时发生 DMA 执行命令时，首先会判定流的软件优先级，优先级高的先执行，
        // 当流的软件优先级相同时，通过硬件优先级判定，硬件优先级高的先执行，
        // 由于我们这里仅启用了一个流，因此优先级也也不需要关心
        dma2_st0.cr.modify(|_, w| {
            // CHSEL: CHannel SELection
            w.chsel().bits(0);
            // PL: Priority Level
            w.pl().medium();
            w
        });

        // 确定 DMA 的传输方向为 内存到内存，于此同时我们要禁用循环模式
        dma2_st0.cr.modify(|_, w| {
            // 在这里我们希望 DMA 的传输方向是 内存到内存（memory-to-memory）
            // 在该模式下 DMA 的 Peripheral Port 是输入端口，Memeory Port 是输出端口，这个内容下面的设置中会用到
            w.dir().memory_to_memory();

            // 配置循环模式，启用本配置位后，在 NTDR 寄存器的值到达 0 的时，会自动重装 NTDR 寄存器，以达到再一次获取数据的流程
            // 注意，在 memory-to-memory 的模式下，禁止启用该配置
            // CIRC: CIRCular mode
            w.circ().disabled();

            w
        });

        // 确定了 DMA 的方向之后，我们就要确定输入端口和输出端口是否需要地址自增，以及它们的数据格式
        dma2_st0.cr.modify(|_, w| {
            // 设置 Peripheral Port 的单次转运的数据尺寸为 8 位（1 字节）
            // PSIZE: Peripheral data SIZE
            w.psize().bits8();

            // 设置 Peripheral Port 的内存地址自增模式
            // 在自增模式启动的情况下，DMA 按照 PSIZE 位 定义的大小读取完第一个数据之后，会偏移 PSIZE 位 定义的大小，再读取下一个数据
            // 在自增模式关闭的情况下，DMA 每次读取都仅会读取同一个地址的数据
            // PINC: Peripheral INCrement mode
            w.pinc().incremented();

            // 对于 Memory Port 的配置，参见上面对于 Peripheral Port 的配置
            w.minc().incremented();
            w.msize().bits8();

            w
        });

        // 设置目标地址，该地址表示 DMA 将最终会将数据存放的位置
        // 在 memory-to-memory 的状态下，输出端口为 Memory Port，因此这里目标地址需要写在 M0AR 中
        // M0AR: Memory 0 Address Register
        //
        // 需要注意的是，本寄存器需要提供一个实际地址，而在 Rust 中，要获取一个变量对应的实际地址，
        // 首先要获取变量的“裸指针”（Raw Pointer），它可以通过 pointer as *const _ 来获取一个不可变裸指针，
        // 然后根据 Cortex 核心的架构，再 as u32 一下，将这个裸指针的地址转换为 u32 的数值，最后就可以输入到 M0AR 中了
        // 不过取裸指针的过程被 Rust 认为是“需要人工保证安全的”，因此需要包含在 unsafe 块中
        // 这里原始类型我们明确标注了，仅作为演示使用，实际上直接写 `_`，让编译器自行推断即可
        dma2_st0
            .m0ar
            .write(|w| unsafe { w.m0a().bits((&dst_list as *const [u8; 8]) as u32) });

        // 设置源地址，该地址表示 DMA 将从何位置读取数据
        // 在 memory-to-memory 的状态下，输入端口为 Peripheral Port，因此这里目标地址需要写在 PAR 中
        // PAR: Peripheral Address Register
        //
        // 关于在 Rust 中获取一个变量的实际地址，参见上方配置 M0AR 时的说明
        // 这里类型名直接使用了 `_`，让编译器自行推断即可
        dma2_st0
            .par
            .write(|w| unsafe { w.pa().bits((&src_list as *const _) as u32) });

        // 设置 DMA 总计移动数据的次数
        // 每次 DMA 执行一次操作，移动一个数据，这个值就减一，当这个值减到 0 的时候，本次 DMA 的移动操作就完成了
        // 由于我们这里一共有 8 个数据需要移动，因此这里直接写 8 即可
        // NDTR: Number of Data (Transfer) Register
        //
        // 这里有三点需要注意，
        // 1. 这里记录的是 DMA 移动数据的数量，而非**字节数**，也就是说 DMA 实际移动了多少数据，还需要结合 DMA_SxCR 寄存器中定义的单次源数据读取量来确定
        // 2. 在循环模式（Circular Mode）下，DMA 的值减到 0 后，会再次被装载为初始值，接着执行下一轮的 DMA 操作
        // 3. 在非循环模式下，NDTR 减到 0 后，DMA_SxCR 的 EN 位会被置 0，此时若我们直接将 EN 置 1，NDTR 的值会恢复到上次启动前设置的值（就像有一个自动重载器一样）
        dma2_st0.ndtr.write(|w| w.ndt().bits(8));

        // 设置 FIFO 相关的配置
        //
        // 为什么有 FIFO
        // 通过 Reference Manual 的 System architecture 图我们可以发现，
        // Cortex 和两个 DMA 要想访问 Flash / SRAM / AHB / APB 几乎都要通过 Bus Matrix
        // 而在 Bus Matrix 的设计中，在同一个时刻，一个 Slave Port 仅能被一个 Master Port 占用，而一个 Master Port 也只能占用一个 Slave Port
        // 而在本案例中，DMA2 的两个 Master Port 都要访问 SRAM 这一个 Slave Port，因此 DMA 必须要
        // 1. DMA2 Peripheral Port 申请 Bus Matrix 中到 SRAM 的线路来读取 SRAM
        // 2. DMA2 通过 Peripheral Port 读取 SRAM 中的数据
        // 3. DMA2 Peripheral Port 释放 Bus Matrix 中到 SRAM 的线路的占用
        // 4. DMA2 Memory Port 申请 Bus Matrix 中到 SRAM 的线路来写入 SRAM
        // 5. DMA2 通过 Memory Port 向 SRAM 中写入数据
        // 6. DMA2 Memory Port 释放 Bus Matrix 中到 SRAM 的线路的占用
        // 由于 memory-to-memory 的过程不能一次性完成，所以 DMA 中必然存在一个模块，可以暂存获取到的数据
        // 这个模块就是 FIFO（First-In First-Out）
        //
        // FCR:  First-in first-out Control Register
        dma2_st0.fcr.modify(|_, w| {
            // 从上面的论述中我们可以发现，在 memory-to-memory 模式下，FIFO 是必然被使用的
            // 因此 DMDIS 位由硬件控制，我们没有必要控制这个位
            // DMDIS: Direct Mode DISable
            // w.dmdis().disabled();

            // 反正我们也要用 FIFO，根据我们要传输的数据总量 8 个 byte，
            // 我们需要 8 * 8 / 32 = 2 个 FIFO 位（共 4 个 FIFO 位），
            // 设置后，我们就可以让 DMA 最大程度利用占用上 Bus Matrix 后的时机，一次性读取/写入尽量多的数据，减少 Bus Matrix 的切换开销，以及切换可能导致数据不同一的问题（Bus Matrix 切换后，内存数据可能被其他设备修改）
            // 本参数需要搭配 DMA_SxCR 的 PBURST 位 / MBURST 位使用，才能获得想要实现的效果
            // FTH: FIFO ThresHold selection
            w.fth().half();

            // 当 FIFO 出现了错误（过载 overrun /欠载 underrun），应该要拉起对应的中断位
            // 不过在这里，我们是让 Cortex 轮询 DMA 控制接口的寄存器来判断 DMA 的状态的，因此这里也是不用设置的
            // FEIE: FIFO Error Interrupt Enable
            // w.feie().enabled();
            w
        });

        // 为了 FIFO 模块最大效用化，我们还可以开启 BURST 传输
        dma2_st0.cr.modify(|_, w| {
            // 设置 Peripheral Port 自增的偏移尺寸
            // 由于 PBURST 非 00，因此该位由硬件控制
            // PINCOS: Peripheral Port INCrement Offset Size
            // w.pincos().psize();

            // 设置 Peripheral Port 获得 Bus Matrix 的某个线路的使用权之后，会执行传输的次数，
            // 在该次数未执行完之前，除非产生错误，否则 Peripheral Port 都不会释放对 Bus Matrix 线路的占用
            // 要让本参数确实起效，还需要恰当地配置 DMA_SxFCR 寄存器的 DMDIS 位 和 FTH 位
            // 在这里，由于我们只用移动 8 个数据，因此这里直接设置为 8 即可
            // PBURST: Peripheral BURST transfer configuration
            w.pburst().incr8();

            // 对于 Memory Port 也是一样，我们希望一次 Bus Matrix 线路的占用，尽量多写一些数据到内存中
            w.mburst().incr8();

            w
        });

        // 【重要步骤】在启动前，清理全部的中断 flag
        // 这里不可以使用 .reset() 方法，因为这个方法并不能触发 HISR 和 LISR 清空
        dma2.hifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });
        dma2.lifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });

        /*
        ma2_st0.cr.modify(|_, w| {
            // DMA 在传输数据的过程中可能会发生错误，此时会置 TEIF 位，设置本位可以出发本流的中断
            // TEIE: Transfer Error Interrupt Enable
            w.teie().enabled();

            // 在 DMA 向输出端口发送数据过半时，会置 HTIF 位，设置本位可以出发本流的中断
            // 我们这里不需要触发中断，因为 DMA 的状态是通过 Cortex 核心 轮询 DMA 的控制接口的寄存器来完成的
            // HTIE: Half Transfer Interrupt Enable
            w.htie().enabled();

            // 在 DMA 完成当前的传输任务后（DTNR 减小到 0），会置 TCIF，设置本位可以出发本流的中断
            // 我们这里不需要触发中断，因为 DMA 的状态是通过 Cortex 核心 轮询 DMA 的控制接口的寄存器来完成的
            // TCIE: Transfer Complete Interrupt Enable
            w.tcie().enabled();

            w
        });
        */

        dma2_st0.cr.modify(|_, w| {
            // 启动 DMA，开始执行拷贝操作
            w.en().enabled();
            w
        });

        /*
        这里我们不可以等待 DMA2 STREAM0 的完成
        因为在 Cortex 访问 DMA2 的控制寄存器之前，DMA2 的操作可能已经完成，此时 EN 位会被 DMA 置 0
        那么我们是不可能从下面这个循环中跳出来的
        while dma2_st0.cr.read().en().is_disabled() {}
        */

        loop {
            // 读一次 DMA2_LISR 寄存器的内容，然后逐步分析寄存器的状态
            let dma2_lisr = dma2.lisr.read();

            // 如果出现 FIFO 错误，就清除该位，报错错误，并进入 panic 状态
            if dma2_lisr.feif0().is_error() {
                dma2.lifcr.write(|w| w.cfeif0().clear());
                panic!("DMA2 STREAM0 FIFO error!\r\n");
            }

            // 当有半数的数据已经从 DMA 的发送端发出后，会设置 DMA 的 Half Transfer 位，
            // 此时我们清除该位，并打印一下相关的提示信息
            if dma2_lisr.htif0().is_half() {
                dma2.lifcr.write(|w| w.chtif0().clear());
                rprintln!("DMA2 STREAM0 Half Tranfer Complete\r");
            }

            // 当全部的数据都完成发送之后，会设置 DMA
            // 此时我们清除该位，并读取一下目标地址的数据
            if dma2_lisr.tcif0().is_complete() {
                dma2.lifcr.write(|w| w.ctcif0().clear());

                // 在这个案例中，由于我们没有使用 Circular Mode，
                // 因此当 TCIF 被设置的时候，DMA2 STREAM0 的 EN 位肯定被硬件设置为 0 了
                // 我们是没有必要手动清除这个位的设置的
                // dma2_st0.cr.modify(|_, w| w.en().disabled());

                rprintln!("DMA2 STREAM0 Tranfer Complete\r");

                rprintln!("dst_list end value: {:?}\r", dst_list);

                #[allow(clippy::empty_loop)]
                loop {}
            }
        }
    } else {
        panic!("Cannot Get Peripheral\r\n");
    }
}
