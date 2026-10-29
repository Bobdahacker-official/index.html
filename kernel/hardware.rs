use core::{arch::{asm,x86_64::{__cpuid,__cpuid_count,_rdtsc}},fmt::{self,Write}};
pub unsafe fn outb(port:u16,value:u8){asm!("out dx, al",in("dx")port,in("al")value,options(nomem,nostack,preserves_flags));}
pub unsafe fn inb(port:u16)->u8 {let v:u8;asm!("in al, dx",in("dx")port,out("al")v,options(nomem,nostack,preserves_flags));v}
pub fn serial_init(){unsafe{outb(0x3f9,0);outb(0x3fb,0x80);outb(0x3f8,1);outb(0x3f9,0);outb(0x3fb,3);outb(0x3fa,0xc7);outb(0x3fc,0x0b);}}
struct Serial;
impl Write for Serial {fn write_str(&mut self,s:&str)->fmt::Result {for b in s.bytes(){unsafe{for _ in 0..100_000 {if inb(0x3fd)&32!=0{break;}}outb(0x3f8,b);}}Ok(())}}
pub fn log(args:fmt::Arguments){let _=Serial.write_fmt(args);}
pub fn cycles()->u64{unsafe{_rdtsc()}}
pub fn frequency()->u64{
    unsafe{
        let max=__cpuid(0).eax;
        if max>=0x15 {let v=__cpuid_count(0x15,0);if v.eax!=0&&v.ebx!=0&&v.ecx!=0{return v.ecx as u64*v.ebx as u64/v.eax as u64;}}
        if max>=0x16 {let v=__cpuid(0x16);if v.eax!=0{return v.eax as u64*1_000_000;}}
        let gate=inb(0x61);outb(0x61,gate&!3);outb(0x43,0xb0);outb(0x42,0x9c);outb(0x42,0x2e);
        let start=cycles();outb(0x61,(gate&!2)|1);
        for _ in 0..20_000_000 {if inb(0x61)&0x20!=0 {let elapsed=cycles()-start;outb(0x61,gate);return elapsed*100;}}
        outb(0x61,gate);panic!("TSC calibration timeout")
    }
}
pub fn time()->(u8,u8,u8){unsafe{
    fn bcd(x:u8)->u8{(x&15)+((x>>4)*10)}
    unsafe fn cmos(r:u8)->u8{outb(0x70,r);inb(0x71)}
    let mut last=(0,0,0);
    for _ in 0..100 {if cmos(0x0a)&0x80!=0{continue;}let now=(cmos(4),cmos(2),cmos(0));if now==last {let mode=cmos(0x0b);let pm=now.0&0x80!=0;let mut h=now.0&0x7f;let mut m=now.1;let mut s=now.2;if mode&4==0 {h=bcd(h);m=bcd(m);s=bcd(s);}if mode&2==0 {h=h%12+if pm{12}else{0};}return(h,m,s);}last=now;}
    (0,0,0)
}}
#[derive(Clone)] pub struct Pci {pub bus:u8,pub slot:u8,pub function:u8,pub vendor:u16,pub device:u16,pub class:u8,pub subclass:u8}
pub fn pci()->alloc::vec::Vec<Pci>{
    unsafe fn read(b:u8,s:u8,f:u8,off:u8)->u32{let a=0x80000000u32|(b as u32)<<16|(s as u32)<<11|(f as u32)<<8|off as u32;asm!("out dx, eax",in("dx")0xcf8u16,in("eax")a,options(nostack));let v:u32;asm!("in eax, dx",in("dx")0xcfcu16,out("eax")v,options(nostack));v}
    let mut devices=alloc::vec::Vec::new();
    for bus in 0..=255 {for slot in 0..32 {for function in 0..8 {let id=unsafe{read(bus,slot,function,0)};if id&65535==65535{continue;}let class=unsafe{read(bus,slot,function,8)};devices.push(Pci{bus,slot,function,vendor:id as u16,device:(id>>16)as u16,class:(class>>24)as u8,subclass:(class>>16)as u8});if function==0&&unsafe{read(bus,slot,0,12)}&(1<<23)==0{break;}}}}
    devices
}
pub fn reboot()->!{unsafe{for _ in 0..100_000{if inb(0x64)&2==0{outb(0x64,0xfe);break;}}}loop{core::hint::spin_loop();}}
