// 将 println! 包裹了一下，节省了一点重复的格式化代码

macro_rules! master_rprintln {
    ($s:literal)=>{
        rtt_target::rprintln!(concat!("\x1b[91mMaster:\t", $s ,"\x1b[0m"));
    };
    ($s:literal, $($arg:tt)*) =>{
        rtt_target::rprintln!(concat!("\x1b[91mMaster:\t", $s ,"\x1b[0m"), $($arg)*);
    };
}

macro_rules! slave_rprintln {
    ($s:literal)=>{
        rtt_target::rprintln!(concat!("\x1b[92mSlave:\t", $s ,"\x1b[0m"));
    };
    ($s:literal, $($arg:tt)*) =>{
        rtt_target::rprintln!(concat!("\x1b[92mSlave:\t", $s ,"\x1b[0m"), $($arg)*);
    };
}

// 为了让 macro 属于某个层级，使用我看不懂的什么奇淫巧计……
// https://users.rust-lang.org/t/how-to-namespace-a-macro-rules-macro-within-a-module-or-macro-export-it-without-polluting-the-top-level-namespace/63779/4
pub(crate) use master_rprintln;
pub(crate) use slave_rprintln;
