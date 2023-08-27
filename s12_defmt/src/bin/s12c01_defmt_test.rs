//! defmt
//!
//! defmt 作为一个为嵌入式设计的日志 crate，还是比较简单易用的。
//!
//! 就是在使用的时候有一些注意事项
//!
//! 1. 首先要注意的是 defmt 的设计理念
//!    defmt 在设计上来说，是一个“Host 端”日志系统，也就是说，日志的内容，实际上是在 Host 上“生成”的
//!    defmt crate 在 MCU 上运行的时候，只是会输出很少的一些 bit，
//!    这些 bit 需要被 Host 上运行的某个特定的程序（比如 defmt-print）读取，并比对主程序的 ELF 文件，才能打印出正确的 log 信息
//!    这点与 rtt-target 非常不同，rtt-target 是在 MCU 上生成文本的，所以只需要使用 telnet 读取就可以了，
//!    而 defmt 返回的内容更像是一个“索引”，它必须由 Host 端的特定程序比对 ELF 文件，才能打印出人类可读的日志信息
//!
//! 2. defmt 自己并不负责日志传输的功能，或者说，defmt 支持多种多样的日志传输方式，比如
//!    defmt-rtt crate 提供了经由 RTT 转发的日志，
//!    defmt-uart crate 提供了经由 串口 转发的日志，
//!    defmt-semihosting crate 提供了经由 半主机 转发的日志
//!    其中 defmt-rtt 是我所使用的传输方式（这样我就可以复用为 rtt-target 设计的基础构架）
//!
//! 3. 在库的使用上，除了我们上面提到的 defmt 和 defmt-rtt，我们还需要额外添加一个库 panic-probe
//!    并启用特性 print-defmt，这样 panic 信息就会传递到 defmt 中，并通过 defmt-rtt 传递到 Host 上
//!
//! 4. 在主程序的源码中，我们要使用 use defmt_rtt as _; use panic_probe as _; 将两个 crate 定义的相关函数直接引入到主程序中
//!    在需要打印日志信息的地方，使用 defmt::宏 的形式打印 log
//!
//! 5. defmt 有自己特定的链接器脚本，需要添加至链接器参数中
//!    由于我们使用的是 build.rs，因此我们需要添加下面这些内容
//!    ```rust
//!    // 使用 defmt 所必要的额外的链接器脚本
//!    println!("cargo:rustc-link-arg=-Tdefmt.x");
//!
//!    // 可选，将 defmt 的日志等级调整为最详尽的状态
//!    println!("cargo:rustc-env=DEFMT_LOG=trace");
//!    ```
//!    这两行需要添加在用于包含 cortex-m-rt 的链接器脚本 link.x 的命令之后
//!
//! 6. 最后是，在程序运行的过程中，我们需要做一些设置，才能正常读取到日志
//!    这里我们选用的方案，依旧是 OpenOCD 负责 RTT 通信，并架起 RTT Server
//!    不过就不能直接使用 telnet 打印收到的字节了，这里我们需要安装另一个软件
//!    cargo install defmt-print
//!    让 defmt-print 来解析 Host 捕获到的 RTT 信息
//!    `defmt-print -e <ELF 文件路径> tcp --port 8888` 来打印日志
//!
//! 7. 其它的注意事项，
//!    第一个是，由于 defmt-print 是需要读取正确的 ELF 文件，才能正确的显示日志信息
//!    因此每次重新编译，并上传固件之后，我们都需要手动重启 defmt-print 程序，以让其获取正确的 ELF 文件
//!    第二个是，如果想要完全展示 defmt-rtt 发过来的（有效和无效的）信息，可以尝试为 defmt-print 添加参数 `--show-skipped-frames`
//!
//! 最后要注意的是，我这里并没有深究 defmt 的扩展用法，以及使用 probe-rs、probe-rs-cli 以及 vscode 的 probe-rs 插件来实现更高效的 debug 流程
//! 这里仅做了一个最基础的使用流程记录

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

#[allow(unused_imports, clippy::single_component_path_imports)]
use stm32f4;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::println!("Hello world!");
    defmt::trace!("trace");
    defmt::debug!("debug");
    defmt::info!("info");
    defmt::warn!("warn");
    defmt::error!("error");
    // defmt 有自己的一套 assert! / panic! / todo! / unreechable! 宏
    // 这里我们就不逐一演示了
    defmt::panic!("panicking!");
    //loop {}
}
