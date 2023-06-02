#[derive(Clone, Copy)]
pub(super) enum CommandSet {
    ClearDisplay,
    ReturnHome,
    EntryModeSet(MoveDirection, ShiftType),
    DisplayOnOff {
        display: State,
        cursor: State,
        cursor_blink: State,
    },
    #[allow(dead_code)]
    CursorOrDisplayShift(ShiftType, MoveDirection),
    // 这个 HalfFunctionSet 比较特殊，是在初始化 LCD1602 到 4 bit 模式所特有的“半条指令”
    // 而且 ST7066U 中并没有给这半条指令取新的名字，这里是我为了规整自行确定的名称
    HalfFunctionSet,
    FunctionSet(DataWidth, LineMode, Font),
    SetCGRAM(u8),
    SetDDRAM(u8),
    ReadBusyFlagAndAddress,
    // 这条指令没有唯一对应的函数，它分布在对 DDRAM 和 CGRAM 写入数据的函数中
    WriteDataToRAM(u8),
    #[allow(dead_code)]
    ReadDataFromRAM,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum MoveDirection {
    RightToLeft,
    #[default]
    LeftToRight,
}

#[derive(Clone, Copy, Default)]
pub enum ShiftType {
    #[default]
    CursorOnly,
    CursorAndDisplay,
}

#[derive(Clone, Copy, Default)]
pub enum State {
    Off,
    #[default]
    On,
}

#[derive(Clone, Copy, Default)]
pub enum DataWidth {
    #[default]
    Bit4,
    #[allow(dead_code)]
    Bit8,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum LineMode {
    OneLine,
    #[default]
    TwoLine,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Font {
    #[default]
    Font5x8,
    Font5x11,
}
