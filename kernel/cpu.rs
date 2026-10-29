use core::arch::{asm,global_asm};
global_asm!(r#".section .text
fault_common:
mov rdi, [rsp]
mov rsi, [rsp + 8]
mov rdx, [rsp + 16]
and rsp, -16
call avalanche_fault
2: cli
hlt
jmp 2b
.global fault_0
fault_0:
push 0
push 0
jmp fault_common
.global fault_1
fault_1:
push 0
push 1
jmp fault_common
.global fault_2
fault_2:
push 0
push 2
jmp fault_common
.global fault_3
fault_3:
push 0
push 3
jmp fault_common
.global fault_4
fault_4:
push 0
push 4
jmp fault_common
.global fault_5
fault_5:
push 0
push 5
jmp fault_common
.global fault_6
fault_6:
push 0
push 6
jmp fault_common
.global fault_7
fault_7:
push 0
push 7
jmp fault_common
.global fault_8
fault_8:
push 8
jmp fault_common
.global fault_9
fault_9:
push 0
push 9
jmp fault_common
.global fault_10
fault_10:
push 10
jmp fault_common
.global fault_11
fault_11:
push 11
jmp fault_common
.global fault_12
fault_12:
push 12
jmp fault_common
.global fault_13
fault_13:
push 13
jmp fault_common
.global fault_14
fault_14:
push 14
jmp fault_common
.global fault_15
fault_15:
push 0
push 15
jmp fault_common
.global fault_16
fault_16:
push 0
push 16
jmp fault_common
.global fault_17
fault_17:
push 17
jmp fault_common
.global fault_18
fault_18:
push 0
push 18
jmp fault_common
.global fault_19
fault_19:
push 0
push 19
jmp fault_common
.global fault_20
fault_20:
push 0
push 20
jmp fault_common
.global fault_21
fault_21:
push 21
jmp fault_common
.global fault_22
fault_22:
push 0
push 22
jmp fault_common
.global fault_23
fault_23:
push 0
push 23
jmp fault_common
.global fault_24
fault_24:
push 0
push 24
jmp fault_common
.global fault_25
fault_25:
push 0
push 25
jmp fault_common
.global fault_26
fault_26:
push 0
push 26
jmp fault_common
.global fault_27
fault_27:
push 0
push 27
jmp fault_common
.global fault_28
fault_28:
push 0
push 28
jmp fault_common
.global fault_29
fault_29:
push 29
jmp fault_common
.global fault_30
fault_30:
push 30
jmp fault_common
.global fault_31
fault_31:
push 0
push 31
jmp fault_common
.global fault_default
fault_default:
push 0
push 255
jmp fault_common"#);
#[repr(C,packed)]#[derive(Clone,Copy)]struct Gate{low:u16,selector:u16,ist:u8,flags:u8,middle:u16,high:u32,reserved:u32}
#[repr(C,packed)]struct Descriptor{limit:u16,base:u64}
static mut IDT:[Gate;256]=[Gate{low:0,selector:0,ist:0,flags:0,middle:0,high:0,reserved:0};256];
extern "C" {fn fault_0();fn fault_1();fn fault_2();fn fault_3();fn fault_4();fn fault_5();fn fault_6();fn fault_7();fn fault_8();fn fault_9();fn fault_10();fn fault_11();fn fault_12();fn fault_13();fn fault_14();fn fault_15();fn fault_16();fn fault_17();fn fault_18();fn fault_19();fn fault_20();fn fault_21();fn fault_22();fn fault_23();fn fault_24();fn fault_25();fn fault_26();fn fault_27();fn fault_28();fn fault_29();fn fault_30();fn fault_31();fn fault_default();}
pub fn init(){unsafe{
    let selector:u16;asm!("mov ax, cs",out("ax")selector,options(nostack,nomem));
    let handlers=[fault_0 as *const () as u64,fault_1 as *const () as u64,fault_2 as *const () as u64,fault_3 as *const () as u64,fault_4 as *const () as u64,fault_5 as *const () as u64,fault_6 as *const () as u64,fault_7 as *const () as u64,fault_8 as *const () as u64,fault_9 as *const () as u64,fault_10 as *const () as u64,fault_11 as *const () as u64,fault_12 as *const () as u64,fault_13 as *const () as u64,fault_14 as *const () as u64,fault_15 as *const () as u64,fault_16 as *const () as u64,fault_17 as *const () as u64,fault_18 as *const () as u64,fault_19 as *const () as u64,fault_20 as *const () as u64,fault_21 as *const () as u64,fault_22 as *const () as u64,fault_23 as *const () as u64,fault_24 as *const () as u64,fault_25 as *const () as u64,fault_26 as *const () as u64,fault_27 as *const () as u64,fault_28 as *const () as u64,fault_29 as *const () as u64,fault_30 as *const () as u64,fault_31 as *const () as u64];
    let table=core::ptr::addr_of_mut!(IDT)as *mut Gate;
    for i in 0..256{let addr=if i<32{handlers[i]}else{fault_default as *const ()as u64};table.add(i).write(Gate{low:addr as u16,selector,ist:0,flags:0x8e,middle:(addr>>16)as u16,high:(addr>>32)as u32,reserved:0});}
    let descriptor=Descriptor{limit:4095,base:table as u64};asm!("lidt [{}]",in(reg)&descriptor,options(readonly,nostack));
}}
#[no_mangle]pub extern "C" fn avalanche_fault(vector:u64,error:u64,rip:u64)->!{
    let cr2:u64;unsafe{asm!("mov {}, cr2",out(reg)cr2,options(nostack,nomem));}
    crate::hardware::log(format_args!("CPU FAULT vector={} error={:#x} rip={:#x} cr2={:#x}\n",vector,error,rip,cr2));
    loop{unsafe{asm!("cli; hlt");}}
}
