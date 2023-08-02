//! BKP 寄存器
//!
//! RTC 模块中，还有一组被称为 RTC_BKPxR 的寄存器
//! 我们可以用这些寄存器存储少量的数据，
//! 只要 VDD 和 VBAT 两个电源中的一个有电，这些数据就可以跨越 System Reset 而不丢失
//! 且在仅有 VBAT 接入电源的情况下，RTC_BKPxR 寄存器

//! 这个案例中，我们简单演示一下

#![no_std]
#![no_main]

use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::pac;

use panic_rtt_target as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().unwrap();

    let bkp0 = dp.RTC.bkpr[0].read().bkp().bits();

    if bkp0 == 0 {
        // 这里有一个 rprintln!() 的小细节，如果我们在格式化的时候，使用了 `#` 修饰符，那么打印出来的 `0x` `0b` 等符号也是占长度的
        // 因此，虽然此处 RTC_BKP0R 只有 4 个 byte，但我们需要将最小长度填充为 2+2*4 = 10 位
        rprintln!(
            "RTC_BKP0R value is {:#010X}\nwill try unlock RTC, and write 0xFAFAFAFA to RTC_BKP0R",
            bkp0
        );

        dp.RCC.apb1enr.modify(|_, w| w.pwren().enabled());
        dp.PWR.cr.modify(|_, w| w.dbp().set_bit());
        dp.RTC.bkpr[0].write(|w| w.bkp().bits(0xFAFAFAFA));

        let bkp0 = dp.RTC.bkpr[0].read().bkp().bits();
        rprintln!("write done, current RTC_BKP0R value: {:#010X?}\nyou can try push reset button to see the phenomenon", bkp0);
    } else {
        rprintln!("RTC_BKP0R value is {:#010X?}", bkp0);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
