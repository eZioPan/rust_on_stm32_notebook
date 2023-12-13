/*
在编译的过程中，链接器最后会依照 cortex-m-rt crate 提供的 link.x 文件执行 elf 文件的生成
而 link.x 的脚本又会要求我们提供一个 memory.x 文件，用来表示单片机的内存映射情况。

内存映射情况一般由芯片的 datasheet 中的 Memory Map 提供
比如我们手上的 STM32F413VGT6，datasheet 上说，
Flash Memory 的起始地址为 0x0800 0000，终止地址为 0x0807 FFFF，
于是 FLASH 的 ORIGIN 要设置为 0x80000000，而总闪存大小为 0x0807 FFFF - 0x0800 0000 + 0x1 = 0x80000 Byte = 512KiB
同理，
SRAM 的起始地址为 0x2000 0000，终止地址为 0x2004 FFFF，
于是 RAM 的 ORIGIN 要设置为 0x2000 0000，而总内存大小为 0x2004 FFFF - 0x2000 0000 + 0x1 = 0x50000 Byte = 320KiB

NOTE:
RAM 的 LENGHT 可以设置的比实际 SRAM 要小，但不可以比实际的 SRAM 要大，这是因为
栈空间是从地址的高位向地址的低位“生长”的，如果我们设置的 LENGTH 大于 SRAM 的容量，那么在我们的代码创建栈顶的时候，
就会直接写到一个不可以被访问的地址上去，会立刻产生 HardFault 硬错误

关于 linker script 语法的说明，见 https://sourceware.org/binutils/docs/ld/MEMORY.html

需要注意的是，在这里，FLASH 和 RAM 这两个名字，是由 link.x 文件确定的，而非 linker script 定义的
*/
/*
特别的，Flash 是以 Sector 进行设计的，见 Reference Manual 的 Flash module organization 表
*/
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM : ORIGIN = 0x20000000, LENGTH = 320K
}
