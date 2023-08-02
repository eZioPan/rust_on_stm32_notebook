//! 使用 DMA，将数据从 Flash 搬运到 内存中
//!
//! 虽然 Flash 是片上外设，看起来需要用到 DMA 的 外设到内存（peripheral-to-memory）模式，
//! 但由于 Flash 存储的内容可以直接通过 Bus Matrix 访问，因此这里我们依旧可以使用 memory-to-memory 模式
//!
//! 另外我们还会遇到一个问题，那就是，我们不希望通过 Cortex 核心轮询 DMA 控制寄存器的方式查看 DMA 的完成状态，
//! 我们希望 DMA 完成后触发中断来通知 Cortex 核心 DMA 转运完成，因此目标列表得保存在全局静态量中
//! 但这里有一个问题，那就是默认情况下，全局静态量会被链接器放在 Flash 中，
//! 而正常情况下，DMA 是不能写入 Flash 的，因此我们得想办法让目标列表存放在内存地址中

#![no_std]
#![no_main]

use core::{cell::RefCell, panic};

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac::{self, interrupt, Peripherals, NVIC};

// 确定源列表和目标列表的长度，该长度还会用作 DMA NDTR 的值
const LIST_LEN: u16 = 16;

// 源列表
const SRC_LIST: [u8; LIST_LEN as usize] = *b"ABCDEFGHIJKLMNOP";

// 目标列表，在这里，由于使用了全局静态量，链接器默认会将其放置在 Flash 中
// 而 DMA 是不可以写入 Flash 的，因此我们要告诉连接器，这个值应该放在内存中
// 宏 link_section 就可以告诉连接器，它所标记的符号应该存放的位置
// 在这里，我们让连接器将目标列表链接到 .data 段，这个段表示的是已经初始化过的全局变量
// 而 .data 段是放置在 SRAM 中的，于是就达到了我们的目的
// 另外，除了 .data 段，如果这个列表是空值（或者我们并不关心其初始值），那么它还可以放在 .bss 段
// 放在 .bss 段的好处是，.bss 段并不实际占用 Flash 的空间，它仅会在程序运行时被展开到内存中
#[link_section = ".data"]
static DST_LIST: [u8; LIST_LEN as usize] = *b"abcdefghijklmnop";

// 为了尽量避免使用 unsafe 块，这里我们将初始化好的 Peripherals 移动到全局变量中
// 在之后的中断中，我们就可以用这里保存的结构体了
static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    // 开头打印以下目标列表的初始值
    rprintln!(
        "DST_LIST Start Value: {:?}\r",
        core::str::from_utf8(&DST_LIST).unwrap()
    );

    if let Some(dp) = pac::Peripherals::take() {
        // 和 01mem2mem_01polling 的操作几乎一模一样，这里就不做过多解释了

        let ahb1enr = &dp.RCC.ahb1enr;
        ahb1enr.modify(|_, w| w.dma2en().enabled());

        let dma2 = &dp.DMA2;
        let dma2_st0 = &dp.DMA2.st[0];

        if dma2_st0.cr.read().en().is_enabled() {
            dma2_st0.cr.modify(|_, w| w.en().disabled());
            while dma2_st0.cr.read().en().is_enabled() {}
        }

        dma2_st0.cr.modify(|_, w| {
            // 这里我们还是启用了 memory-to-memory 模式
            w.dir().memory_to_memory();
            w.mburst().incr16();
            w.minc().incremented();
            w.msize().bits8();
            w.pburst().incr16();
            w.pinc().incremented();
            w.psize().bits8();
            // 然后我们把 半发送、全发送、发送错误的中断全部拉起来了
            w.htie().enabled();
            w.tcie().enabled();
            w.teie().enabled();
            w
        });

        dma2_st0.fcr.modify(|_, w| {
            // FIFO 错误也被我们拉起来了
            w.feie().enabled();
            // 这里直接让 FIFO 的全部容量可用
            w.fth().full();
            w
        });

        dma2_st0
            .m0ar
            .write(|w| unsafe { w.m0a().bits((&DST_LIST as *const _) as u32) });

        dma2_st0
            .par
            .write(|w| unsafe { w.pa().bits((&SRC_LIST as *const _) as u32) });

        // 这里的搬运数必须不大于 DST_LIST 的长度
        // 否则我们就会读取和修改我们不想修改的内存位置
        dma2_st0.ndtr.write(|w| w.ndt().bits(LIST_LEN));

        // 还是一样，启用前需要清除所有的错误位
        dma2.hifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });
        dma2.lifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });

        rprintln!("Will start DMA\r");

        dma2_st0.cr.modify(|_, w| w.en().enabled());

        cortex_m::interrupt::free(|cs| {
            G_DP.borrow(cs).borrow_mut().replace(dp);

            unsafe { NVIC::unmask(interrupt::DMA2_STREAM0) }
        });

        #[allow(clippy::empty_loop)]
        loop {}
    } else {
        panic!("Cannot Get Peripheral\r\n");
    }
}

#[interrupt]
fn DMA2_STREAM0() {
    cortex_m::interrupt::free(|cs| {
        let dp_cell = G_DP.borrow(cs);

        if dp_cell.borrow().is_none() {
            rprintln!("Device Peripherals is not store in global static, will mask NVIC");
            NVIC::mask(interrupt::DMA2_STREAM0);
            return;
        }

        let dp_ref = dp_cell.borrow();
        let dp = dp_ref.as_ref().unwrap();

        let dma2 = &dp.DMA2;
        let dma2_lisr = dma2.lisr.read();

        // 逐个检测我们关心的标识位

        if dma2_lisr.feif0().is_error() {
            dma2.lifcr.write(|w| w.cfeif0().clear());
            panic!("FIFO Error\r\n");
        }

        if dma2_lisr.teif0().is_error() {
            dma2.lifcr.write(|w| w.cteif0().clear());
            panic!("Transfer Error\r\n");
        }

        if dma2_lisr.htif0().is_half() {
            rprintln!("Half Transfered\r");
            dma2.lifcr.write(|w| w.chtif0().clear());
        }

        // 若传输完成位被挂起，则打印结果、掩蔽对应的中断、并关闭 DMA 时钟
        if dma2_lisr.tcif0().is_complete() {
            rprintln!("Transfer Completed\r");
            dma2.lifcr.write(|w| w.ctcif0().clear());
            rprintln!(
                "DST_LIST End Value: {:?}\r",
                core::str::from_utf8(&DST_LIST).unwrap()
            );
            rprintln!("DMA2 transfer finish, will mask NVIC\r");
            NVIC::mask(interrupt::DMA2_STREAM0);
            rprintln!("and turn off DMA2\r");
            dp.RCC.ahb1enr.modify(|_, w| w.dma2en().disabled());
        }
    })
}
