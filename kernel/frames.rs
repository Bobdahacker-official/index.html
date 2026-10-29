use alloc::{collections::BTreeMap,vec::Vec};
use core::ptr::{read,write};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Unaligned,InvalidLength,UnknownFrame,AlreadyFree,SharedFrame,ReferenceOverflow}
pub struct Region{base:u64,pages:u64,bits:*mut u64,hint:u64}
impl Region {
    /// The bitmap is private, writable direct-mapped storage covering `pages` bits.
    pub unsafe fn new(base:u64,pages:u64,bits:*mut u64)->Self{
        assert!(base&4095==0&&!bits.is_null());
        core::ptr::write_bytes(bits,0,pages.div_ceil(64)as usize);
        Self{base,pages,bits,hint:0}
    }
    fn contains(&self,address:u64)->bool{address>=self.base&&(address-self.base)/4096<self.pages}
    fn allocated(&self,index:u64)->bool{unsafe{read(self.bits.add((index/64)as usize))&(1<<(index%64))!=0}}
    fn set(&mut self,index:u64,busy:bool){unsafe{let ptr=self.bits.add((index/64)as usize);let old=read(ptr);write(ptr,if busy{old|(1<<(index%64))}else{old&!(1<<(index%64))});}}
    fn first_free(&self,mut from:u64)->Option<u64>{
        while from<self.pages{
            let word=unsafe{read(self.bits.add((from/64)as usize))};
            let available=(!word)&(u64::MAX<<(from%64));
            if available!=0{let index=(from/64)*64+available.trailing_zeros()as u64;return(index<self.pages).then_some(index);}
            from=(from/64+1)*64;
        }None
    }
    fn allocate(&mut self,pages:u64,alignment:u64)->Option<u64>{
        if pages==0||pages>self.pages{return None;}
        let mut index=self.hint;
        while let Some(free)=self.first_free(index){
            let address=self.base.checked_add(free.checked_mul(4096)?)?;
            let aligned=address.checked_add(alignment-1)?&!(alignment-1);
            let candidate=(aligned-self.base)/4096;
            let end=candidate.checked_add(pages)?;if end>self.pages{return None;}
            let mut busy=None;let mut cursor=candidate;
            while cursor<end{
                let chunk_end=end.min((cursor/64+1)*64);let count=chunk_end-cursor;
                let mask=if count==64{u64::MAX}else{((1u64<<count)-1)<<(cursor%64)};
                let occupied=unsafe{read(self.bits.add((cursor/64)as usize))}&mask;
                if occupied!=0{busy=Some((cursor/64)*64+occupied.trailing_zeros()as u64);break;}
                cursor=chunk_end;
            }
            if let Some(blocked)=busy{index=blocked+1;continue;}
            for p in candidate..end{self.set(p,true);}
            if candidate==self.hint{self.hint=self.first_free(end).unwrap_or(self.pages);}
            return Some(aligned);
        }None
    }
    fn release(&mut self,index:u64){self.set(index,false);self.hint=self.hint.min(index);}
}

pub struct FrameMap{regions:Vec<Region>,shared:BTreeMap<u64,u32>,pub available:u64}
impl FrameMap {
    pub fn new(regions:Vec<Region>)->Self{let available=regions.iter().map(|r|r.pages*4096).sum();Self{regions,shared:BTreeMap::new(),available}}
    pub fn allocate(&mut self,bytes:u64,alignment:u64)->Option<u64>{
        if bytes==0||!alignment.is_power_of_two()||alignment<4096{return None;}
        let pages=bytes.checked_add(4095)?/4096;
        for region in &mut self.regions{if let Some(address)=region.allocate(pages,alignment){self.available-=pages*4096;return Some(address);}}None
    }
    fn locate(&self,address:u64)->Result<(usize,u64),Error>{
        if address&4095!=0{return Err(Error::Unaligned);}
        let index=self.regions.iter().position(|r|r.contains(address)).ok_or(Error::UnknownFrame)?;
        let page=(address-self.regions[index].base)/4096;
        if !self.regions[index].allocated(page){return Err(Error::AlreadyFree);}Ok((index,page))
    }
    pub fn references(&self,address:u64)->Result<u32,Error>{self.locate(address)?;Ok(self.shared.get(&address).copied().unwrap_or(1))}
    pub fn retain(&mut self,address:u64)->Result<u32,Error>{let count=self.references(address)?.checked_add(1).ok_or(Error::ReferenceOverflow)?;self.shared.insert(address,count);Ok(count)}
    /// The caller has revoked its mappings and stopped DMA access to the frame.
    pub unsafe fn release(&mut self,address:u64)->Result<bool,Error>{
        let(region,index)=self.locate(address)?;
        if let Some(&count)=self.shared.get(&address){if count>2{self.shared.insert(address,count-1);}else{self.shared.remove(&address);}return Ok(false);}
        self.regions[region].release(index);self.available+=4096;Ok(true)
    }
    /// Reject the entire operation if any page is free, shared, or outside a
    /// managed extent. No bitmap is changed before validation has succeeded.
    pub unsafe fn release_range(&mut self,address:u64,bytes:u64)->Result<(),Error>{
        if bytes==0||bytes&4095!=0{return Err(Error::InvalidLength);}
        let end=address.checked_add(bytes).ok_or(Error::InvalidLength)?;
        for page in (address..end).step_by(4096){if self.references(page)?!=1{return Err(Error::SharedFrame);}}
        for page in (address..end).step_by(4096){let(region,index)=self.locate(page)?;self.regions[region].release(index);}
        self.available+=bytes;Ok(())
    }
    pub fn allocated_pages(&self)->u64{self.regions.iter().map(|r|r.pages).sum::<u64>()-self.available/4096}
}
