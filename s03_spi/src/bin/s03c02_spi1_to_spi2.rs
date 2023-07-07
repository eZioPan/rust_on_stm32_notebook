//! 让 SPI1 作为主机，向作为从机的 SPI2 发送 2 个字节

//! 本次实现尽量使用 hal 提供的功能，而不使用 pac
//!
//! 引脚接线表
//!           SPI1 <-> SPI2
//! CS        PA04 >-> PB12  SPI2_NSS
//! SPI1_SCK  PA05 >-> PB13  SPI2_SCK
//! SPI1_MISO PA06 <-< PB14 SPI2_MISO
//! SPI1_MOSI PA07 >-> PB15 SPI2_MOSI

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use cortex_m::{interrupt::Mutex, prelude::*};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use stm32f4xx_hal::{
    gpio::{Output, Pin, PinState},
    hal as ehal, // stm32f4xx_hal::hal 其实是 embedded_hal 的再次导出，因此这里将其标记为 ehal
    interrupt,
    pac::{self, NVIC},
    prelude::*,
    spi::{self, Spi1, SpiSlave2},
};

// SPI1 的全局静态量，SPI1 作为主控端，并发出数据
static G_SPI_MASTER: Mutex<RefCell<Option<Spi1<false, u16>>>> = Mutex::new(RefCell::new(None));

// SPI1 片选从机的引脚 1
// 这里使用了 GPIO PA04，这个引脚是我们任选的
static G_SPI_MASTER_CS: Mutex<RefCell<Option<Pin<'A', 4, Output>>>> =
    Mutex::new(RefCell::new(None));

// SPI2 的全局静态量，SPI2 作为从机端，并接收数据
static G_SPI_SLAVE: Mutex<RefCell<Option<SpiSlave2<false, u16>>>> = Mutex::new(RefCell::new(None));

// 记录一下发送是否完成
static G_SENT: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot get Device Peripherals");
    let mut cp = pac::CorePeripherals::take().expect("Cannot get Cortex Peripherals");

    // 初始化 RCC，主要是初始化系统时钟
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.use_hse(8.MHz()).sysclk(64.MHz()).freeze();

    // 初始化 SPI1 要使用的 GPIO Port A
    let gpioa = dp.GPIOA.split();

    // 这里有一个额外的步骤：预先设置了将要成为 SPI1_SCK、SPI1_MISO、SPI1_MOSI 的 GPIOPA5、GPIOPA6、GPIOPA7 的内部上下拉电阻的状态
    // 这一步操作是为了保证，在结束传输，SPI1 模块关闭之后，到 SPI2 模块关闭之前，
    // 被还原为普通 GPIO 引脚的三个引脚，不会随意变换电平，以至于 SPI2 在此期间因扰动而产生错误的信息
    //
    // 由于下方我们配置 SPI1 的时候，选择的模式是 MODE_0，其中设置了 SCK 低电平表示空置，因此这里 sck_pin 要启用下拉电阻
    let sck_pin = gpioa.pa5.internal_pull_down(true);
    let miso_pin = gpioa.pa6.internal_pull_down(true);
    let mosi_pin = gpioa.pa7.internal_pull_down(true);

    // 除了上面初始化的三个 SPI1 所要使用到的引脚，我们还要初始化一个用于片选 SPI2 的额外的引脚
    // 而且，由于 SPI2 的 NSS 是低电平有效，因此这个引脚必须要切换为高电平输出状态
    let cs_pin = gpioa.pa4.into_push_pull_output_in_state(PinState::High);

    // 初始化 SPI1
    // 设置 SCK、MISO、MOSI 三个引脚，设置 SPI 通信频率，设置数据帧大小为 2 bytes
    let mut spi_master = dp
        .SPI1
        .spi(
            (sck_pin, miso_pin, mosi_pin),
            ehal::spi::MODE_0,
            1.MHz(),
            &clocks,
        )
        .frame_size_16bit();

    // 让 SPI1 发出发送缓冲为空的中断
    // 该中断表示可以发送数据
    spi_master.listen(spi::Event::Txe);

    // 初始化 SPI2 要使用的 GPIO Port B
    let gpiob = dp.GPIOB.split();

    let slave_nss = gpiob.pb12.internal_pull_up(true);

    let mut spi_slave = dp
        .SPI2
        .spi_slave(
            (gpiob.pb13, gpiob.pb14, gpiob.pb15, Some(slave_nss.into())),
            ehal::spi::MODE_0,
        )
        .frame_size_16bit();

    // 让 SPI2 发出接收缓冲为空的中断
    // 该中断表示可以接收数据
    spi_slave.listen(spi::Event::Rxne);

    cortex_m::interrupt::free(|cs| {
        rprintln!("setup NVIC\r\n");

        // 将本地变量注入到全局静态量中
        G_SPI_MASTER.borrow(cs).borrow_mut().replace(spi_master);
        G_SPI_MASTER_CS.borrow(cs).borrow_mut().replace(cs_pin);
        G_SPI_SLAVE.borrow(cs).borrow_mut().replace(spi_slave);

        unsafe {
            // 让 SPI1 的优先级低于 SPI2
            // 首先保证接收端可以接收，再让发送端可以发送
            cp.NVIC.set_priority(interrupt::SPI1, 20);
            cp.NVIC.set_priority(interrupt::SPI2, 10);

            NVIC::unmask(interrupt::SPI1);
            NVIC::unmask(interrupt::SPI2);
        }
    });

    loop {}
}

#[interrupt]
fn SPI1() {
    cortex_m::interrupt::free(|cs| {
        rprintln!("SPI1 interrupt triggered\r");
        let master_refcell = G_SPI_MASTER.borrow(cs);
        // 这里我们另起了一个作用域，这样对 master_refcell 的借用会限制在这个作用域里，
        // 在这个作用域之外，我们会尝试释放 master_refcell 中包含的对象
        {
            let mut master_mut = master_refcell.borrow_mut();
            match master_mut.as_mut() {
                Some(master) => {
                    // 若 SPI1 处于繁忙状态，则立刻返回
                    if master.is_busy() {
                        rprintln!("SPI1 is busy\r\n");
                        return;
                    }

                    if master.is_tx_empty() {
                        rprintln!("SPI1 TX is Empty\r");
                        let mut cs_pin_mut = G_SPI_MASTER_CS.borrow(cs).borrow_mut();
                        let cs_pin = cs_pin_mut.as_mut().unwrap();
                        if cs_pin.is_set_high() {
                            rprintln!("will pull down SPI2 NSS...\r");
                            cs_pin.set_low();
                        }
                        rprintln!(".. and send 0xFFAA\r\n");
                        // 注意目前 hal 提供的方法为阻塞式发送，不结束不返回
                        master
                            .send(0xFFAA)
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
        }

        // 若已经是发送完成的状态，则掩蔽 SPI1 中断，并关闭 SPI1
        if G_SENT.borrow(cs).get() {
            rprintln!("Data sending completed, will mask out SPI1 from NVIC, then shutdown SPI1\r");
            // 这个花括号必不可少，它标识了 spi1_ref 的作用域
            // 在离开作用域之后，spi1_ref、spi1 就都被丢弃了
            // 防止与后面的 master_refcell.replace() 冲突
            {
                // 等待 SPI1 处于非繁忙的状态，再关闭 SPI1
                let master_ref = master_refcell.borrow();
                let master = master_ref.as_ref().unwrap();
                rprintln!("Waiting for BSY bit clean\r");
                while master.is_busy() {}
            }
            // 第一步，关闭 NVIC 中对应的中断
            NVIC::mask(interrupt::SPI1);
            // 第二步，将存储在 RefCell 中的 SPI 对象移动出来，并重新向全部变量中灌注 None
            let mut master = master_refcell.replace(None).unwrap();
            // 第三步，关闭 SPI1 模块
            master.enable(false);
            rprintln!("SPI1 disabled\r");
            // 第四步，将 master 绑定的引脚释放出来，同时也解构了 master
            master.release();
            rprintln!("SPI1 pins released\r\n");
        };
    });
}

#[interrupt]
fn SPI2() {
    cortex_m::interrupt::free(|cs| {
        let send_state = G_SENT.borrow(cs).get();

        let slave_refcell = G_SPI_SLAVE.borrow(cs);

        // 与 SPI1 中断处理函数类似，这里也要另开一个作用域，方便后面的
        {
            rprintln!("SPI2 interrupt triggered\r");
            let mut slave_mut = slave_refcell.borrow_mut();
            let slave = slave_mut.as_mut().unwrap();

            // 中断触发，检查 Rx 是否为空，
            // 为空读一下数据，不为空说明产生了错误，这里我们直接 panic
            if slave.is_rx_not_empty() {
                rprintln!("SPI2 RX is Not Empty, will read data\r");
                let data = slave.read_nonblocking().unwrap();
                rprintln!("Get Data: 0x{:X}\r\n", data);
            } else {
                panic!("Something Wrong!\r\n");
            }
        }

        // 检测发送状态，若发送被标记为完成，则逐步关闭 SPI2
        if send_state {
            {
                // 在这个作用域中，我们还是需要 .borrow_mut() 的，因此，
                // 我们尽量检查 SPI2 的各种状态，保证 SPI2 处于可以解构的状态
                // 这样，在脱离这个作用域之后，我们就可以通过 .replace() 和 .release() 解构 slave 了

                let mut slave_mut = slave_refcell.borrow_mut();
                let slave = slave_mut.as_mut().unwrap();
                // 等待 Slave 的 Busy Flag 置空
                rprintln!("Waiting for BSY bit clean\r");
                while slave.is_busy() {}
                rprintln!("Data receiving completed ...\r");
                rprintln!("... will mask out SPI2 from NVIC ...\r");
                NVIC::mask(interrupt::SPI2);
                rprintln!("... then disable SPI2\r");
                slave.enable(false);
            }

            // 此处我们正式释放 slave 控制的引脚
            let slave = slave_refcell.replace(None).unwrap();
            slave.release();
            rprintln!("SPI2 pins released\r\n");
        }
    });
}
