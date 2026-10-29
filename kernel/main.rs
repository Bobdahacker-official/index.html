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
pub mod paging;
pub mod firmware;
pub mod acpi;
pub mod frames;
pub mod vm;
mod shell;
mod cpu;
use core::arch::asm;
#[no_mangle]
pub extern "C" fn _start()->! {
    unsafe { asm!("cli"); }
    hardware::serial_init();
    cpu::init();
    boot::validate_revision();
    hardware::log(format_args!("AvalancheOS native x86_64 kernel\n"));
    let mut physical=memory::Physical::init();
    let heap=physical.allocate(64*1024*1024);
    unsafe{memory::ALLOCATOR.init(heap,64*1024*1024);}
    physical.enable_tracking();
    hardware::log(format_args!("RAM total={} MiB free={} MiB\n",physical.total>>20,physical.free>>20));
    let boot_information=firmware::BootInformation::capture().expect("Read UEFI boot information");
    hardware::log(format_args!("UEFI descriptors={} runtime={} KiB command={}\n",boot_information.efi_memory.len(),boot_information.runtime_bytes()/1024,boot_information.command_line.raw));
    let mut mapper=unsafe{paging::Mapper::current(&physical)}.expect("Initialize four-level paging");
    let acpi=acpi::Acpi::discover(&mut mapper,&mut physical).expect("Validate ACPI tables");
    let cpu_tables=cpu::install_descriptors(&mut mapper,&mut physical).expect("Install GDT and TSS");
    hardware::log(format_args!("CPU GDT/TSS active; kernel stack={:#x}..{:#x}; guarded IST stacks=3\n",cpu_tables.kernel_stack_bottom(),cpu_tables.kernel_stack()));
    let response=boot::FRAMEBUFFER.get().expect("No framebuffer");
    assert!(response.count>0);
    let fb=unsafe { &**response.buffers };
    assert_eq!(fb.bpp,32);
    let canvas=graphics::Canvas::new(fb);
    shell::Shell::new(canvas,&mut physical,firmware::Platform{boot:boot_information,acpi}).run()
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo)->! { hardware::log(format_args!("PANIC: {}\n",info)); loop { unsafe { asm!("cli; hlt"); } } }
