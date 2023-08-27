// 说明见 s01_rcc 的 build.rs
//
// 最下方有 defmt 所需的额外的连接器脚本，请注意

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out.display());

    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();

    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg=--nmagic");

    println!("cargo:rustc-link-arg=-Tlink.x");

    // 使用 defmt 所必要的额外的链接器脚本
    println!("cargo:rustc-link-arg=-Tdefmt.x");

    // 可选，将 defmt 的日志等级调整为最详尽的状态
    println!("cargo:rustc-env=DEFMT_LOG=trace");
}
