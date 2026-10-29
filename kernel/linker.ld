OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)
PHDRS { requests PT_LOAD; text PT_LOAD; rodata PT_LOAD; data PT_LOAD; }
SECTIONS {
    . = 0xffffffff80000000;
    .requests : { KEEP(*(.requests_start)) KEEP(*(.requests)) KEEP(*(.requests_end)) } :requests
    . = ALIGN(4096);
    .text : { *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : { *(.rodata .rodata.*) } :rodata
    . = ALIGN(4096);
    .data : { *(.data .data.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.eh_frame*) *(.note*) }
}
