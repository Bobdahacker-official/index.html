use alloc::{string::{String,ToString},vec::Vec};
use crate::boot;

#[derive(Clone,Debug)]
pub struct CommandLine {pub raw:String,pub arguments:Vec<(String,String)>}
impl CommandLine {
    pub fn parse(raw:&str)->Result<Self,&'static str>{
        if raw.len()>4096{return Err("Command line exceeds 4096 bytes");}
        let mut tokens=Vec::new();let mut token=String::new();let mut quote=None;let mut escaped=false;
        for ch in raw.chars(){
            if escaped{token.push(ch);escaped=false;continue;}
            if ch=='\\'&&quote!=Some('\''){escaped=true;continue;}
            if let Some(delimiter)=quote{if ch==delimiter{quote=None;}else{token.push(ch);}continue;}
            if ch=='\''||ch=='"'{quote=Some(ch);continue;}
            if ch.is_ascii_whitespace(){if !token.is_empty(){tokens.push(core::mem::take(&mut token));}}else{token.push(ch);}
        }
        if quote.is_some()||escaped{return Err("Unterminated command-line quote or escape");}
        if !token.is_empty(){tokens.push(token);}
        let mut arguments:Vec<(String,String)>=Vec::new();
        for token in tokens{
            let(key,value)=token.split_once('=').unwrap_or((&token,""));
            if key.is_empty()||!key.bytes().all(|c|c.is_ascii_alphanumeric()||matches!(c,b'_'|b'-'|b'.')){return Err("Invalid command-line key");}
            if let Some(existing)=arguments.iter_mut().find(|(k,_)|k==key){existing.1=value.to_string();}else{arguments.push((key.to_string(),value.to_string()));}
        }
        Ok(Self{raw:raw.to_string(),arguments})
    }
    pub fn get(&self,key:&str)->Option<&str>{self.arguments.iter().find(|(k,_)|k==key).map(|(_,v)|v.as_str())}
}

#[derive(Clone,Copy,Debug)]
pub struct EfiDescriptor {pub kind:u32,pub physical:u64,pub virtual_address:u64,pub pages:u64,pub attributes:u64}
impl EfiDescriptor {
    pub fn bytes(self)->u64{self.pages*4096}
    pub fn runtime(self)->bool{self.attributes&(1<<63)!=0}
}
pub struct BootInformation {pub command_line:CommandLine,pub executable_path:String,pub efi_memory:Vec<EfiDescriptor>,pub descriptor_version:u64}
impl BootInformation {
    pub fn capture()->Result<Self,&'static str>{
        unsafe fn string(ptr:*const u8)->Result<String,&'static str>{
            if ptr.is_null(){return Ok(String::new());}
            let mut length=0;
            while length<=4096 {if core::ptr::read_volatile(ptr.add(length))==0{let bytes=core::slice::from_raw_parts(ptr,length);return core::str::from_utf8(bytes).map(str::to_string).map_err(|_|"Boot string is not UTF-8");}length+=1;}
            Err("Boot string is not terminated")
        }
        let executable=boot::EXECUTABLE.get().ok_or("Executable response absent")?;
        let file=unsafe{executable.file.as_ref()}.ok_or("Executable descriptor absent")?;
        let command_line=CommandLine::parse(&unsafe{string(file.command_line)}?)?;
        let executable_path=unsafe{string(file.path)}?;
        let map=boot::EFI_MEMORY.get().ok_or("EFI memory-map response absent")?;
        if map.map.is_null()||map.descriptor_size<40||map.descriptor_size>4096||map.size>64*1024*1024||map.size%map.descriptor_size!=0{return Err("Invalid EFI memory-map layout");}
        if map.descriptor_version!=1{return Err("Unknown EFI memory descriptor version");}
        let bytes=unsafe{core::slice::from_raw_parts(map.map,map.size as usize)};
        let mut efi_memory=Vec::with_capacity((map.size/map.descriptor_size)as usize);
        for descriptor in bytes.chunks_exact(map.descriptor_size as usize){
            let read64=|offset|u64::from_le_bytes(descriptor[offset..offset+8].try_into().unwrap());
            let entry=EfiDescriptor{kind:u32::from_le_bytes(descriptor[..4].try_into().unwrap()),physical:read64(8),virtual_address:read64(16),pages:read64(24),attributes:read64(32)};
            let length=entry.pages.checked_mul(4096).ok_or("EFI page count overflow")?;
            if entry.physical&4095!=0||entry.physical.checked_add(length).is_none(){return Err("Invalid EFI physical extent");}
            efi_memory.push(entry);
        }
        Ok(Self{command_line,executable_path,efi_memory,descriptor_version:map.descriptor_version})
    }
    pub fn runtime_bytes(&self)->u64{self.efi_memory.iter().filter(|e|e.runtime()).map(|e|e.bytes()).sum()}
}

pub struct Platform {pub boot:BootInformation,pub acpi:crate::acpi::Acpi}
