use crate::hardware::{inb,outb};
#[derive(Clone,Copy,PartialEq)] pub enum Key{Char(char),Enter,Backspace,Escape,Tab,Left,Right,Up,Down,Delete,Home,End,Function(u8)}
pub enum Event{Pointer{i:i32,j:i32,buttons:u8},Keyboard{key:Key,ctrl:bool,alt:bool}}
pub struct Input{packet:[u8;3],index:usize,shift:bool,ctrl:bool,alt:bool,extended:bool,caps:bool,pub mouse:bool}
impl Default for Input{fn default()->Self{Self::new()}}
impl Input{
    pub fn new()->Self{Self{packet:[0;3],index:0,shift:false,ctrl:false,alt:false,extended:false,caps:false,mouse:false}}
    unsafe fn write(port:u16,value:u8)->bool{for _ in 0..100_000{if inb(0x64)&2==0{outb(port,value);return true;}}false}
    unsafe fn response()->Option<u8>{for _ in 0..100_000{if inb(0x64)&1!=0{return Some(inb(0x60));}}None}
    pub fn init(&mut self){unsafe{
        for _ in 0..64{if inb(0x64)&1==0{break;}let _=inb(0x60);}
        Self::write(0x64,0xa8);Self::write(0x64,0x20);
        if let Some(config)=Self::response(){Self::write(0x64,0x60);Self::write(0x60,(config|0x40)&!0x23);}
        for command in [0xf6,0xf4]{if !Self::write(0x64,0xd4)||!Self::write(0x60,command)||Self::response()!=Some(0xfa){return;}}
        self.mouse=true;
    }}
    pub fn poll(&mut self)->Option<Event>{unsafe{
        let status=inb(0x64);if status&1==0{return None;}let byte=inb(0x60);
        if status&0xc0!=0{self.index=0;return None;}
        if status&0x20!=0{
            if self.index==0&&byte&8==0{return None;}self.packet[self.index]=byte;self.index+=1;
            if self.index==3{self.index=0;let p=self.packet;if p[0]&0xc0==0{return Some(Event::Pointer{i:p[1]as i8 as i32,j:-(p[2]as i8 as i32),buttons:p[0]&7});}}
            return None;
        }
        if byte==0xe0{self.extended=true;return None;}
        let release=byte&128!=0;let code=byte&127;let extended=self.extended;self.extended=false;
        match code{42|54=>{self.shift=!release;return None;},29=>{self.ctrl=!release;return None;},56=>{self.alt=!release;return None;},58=>{if !release{self.caps=!self.caps;}return None;},_=>{}}
        if release{return None;}
        let key=match code{
            1=>Key::Escape,14=>Key::Backspace,15=>Key::Tab,28=>Key::Enter,59..=68=>Key::Function(code-58),87=>Key::Function(11),88=>Key::Function(12),
            71 if extended=>Key::Home,72 if extended=>Key::Up,75 if extended=>Key::Left,77 if extended=>Key::Right,79 if extended=>Key::End,80 if extended=>Key::Down,83 if extended=>Key::Delete,
            _=>{
                let ch=match code{2..=11=>b"1234567890"[(code-2)as usize],12=>b'-',13=>b'=',16..=25=>b"qwertyuiop"[(code-16)as usize],26=>b'[',27=>b']',30..=38=>b"asdfghjkl"[(code-30)as usize],39=>b';',40=>b'\'',41=>b'`',43=>b'\\',44..=50=>b"zxcvbnm"[(code-44)as usize],51=>b',',52=>b'.',53=>b'/',57=>b' ',_=>return None};
                let ch=if ch.is_ascii_alphabetic(){if self.shift^self.caps{ch.to_ascii_uppercase()}else{ch}}else if self.shift{match ch{b'1'=>b'!',b'2'=>b'@',b'3'=>b'#',b'4'=>b'$',b'5'=>b'%',b'6'=>b'^',b'7'=>b'&',b'8'=>b'*',b'9'=>b'(',b'0'=>b')',b'-'=>b'_',b'='=>b'+',b'['=>b'{',b']'=>b'}',b';'=>b':',b'\''=>b'"',b'`'=>b'~',b'\\'=>b'|',b','=>b'<',b'.'=>b'>',b'/'=>b'?',_=>ch}}else{ch};Key::Char(ch as char)
            }
        };Some(Event::Keyboard{key,ctrl:self.ctrl,alt:self.alt})
    }}
}
