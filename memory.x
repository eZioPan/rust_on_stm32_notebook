/*
在编译的过程中，链接器最后会依照 cortex-m-rt crate 提供的 link.x 文件执行 elf 文件的生成
而 link.x 的脚本又会要求我们提供一个 memory.x 文件，用来表示单片机的内存映射情况。

内存映射情况一般由芯片的 datasheet 中的 Memory Map 提供
比如我们手上的 STM32F411RET6，datasheet 上说，
Flash Memory 的起始地址为 0x0800 0000，终止地址为 0x0807 FFFF，
于是 FLASH 的 ORIGIN 要设置为 0x80000000，而总闪存大小为 0x0807 FFFF - 0x0800 0000 + 0x1 = 0x80000 Byte = 512KiB
同理，
SRAM 的起始地址为 0x2000 0000，终止地址为 0x2002 0000，
于是 RAM 的 ORIGIN 要设置为 0x20000000，而总内存大小为 0x2002 0000 - 0x2000 0000 + 0x1 = 0x20000 Byte = 128KiB

关于 linker script 语法的说明，见 https://sourceware.org/binutils/docs/ld/MEMORY.html

需要注意的是，在这里，FLASH 和 RAM 这两个名字，是由 link.x 文件确定的，而非 linker script 定义的
*/
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM : ORIGIN = 0x20000000, LENGTH = 128K
}
