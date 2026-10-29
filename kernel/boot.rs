use core::cell::UnsafeCell;
#[repr(C)]
pub struct Request<T> { id: [u64;4], revision:u64, response: UnsafeCell<*const T> }
unsafe impl<T> Sync for Request<T> {}
impl<T> Request<T> {
    const fn new(a:u64,b:u64)->Self { Self { id:[0xc7b1dd30df4c8b88,0x0a82e883a194f07b,a,b],revision:0,response:UnsafeCell::new(core::ptr::null()) } }
    pub fn get(&self)->Option<&'static T> { unsafe { core::ptr::read_volatile(self.response.get()).as_ref() } }
}
#[used] #[link_section=".requests_start"]
static START:[u64;4]=[0xf6b8f4b39de7d1ae,0xfab91a6940fcb9cf,0x785c6ed015d3e316,0x181e920a7852b9d9];
#[used] #[link_section=".requests"]
static BASE:[u64;3]=[0xf9562b2d5c95a6c8,0x6a7b384944536bdc,3];
#[used] #[link_section=".requests"]
pub static FRAMEBUFFER:Request<FramebufferResponse>=Request::new(0x9d5827dcd881dd75,0xa3148604f6fab11b);
#[used] #[link_section=".requests"]
pub static MEMORY:Request<MemoryResponse>=Request::new(0x67cf3d9d378a806f,0xe304acdfc50c3c62);
#[used] #[link_section=".requests"]
pub static HHDM:Request<Hhdm>=Request::new(0x48dcf1cb8ad2b852,0x63984e959a98244b);
#[used] #[link_section=".requests"]
pub static RSDP:Request<RsdpResponse>=Request::new(0xc5e77b6b397e7b43,0x27637845accdcf3c);
#[used] #[link_section=".requests"]
pub static EFI_MEMORY:Request<EfiMemoryResponse>=Request::new(0x7df62a431d6872d5,0xa4fcdfb3e57306c8);
#[used] #[link_section=".requests"]
pub static EXECUTABLE:Request<ExecutableResponse>=Request::new(0xad97e90e83f1ed67,0x31eb5d1c5ff23b69);
#[used] #[link_section=".requests_end"]
static END:[u64;2]=[0xadc0e0531bb10d03,0x9572709f31764c62];
#[repr(C)] pub struct Hhdm { pub revision:u64,pub offset:u64 }
#[repr(C)] pub struct MemoryResponse { pub revision:u64,pub count:u64,pub entries:*const *const MemoryEntry }
#[repr(C)] pub struct MemoryEntry { pub base:u64,pub length:u64,pub kind:u64 }
#[repr(C)] pub struct FramebufferResponse { pub revision:u64,pub count:u64,pub buffers:*const *mut Framebuffer }
#[repr(C)] pub struct Framebuffer { pub address:*mut u8,pub width:u64,pub height:u64,pub pitch:u64,pub bpp:u16,pub model:u8,pub red_size:u8,pub red_shift:u8,pub green_size:u8,pub green_shift:u8,pub blue_size:u8,pub blue_shift:u8 }
#[repr(C)] pub struct RsdpResponse {pub revision:u64,pub address:u64}
#[repr(C)] pub struct EfiMemoryResponse {pub revision:u64,pub map:*const u8,pub size:u64,pub descriptor_size:u64,pub descriptor_version:u64}
#[repr(C)] pub struct ExecutableResponse {pub revision:u64,pub file:*const ExecutableFile}
#[repr(C)] pub struct ExecutableFile {pub revision:u64,pub address:*const u8,pub size:u64,pub path:*const u8,pub command_line:*const u8}

pub fn validate_revision() {
    let revision=unsafe{core::ptr::read_volatile(core::ptr::addr_of!(BASE).cast::<u64>().add(2))};
    assert_eq!(revision,0,"Bootloader rejected the requested protocol revision");
}
