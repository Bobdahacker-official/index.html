use alloc::{collections::BTreeMap,sync::Arc,vec::Vec};
use crate::{memory::Physical,paging::{Mapper,Permissions}};

const PAGE:u64=4096;
const USER_START:u64=0x10000;
const USER_END:u64=0x0000_7fff_0000_0000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Access{None,Read,ReadWrite,ReadExecute}
impl Access{
    fn writable(self)->bool{self==Self::ReadWrite}
    fn permissions(self,cow:bool)->Permissions{Permissions{writable:self.writable()&&!cow,executable:self==Self::ReadExecute,user:true,uncached:false}}
}
#[derive(Clone)]pub enum Backing{Anonymous,FilePrivate{bytes:Arc<[u8]>,offset:u64}}
#[derive(Clone)]pub struct Area{pub start:u64,pub end:u64,pub access:Access,pub backing:Backing}
impl Area{
    fn slice(&self,start:u64,end:u64)->Self{let mut area=self.clone();if let Backing::FilePrivate{offset,..}=&mut area.backing{*offset+=start-self.start;}area.start=start;area.end=end;area}
}
#[derive(Clone,Copy)]struct Resident{physical:u64,cow:bool,mapped:bool}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Error{Address,Length,Collision,Unmapped,Permission,EndOfFile,OutOfMemory,Active,PageTable(crate::paging::Error),Frame(crate::frames::Error)}
impl From<crate::paging::Error> for Error{fn from(error:crate::paging::Error)->Self{Self::PageTable(error)}}
impl From<crate::frames::Error> for Error{fn from(error:crate::frames::Error)->Self{Self::Frame(error)}}
pub struct AddressSpace{mapper:Mapper,areas:Vec<Area>,resident:BTreeMap<u64,Resident>}
impl AddressSpace{
    pub fn new(memory:&mut Physical,kernel:&Mapper)->Result<Self,Error>{Ok(Self{mapper:Mapper::user(memory,kernel)?,areas:Vec::new(),resident:BTreeMap::new()})}
    pub fn root(&self)->u64{self.mapper.root()}
    pub fn areas(&self)->&[Area]{&self.areas}
    pub fn resident_bytes(&self)->u64{self.resident.len()as u64*PAGE}
    pub unsafe fn activate(&self){self.mapper.activate();}
    fn range(start:u64,length:u64)->Result<(u64,u64),Error>{
        if length==0{return Err(Error::Length);}if start<PAGE||start&PAGE.wrapping_sub(1)!=0{return Err(Error::Address);}
        let length=length.checked_add(PAGE-1).ok_or(Error::Length)?&!(PAGE-1);let end=start.checked_add(length).ok_or(Error::Address)?;
        if start<USER_START||end>USER_END{return Err(Error::Address);}Ok((start,end))
    }
    pub fn mmap(&mut self,address:Option<u64>,length:u64,access:Access,backing:Backing)->Result<u64,Error>{
        let length=length.checked_add(PAGE-1).ok_or(Error::Length)?&!(PAGE-1);if length==0{return Err(Error::Length);}
        if let Backing::FilePrivate{offset,..}=&backing{if offset&(PAGE-1)!=0||offset.checked_add(length).is_none(){return Err(Error::Address);}}
        let start=if let Some(start)=address{start}else{
            let mut candidate=USER_START;
            for area in &self.areas{if candidate.checked_add(length).is_some_and(|end|end<=area.start){break;}candidate=candidate.max(area.end);}
            candidate
        };
        let(start,end)=Self::range(start,length)?;
        if self.areas.iter().any(|a|a.start<end&&start<a.end){return Err(Error::Collision);}
        let position=self.areas.partition_point(|a|a.start<start);self.areas.insert(position,Area{start,end,access,backing});Ok(start)
    }
    fn area(&self,address:u64)->Option<&Area>{let index=self.areas.partition_point(|area|area.start<=address);index.checked_sub(1).and_then(|i|self.areas.get(i)).filter(|area|address<area.end)}
    /// Resolves a user access fault. The caller supplies the CPU fault's write and
    /// instruction-fetch bits and exclusive access to this address space.
    pub fn fault(&mut self,address:u64,write_access:bool,instruction_fetch:bool,memory:&mut Physical)->Result<(),Error>{
        let page=address&!(PAGE-1);let area=self.area(address).ok_or(Error::Unmapped)?.clone();
        if area.access==Access::None||write_access&&!area.access.writable()||instruction_fetch&&area.access!=Access::ReadExecute{return Err(Error::Permission);}
        if let Some(resident)=self.resident.get_mut(&page){
            if write_access&&resident.cow{
                if memory.frame_references(resident.physical)?==1{if resident.mapped{unsafe{self.mapper.protect_page(page,area.access.permissions(false))}?;}resident.cow=false;}
                else{
                    let frame=memory.try_allocate(PAGE).ok_or(Error::OutOfMemory)?;let physical=frame as u64-memory.direct;
                    unsafe{core::ptr::copy_nonoverlapping((resident.physical+memory.direct)as *const u8,frame,PAGE as usize);}
                    if resident.mapped{if let Err(error)=unsafe{self.mapper.replace_page(page,physical,area.access.permissions(false))}{unsafe{memory.release_frame(physical)}?;return Err(error.into());}}
                    unsafe{memory.release_frame(resident.physical)}?;resident.physical=physical;resident.cow=false;
                }
            }
            if !resident.mapped{unsafe{self.mapper.map_page(memory,page,resident.physical,area.access.permissions(resident.cow))}?;resident.mapped=true;}
            return Ok(());
        }
        let file_offset=match &area.backing{Backing::Anonymous=>None,Backing::FilePrivate{bytes,offset}=>{let at=offset.checked_add(page-area.start).ok_or(Error::Address)?;if at>=bytes.len()as u64{return Err(Error::EndOfFile);}Some((bytes,at as usize))}};
        let frame=memory.try_allocate(PAGE).ok_or(Error::OutOfMemory)?;let physical=frame as u64-memory.direct;
        unsafe{core::ptr::write_bytes(frame,0,PAGE as usize);if let Some((bytes,offset))=file_offset{let count=(bytes.len()-offset).min(PAGE as usize);core::ptr::copy_nonoverlapping(bytes.as_ptr().add(offset),frame,count);}}
        if let Err(error)=unsafe{self.mapper.map_page(memory,page,physical,area.access.permissions(false))}{unsafe{memory.release_frame(physical)}?;return Err(error.into());}
        self.resident.insert(page,Resident{physical,cow:false,mapped:true});Ok(())
    }
    pub fn munmap(&mut self,start:u64,length:u64,memory:&mut Physical)->Result<(),Error>{
        let(start,end)=Self::range(start,length)?;let mut areas=Vec::with_capacity(self.areas.len()+1);
        for area in &self.areas{if area.end<=start||area.start>=end{areas.push(area.clone());continue;}if area.start<start{areas.push(area.slice(area.start,start));}if area.end>end{areas.push(area.slice(end,area.end));}}
        let pages:Vec<u64>=self.resident.range(start..end).map(|(&address,_)|address).collect();
        for address in pages{let resident=self.resident[&address];if resident.mapped{unsafe{self.mapper.unmap_page(address)}?;}unsafe{memory.release_frame(resident.physical)}?;self.resident.remove(&address);}
        self.areas=areas;Ok(())
    }
    pub fn mprotect(&mut self,start:u64,length:u64,access:Access,memory:&mut Physical)->Result<(),Error>{
        let(start,end)=Self::range(start,length)?;let mut covered=start;
        for area in &self.areas{if area.end<=covered{continue;}if area.start>covered{break;}covered=area.end.min(end);if covered==end{break;}}
        if covered!=end{return Err(Error::Unmapped);}
        let mut areas=Vec::with_capacity(self.areas.len()+2);
        for area in &self.areas{if area.end<=start||area.start>=end{areas.push(area.clone());continue;}if area.start<start{areas.push(area.slice(area.start,start));}let mut center=area.slice(area.start.max(start),area.end.min(end));center.access=access;areas.push(center);if area.end>end{areas.push(area.slice(end,area.end));}}
        // Shared read-only frames also become COW when write access is granted.
        for(&address,resident)in self.resident.range_mut(start..end){
            if access.writable()&&memory.frame_references(resident.physical)?>1{resident.cow=true;}
            if access==Access::None{if resident.mapped{unsafe{self.mapper.unmap_page(address)}?;resident.mapped=false;}}
            else if resident.mapped{unsafe{self.mapper.protect_page(address,access.permissions(resident.cow))}?;}
            // A non-present resident is restored by the next actual access fault.
        }
        self.areas=areas;Ok(())
    }
    pub fn fork(&mut self,memory:&mut Physical)->Result<Self,Error>{
        let mut child=Self{mapper:Mapper::user(memory,&self.mapper)?,areas:self.areas.clone(),resident:BTreeMap::new()};
        let result=(||->Result<(),Error>{
            for(&address,resident)in &mut self.resident{
                let area=self.areas.iter().find(|a|a.start<=address&&address<a.end).ok_or(Error::Unmapped)?;
                memory.retain_frame(resident.physical)?;
                let cow=resident.cow||area.access.writable();child.resident.insert(address,Resident{physical:resident.physical,cow,mapped:false});
                if cow{resident.cow=true;if resident.mapped{unsafe{self.mapper.protect_page(address,area.access.permissions(true))}?;}}
                if resident.mapped{unsafe{child.mapper.map_page(memory,address,resident.physical,area.access.permissions(cow))}?;child.resident.get_mut(&address).unwrap().mapped=true;}
            }Ok(())
        })();
        if let Err(error)=result{unsafe{child.destroy(memory)}?;return Err(error);}Ok(child)
    }
    /// Access bytes through the direct map after enforcing the same protection
    /// and demand-paging rules used by user faults. Used by syscall copy routines.
    pub fn read(&mut self,address:u64,destination:&mut[u8],memory:&mut Physical)->Result<(),Error>{
        let end=address.checked_add(destination.len()as u64).ok_or(Error::Address)?;if end>USER_END{return Err(Error::Address);}
        let mut copied=0;while copied<destination.len(){let current=address+copied as u64;self.fault(current,false,false,memory)?;let page=current&!(PAGE-1);let offset=(current-page)as usize;let count=(PAGE as usize-offset).min(destination.len()-copied);let resident=self.resident[&page];unsafe{core::ptr::copy_nonoverlapping((resident.physical+memory.direct+offset as u64)as *const u8,destination.as_mut_ptr().add(copied),count);}copied+=count;}Ok(())
    }
    pub fn write(&mut self,address:u64,source:&[u8],memory:&mut Physical)->Result<(),Error>{
        let end=address.checked_add(source.len()as u64).ok_or(Error::Address)?;if end>USER_END{return Err(Error::Address);}
        let mut copied=0;while copied<source.len(){let current=address+copied as u64;self.fault(current,true,false,memory)?;let page=current&!(PAGE-1);let offset=(current-page)as usize;let count=(PAGE as usize-offset).min(source.len()-copied);let resident=self.resident[&page];unsafe{core::ptr::copy_nonoverlapping(source.as_ptr().add(copied),(resident.physical+memory.direct+offset as u64)as *mut u8,count);}copied+=count;}Ok(())
    }
    /// The address space is inactive on every CPU and no DMA operation targets it.
    pub unsafe fn destroy(mut self,memory:&mut Physical)->Result<(),Error>{
        let current:u64;core::arch::asm!("mov {}, cr3",out(reg)current,options(nomem,nostack));if current&0x000f_ffff_ffff_f000==self.root(){return Err(Error::Active);}
        for(address,resident)in core::mem::take(&mut self.resident){if resident.mapped{self.mapper.unmap_page(address)?;}memory.release_frame(resident.physical)?;}
        self.mapper.destroy_user(memory)?;Ok(())
    }
}
