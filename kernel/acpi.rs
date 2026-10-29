use alloc::vec::Vec;
use crate::{boot,memory::Physical,paging::{Mapper,Permissions},hardware};

const MAX_TABLE_LENGTH:usize=1024*1024;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error {Absent,Address,Length,Checksum,Signature,Revision,MalformedRecord,DuplicateProcessor,OverlappingEcam,Mapping(crate::paging::Error)}
impl From<crate::paging::Error> for Error{fn from(error:crate::paging::Error)->Self{Self::Mapping(error)}}
fn u16_at(b:&[u8],offset:usize)->u16{u16::from_le_bytes(b[offset..offset+2].try_into().unwrap())}
fn u32_at(b:&[u8],offset:usize)->u32{u32::from_le_bytes(b[offset..offset+4].try_into().unwrap())}
fn u64_at(b:&[u8],offset:usize)->u64{u64::from_le_bytes(b[offset..offset+8].try_into().unwrap())}
fn checksum(bytes:&[u8])->bool{bytes.iter().fold(0u8,|sum,&b|sum.wrapping_add(b))==0}

fn firmware_range(address:u64,length:usize)->Result<(),Error>{
    let end=address.checked_add(length as u64).ok_or(Error::Address)?;
    if address==0||length==0||length>MAX_TABLE_LENGTH{return Err(Error::Address);}
    let map=boot::MEMORY.get().ok_or(Error::Absent)?;let mut cursor=address;
    while cursor<end{
        let mut next=cursor;
        for index in 0..map.count as usize{let region=unsafe{&**map.entries.add(index)};let limit=region.base.checked_add(region.length).ok_or(Error::Address)?;
            if region.kind!=4&&region.kind!=7&&region.base<=cursor&&limit>cursor{next=next.max(limit.min(end));}
        }
        if next==cursor{return Err(Error::Address);}cursor=next;
    }Ok(())
}
fn view(mapper:&mut Mapper,memory:&mut Physical,address:u64,length:usize)->Result<&'static[u8],Error>{
    firmware_range(address,length)?;
    let pointer=unsafe{mapper.map_physical(memory,address,length,Permissions::FIRMWARE)}?;
    Ok(unsafe{core::slice::from_raw_parts(pointer,length)})
}

#[derive(Clone)]
pub struct Table {pub physical:u64,pub signature:[u8;4],pub revision:u8,pub oem_id:[u8;6],pub oem_table_id:[u8;8],bytes:&'static[u8]}
impl Table {
    fn read(mapper:&mut Mapper,memory:&mut Physical,address:u64)->Result<Self,Error>{
        let header=view(mapper,memory,address,36)?;let length=u32_at(header,4)as usize;
        if !(36..=MAX_TABLE_LENGTH).contains(&length){return Err(Error::Length);}
        let bytes=view(mapper,memory,address,length)?;if !checksum(bytes){return Err(Error::Checksum);}
        let signature:[u8;4]=bytes[..4].try_into().unwrap();if !signature.iter().all(|c|c.is_ascii_uppercase()||c.is_ascii_digit()||*c==b'!'){return Err(Error::Signature);}
        Ok(Self{physical:address,signature,revision:bytes[8],oem_id:bytes[10..16].try_into().unwrap(),oem_table_id:bytes[16..24].try_into().unwrap(),bytes})
    }
    pub fn bytes(&self)->&[u8]{self.bytes}
    pub fn length(&self)->usize{self.bytes.len()}
}

#[derive(Clone,Copy,Debug)]
pub struct GenericAddress {pub space:u8,pub width:u8,pub offset:u8,pub access_size:u8,pub address:u64}
impl GenericAddress {
    fn parse(bytes:&[u8],offset:usize)->Result<Option<Self>,Error>{
        if offset+12>bytes.len(){return Ok(None);}
        let g=Self{space:bytes[offset],width:bytes[offset+1],offset:bytes[offset+2],access_size:bytes[offset+3],address:u64_at(bytes,offset+4)};
        if g.address==0{return Ok(None);}
        if g.access_size>4{return Err(Error::MalformedRecord);}
        Ok(Some(g))
    }
    fn legacy(port:u32,width:u8)->Option<Self>{if port==0{None}else{Some(Self{space:1,width,offset:0,access_size:0,address:port as u64})}}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Polarity{Conforms,High,Low}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Trigger{Conforms,Edge,Level}
#[derive(Clone,Copy,Debug)]pub struct InterruptFlags{pub polarity:Polarity,pub trigger:Trigger}
impl InterruptFlags{
    fn parse(bits:u16)->Result<Self,Error>{
        if bits&!15!=0{return Err(Error::MalformedRecord);}
        let polarity=match bits&3{0=>Polarity::Conforms,1=>Polarity::High,3=>Polarity::Low,_=>return Err(Error::MalformedRecord)};
        let trigger=match(bits>>2)&3{0=>Trigger::Conforms,1=>Trigger::Edge,3=>Trigger::Level,_=>return Err(Error::MalformedRecord)};
        Ok(Self{polarity,trigger})
    }
}
#[derive(Clone,Copy,Debug)]pub struct Processor{pub uid:u32,pub apic_id:u32,pub enabled:bool,pub online_capable:bool,pub x2apic:bool}
#[derive(Clone,Copy,Debug)]pub struct IoApic{pub id:u8,pub address:u64,pub gsi_base:u32}
#[derive(Clone,Copy,Debug)]pub struct InterruptOverride{pub bus:u8,pub source:u8,pub gsi:u32,pub flags:InterruptFlags}
#[derive(Clone,Copy,Debug)]pub struct NmiSource{pub gsi:u32,pub flags:InterruptFlags}
#[derive(Clone,Copy,Debug)]pub struct LocalNmi{pub uid:Option<u32>,pub lint:u8,pub flags:InterruptFlags}
pub struct InterruptTopology{pub local_apic:u64,pub legacy_pic:bool,pub processors:Vec<Processor>,pub io_apics:Vec<IoApic>,pub overrides:Vec<InterruptOverride>,pub nmi_sources:Vec<NmiSource>,pub local_nmis:Vec<LocalNmi>}
impl InterruptTopology {
    fn parse(table:&Table)->Result<Self,Error>{
        let b=table.bytes;if b.len()<44{return Err(Error::Length);}
        let mut result=Self{local_apic:u32_at(b,36)as u64,legacy_pic:u32_at(b,40)&1!=0,processors:Vec::new(),io_apics:Vec::new(),overrides:Vec::new(),nmi_sources:Vec::new(),local_nmis:Vec::new()};
        let mut offset=44;let mut override_address=false;
        while offset<b.len(){
            if offset+2>b.len(){return Err(Error::MalformedRecord);}let length=b[offset+1]as usize;
            if length<2||offset+length>b.len(){return Err(Error::MalformedRecord);}let record=&b[offset..offset+length];
            let minimum=match record[0]{0=>8,1=>12,2=>10,3=>8,4=>6,5=>12,9=>16,10=>12,_=>2};if length<minimum{return Err(Error::MalformedRecord);}
            match record[0]{
                0=>result.add_processor(Processor{uid:record[2]as u32,apic_id:record[3]as u32,enabled:u32_at(record,4)&1!=0,online_capable:u32_at(record,4)&2!=0,x2apic:false})?,
                1=>{let entry=IoApic{id:record[2],address:u32_at(record,4)as u64,gsi_base:u32_at(record,8)};if entry.address==0||entry.address&4095!=0{return Err(Error::Address);}if result.io_apics.iter().any(|a|a.id==entry.id||a.gsi_base==entry.gsi_base){return Err(Error::MalformedRecord);}result.io_apics.push(entry);},
                2=>{let entry=InterruptOverride{bus:record[2],source:record[3],gsi:u32_at(record,4),flags:InterruptFlags::parse(u16_at(record,8))?};if result.overrides.iter().any(|o|o.bus==entry.bus&&o.source==entry.source){return Err(Error::MalformedRecord);}result.overrides.push(entry);},
                3=>result.nmi_sources.push(NmiSource{flags:InterruptFlags::parse(u16_at(record,2))?,gsi:u32_at(record,4)}),
                4=>{if record[5]>1{return Err(Error::MalformedRecord);}result.local_nmis.push(LocalNmi{uid:if record[2]==0xff{None}else{Some(record[2]as u32)},flags:InterruptFlags::parse(u16_at(record,3))?,lint:record[5]});},
                5=>{if override_address{return Err(Error::MalformedRecord);}override_address=true;result.local_apic=u64_at(record,4);},
                9=>result.add_processor(Processor{uid:u32_at(record,12),apic_id:u32_at(record,4),enabled:u32_at(record,8)&1!=0,online_capable:u32_at(record,8)&2!=0,x2apic:true})?,
                10=>{if record[8]>1{return Err(Error::MalformedRecord);}let uid=u32_at(record,4);result.local_nmis.push(LocalNmi{uid:if uid==u32::MAX{None}else{Some(uid)},flags:InterruptFlags::parse(u16_at(record,2))?,lint:record[8]});},
                _=>{}
            }
            offset+=length;
        }
        if result.local_apic==0||result.local_apic&4095!=0{return Err(Error::Address);}Ok(result)
    }
    fn add_processor(&mut self,processor:Processor)->Result<(),Error>{
        if self.processors.iter().any(|p|p.uid==processor.uid||p.apic_id==processor.apic_id){return Err(Error::DuplicateProcessor);}
        self.processors.push(processor);Ok(())
    }
    pub fn isa_route(&self,irq:u8)->(u32,Polarity,Trigger){
        if let Some(route)=self.overrides.iter().find(|o|o.bus==0&&o.source==irq){(route.gsi,if route.flags.polarity==Polarity::Conforms{Polarity::High}else{route.flags.polarity},if route.flags.trigger==Trigger::Conforms{Trigger::Edge}else{route.flags.trigger})}else{(irq as u32,Polarity::High,Trigger::Edge)}
    }
}

#[derive(Clone,Copy,Debug)]
pub struct PciConfigWindow {pub address:u64,pub segment:u16,pub first_bus:u8,pub last_bus:u8}
impl PciConfigWindow {
    pub fn function_address(self,bus:u8,device:u8,function:u8)->Option<u64>{
        if bus<self.first_bus||bus>self.last_bus||device>31||function>7{return None;}
        self.address.checked_add((bus as u64)<<20)?.checked_add((device as u64)<<15)?.checked_add((function as u64)<<12)
    }
}
fn parse_mcfg(table:&Table)->Result<Vec<PciConfigWindow>,Error>{
    let b=table.bytes;if b.len()<44||(b.len()-44)%16!=0{return Err(Error::Length);}
    let mut windows:Vec<PciConfigWindow>=Vec::new();
    for entry in b[44..].chunks_exact(16){
        let window=PciConfigWindow{address:u64_at(entry,0),segment:u16_at(entry,8),first_bus:entry[10],last_bus:entry[11]};
        if window.first_bus>window.last_bus||window.address==0||window.address&0xfffff!=0||window.function_address(window.last_bus,31,7).and_then(|n|n.checked_add(4096)).is_none(){return Err(Error::Address);}
        if windows.iter().any(|w|w.segment==window.segment&&w.first_bus<=window.last_bus&&window.first_bus<=w.last_bus){return Err(Error::OverlappingEcam);}
        windows.push(window);
    }Ok(windows)
}

pub struct FixedDescription {
    pub revision:u8,pub minor_version:u8,pub preferred_profile:u8,pub sci_interrupt:u16,pub smi_command:u32,pub acpi_enable:u8,pub acpi_disable:u8,
    pub firmware_control:u64,pub dsdt:u64,pub boot_architecture:u16,pub flags:u32,pub century_register:u8,
    pub pm1a_event:Option<GenericAddress>,pub pm1b_event:Option<GenericAddress>,pub pm1a_control:Option<GenericAddress>,pub pm1b_control:Option<GenericAddress>,
    pub pm2_control:Option<GenericAddress>,pub pm_timer:Option<GenericAddress>,pub gpe0:Option<GenericAddress>,pub gpe1:Option<GenericAddress>,pub gpe1_base:u8,
    pub reset:Option<GenericAddress>,pub reset_value:u8,pub sleep_control:Option<GenericAddress>,pub sleep_status:Option<GenericAddress>,
}
impl FixedDescription {
    fn parse(table:&Table)->Result<Self,Error>{
        let b=table.bytes;if b.len()<116{return Err(Error::Length);}
        let flags=u32_at(b,112);
        let address=|extended,legacy|->u64{if b.len()>=extended+8&&u64_at(b,extended)!=0{u64_at(b,extended)}else{u32_at(b,legacy)as u64}};
        let register=|extended:usize,legacy:usize,length:usize|->Result<Option<GenericAddress>,Error>{
            if let Some(gas)=GenericAddress::parse(b,extended)?{return Ok(Some(gas));}
            let width=b[length].checked_mul(8).unwrap_or(0);Ok(GenericAddress::legacy(u32_at(b,legacy),width))
        };
        Ok(Self{revision:table.revision,minor_version:if b.len()>131{b[131]}else{0},preferred_profile:b[45],sci_interrupt:u16_at(b,46),smi_command:u32_at(b,48),acpi_enable:b[52],acpi_disable:b[53],
            firmware_control:address(132,36),dsdt:address(140,40),boot_architecture:u16_at(b,109),flags,century_register:b[108],
            pm1a_event:register(148,56,88)?,pm1b_event:register(160,60,88)?,pm1a_control:register(172,64,89)?,pm1b_control:register(184,68,89)?,
            pm2_control:register(196,72,90)?,pm_timer:register(208,76,91)?,gpe0:register(220,80,92)?,gpe1:register(232,84,93)?,gpe1_base:b[94],
            reset:if flags&(1<<10)!=0{GenericAddress::parse(b,116)?}else{None},reset_value:if b.len()>128{b[128]}else{0},sleep_control:GenericAddress::parse(b,244)?,sleep_status:GenericAddress::parse(b,256)?})
    }
    pub fn hardware_reduced(&self)->bool{self.flags&(1<<20)!=0}
    pub fn timer_bits(&self)->u8{if self.flags&(1<<8)!=0{32}else{24}}
}

#[derive(Clone,Copy,Debug)]pub struct HpetDescription{pub block_id:u32,pub registers:GenericAddress,pub number:u8,pub minimum_tick:u16,pub page_protection:u8}
impl HpetDescription{
    fn parse(table:&Table)->Result<Self,Error>{let b=table.bytes;if b.len()<56{return Err(Error::Length);}let registers=GenericAddress::parse(b,40)?.ok_or(Error::Address)?;if registers.space!=0||registers.address&7!=0{return Err(Error::Address);}Ok(Self{block_id:u32_at(b,36),registers,number:b[52],minimum_tick:u16_at(b,53),page_protection:b[55]})}
}

pub struct Acpi {
    pub rsdp_address:u64,pub revision:u8,pub oem_id:[u8;6],pub root:Table,pub tables:Vec<Table>,
    pub interrupts:Option<InterruptTopology>,pub fixed:Option<FixedDescription>,pub pci_windows:Vec<PciConfigWindow>,pub hpet:Option<HpetDescription>,
}
impl Acpi {
    pub fn discover(mapper:&mut Mapper,memory:&mut Physical)->Result<Self,Error>{
        let address=boot::RSDP.get().ok_or(Error::Absent)?.address;
        let rsdp=view(mapper,memory,address,20)?;
        if &rsdp[..8]!=b"RSD PTR "{return Err(Error::Signature);}if !checksum(rsdp){return Err(Error::Checksum);}
        let revision=rsdp[15];let oem_id=rsdp[9..15].try_into().unwrap();let mut root_address=u32_at(rsdp,16)as u64;let mut extended=false;
        if revision>=2{
            let header=view(mapper,memory,address,36)?;let length=u32_at(header,20)as usize;if !(36..=4096).contains(&length){return Err(Error::Length);}
            let bytes=view(mapper,memory,address,length)?;if !checksum(bytes){return Err(Error::Checksum);}
            let xsdt=u64_at(bytes,24);if xsdt!=0{root_address=xsdt;extended=true;}
        }else if revision!=0{return Err(Error::Revision);}
        let root=Table::read(mapper,memory,root_address)?;
        if root.signature!=if extended{*b"XSDT"}else{*b"RSDT"}{return Err(Error::Signature);}
        let stride=if extended{8}else{4};if(root.length()-36)%stride!=0{return Err(Error::Length);}
        let mut tables:Vec<Table>=Vec::new();
        for entry in root.bytes[36..].chunks_exact(stride){let physical=if extended{u64_at(entry,0)}else{u32_at(entry,0)as u64};if physical==root_address||tables.iter().any(|t|t.physical==physical){return Err(Error::MalformedRecord);}let table=Table::read(mapper,memory,physical)?;tables.push(table);}
        let mut interrupts=None;let mut fixed=None;let mut pci_windows=Vec::new();let mut hpet=None;
        for table in &tables{match &table.signature{
            b"APIC"=>{if interrupts.is_some(){return Err(Error::MalformedRecord);}interrupts=Some(InterruptTopology::parse(table)?);},
            b"FACP"=>{if fixed.is_some(){return Err(Error::MalformedRecord);}fixed=Some(FixedDescription::parse(table)?);},
            b"MCFG"=>{if !pci_windows.is_empty(){return Err(Error::MalformedRecord);}pci_windows=parse_mcfg(table)?;},
            b"HPET"=>{if hpet.is_some(){return Err(Error::MalformedRecord);}hpet=Some(HpetDescription::parse(table)?);},_=>{}
        }}
        if let Some(fadt)=&fixed{
            if fadt.dsdt!=0&&!tables.iter().any(|t|t.physical==fadt.dsdt){let dsdt=Table::read(mapper,memory,fadt.dsdt)?;if dsdt.signature!=*b"DSDT"{return Err(Error::Signature);}tables.push(dsdt);}
            if fadt.firmware_control!=0{let facs=view(mapper,memory,fadt.firmware_control,8)?;if &facs[..4]!=b"FACS"{return Err(Error::Signature);}let length=u32_at(facs,4)as usize;if length<64{return Err(Error::Length);}view(mapper,memory,fadt.firmware_control,length)?;}
        }
        let result=Self{rsdp_address:address,revision,oem_id,root,tables,interrupts,fixed,pci_windows,hpet};
        hardware::log(format_args!("ACPI revision={} tables={} root={:#x}\n",result.revision,result.tables.len(),result.root.physical));
        for table in &result.tables{hardware::log(format_args!("ACPI {} rev={} bytes={} physical={:#x}\n",core::str::from_utf8(&table.signature).unwrap_or("????"),table.revision,table.length(),table.physical));}
        if let Some(topology)=&result.interrupts{hardware::log(format_args!("ACPI processors={} enabled={} IOAPICs={} LAPIC={:#x}\n",topology.processors.len(),topology.processors.iter().filter(|p|p.enabled).count(),topology.io_apics.len(),topology.local_apic));}
        Ok(result)
    }
    pub fn table(&self,signature:&[u8;4])->Option<&Table>{self.tables.iter().find(|t|&t.signature==signature)}
}
