//! 用 STM32F411RET6 驱动一个 LCD1602
//! 使用了 LCD1602 的 4 bit 模式

//! 接线图
//!
//! 其实这个连线图还是比较随意的，除了 GND 和 V5 是固定的引脚之外，其它的 GPIO 引脚是可以随便调整的
//!
//! LCD <-> STM32
//! Vss <-> GND
//! Vdd <-> 5V
//! V0 <-> 可变电阻 <-> 5V（调节显示对比度）
//! RS <-> PA0
//! RW <-> PA1
//! EN [<-> PA2, <-> 4.7 kOhm 下拉电阻 <-> GND]
//! D4 <-> PA3
//! D5 <-> PA4
//! D6 <-> PA5
//! D7 <-> PA6
//! A <-> 可变电阻 <-> 5V（这里路的可变电阻我设计用来调节背光亮度，是可选的，而且准确来说应该用 PWM 调光，我这里就不再设计了）
//! K <-> GND

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

    // 准确来说，这三个引脚应该在外部接分别接一个小一点的上拉电阻（比如 4.7KOhm 的）
    // 不过我手上没有合适的电阻，这里就先用 pull_push 模式替代了
    let rs_pin = gpioa.pa0.into_push_pull_output().erase();
    let rw_pin = gpioa.pa1.into_push_pull_output().erase();

    // EN 引脚的问题，我还么有想好，准确来说，它应该在外部接一个下拉电阻，防止单片机重启的时候，电平跳动，导致 LCD1602 收到奇怪的信号
    // 但如果我们将这个口设置为开漏输出，则它又要求接一个上拉电阻，这和我们默认需要将其下拉的要求相冲突
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

    lcd.delay_and_send(CommandSet::HalfFunctionSet, 40_000);

    lcd.delay_and_send(
        CommandSet::FunctionSet(DataWidth::Bit4, Line::Line2, Font::Font5x8),
        40,
    );

    lcd.delay_and_send(
        CommandSet::FunctionSet(DataWidth::Bit4, Line::Line2, Font::Font5x8),
        40,
    );

    lcd.wait_and_send(
        CommandSet::DisplayOnOff {
            display: State::On,
            cursor: State::On,
            cursor_blink: State::On,
        },
        10,
    );

    lcd.wait_and_send(CommandSet::ClearDisplay, 10);

    lcd.wait_and_send(
        CommandSet::EntryModeSet(MoveDirection::Right, ShiftType::Cursor),
        10,
    );

    lcd.wait_and_send(CommandSet::SetDDRAM(0b000_0000), 10);

    for data in "hello, world!".as_bytes() {
        lcd.delayer.delay_ms(250u32);
        lcd.wait_and_send(CommandSet::WriteDataToRAM(*data), 10);
    }

    lcd.delayer.delay_ms(250u32);
    lcd.wait_and_send(CommandSet::SetDDRAM(0x40), 10);

    for data in "hello, LCD1602!".as_bytes() {
        lcd.delayer.delay_ms(250u32);
        lcd.wait_and_send(CommandSet::WriteDataToRAM(*data), 10);
    }

    loop {}
}

enum CommandSet {
    ClearDisplay,
    ReturnHome,
    EntryModeSet(MoveDirection, ShiftType),
    DisplayOnOff {
        display: State,
        cursor: State,
        cursor_blink: State,
    },
    CursorOrDisplayShift(ShiftType, MoveDirection),
    // 这个 HalfFunctionSet 比较特殊，是在初始化 LCD1602 到 4 bit 模式所特有的“半条指令”
    // 而且 ST7066U 中并没有给这半条指令取新的名字，这里是我为了规整自行确定的名称
    HalfFunctionSet,
    FunctionSet(DataWidth, Line, Font),
    SetCGRAM(u8),
    SetDDRAM(u8),
    ReadBusyFlagAndAddress,
    WriteDataToRAM(u8),
    ReadDataFromRAM,
}

enum MoveDirection {
    Left,
    Right,
}

enum ShiftType {
    Cursor,
    Screen,
}

enum State {
    Off,
    On,
}

enum DataWidth {
    Bit4,
    Bit8,
}

enum Line {
    Line1,
    Line2,
}

enum Font {
    Font5x8,
    Font5x11,
}

impl From<CommandSet> for FullCommand {
    fn from(command: CommandSet) -> Self {
        match command {
            CommandSet::ClearDisplay => {
                let raw_bits: u8 = 0b0000_0001;
                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::ReturnHome => {
                let raw_bits: u8 = 0b0000_010;
                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::EntryModeSet(dir, st) => {
                let mut raw_bits: u8 = 0b0000_0100;

                match dir {
                    MoveDirection::Left => clear_bit(&mut raw_bits, 1),
                    MoveDirection::Right => set_bit(&mut raw_bits, 1),
                }

                match st {
                    ShiftType::Cursor => clear_bit(&mut raw_bits, 0),
                    ShiftType::Screen => set_bit(&mut raw_bits, 0),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::DisplayOnOff {
                display,
                cursor,
                cursor_blink,
            } => {
                let mut raw_bits = 0b0000_1000;

                match display {
                    State::Off => clear_bit(&mut raw_bits, 2),
                    State::On => set_bit(&mut raw_bits, 2),
                }
                match cursor {
                    State::Off => clear_bit(&mut raw_bits, 1),
                    State::On => set_bit(&mut raw_bits, 1),
                }
                match cursor_blink {
                    State::Off => clear_bit(&mut raw_bits, 0),
                    State::On => set_bit(&mut raw_bits, 0),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::CursorOrDisplayShift(st, dir) => {
                let mut raw_bits = 0b0001_0000;

                match st {
                    ShiftType::Cursor => clear_bit(&mut raw_bits, 3),
                    ShiftType::Screen => set_bit(&mut raw_bits, 3),
                }

                match dir {
                    MoveDirection::Left => clear_bit(&mut raw_bits, 2),
                    MoveDirection::Right => set_bit(&mut raw_bits, 2),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::HalfFunctionSet => Self {
                rs: RegisterSelection::Command,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit4(0b0010)),
            },

            CommandSet::FunctionSet(width, line, font) => {
                let mut raw_bits = 0b0010_0000;

                match width {
                    DataWidth::Bit4 => clear_bit(&mut raw_bits, 4),
                    DataWidth::Bit8 => set_bit(&mut raw_bits, 4),
                }

                match line {
                    Line::Line1 => clear_bit(&mut raw_bits, 3),
                    Line::Line2 => set_bit(&mut raw_bits, 3),
                }

                match font {
                    Font::Font5x8 => clear_bit(&mut raw_bits, 2),
                    Font::Font5x11 => set_bit(&mut raw_bits, 2),
                }

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::SetCGRAM(addr) => {
                let mut raw_bits = 0b0100_0000;

                if addr > 0b0011_1111 {
                    panic!("CGRAM address out of range")
                }

                raw_bits += addr;

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::SetDDRAM(addr) => {
                let mut raw_bits = 0b1000_0000;

                if addr > 0b0111_1111 {
                    panic!("DDRAM address out of range")
                }

                raw_bits += addr;

                Self {
                    rs: RegisterSelection::Command,
                    rw: ReadWrite::Write,
                    data: Some(Bits::Bit8(raw_bits)),
                }
            }

            CommandSet::ReadBusyFlagAndAddress => Self {
                rs: RegisterSelection::Command,
                rw: ReadWrite::Read,
                data: None,
            },

            CommandSet::WriteDataToRAM(data) => Self {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Write,
                data: Some(Bits::Bit8(data)),
            },

            CommandSet::ReadDataFromRAM => Self {
                rs: RegisterSelection::Data,
                rw: ReadWrite::Read,
                data: None,
            },
        }
    }
}

struct LCD {
    pins: LCDPins,
    delayer: SysDelay,
}

struct FullCommand {
    rs: RegisterSelection,
    rw: ReadWrite,
    data: Option<Bits>, // if it's a read command, then data should be filled by reading process
}

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

struct LCDPins {
    rs_pin: ErasedPin<Output>,
    rw_pin: ErasedPin<Output>,
    en_pin: ErasedPin<Output>,
    db_pins: [ErasedPin<Output<OpenDrain>>; 4],
}

impl LCD {
    fn delay_and_send<IFC: Into<FullCommand>>(
        &mut self,
        command: IFC,
        wait_micro_sec: u32,
    ) -> Option<u8> {
        self.delayer.delay_us(wait_micro_sec);
        self.pins.send(command.into())
    }

    fn wait_and_send<IFC: Into<FullCommand>>(
        &mut self,
        command: IFC,
        poll_interval_micro_sec: u32,
    ) -> Option<u8> {
        self.wait_for_idle(poll_interval_micro_sec);
        self.pins.send(command.into())
    }

    fn wait_for_idle(&mut self, poll_interval_micro_sec: u32) {
        while self.check_busy() {
            self.delayer.delay_us(poll_interval_micro_sec);
        }
    }

    fn check_busy(&mut self) -> bool {
        let busy_state = self.pins.send(CommandSet::ReadBusyFlagAndAddress).unwrap();

        busy_state.checked_shr(7).unwrap() & 1 == 1
    }
}

impl LCDPins {
    fn send<IFC: Into<FullCommand>>(&mut self, command: IFC) -> Option<u8> {
        self.en_pin.set_low();

        let command = command.into();

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

fn set_bit(data: &mut u8, pos: u8) {
    if pos > 7 {
        panic!("pos larger than 7");
    }
    *data |= 1 << pos;
}

fn clear_bit(data: &mut u8, pos: u8) {
    if pos > 7 {
        panic!("pos larger than 7");
    }
    *data &= !(1 << pos);
}
