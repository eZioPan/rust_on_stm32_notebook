#![no_std]
#![no_main]

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
use stm32f4xx_hal::{
    gpio::{ErasedPin, OpenDrain, Output},
    pac,
    prelude::*,
    timer::SysDelay,
};

enum Bits {
    Bit4(u8),
    Bit8(u8),
}

#[derive(PartialEq)]
enum ReadWrite {
    Write,
    Read,
}

enum RegisterSelection {
    Command,
    Data,
}

struct FullCommand {
    rs: RegisterSelection,
    rw: ReadWrite,
    data: Option<Bits>, // if it's a read command, then data should be filled by reading process
}

struct LCD {
    pins: LCDPins,
    delayer: SysDelay,
}

struct LCDPins {
    rs_pin: ErasedPin<Output>,
    rw_pin: ErasedPin<Output>,
    en_pin: ErasedPin<Output>,
    db_pins: [ErasedPin<Output<OpenDrain>>; 4],
}

#[cortex_m_rt::entry]
fn main() -> ! {
    rtt_init_print!();

    let dp = pac::Peripherals::take().expect("Cannot take device peripherals");
    let cp = pac::CorePeripherals::take().expect("Cannot take core peripherals");

    let rcc = dp.RCC.constrain();

    // 其实这个 Clocks 还挺有趣的，它记录了各种总线、Cortex 核心，以及 I2S 的运行频率，以及两个 APB 的分频值
    // 算是 STM32CubeMX Clock 视图的替换了
    let clocks = rcc.cfgr.use_hse(8.MHz()).freeze();

    let delayer = cp.SYST.delay(&clocks);

    let gpioa = dp.GPIOA.split();

    let rs_pin = gpioa.pa0.into_push_pull_output().erase();
    let rw_pin = gpioa.pa1.into_push_pull_output().erase();
    let en_pin = gpioa.pa2.into_push_pull_output().erase();

    let db4_pin = gpioa
        .pa3
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db5_pin = gpioa
        .pa4
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db6_pin = gpioa
        .pa5
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();
    let db7_pin = gpioa
        .pa6
        .into_open_drain_output()
        .internal_pull_up(true)
        .erase();

    let lcd_pins = LCDPins {
        rs_pin,
        rw_pin,
        en_pin,
        db_pins: [db4_pin, db5_pin, db6_pin, db7_pin],
    };

    let mut lcd = LCD {
        delayer,
        pins: lcd_pins,
    };

    lcd.delay_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit4(0b0010)),
        },
        40_000,
    );

    lcd.delay_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b0010_1000)),
        },
        40,
    );

    lcd.delay_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b0010_1000)),
        },
        40,
    );

    lcd.wait_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b0000_1111)),
        },
        10,
    );

    lcd.wait_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b0000_0001)),
        },
        10,
    );

    lcd.wait_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b0000_0110)),
        },
        10,
    );

    lcd.wait_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b1000_0000)),
        },
        10,
    );

    for data in "hello, world!".as_bytes() {
        lcd.delayer.delay_ms(250u32);
        lcd.wait_and_send(
            FullCommand {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit8(*data)),
            },
            10,
        );
    }

    lcd.delayer.delay_ms(250u32);
    lcd.wait_and_send(
        FullCommand {
            rs: RegisterSelection::Command,
            rw: ReadWrite::Write,
            data: Some(Bits::Bit8(0b1000_0000 + 0x40)),
        },
        10,
    );

    for data in "hello, LCD1602!".as_bytes() {
        lcd.delayer.delay_ms(250u32);
        lcd.wait_and_send(
            FullCommand {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit8(*data)),
            },
            10,
        );
    }

    loop {}
}

impl LCD {
    fn delay_and_send(&mut self, command: FullCommand, wait_micro_sec: u32) -> Option<u8> {
        self.delayer.delay_us(wait_micro_sec);
        self.pins.send(command)
    }

    fn wait_and_send(&mut self, command: FullCommand, poll_interval_micro_sec: u32) -> Option<u8> {
        self.wait_for_idle(poll_interval_micro_sec);
        self.pins.send(command)
    }

    fn wait_for_idle(&mut self, poll_interval_micro_sec: u32) {
        while self.check_busy() {
            self.delayer.delay_us(poll_interval_micro_sec);
        }
    }

    fn check_busy(&mut self) -> bool {
        let busy_state = self
            .pins
            .send(FullCommand {
                rs: RegisterSelection::Command,
                rw: ReadWrite::Read,
                data: None,
            })
            .unwrap();

        busy_state.checked_shr(7).unwrap() & 1 == 1
    }
}

impl LCDPins {
    fn send(&mut self, command: FullCommand) -> Option<u8> {
        self.en_pin.set_low();

        match command.rs {
            RegisterSelection::Command => self.rs_pin.set_low(),
            RegisterSelection::Data => self.rs_pin.set_high(),
        }

        match command.rw {
            ReadWrite::Write => self.rw_pin.set_low(),
            ReadWrite::Read => self.rw_pin.set_high(),
        }

        match command.rw {
            ReadWrite::Write => {
                let bits = command.data.expect("Write command but no data provide");
                match bits {
                    Bits::Bit4(raw_bits) => {
                        assert!(raw_bits <= 0b1111, "data is greater than 4 bits");
                        self.push_4_bits(raw_bits);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                    }
                    Bits::Bit8(raw_bits) => {
                        self.push_4_bits(raw_bits >> 4);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                        self.push_4_bits(raw_bits & 0b1111);
                        self.en_pin.set_high();
                        self.en_pin.set_low();
                    }
                }
                None
            }
            ReadWrite::Read => {
                self.en_pin.set_high();
                let high_4_bits = self.fetch_4_bits().checked_shl(4).unwrap();
                self.en_pin.set_low();
                self.en_pin.set_high();
                let low_4_bits = self.fetch_4_bits();
                self.en_pin.set_low();
                Some(high_4_bits + low_4_bits)
            }
        }
    }

    fn push_4_bits(&mut self, raw_bits: u8) {
        for (index, pin) in self.db_pins.iter_mut().enumerate() {
            if raw_bits.checked_shr(index as u32).unwrap() & 1 == 1 {
                pin.set_high()
            } else {
                pin.set_low()
            }
        }
    }

    fn fetch_4_bits(&mut self) -> u8 {
        let mut data: u8 = 0;
        for (index, pin) in self.db_pins.iter_mut().enumerate() {
            pin.set_high();
            let cur_pos = 1u8.checked_shl(index as u32).unwrap();
            if pin.is_high() {
                data |= cur_pos;
            } else {
                data &= !cur_pos;
            }
        }
        data
    }
}
