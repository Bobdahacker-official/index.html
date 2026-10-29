#![no_std]
#![no_main]
mod boot;
extern crate alloc;
pub mod memory;
pub mod hardware;
pub mod graphics;
pub mod input;
pub mod apps;
pub mod storage;
mod shell;
mod cpu;
use core::arch::asm;
#[no_mangle]
pub extern "C" fn _start()->! {
    unsafe { asm!("cli"); }
    hardware::serial_init();
    cpu::init();
    hardware::log(format_args!("AvalancheOS native x86_64 kernel\n"));
    let mut physical=memory::Physical::init();
    let heap=physical.allocate(64*1024*1024);
    unsafe{memory::ALLOCATOR.init(heap,64*1024*1024);}
    hardware::log(format_args!("RAM total={} MiB free={} MiB\n",physical.total>>20,physical.free>>20));
    let response=boot::FRAMEBUFFER.get().expect("No framebuffer");
    assert!(response.count>0);
    let fb=unsafe { &**response.buffers };
    assert_eq!(fb.bpp,32);
    let canvas=graphics::Canvas::new(fb);
    shell::Shell::new(canvas,&mut physical).run()
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo)->! { hardware::log(format_args!("PANIC: {}\n",info)); loop { unsafe { asm!("cli; hlt"); } } }
