use core::{arch::{asm,x86_64::{__cpuid,__cpuid_count}},ptr::{read_volatile,write_volatile}};
use crate::memory::Physical;

const ADDRESS:u64=0x000f_ffff_ffff_f000;
const PRESENT:u64=1;
const WRITE:u64=2;
const USER:u64=4;
const HUGE:u64=128;
const NX:u64=1<<63;
const MAPPING_BASE:u64=0xffff_c000_0000_0000;
const MAPPING_END:u64=0xffff_d000_0000_0000;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error {AddressOverflow,Unaligned,OutOfFrames,AlreadyMapped,NotMapped,HugePageConflict,FiveLevelPaging,NoExecuteUnavailable,ActiveAddressSpace,KernelAddressSpace,FrameOwnership}
#[derive(Clone,Copy)]
pub struct Permissions {pub writable:bool,pub executable:bool,pub user:bool,pub uncached:bool}
impl Permissions {
    pub const FIRMWARE:Self=Self{writable:false,executable:false,user:false,uncached:false};
    pub const MMIO:Self=Self{writable:true,executable:false,user:false,uncached:true};
    fn bits(self)->u64 {PRESENT|if self.writable{WRITE}else{0}|if self.executable{0}else{NX}|if self.user{USER}else{0}|if self.uncached{24}else{0}}
}
pub struct Mapper {root:u64,direct:u64,next:u64,pub mapped_pages:u64,owned:bool}
impl Mapper {
    /// Owns mutation of the current CPU's page tables while interrupts are disabled.
    pub unsafe fn current(memory:&Physical)->Result<Self,Error>{
        let cr4:u64;asm!("mov {}, cr4",out(reg)cr4,options(nomem,nostack));
        if cr4&(1<<12)!=0{return Err(Error::FiveLevelPaging);}
        if __cpuid(0x80000000).eax<0x80000001||__cpuid(0x80000001).edx&(1<<20)==0{return Err(Error::NoExecuteUnavailable);}
        let(lo,hi):(u32,u32);asm!("rdmsr",in("ecx")0xc0000080u32,out("eax")lo,out("edx")hi,options(nostack));
        asm!("wrmsr",in("ecx")0xc0000080u32,in("eax")(lo|(1<<11)),in("edx")hi,options(nostack));
        let mut cr0:u64;asm!("mov {}, cr0",out(reg)cr0,options(nomem,nostack));cr0|=1<<16;asm!("mov cr0, {}",in(reg)cr0,options(nostack));
        if __cpuid(0).eax>=7{let features=__cpuid_count(7,0).ebx;let mut control=cr4;
            if features&(1<<7)!=0{control|=1<<20;}
            if features&(1<<20)!=0{control|=1<<21;}
            asm!("mov cr4, {}",in(reg)control,options(nostack));
            if features&(1<<20)!=0{asm!("clac",options(nostack));}
        }
        let root:u64;asm!("mov {}, cr3",out(reg)root,options(nomem,nostack));
        Ok(Self{root:root&ADDRESS,direct:memory.direct,next:MAPPING_BASE+4096,mapped_pages:0,owned:false})
    }
    pub fn user(memory:&mut Physical,kernel:&Self)->Result<Self,Error>{
        let frame=memory.try_allocate(4096).ok_or(Error::OutOfFrames)?;
        unsafe{core::ptr::write_bytes(frame,0,4096);core::ptr::copy_nonoverlapping(((kernel.root+kernel.direct)as *const u64).add(256),(frame as *mut u64).add(256),256);}
        Ok(Self{root:frame as u64-memory.direct,direct:memory.direct,next:MAPPING_BASE,mapped_pages:0,owned:true})
    }
    pub fn root(&self)->u64{self.root}
    /// The executing stack, code, and interrupt machinery must reside in the
    /// shared kernel half, and the caller serializes CPU address-space ownership.
    pub unsafe fn activate(&self){asm!("mov cr3, {}",in(reg)self.root,options(nostack));}
    /// All user leaves must have been unmapped and their frame references released.
    pub unsafe fn destroy_user(self,memory:&mut Physical)->Result<(),Error>{
        if !self.owned{return Err(Error::KernelAddressSpace);}let current:u64;asm!("mov {}, cr3",out(reg)current,options(nomem,nostack));if current&ADDRESS==self.root{return Err(Error::ActiveAddressSpace);}
        unsafe fn verify(mapper:&Mapper,table:u64,level:u8)->Result<(),Error>{
            for index in 0..512{let entry=read_volatile(mapper.slot(table,index));if entry&PRESENT==0{continue;}if level==1{return Err(Error::AlreadyMapped);}if entry&HUGE!=0{return Err(Error::HugePageConflict);}verify(mapper,entry&ADDRESS,level-1)?;}Ok(())
        }
        unsafe fn release(mapper:&Mapper,memory:&mut Physical,table:u64,level:u8)->Result<(),Error>{
            if level>1{for index in 0..512{let entry=read_volatile(mapper.slot(table,index));if entry&PRESENT!=0{release(mapper,memory,entry&ADDRESS,level-1)?;}}}
            memory.release_frame(table).map_err(|_|Error::FrameOwnership)?;Ok(())
        }
        for index in 0..256{let entry=read_volatile(self.slot(self.root,index));if entry&PRESENT!=0{verify(&self,entry&ADDRESS,3)?;}}
        for index in 0..256{let entry=read_volatile(self.slot(self.root,index));if entry&PRESENT!=0{release(&self,memory,entry&ADDRESS,3)?;}}
        memory.release_frame(self.root).map_err(|_|Error::FrameOwnership)?;Ok(())
    }
    fn canonical(address:u64)->bool{address<=0x0000_7fff_ffff_ffff||address>=0xffff_8000_0000_0000}
    unsafe fn slot(&self,table:u64,index:usize)->*mut u64{((table+self.direct)as *mut u64).add(index)}
    fn indexes(address:u64)->[usize;4]{[((address>>39)&511)as usize,((address>>30)&511)as usize,((address>>21)&511)as usize,((address>>12)&511)as usize]}
    unsafe fn leaf(&self,address:u64)->Result<*mut u64,Error>{
        if !Self::canonical(address){return Err(Error::AddressOverflow);}
        let indexes=Self::indexes(address);let mut table=self.root;
        for &index in &indexes[..3]{let entry=read_volatile(self.slot(table,index));if entry&PRESENT==0{return Err(Error::NotMapped);}if entry&HUGE!=0{return Err(Error::HugePageConflict);}table=entry&ADDRESS;}
        Ok(self.slot(table,indexes[3]))
    }
    /// The caller supplies exclusive access to page tables and frames and handles
    /// remote TLB invalidation before exposing a changed mapping on another CPU.
    pub unsafe fn map_page(&mut self,memory:&mut Physical,virtual_address:u64,physical_address:u64,permissions:Permissions)->Result<(),Error>{
        if virtual_address&4095!=0||physical_address&4095!=0{return Err(Error::Unaligned);}
        if !Self::canonical(virtual_address)||physical_address&!ADDRESS!=0{return Err(Error::AddressOverflow);}
        let indexes=Self::indexes(virtual_address);let mut table=self.root;
        let mut created=[(core::ptr::null_mut::<u64>(),0u64);3];let mut count=0;
        let result=(||->Result<(),Error>{
            for &index in &indexes[..3]{
                let slot=self.slot(table,index);let mut entry=read_volatile(slot);
                if entry&PRESENT==0{
                    let frame=memory.try_allocate(4096).ok_or(Error::OutOfFrames)?;core::ptr::write_bytes(frame,0,4096);
                    let physical=frame as u64-self.direct;entry=physical|PRESENT|WRITE|if permissions.user{USER}else{0};
                    created[count]=(slot,physical);count+=1;write_volatile(slot,entry);
                }else{if entry&HUGE!=0{return Err(Error::HugePageConflict);}if permissions.user&&entry&USER==0{return Err(Error::AlreadyMapped);}}
                table=entry&ADDRESS;
            }
            let slot=self.slot(table,indexes[3]);if read_volatile(slot)&PRESENT!=0{return Err(Error::AlreadyMapped);}
            write_volatile(slot,physical_address|permissions.bits());asm!("invlpg [{}]",in(reg)virtual_address,options(nostack));Ok(())
        })();
        if let Err(error)=result{
            for &(slot,physical)in created[..count].iter().rev(){write_volatile(slot,0);memory.release_frame(physical).map_err(|_|Error::FrameOwnership)?;}
            return Err(error);
        }
        self.mapped_pages+=1;Ok(())
    }
    pub unsafe fn unmap_page(&mut self,virtual_address:u64)->Result<u64,Error>{
        if virtual_address&4095!=0{return Err(Error::Unaligned);}let slot=self.leaf(virtual_address)?;let entry=read_volatile(slot);if entry&PRESENT==0{return Err(Error::NotMapped);}
        write_volatile(slot,0);asm!("invlpg [{}]",in(reg)virtual_address,options(nostack));self.mapped_pages=self.mapped_pages.saturating_sub(1);Ok(entry&ADDRESS)
    }
    pub unsafe fn protect_page(&mut self,virtual_address:u64,permissions:Permissions)->Result<(),Error>{
        if virtual_address&4095!=0{return Err(Error::Unaligned);}let slot=self.leaf(virtual_address)?;let entry=read_volatile(slot);if entry&PRESENT==0{return Err(Error::NotMapped);}
        if permissions.user {let mut table=self.root;for &i in &Self::indexes(virtual_address)[..3]{let e=read_volatile(self.slot(table,i));if e&USER==0{return Err(Error::AlreadyMapped);}table=e&ADDRESS;}}
        write_volatile(slot,(entry&ADDRESS)|permissions.bits());asm!("invlpg [{}]",in(reg)virtual_address,options(nostack));Ok(())
    }
    pub unsafe fn replace_page(&mut self,virtual_address:u64,physical_address:u64,permissions:Permissions)->Result<u64,Error>{
        if virtual_address&4095!=0||physical_address&4095!=0{return Err(Error::Unaligned);}if physical_address&!ADDRESS!=0{return Err(Error::AddressOverflow);}
        let slot=self.leaf(virtual_address)?;let entry=read_volatile(slot);if entry&PRESENT==0{return Err(Error::NotMapped);}
        if permissions.user&&entry&USER==0{return Err(Error::AlreadyMapped);}
        write_volatile(slot,physical_address|permissions.bits());asm!("invlpg [{}]",in(reg)virtual_address,options(nostack));Ok(entry&ADDRESS)
    }
    pub fn translate(&self,virtual_address:u64)->Option<u64>{
        if !Self::canonical(virtual_address){return None;}let mut table=self.root;
        for(level,&index)in Self::indexes(virtual_address).iter().enumerate(){let entry=unsafe{read_volatile(self.slot(table,index))};if entry&PRESENT==0{return None;}
            if level==1&&entry&HUGE!=0{return Some((entry&0x000f_ffff_c000_0000)|(virtual_address&0x3fff_ffff));}
            if level==2&&entry&HUGE!=0{return Some((entry&0x000f_ffff_ffe0_0000)|(virtual_address&0x1f_ffff));}
            table=entry&ADDRESS;
        }Some(table|(virtual_address&4095))
    }
    /// Maps a physical range into the kernel's reserved mapping window. Each range
    /// retains an unmapped guard page before the following allocation.
    pub unsafe fn map_physical(&mut self,memory:&mut Physical,address:u64,length:usize,permissions:Permissions)->Result<*mut u8,Error>{
        if length==0{return Err(Error::AddressOverflow);}let offset=address&4095;let bytes=(length as u64).checked_add(offset).and_then(|n|n.checked_add(4095)).ok_or(Error::AddressOverflow)?&!4095;
        let physical=address&!4095;physical.checked_add(bytes).filter(|&end|end<=ADDRESS+4096).ok_or(Error::AddressOverflow)?;
        let end=self.next.checked_add(bytes).and_then(|n|n.checked_add(4096)).filter(|&n|n<=MAPPING_END).ok_or(Error::AddressOverflow)?;
        let base=self.next;
        for page in 0..bytes/4096{if let Err(error)=self.map_page(memory,base+page*4096,physical+page*4096,permissions){for rollback in 0..page{let _=self.unmap_page(base+rollback*4096);}return Err(error);}}
        self.next=end;Ok((base+offset)as *mut u8)
    }
}
