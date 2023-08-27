// 编译当前 crate 前，需要预先执行的操作
//
// 本文件修改自 https://github.com/rust-embedded/cortex-m-quickstart 仓库，可搭配 cortex-m-rt crate 一同使用的
//
// 部分场景下，需要额外添加一些选项，特殊部分在其 build.rs 中会有额外的注解
//
// 这里要指出的是，由于 build.rs 是在编译机器上运行的，因此 build.rs 本身被编译后的二进制文件，一般都直接放在 ./target/debug/build/<crate-name-with-id> 目录下
// 在运行了这个二进制文件后，它输出的所有文件一般都放在 ./taget/<target-platform-triple>/debug/build/<crate-name-with-id> 目录下
// 之后才是 cargo 取读取这个目录下的各种文件，来组成编译时所需的环境（各种有用的环境变量）
//
// 而 cargo 实际上会分三个步骤，来确定 build.rs 是否要重新编译和运行
//
// 1. 比较 build.rs 和编译 build.rs 本身生成的二进制文件的 mtime，来确定 build.rs 需不需要重新编译
// 2. 比较 build.rs 关注的文件（比如在 rerun-if-changed 指定的，以及 build.rs 自身）和 <crate-name-with-id>/invoked.timestamp 的 mtime，来确定二进制文件是否要再次运行
// 3. 比较最终生成的 ELF 文件和 <crate-name-with-id>/invoked.timestamp 的 mtime，来确定是否要重新编译 ELF（当然，除了 build.rs，其它源码的改变自然是要触发再次编译的）
//
// 由此可知，我们其实可以查看 ./taget/<target-platform-triple>/debug/build/<crate-name-with-id> 目录下的文件，来确定 cargo 到底都识别了哪些参数和文件
// 【不建议这样操作】我们也可以直接修改这些文件的内容，来影响最终生成的 ELF 文件

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // 首先是，在运行时，取得环境 OUT_DIR 中存储的路径
    // 依照 https://doc.rust-lang.org/cargo/reference/environment-variables.html
    // 的说法，该路径为当前这个 build script 存放其生成的文件的目录
    // 一般为 ./taget/<target-platform-triple>/debug/build/<crate-name-with-id>/out
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    // 告知编译器，让连接器的搜索目录包含 OUT_DIR
    println!("cargo:rustc-link-search={}", out.display());
    // 然后我们要从 build.rs 所在的目录下，将 memory.x 的内容拷贝到 OUT_DIR 指向的路径下
    // 这里的实现方法其实是，在编译 build.rs 时，将 memory.x 的内容注入到 build script 自身的二进制文件中
    // 然后在 build script 运行时，将其中包含的数据写入到 OUT_DIR 下的 memeory.x 文件中
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();

    // 默认情况下，build script 会在所在的 crate 中有任何文件修改时，重新编译并执行
    // 不过就目前来说，这个 build script 需要监控的外部文件只有 memory.x
    // 当其它文件修改时，build script 本身是不用再次运行的
    // 这里我们通过 rerun-if-change 命令，告知编译器，
    // 仅在 memory.x 修改后，才重新编译和运行 build script
    println!("cargo:rerun-if-changed=memory.x");

    // 若段地址没有对齐 0x10000（不是 0x10000 的倍数），那么默认情况下，连接器会强制将段的位置对齐 0x10000
    // 这里我们要向连接器传递 nmagic 参数，告知连接器不要强制对齐段的位置，按照我们指定的来即可
    // 见 https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    //
    // 不过就我们当前 memory.x 中简单的段分配来说，这个设置其实没有什么意义
    // println!("cargo:rustc-link-arg=--nmagic");

    // 告诉连接器，在编译当前的 crate 的时候，使用这里指定的编译脚本
    // 也就是 cortex-m-rt crate 提供的 link.x
    println!("cargo:rustc-link-arg=-Tlink.x");
}
