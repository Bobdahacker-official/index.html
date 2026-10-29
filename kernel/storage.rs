use core::{arch::asm,ptr::{read_volatile,write_volatile},sync::atomic::{fence,Ordering}};
use alloc::vec::Vec;
use crate::{hardware, memory::Physical,apps::Files};
unsafe fn out16(p:u16,v:u16){asm!("out dx, ax",in("dx")p,in("ax")v,options(nostack));}
unsafe fn out32(p:u16,v:u32){asm!("out dx, eax",in("dx")p,in("eax")v,options(nostack));}
unsafe fn in16(p:u16)->u16{let v:u16;asm!("in ax, dx",in("dx")p,out("ax")v,options(nostack));v}
unsafe fn in32(p:u16)->u32{let v:u32;asm!("in eax, dx",in("dx")p,out("eax")v,options(nostack));v}
#[repr(C)]#[derive(Clone,Copy)]struct Desc{addr:u64,len:u32,flags:u16,next:u16}
pub struct Block{io:u16,queue:*mut u8,dma:*mut u8,physical:u64,size:usize,used:usize,index:u16,flush:bool,pub sectors:u64}
impl Block{
    pub fn init(memory:&mut Physical,devices:&[hardware::Pci])->Option<Self>{unsafe{
        let d=devices.iter().find(|d|d.vendor==0x1af4&&d.device==0x1001)?;
        let config=0x80000000u32|(d.bus as u32)<<16|(d.slot as u32)<<11|(d.function as u32)<<8;
        out32(0xcf8,config|0x10);let bar=in32(0xcfc);if bar&1==0{return None;}let io=(bar&0xfffc)as u16;
        out32(0xcf8,config|4);let command=in32(0xcfc);out32(0xcf8,config|4);out32(0xcfc,(command&0xffff)|5);
        hardware::outb(io+18,0);hardware::outb(io+18,1);hardware::outb(io+18,3);
        let features=in32(io);let flush=features&(1<<9)!=0;out32(io+4,features&(1<<9));
        out16(io+14,0);let size=in16(io+12)as usize;if !(3..=256).contains(&size){return None;}
        let used=(size*16+6+size*2+4095)&!4095;let bytes=(used+6+size*8+4095)&!4095;
        let queue=memory.allocate(bytes as u64);core::ptr::write_bytes(queue,0,bytes);
        let pfn=(queue as u64-memory.direct)>>12;if pfn>u32::MAX as u64{return None;}out32(io+8,pfn as u32);
        let dma=memory.allocate(4096);let physical=dma as u64-memory.direct;
        let sectors=in32(io+20)as u64|((in32(io+24)as u64)<<32);hardware::outb(io+18,7);
        Some(Self{io,queue,dma,physical,size,used,index:0,flush,sectors})
    }}
    fn request(&mut self,kind:u32,sector:u64,buffer:&mut[u8;512])->Result<(),&'static str>{unsafe{
        if kind!=4&&sector>=self.sectors{return Err("Sector outside disk");}
        write_volatile(self.dma as *mut u32,kind);write_volatile(self.dma.add(4)as *mut u32,0);write_volatile(self.dma.add(8)as *mut u64,sector);
        if kind==1{core::ptr::copy_nonoverlapping(buffer.as_ptr(),self.dma.add(32),512);}write_volatile(self.dma.add(544),0xff);
        let desc=self.queue as *mut Desc;
        write_volatile(desc,Desc{addr:self.physical,len:16,flags:1,next:if kind==4{2}else{1}});
        write_volatile(desc.add(1),Desc{addr:self.physical+32,len:512,flags:1|if kind==0{2}else{0},next:2});
        write_volatile(desc.add(2),Desc{addr:self.physical+544,len:1,flags:2,next:0});
        let avail=self.queue.add(self.size*16)as *mut u16;write_volatile(avail,1);write_volatile(avail.add(2+self.index as usize%self.size),0);
        fence(Ordering::SeqCst);self.index=self.index.wrapping_add(1);write_volatile(avail.add(1),self.index);fence(Ordering::SeqCst);out16(self.io+16,0);
        let begin=hardware::cycles();loop{if read_volatile(self.queue.add(self.used+2)as *const u16)==self.index{break;}if hardware::cycles().wrapping_sub(begin)>10_000_000_000{return Err("Disk request timed out");}core::hint::spin_loop();}
        fence(Ordering::SeqCst);let status=read_volatile(self.dma.add(544));let _=hardware::inb(self.io+19);if status!=0{return Err("Disk I/O error");}
        if kind==0{core::ptr::copy_nonoverlapping(self.dma.add(32),buffer.as_mut_ptr(),512);}Ok(())
    }}
    pub fn read(&mut self,sector:u64,buffer:&mut[u8;512])->Result<(),&'static str>{self.request(0,sector,buffer)}
    pub fn write(&mut self,sector:u64,buffer:&mut[u8;512])->Result<(),&'static str>{self.request(1,sector,buffer)}
    pub fn sync(&mut self)->Result<(),&'static str>{if self.flush{self.request(4,0,&mut[0;512])}else{Err("Disk does not expose durable flush")}}
}
const START:u64=133120;
const BANK:u64=2048;
fn crc32(data:&[u8])->u32{let mut crc=!0u32;for &b in data{crc^=b as u32;for _ in 0..8{crc=(crc>>1)^((0u32.wrapping_sub(crc&1))&0xedb88320);}}!crc}
pub struct AvaFs{pub block:Block,generation:u64,bank:u64,pub mounted:bool}
impl AvaFs{
    pub fn mount(block:Block,files:&mut Files)->Result<Self,&'static str>{
        if block.sectors<START+2*BANK{return Err("Disk lacks an AvaFS partition");}
        let mut fs=Self{block,generation:0,bank:1,mounted:true};let mut best:Option<(u64,u64,Vec<u8>)>=None;
        let mut partitions=[0u8;512];fs.block.read(2,&mut partitions)?;
        if partitions[128..144]!=[0x55,0xb3,0xc1,0xd5,0xb1,0x6c,0x64,0x4d,0xa6,0xd5,0x41,0x56,0x41,0x46,0x53,0x30]||u64::from_le_bytes(partitions[160..168].try_into().unwrap())!=START||u64::from_le_bytes(partitions[168..176].try_into().unwrap())<START+2*BANK-1{return Err("AvaFS partition identity mismatch");}
        for bank in 0..2{let mut header=[0u8;512];fs.block.read(START+bank*BANK,&mut header)?;if &header[..8]!=b"AVAFS001"{continue;}let generation=u64::from_le_bytes(header[8..16].try_into().unwrap());let len=u32::from_le_bytes(header[16..20].try_into().unwrap())as usize;let crc=u32::from_le_bytes(header[20..24].try_into().unwrap());if len>=(BANK as usize-1)*512{continue;}let mut payload=alloc::vec![0;len];for(i,chunk)in payload.chunks_mut(512).enumerate(){let mut sector=[0;512];fs.block.read(START+bank*BANK+1+i as u64,&mut sector)?;chunk.copy_from_slice(&sector[..chunk.len()]);}if crc32(&payload)==crc&&best.as_ref().is_none_or(|(g,_,_)|generation>*g){best=Some((generation,bank,payload));}}
        if let Some((generation,bank,payload))=best{let mut at=0;let mut loaded=Files::new();while at<payload.len(){if at+8>payload.len(){return Err("Invalid AvaFS directory");}let n=u32::from_le_bytes(payload[at..at+4].try_into().unwrap())as usize;let l=u32::from_le_bytes(payload[at+4..at+8].try_into().unwrap())as usize;at+=8;if n>80||l>1024*1024||at+n+l>payload.len(){return Err("Invalid AvaFS extent");}let name=core::str::from_utf8(&payload[at..at+n]).map_err(|_|"Invalid filename")?;let data=core::str::from_utf8(&payload[at+n..at+n+l]).map_err(|_|"Invalid document encoding")?;loaded.save(name,data)?;at+=n+l;}*files=loaded;fs.generation=generation;fs.bank=bank;}
        Ok(fs)
    }
    pub fn save(&mut self,files:&Files)->Result<(),&'static str>{
        let mut payload=Vec::new();for d in &files.entries{payload.extend_from_slice(&(d.name.len()as u32).to_le_bytes());payload.extend_from_slice(&(d.text.len()as u32).to_le_bytes());payload.extend_from_slice(d.name.as_bytes());payload.extend_from_slice(d.text.as_bytes());}
        if payload.len()>=(BANK as usize-1)*512{return Err("AvaFS volume is full");}
        let bank=1-self.bank;let generation=self.generation.checked_add(1).ok_or("Generation overflow")?;
        for(i,chunk)in payload.chunks(512).enumerate(){let mut buffer=[0u8;512];buffer[..chunk.len()].copy_from_slice(chunk);self.block.write(START+bank*BANK+1+i as u64,&mut buffer)?;}
        self.block.sync()?;let mut header=[0u8;512];header[..8].copy_from_slice(b"AVAFS001");header[8..16].copy_from_slice(&generation.to_le_bytes());header[16..20].copy_from_slice(&(payload.len()as u32).to_le_bytes());header[20..24].copy_from_slice(&crc32(&payload).to_le_bytes());self.block.write(START+bank*BANK,&mut header)?;self.block.sync()?;self.generation=generation;self.bank=bank;Ok(())
    }
}
