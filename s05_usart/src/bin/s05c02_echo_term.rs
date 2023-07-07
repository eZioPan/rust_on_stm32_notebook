//! 简易 echo terminal
//!
//! 目标：实现一个串行终端，它能实时显示每个输入的字符，并在按下回车之后，再次显示当前行的内容
//!
//! 电路连接方案：
//! GPIO PA9 <-> DAPLink Rx
//! GPIO PA10 <-> DAPLink Tx

#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};

use panic_rtt_target as _;

use cortex_m::interrupt::Mutex;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac::{self, interrupt, NVIC, USART1};

static G_DP: Mutex<RefCell<Option<pac::Peripherals>>> = Mutex::new(RefCell::new(None));

// BUG:
// 这里我遇见了一个小问题，如果 RefCell 中的数据长度超过 112，且我们在（任何）中断处理函数中 .borrow(cs) 了该变量
// 那么 rprintln!() 就会失效，不仅是中断中的 rprintln!() 会失效，整个程序中的 rprintln!() 都会失效
// 如果用的是 Cell，那么列表的长度可以稍长一些，超过 116 才会失效
// 感觉是 cortex_m::interrupt::Mutex 和 rtt_target create 之间的冲突
const BUF_LENGTH: usize = 64;
static G_LINE_BUF: Mutex<RefCell<[u8; BUF_LENGTH]>> = Mutex::new(RefCell::new([0u8; BUF_LENGTH]));
// 这里，G_LINE_BUF_INDEX 里包裹的数据最好是 usize 类型的，毕竟是用来索引数组的
static G_LINE_BUF_INDEX: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));

static G_LINE_COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(1));

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    rprintln!("Start to Process\r");

    if let Some(dp) = pac::Peripherals::take() {
        // 这里我们换一个思路，上来我们直接把 dp 注入到全局静态量中
        // 之后的操作就可以无需考虑中断发生时 dp 未注入的问题了
        cortex_m::interrupt::free(|cs| {
            G_DP.borrow(cs).borrow_mut().replace(dp);
        });

        setup_hse();
        setup_gpio_pins();
        setup_uart1();

        prepare_echo_term();

        loop {}
    } else {
        panic!("Cannot Get Peripherals");
    }
}

// 切换到 HSE 时钟源
fn setup_hse() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().expect("Empty G_DP\r\n");

        let rcc = &dp.RCC;
        rcc.cr.modify(|_, w| w.hseon().on());
        while rcc.cr.read().hserdy().is_not_ready() {}
        rcc.cfgr.modify(|_, w| w.sw().hse());
        while !rcc.cfgr.read().sws().is_hse() {}
    });
}

// 准备好 USART1 要使用的 GPIO PA9 和 PA10
fn setup_gpio_pins() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().expect("Empty G_DP\r\n");

        dp.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

        let gpioa = &dp.GPIOA;

        gpioa.afrh.modify(|_, w| {
            w.afrh9().af7(); // Tx
            w.afrh10().af7(); // Rx
            w
        });

        gpioa.pupdr.modify(|_, w| {
            w.pupdr9().pull_up(); // 在空闲时，自己的 Tx 线应该被拉高
            w
        });

        gpioa.moder.modify(|_, w| {
            w.moder9().alternate();
            w.moder10().alternate();
            w
        });
    })
}

fn setup_uart1() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().expect("Empty G_DP\r\n");

        dp.RCC.apb2enr.modify(|_, w| w.usart1en().enabled());

        let serial1 = &dp.USART1;

        // 这里我们选择了 9 bit 数据为一个 frame
        // 并启用了 1 bit 的奇偶校验
        serial1.cr1.modify(|_, w| {
            w.ue().enabled();
            // 9 bit 模式，其中包含 8 bit 的数据，以及 1 bit 的奇偶校验
            w.m().m9();
            // 设置奇偶校验为偶校验
            w.ps().even();
            // 启用奇偶校验
            w.pce().enabled();
            w
        });

        serial1.cr2.modify(|_, w| w.stop().stop1());

        // 波特值还是取 115200
        serial1.brr.write(|w| {
            w.div_mantissa().bits(4);
            w.div_fraction().bits(5);
            w
        });

        unsafe { NVIC::unmask(interrupt::USART1) };

        serial1.cr1.modify(|_, w| {
            // 挂起接收非空的中断
            w.rxneie().enabled();
            // 由于我们做的是 echo terminal，
            // 是不需要 TXE 触发中断的，要发送的时候直接轮询 TXE 标识位即可
            // w.txeie().enabled();
            w.re().enabled();
            w.te().enabled();
            w
        });
    });
}

// 工具函数：让 USART1 发送单个字节
fn send_byte_to_usart1(serial1: &USART1, byte: u8) {
    // 每次发送前，都等待 USART1 发送空闲
    while serial1.sr.read().txe().bit_is_clear() {}
    // 由于 dr 最多可以写入 9 个 bit，因此这里 DR 位得使用 u16
    // 但实际上我们还是只能发送 8 bit 的数据
    serial1.dr.write(|w| w.dr().bits(byte as u16));
}

// 工具函数：让 USART1 发送一串字节
fn send_bytes_to_usart1(serial1: &USART1, bytes: &[u8]) {
    // 这个 &byte 的写法，算是解了 byte 的引用了
    // 而且 u8 实现了 Copy trait，因此这里不会有移出错误
    for &byte in bytes {
        send_byte_to_usart1(serial1, byte);
    }
}

// 工具函数：让 USART1 发送一个字符串
fn send_str_to_usart1(serial1: &USART1, str: &str) {
    send_bytes_to_usart1(serial1, str.as_bytes());
}

// 在正式输出前，提前打印一下输入提示符
fn prepare_echo_term() {
    cortex_m::interrupt::free(|cs| {
        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().expect("Empty G_DP\r\n");

        let serial1 = &dp.USART1;

        send_str_to_usart1(serial1, ">>> ");
    })
}

#[interrupt]
fn USART1() {
    cortex_m::interrupt::free(|cs| {
        let buf_index = G_LINE_BUF_INDEX.borrow(cs).get();

        let mut buf_refmut = G_LINE_BUF.borrow(cs).borrow_mut();
        let buf = buf_refmut.as_mut();

        let dp_ref = G_DP.borrow(cs).borrow();
        let dp = dp_ref.as_ref().expect("Empty G_DP\r\n");

        let serial1 = &dp.USART1;

        // 只要读取过 SR，当前的中断触发就会被清理，不会产生多次触发
        serial1.sr.read();

        let cur_char = serial1.dr.read().dr().bits() as u8;

        // 检测输入的字符是否为回车
        // 是回车就把缓存中的数据发送出去
        // 不是回车就存储数据
        match cur_char {
            b'\r' => {
                send_str_to_usart1(serial1, "\r\n");

                // 打印行计数
                let line_cnt = G_LINE_COUNT.borrow(cs).get();
                let mut buffer = itoa::Buffer::new();
                let num_str = buffer.format(line_cnt);
                send_str_to_usart1(serial1, num_str);
                send_str_to_usart1(serial1, ": ");

                // 打印行缓冲内容
                send_bytes_to_usart1(serial1, &buf[0..buf_index]);

                // 最后额外输出一个换行，并打印提示符
                send_str_to_usart1(serial1, "\r\n>>> ");

                // 索引清零
                G_LINE_BUF_INDEX.borrow(cs).set(0);
                // 清空 buf
                buf.fill(0u8);

                // 最后递增一下行计数
                G_LINE_COUNT.borrow(cs).set(line_cnt + 1);
            }
            _ => {
                // 回显当前输出的字符
                send_byte_to_usart1(serial1, cur_char);

                // 判定当前是否有足够大的空间容纳新的字符，若没有，则直接丢弃新来的字符
                if buf_index == BUF_LENGTH - 1 {
                    return;
                }
                // 将字符保存到 buf 里
                buf[buf_index] = cur_char;
                // 并让 buf 的索引 +1
                G_LINE_BUF_INDEX.borrow(cs).set(buf_index + 1);
            }
        };

        rprintln!("{:?}", core::str::from_utf8(buf).unwrap());
    });
}
