//! 让 SPI1 作为主机，向作为从机的 SPI2 发送 2 个字节

//! 本次实现尽量使用 hal 提供的功能，而不使用 pac
//! 但由于 hal 中 spi slave 仅实现了 DMA 的读写，没有实现直接读写，因此 SPI2 的读取部分依旧得使用 pac 实现
//!
//! 目前 hal 仅支持 SSM 模式，因此，本次需要连接的引脚如下
//!           SPI1 <-> SPI2
//! SPI1_SCK  PA05 >-> PB13  SPI2_SCK
//! SPI1_MISO PA06 <-< PB14 SPI2_MISO
//! SPI1_MOSI PA07 >-> PB15 SPI2_MOSI

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::{
    gpio::{self, Input, Output, PinState},
    hal, interrupt, pac,
    prelude::*,
    spi::{self, Master, Spi1},
};

// SPI1 的全局静态量
// 比较特殊的是 gpio::Pin 的部分，三个引脚都额外标注了是处于输入还是输出的状态
// 这是由于，我们希望 SPI1 在停止工作，SPI1 的引脚被还原为普通的 GPIO 后，引脚的电平不要随意波动
// 因此我们将引脚组合为 SPI1 之前，就为它们设置好输入输出状态，并设置相应的高低拉/高低电平
// 这样 SPI1 结构，它们被还原后，电平就能保持稳定
static G_SPI1: Mutex<
    RefCell<
        Option<
            Spi1<
                (
                    gpio::Pin<'A', 5, Output>, // SCK
                    gpio::Pin<'A', 6, Input>,  // MISO
                    gpio::Pin<'A', 7, Output>, // MOSI
                ),
                false,  // BIDI
                u16,    // DDF
                Master, // MSTR
            >,
        >,
    >,
> = Mutex::new(RefCell::new(None));

// 记录一下发送是否完成
static G_SENT: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    if let (Some(dp), Some(mut cp)) = (pac::Peripherals::take(), pac::CorePeripherals::take()) {
        // 初始化 RCC，主要是初始化系统时钟
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.use_hse(8.MHz()).sysclk(64.MHz()).freeze();

        // 初始化 SPI1 要使用的 GPIO Port A
        let gpioa = dp.GPIOA.split();

        // 这里有一个额外的步骤：预先设置了将要成为 SPI1_SCK、SPI1_MISO、SPI1_MOSI 的 GPIOPA5、GPIOPA6、GPIOPA7 的状态
        // 这一步操作是为了保证，在结束传输，SPI1 模块关闭之后，到 SPI2 模块关闭之前，
        // 被还原为普通 GPIO 引脚的三个引脚，不会随意变换电平，以至于 SPI2 在此期间因扰动而产生错误的信息
        let sck_pin = gpioa.pa5.into_push_pull_output_in_state(PinState::High);
        let miso_pin = gpioa.pa6.into_pull_down_input();
        let mosi_pin = gpioa.pa7.into_push_pull_output_in_state(PinState::Low);

        // 初始化 SPI1
        // 设置 SCK、MISO、MOSI 三个引脚，设置 SPI 通信频率，设置数据帧大小为 2 bytes
        let mut spi1 = dp
            .SPI1
            .spi(
                (sck_pin, miso_pin, mosi_pin),
                hal::spi::MODE_0,
                1.MHz(),
                &clocks,
            )
            .frame_size_16bit();

        // 让 SPI1 发出发送缓冲为空的中断
        // 该中断表示可以发送数据
        spi1.listen(spi::Event::Txe);

        // 初始化 SPI1 要使用的 GPIO Port B
        let gpiob = dp.GPIOB.split();

        let mut spi2 = dp
            .SPI2
            .spi_slave(
                (gpiob.pb13, gpiob.pb14, gpiob.pb15),
                hal::spi::MODE_0,
                1.MHz(),
                &clocks,
            )
            .frame_size_16bit();

        // 让 SPI2 发出接收缓冲为空的中断
        // 该中断表示可以接收数据
        spi2.listen(spi::Event::Rxne);

        cortex_m::interrupt::free(|cs| {
            rprintln!("setup NVIC\r\n");

            // 将本地变量注入到全局静态量中
            G_SPI1.borrow(cs).replace(Some(spi1));

            unsafe {
                // 让 SPI1 的优先级低于 SPI2
                // 首先保证接收端可以接收，再让发送端可以发送
                cp.NVIC.set_priority(interrupt::SPI1, 20);
                cp.NVIC.set_priority(interrupt::SPI2, 10);

                NVIC::unmask(interrupt::SPI1);
                NVIC::unmask(interrupt::SPI2);
            }
        });
    }

    loop {
        #[cfg(not(debug_assertions))]
        cortex_m::asm::wfi()
    }
}

#[interrupt]
fn SPI1() {
    cortex_m::interrupt::free(|cs| {
        rprintln!("SPI1 interrupt triggered\r");

        let spi1_refcell = G_SPI1.borrow(cs);

        // 需要在使用 G_SPI1 之前做出判定
        // 若已经是发送完成的状态，则掩蔽 SPI1 中断，并关闭 SPI1
        if G_SENT.borrow(cs).get() {
            rprintln!("Data sending completed, will mask out SPI1 from NVIC, then shutdown SPI1\r");
            // 这个花括号必不可少，它标识了 spi1_ref 的作用域
            // 在离开作用域之后，spi1_ref、spi1 就都被丢弃了
            // 防止与后面的 spi1_refcell.replace() 冲突
            {
                // 等待 SPI1 处于非繁忙的状态，再关闭 SPI1
                let spi1_ref = spi1_refcell.borrow();
                let spi1 = spi1_ref.as_ref().unwrap();
                rprintln!("Waiting for BSY bit clean\r");
                while spi1.is_busy() {}
            }
            // 第一步，关闭 NVIC 中对应的中断
            NVIC::mask(interrupt::SPI1);
            // 第二步，将存储在 RefCell 中的 SPI 对象移动出来，并重新向全部变量中灌注 None
            let spi1 = spi1_refcell.replace(None).unwrap();
            // 第三步，将 spi1 绑定的引脚释放出来，同时也解构了 spi1
            spi1.release();
            rprintln!("SPI1 closed\r\n");
            return;
        };

        let mut spi1_mut = spi1_refcell.borrow_mut();
        match spi1_mut.as_mut() {
            Some(spi1) => {
                // 若 SPI1 处于繁忙状态，则立刻返回
                if spi1.is_busy() {
                    rprintln!("SPI1 is busy\r\n");
                    return;
                }

                if spi1.is_tx_empty() {
                    rprintln!("SPI1 TX is Empty, will send 0xFFAA\r\n");
                    // 注意目前 hal 提供的方法为阻塞式发送，不结束不返回
                    spi1.send(0xFFAA)
                        .and_then(|_| {
                            G_SENT.borrow(cs).set(true);
                            Ok(())
                        })
                        .unwrap();
                }
            }
            None => {
                // 注意，如果 SPI 没有被设置到全局静态量中，则不应该启用中断函数
                NVIC::mask(interrupt::SPI1);
                rprintln!("SPI1 not avaliable, interrupt SPI1 masked\r\n");
            }
        }
    });
}

// Slave 模式没有实现直接的 .read() 方法，这里我们还是简单使用 PAC 的实现一下读取效果
#[interrupt]
fn SPI2() {
    cortex_m::interrupt::free(|cs| {
        rprintln!("SPI2 interrupt triggered\r");
        unsafe {
            let dp = pac::Peripherals::steal();
            if dp.SPI2.sr.read().rxne().is_not_empty() {
                rprintln!("SPI2 RX is Not Empty, will read data\r");
                rprintln!("Get Data: {:X}\r", dp.SPI2.dr.read().bits());

                // 检测发送状态，若发送被标记为完成，则逐步关闭 SPI2
                if G_SENT.borrow(cs).get() {
                    rprintln!("Waiting for BSY bit clean\r");
                    while dp.SPI2.sr.read().bsy().is_busy() {}
                    rprintln!("Data recieving completed, will mask out SPI2 from NVIC, then shutdown SPI2\r");
                    NVIC::mask(interrupt::SPI2);
                    dp.RCC.apb1enr.modify(|_, w| w.spi2en().disabled());
                    rprintln!("SPI2 closed\r\n");
                }
            }
        }
    });
}
