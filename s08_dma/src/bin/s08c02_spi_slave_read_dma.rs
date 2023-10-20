//! 尝试将 SPI1-to-SPI2 中的接收端通过触发 DMA 来获取
//!
//! 这样 SPI1 是通过中断通知 Cortex 核心写 SPI 的 DR 寄存器完成逐字节发送的，SPI2 收到字节之后，会通知 DMA 将收到的数据从 DR 中转移到我们指定的 SRAM 的位置中，
//! 而且 DMA 在接收了预设数量的字节之后，会再通过中断通知 Cortex 核心执行下一步的处理

#![no_main]
#![no_std]

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU8, Ordering},
};

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use stm32f4xx_hal::{interrupt, pac::Peripherals};

static G_DP: Mutex<RefCell<Option<Peripherals>>> = Mutex::new(RefCell::new(None));

const SRC_LIST: [u8; 8] = [10, 11, 12, 13, 14, 15, 16, 17];
const LIST_LEN: usize = SRC_LIST.len();

static INDEX: AtomicU8 = AtomicU8::new(0);

#[link_section = ".data"]
static DST_LIST: [u8; LIST_LEN] = [1, 2, 3, 4, 5, 6, 7, 8];

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Program Start");

    rprintln!("DST_LIST begin state: {:?}", DST_LIST);

    let dp = Peripherals::take().unwrap();

    // 总的来说，配置的顺序为“防御性配置顺序”，目标只有一个，防止错误的触发，毕竟控制流不再是简单的代码书写顺序，
    // 其中有中断打断执行顺序，还有 DMA 会独立运行在 Cortex 核心之外
    setup_dma1(&dp);
    setup_spi2(&dp);
    setup_spi1(&dp);

    cortex_m::interrupt::free(|cs| {
        G_DP.borrow(cs).borrow_mut().replace(dp);

        // 由于上面我们已经配置好了所有的外设
        // 因此一旦我们 unmask 了中断，发送就会自动开始
        unsafe {
            NVIC::unmask(interrupt::DMA1_STREAM3);
            NVIC::unmask(interrupt::SPI1);
        };
    });

    #[allow(clippy::empty_loop)]
    loop {}
}

// 首先我们要配置 DMA
// 从 RM 的表 DMA1 request mapping 可知
// SPI2_RX 发出的 DMA 请求，可以通过 DMA1 的 STREAM3 的 Channel 0 发给 DMA1
// 而且在与 peripheral 交互的时候 DMA 实际上是等待外设的指令，才会开启操作的
fn setup_dma1(dp: &Peripherals) {
    rprintln!("Setup DMA1");

    let rcc = &dp.RCC;

    rcc.ahb1rstr.write(|w| w.dma1rst().set_bit());
    rcc.ahb1rstr.write(|w| w.dma1rst().clear_bit());
    rcc.ahb1enr.modify(|_, w| w.dma1en().enabled());

    let dma1 = &dp.DMA1;
    let dma1_st3 = &dma1.st[3];

    if dma1_st3.cr.read().en().is_enabled() {
        dma1_st3.cr.modify(|_, w| w.en().disabled());
        while dma1_st3.cr.read().en().is_enabled() {}
    }

    dma1_st3.cr.modify(|_, w| {
        // 由于我们要将 SPI2 的收到的数据拷贝至内存中，
        // 因此我们要使用 peripheral-to-memory 模式
        w.dir().peripheral_to_memory();
        // Channel 要选择 0 号
        w.chsel().bits(0);
        // 从 DMA 的 FIFO 到 memory，我们依旧可以使用单次 AHB 访问发送 8 单位数据
        w.mburst().incr8();
        // 内存地址需要偏移，因为我们需要把收到的数据保存到内存中
        w.minc().incremented();
        // 在我们的设置中 SPI 一次性发送 1 byte 数据，因此这里也设置成 1 byte 数据
        w.msize().bits8();
        // SPI 每收到一个数据，我们就让 DMA 访问一次 DR 寄存器即可
        w.pburst().single();
        // 外设寄存器的地址就应该是固定的了，因为我们每次访问的都是 DR 寄存器了
        w.pinc().fixed();
        // 同上，SPI 每次发送 1 byte，于是这里就设置为 1 byte
        w.psize().bits8();
        // 然后我们把 全发送、发送错误的中断拉起来
        w.tcie().enabled();
        w.teie().enabled();
        w
    });

    dma1_st3.fcr.modify(|_, w| {
        // 在与 peripheral 交换数据的情况下，我们必须要手动设置这个位
        w.dmdis().disabled();
        // FIFO 错误也被我们拉起来了
        w.feie().enabled();
        // 依照 RM 的表 FIFO threshold configurations
        // 在 MSIZE 1 byte，MBURST 8 步进的情况下，FIFO 处于半满状态，刚好满足 8 byte 字节的单次传输
        // 因此我们这里可以将 FIFO 设置为 8 byte
        w.fth().half();
        w
    });

    // 设置 DMA 外设端对应的起始地址
    dma1_st3
        .par
        .write(|w| unsafe { w.pa().bits(dp.SPI2.dr.as_ptr() as u32) });

    // 设置 DMA 内存端对应的起始地址
    dma1_st3
        .m0ar
        .write(|w| unsafe { w.m0a().bits((&DST_LIST as *const _) as u32) });

    // 设置 DMA 的转运次数为源列表的长度
    dma1_st3.ndtr.write(|w| w.ndt().bits(LIST_LEN as u16));

    // 还是一样，启用前需要清除所有的错误位
    dma1.hifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });
    dma1.lifcr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });

    // 然后我们就启用 DMA
    // 此时 DMA 就会进入等待外设请求的状态
    dma1_st3.cr.modify(|_, w| w.en().enabled());

    rprintln!("DMA1 ready");
}

fn setup_spi2(dp: &Peripherals) {
    rprintln!("Setup SPI2 (slave mode)");

    let rcc = &dp.RCC;

    rcc.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    let gpioa = &dp.GPIOA;

    gpioa.afrh.modify(|_, w| {
        w.afrh9().af5();
        w.afrh10().af5();
        w.afrh11().af5();
        w.afrh12().af5();
        w
    });

    gpioa.moder.modify(|_, w| {
        w.moder9().alternate();
        w.moder10().alternate();
        w.moder11().alternate();
        w.moder12().alternate();
        w
    });

    rcc.apb1enr.modify(|_, w| w.spi2en().enabled());

    let spi2 = &dp.SPI2;

    spi2.cr1.modify(|_, w| w.mstr().slave());
    spi2.cr2.modify(|_, w| w.rxdmaen().enabled());
    spi2.cr1.modify(|_, w| w.spe().enabled());

    rprintln!("SPI2 (slave mode) ready");
}

fn setup_spi1(dp: &Peripherals) {
    rprintln!("Setup SPI1 (master mode)");

    let rcc = &dp.RCC;

    rcc.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    let gpioa = &dp.GPIOA;

    gpioa.afrl.modify(|_, w| {
        w.afrl4().af5();
        w.afrl5().af5();
        w.afrl6().af5();
        w.afrl7().af5();
        w
    });

    gpioa.moder.modify(|_, w| {
        w.moder4().alternate();
        w.moder5().alternate();
        w.moder6().alternate();
        w.moder7().alternate();
        w
    });

    rcc.apb2enr.modify(|_, w| w.spi1en().enabled());

    let spi1 = &dp.SPI1;

    // 由于我们不使用外部电路拉高 SPI1 的 NSS 脚，因此这里使用软件管理 NSS
    // 然后我们指定 SPI1 为非 slave 模式
    spi1.cr1.modify(|_, w| {
        w.ssm().enabled();
        w.ssi().slave_not_selected();
        w.mstr().master()
    });
    spi1.cr2.modify(|_, w| {
        w.txeie().not_masked();
        // 这里我们反向操作，将 SPI1 的 NSS 引脚当作 CS 引脚，输出 CS 信号
        // 这样我们就不用在外部设置一个引脚
        // 在同时使用 SSM 和 SSOE 的情况下
        // SPE 置 1 时，NSS 引脚被拉低，SPE 置 0 时 NSS 引脚被拉高
        w.ssoe().enabled();
        w
    });
    spi1.cr1.modify(|_, w| w.spe().enabled());

    rprintln!("SPI1 (master mode) ready");
}

#[interrupt]
fn SPI1() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let cur_index = INDEX.fetch_add(1, Ordering::AcqRel);

        let spi1 = &dp.SPI1;

        let cur_data = SRC_LIST[cur_index as usize];

        rprintln!("SPI1 sending data: {}", cur_data);

        spi1.dr.write(|w| w.dr().bits(cur_data as u16));

        if cur_index as usize >= LIST_LEN - 1 {
            rprintln!("SPI1 sending finish, will disable SPE of SPI1");
            // 注意，这里不要随便关闭 SPI1 外设，防止 NSS 和 SCK 引脚悬空，导致接收端收到错误的数据
            dp.RCC.apb2enr.modify(|_, w| w.spi1en().disabled());
            NVIC::mask(interrupt::SPI1);
        }
    });
}

#[interrupt]
fn DMA1_STREAM3() {
    rprintln!("DMA1_STREAM3 interrupt triggered");

    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().unwrap();

        let dma1 = &dp.DMA1;
        let dma1_lisr = dma1.lisr.read();

        // 逐个检测我们关心的标识位

        if dma1_lisr.feif3().is_error() {
            dma1.lifcr.write(|w| w.cfeif3().clear());
            panic!("FIFO Error\r\n");
        }

        if dma1_lisr.teif3().is_error() {
            dma1.lifcr.write(|w| w.cteif3().clear());
            panic!("Transfer Error\r\n");
        }

        if dma1_lisr.htif3().is_half() {
            dma1.lifcr.write(|w| w.chtif3().clear());
            rprintln!("Half Transfered");
        }

        // 若传输完成位被挂起，则打印结果、掩蔽对应的中断、并关闭 DMA 时钟
        if dma1_lisr.tcif3().is_complete() {
            rprintln!("Transfer Completed");
            dma1.lifcr.write(|w| w.ctcif3().clear());
            rprintln!("DST_LIST end state: {:?}", DST_LIST);
            rprintln!("DMA1 transfer finish, will mask NVIC\r");
            NVIC::mask(interrupt::DMA1_STREAM3);
            rprintln!("and turn off DMA1 & SPI2");
            dp.RCC.ahb1enr.modify(|_, w| w.dma1en().disabled());
            dp.RCC.apb1enr.modify(|_, w| w.spi2en().disabled());
        }
    })
}
