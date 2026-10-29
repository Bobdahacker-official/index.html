use core::{alloc::{GlobalAlloc,Layout},cell::UnsafeCell,ptr::null_mut,sync::atomic::{AtomicBool,AtomicU64,Ordering}};
use crate::boot;
#[derive(Clone,Copy)] struct Extent { start:u64,end:u64 }
pub struct Physical { ranges:[Extent;512], count:usize,pub total:u64,pub free:u64,pub direct:u64,tracked:Option<crate::frames::FrameMap> }
impl Physical {
    pub fn init()->Self {
        let map=boot::MEMORY.get().expect("No memory map");
        let mut p=Self { ranges:[Extent{start:0,end:0};512],count:0,total:0,free:0,direct:boot::HHDM.get().expect("No direct mapping").offset,tracked:None };
        for i in 0..map.count as usize {
            let e=unsafe{&**map.entries.add(i)};
            if matches!(e.kind,0|2|3|5|6) {p.total=p.total.checked_add(e.length).expect("RAM accounting overflow");}
            if e.kind==0 {
                let start=e.base.checked_add(4095).expect("Memory-map address overflow")&!4095;let end=e.base.checked_add(e.length).expect("Memory-map extent overflow")&!4095;
                if end>start {assert!(p.count<p.ranges.len());p.ranges[p.count]=Extent{start,end};p.count+=1;p.free+=end-start;}
            }
        }
        p
    }
    pub fn allocate(&mut self,bytes:u64)->*mut u8 {
        self.try_allocate(bytes).expect("Out of contiguous physical memory")
    }
    pub fn try_allocate(&mut self,bytes:u64)->Option<*mut u8> {
        if let Some(frames)=&mut self.tracked{let address=frames.allocate(bytes,4096)?;self.free=frames.available;return Some((address+self.direct)as *mut u8);}
        if bytes==0{return None;}
        let n=bytes.checked_add(4095)?&!4095;
        for r in &mut self.ranges[..self.count] {if r.end-r.start>=n {let address=r.start;r.start+=n;self.free-=n;return Some((address+self.direct) as *mut u8);}}
        None
    }
    pub fn enable_tracking(&mut self){
        assert!(self.tracked.is_none());
        let mut bitmaps=alloc::vec::Vec::with_capacity(self.count);
        for i in 0..self.count{let pages=(self.ranges[i].end-self.ranges[i].start)/4096;let bytes=pages.div_ceil(64)*8;bitmaps.push(if bytes==0{core::ptr::null_mut()}else{self.allocate(bytes)as *mut u64});}
        let mut regions=alloc::vec::Vec::with_capacity(self.count);
        for(i,&bitmap)in bitmaps.iter().enumerate(){let range=self.ranges[i];let pages=(range.end-range.start)/4096;if pages>0{regions.push(unsafe{crate::frames::Region::new(range.start,pages,bitmap)});}}
        let frames=crate::frames::FrameMap::new(regions);assert_eq!(frames.available,self.free);self.tracked=Some(frames);
    }
    pub fn allocate_aligned(&mut self,bytes:u64,alignment:u64)->Option<*mut u8>{let frames=self.tracked.as_mut()?;let physical=frames.allocate(bytes,alignment)?;self.free=frames.available;Some((physical+self.direct)as *mut u8)}
    pub fn retain_frame(&mut self,physical:u64)->Result<u32,crate::frames::Error>{self.tracked.as_mut().ok_or(crate::frames::Error::UnknownFrame)?.retain(physical)}
    pub fn frame_references(&self,physical:u64)->Result<u32,crate::frames::Error>{self.tracked.as_ref().ok_or(crate::frames::Error::UnknownFrame)?.references(physical)}
    pub unsafe fn release_frame(&mut self,physical:u64)->Result<bool,crate::frames::Error>{let frames=self.tracked.as_mut().ok_or(crate::frames::Error::UnknownFrame)?;let freed=frames.release(physical)?;self.free=frames.available;Ok(freed)}
    pub unsafe fn release_range(&mut self,physical:u64,bytes:u64)->Result<(),crate::frames::Error>{let frames=self.tracked.as_mut().ok_or(crate::frames::Error::UnknownFrame)?;frames.release_range(physical,bytes)?;self.free=frames.available;Ok(())}
    pub fn allocated_pages(&self)->u64{self.tracked.as_ref().map_or(0,|frames|frames.allocated_pages())}
}
#[repr(C)] struct Hole { size:usize,next:*mut Hole }
struct Heap { head:*mut Hole }
pub struct Allocator { inner:UnsafeCell<Heap>,lock:AtomicBool,pub used:AtomicU64 }
unsafe impl Sync for Allocator {}
#[global_allocator] pub static ALLOCATOR:Allocator=Allocator {inner:UnsafeCell::new(Heap{head:null_mut()}),lock:AtomicBool::new(false),used:AtomicU64::new(0)};
impl Allocator {
    pub unsafe fn init(&self,start:*mut u8,len:usize) {let h=start as *mut Hole;h.write(Hole{size:len,next:null_mut()});(*self.inner.get()).head=h;}
    fn acquire(&self) {while self.lock.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
    fn release(&self) {self.lock.store(false,Ordering::Release);}
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self,layout:Layout)->*mut u8 {
        self.acquire();let size=(layout.size().max(16)+15)&!15;let alignment=layout.align().max(16);
        let mut link=&mut (*self.inner.get()).head as *mut *mut Hole;
        while !(*link).is_null() {
            let hole=*link;let start=hole as usize;let end=start+(*hole).size;let Some(at)=start.checked_add(alignment-1).map(|n|n&!(alignment-1))else{link=&mut (*hole).next;continue;};
            if at.checked_add(size).is_some_and(|v|v<=end) {
                let next=(*hole).next;let tail=at+size;
                let suffix=if tail<end {let h=tail as *mut Hole;h.write(Hole{size:end-tail,next});h} else {next};
                if at>start {(*hole).size=at-start;(*hole).next=suffix;} else {*link=suffix;}
                self.used.fetch_add(size as u64,Ordering::Relaxed);self.release();return at as *mut u8;
            }
            link=&mut (*hole).next;
        }
        self.release();null_mut()
    }
    unsafe fn dealloc(&self,ptr:*mut u8,layout:Layout) {
        self.acquire();let size=(layout.size().max(16)+15)&!15;
        let mut link=&mut (*self.inner.get()).head as *mut *mut Hole;let mut prev:*mut Hole=null_mut();
        while !(*link).is_null()&&(*link as usize)<ptr as usize {prev=*link;link=&mut (**link).next;}
        let node=ptr as *mut Hole;node.write(Hole{size,next:*link});*link=node;
        if !(*node).next.is_null() && ptr as usize+size==(*node).next as usize {let n=(*node).next;(*node).size+=(*n).size;(*node).next=(*n).next;}
        if !prev.is_null() && prev as usize+(*prev).size==node as usize {(*prev).size+=(*node).size;(*prev).next=(*node).next;}
        self.used.fetch_sub(size as u64,Ordering::Relaxed);self.release();
    }
}
