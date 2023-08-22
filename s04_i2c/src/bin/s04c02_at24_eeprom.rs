//! I2C 接口的 EEPROM 的读和写
//!
//! EEPROM：电子式可擦除可编程只读存储器
//!
//! EEPROM 相较于 NOR 和 NAND 的特点是，EEPROM 是可以逐字节擦除和写入的，而后两者写入可以按照字节的大小执行，但是擦除得按照一个一个的块（block）来执行
//! 由于 EEPROM 可以逐字节写入，因此 EEPROM 的操作中，没有单独的“擦除”操作，写操作本身就是对原有数据的抹除，以及对新数据的写入
//! 而 NOR 和 NAND 都得成块擦除，因此它们有独立的擦除操作
//!
//! EEPROM 也是三者中单字节需要的晶体管最多、制造成本最高的一种非易失性存储器，大小一般在几 KB 到几百 KB，和 NOR 随随便便几 MB，NAND 随随便便几个 GB 的容量比起来，就小了非常多
//!
//! 我手上的这块芯片是 Atmel 生产的 AT24C02C 系列的芯片，8 引脚 PDIP（plastic dual in-line package）
//!
//! 它上面的丝印迷惑了我很久，
//!
//! 第一行写着 ATMLU105，看起来就像是一个产品型号一样，但实际你怎么搜索第一行的信息，都查不到有效的内容，因为它压根就不是型号（不是型号为啥要写第一行……）
//! 第二行的几个字母 02CM 才是实际的型号，连着 atmel 一起查，才能找到 AT24C02C 芯片的 datasheet，而且你第一眼还不一定会确定这个就是你要找的 datasheet
//! 因为丝印的信息被放到了 datasheet 的 Part Marking 章节，而且是分开说明的……直接搜索还不一定知道是啥意思……
//! 根据 datasheet，我这颗芯片的前两行的丝印 ATMLU105 02CM PH，表示的是，
//! 我这块芯片是 Ateml 生产的、工业级（锡铅铜引脚）、2011 年、第 5 周生产的、型号为 AT24C02C、最小电压为 1.7 V、产地为菲律宾的芯片
//! 最后一行是生产批号，对于我们来说没有啥意义
//!
//! 由于 EEPROM 没有擦除操作，因此，EEPROM 的操作就可以简单分为两类“读”和“写”
//! 而且由于 I2C 指令本来就带有“读”和“写”和区分，因此 I2C 接口的 EEPROM 甚至不需要额外约定的指令
//! 大部分操作就是简单的要读写的 EEPROM 的内存地址 + 要读写的数据，这样超级简单的形式
//!
//! 我手上这颗芯片所在的小开发板，除了 2 个 I2C 相关的引脚和 2 个电源引脚以外，还有四个额外的跳线帽，分别控制写保护（拉到高电平禁止写入），和三个地址修改位
//! 这里我们将它们全部拉到 GND，这样就保证了可写入、最后三地址均为 000

#![no_std]
#![no_main]

use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::{
    i2c::{I2c, Mode},
    pac::Peripherals,
    prelude::*,
};

use panic_rtt_target as _;

// 需要注意的是，I2C 的地址一直是 7 bit 或 10 bit 的，因此在使用 hal crate 的时候地址是不需要左移一位的，这个操作 hal crate 会代为行使
const AT24C02C_I2C_ADDR: u8 = 0b1010000;
// 让我们确定一个要写入的内容
const WRITE_STRING: &str = "hello";
// 以及起始地址
const EEMPROM_MEMORY_ADDRESS: u8 = 0x0;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Start Progrme");

    let dp = Peripherals::take().unwrap();
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.use_hse(12.MHz()).freeze();

    let gpiob = dp.GPIOB.split();

    let mut eeprom_i2c = I2c::new(
        dp.I2C1,
        (gpiob.pb6, gpiob.pb7),
        Mode::standard(100.kHz()),
        &clocks,
    );

    // 反复检查 EEPROM 是否已经可以读写
    // 方法就是反复发送空的写指令，如果 EEPROM 返回了 ACK，就表示芯片准备好了
    let mut wait_cnt = 0;
    while eeprom_i2c.write(AT24C02C_I2C_ADDR, &[]).is_err() {
        wait_cnt += 1;
    }
    rprintln!("wait EEPROM ready wait count: {}", wait_cnt);

    let mut buf = [0u8; WRITE_STRING.len()];

    // 在写入之前，我们可以先读取一下 EEPROM 上指定位置原有的数据
    // 这里我们选用在指定位置读的方法
    // 先通过写方法，写一个地址进去，这样 EEPROM 内部的指针就指向了我们需要的内存位置
    // 然后再另起一个读操作，读取我们要的数据
    eeprom_i2c
        .write_read(AT24C02C_I2C_ADDR, &[EEMPROM_MEMORY_ADDRESS], &mut buf)
        .unwrap();

    rprintln!("original data: {:X?}", buf);

    // 写操作则更加直接，就是连续的写，先写地址，然后跟随所有要写入的数据
    eeprom_i2c
        .write_iter(
            AT24C02C_I2C_ADDR,
            [EEMPROM_MEMORY_ADDRESS]
                .into_iter()
                .chain(WRITE_STRING.as_bytes().iter().cloned()),
        )
        .unwrap();

    // 然后我们得等最后一个写操作完成
    wait_cnt = 0;
    while eeprom_i2c.write(AT24C02C_I2C_ADDR, &[]).is_err() {
        wait_cnt += 1;
    }
    rprintln!("wait between write and read, count: {}", wait_cnt);

    // 最后我们来读取一些写入的数据
    eeprom_i2c
        .write_read(AT24C02C_I2C_ADDR, &[EEMPROM_MEMORY_ADDRESS], &mut buf)
        .unwrap();

    rprintln!(
        "Read from written place: {}",
        core::str::from_utf8(&buf).unwrap()
    );

    #[allow(clippy::empty_loop)]
    loop {}
}
