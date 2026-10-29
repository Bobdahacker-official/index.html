use alloc::{string::{String,ToString},vec::Vec,format};
use core::sync::atomic::Ordering;
use crate::{apps::{Files,Editor,Terminal,calculate},graphics::{Canvas,Rect,mix},hardware,input::{Input,Event,Key},memory::{Physical,ALLOCATOR},storage::{Block,AvaFs}};
const TITLES:[&str;8]=["Welcome","Files","Terminal","Text Editor","System Monitor","Calculator","Settings","Paint"];
const COLORS:[u32;8]=[0xb9b0ff,0x82b8ff,0xa1e2c1,0xf0d394,0x9dd8f0,0xe2b3ff,0xc2cadb,0xf3a8b9];
struct Stroke{points:Vec<(i32,i32)>,color:u32}
enum Content{Welcome,Files{selected:usize},Terminal(Terminal),Editor(Editor),Monitor{tab:usize},Calculator{expression:String,result:String},Settings,Paint{strokes:Vec<Stroke>,color:u32}}
struct Window{app:usize,rect:Rect,restore:Option<Rect>,minimized:bool,workspace:usize,content:Content}
#[derive(Clone,Copy)]enum Drag{None,Move(i32,i32),Resize(i32,i32),Paint}
pub struct Shell{canvas:Canvas,platform:crate::firmware::Platform,wall:Vec<u32>,windows:Vec<Window>,files:Files,disk:Option<AvaFs>,devices:Vec<hardware::Pci>,total:u64,free:u64,hz:u64,start:u64,mx:i32,my:i32,buttons:u8,drag:Drag,launcher:bool,query:String,quick:bool,light:bool,wallpaper:usize,workspace:usize,toast:String,toast_until:u64,frames:u64,frame_ms:u64,paint_color:usize}
fn rect(x:i32,y:i32,w:i32,h:i32)->Rect{Rect{x,y,w,h}}
fn button(c:&mut Canvas,r:Rect,label:&str,active:bool,light:bool){let bg=if active{0xa99cf4}else if light{0xe2e6ef}else{0x293448};c.round(r,8,bg,255);let color=if active||light{0x151c2b}else{0xe5e9f2};c.text(r.x+(r.w-Canvas::width(label,0))/2,r.y+(r.h-19)/2,label,0,color);}
fn mountain(x:i32,points:&[(i32,i32)])->i32{for p in points.windows(2){if x>=p[0].0&&x<=p[1].0{return p[0].1+(p[1].1-p[0].1)*(x-p[0].0)/(p[1].0-p[0].0);}}600}
impl Shell{
    pub fn new(canvas:Canvas,physical:&mut Physical,platform:crate::firmware::Platform)->Self{
        let hz=hardware::frequency();let devices=hardware::pci();let mut files=Files::new();
        let disk=Block::init(physical,&devices).and_then(|b|match AvaFs::mount(b,&mut files){Ok(fs)=>Some(fs),Err(e)=>{hardware::log(format_args!("Storage: {}\n",e));None}});
        let light=platform.boot.command_line.get("theme")==Some("light");
        let wallpaper=match platform.boot.command_line.get("wallpaper"){Some("rose")=>1,Some("forest")=>2,_=>0};
        let initial_app=match platform.boot.command_line.get("init"){Some("files")=>1,Some("terminal")=>2,Some("editor")=>3,Some("monitor")=>4,Some("calculator")=>5,Some("settings")=>6,Some("paint")=>7,_=>0};
        let mut shell=Self{canvas,platform,wall:Vec::new(),windows:Vec::new(),files,disk,devices,total:physical.total,free:physical.free,hz,start:hardware::cycles(),mx:900,my:600,buttons:0,drag:Drag::None,launcher:false,query:String::new(),quick:false,light,wallpaper,workspace:0,toast:String::new(),toast_until:0,frames:0,frame_ms:0,paint_color:0};
        shell.wallpaper();shell.open(initial_app);shell
    }
    fn elapsed(&self)->u64{(hardware::cycles()-self.start)/self.hz}
    fn notify(&mut self,s:&str){self.toast=s.to_string();self.toast_until=self.elapsed()+5;}
    fn wallpaper(&mut self){let w=self.canvas.w;let h=self.canvas.h;self.wall=alloc::vec![0;w*h];let palettes=[(0x10162d,0x63759a,0x233650,0x172536,0x0c1927),(0x211529,0xb2847e,0x664d69,0x3e354f,0x211c35),(0x081e25,0x619c99,0x285760,0x163c48,0x0c252e)];let(top,bottom,far,mid,near)=palettes[self.wallpaper];
        let a=[(0,490),(90,420),(180,462),(310,285),(360,330),(410,300),(555,460),(690,390),(800,470),(945,320),(1040,390),(1140,340),(1280,460)];
        let b=[(0,590),(150,490),(260,535),(415,415),(540,545),(620,500),(765,575),(930,465),(1080,570),(1280,490)];
        let d=[(0,650),(210,610),(360,685),(555,595),(710,665),(900,615),(1080,690),(1280,595)];
        for x in 0..w{let xx=x as i32*1280/w as i32;let ya=mountain(xx,&a)*h as i32/800;let yb=mountain(xx,&b)*h as i32/800;let yd=mountain(xx,&d)*h as i32/800;for y in 0..h{let mut color=mix(top,bottom,(y*255/h)as u32);let glow=(x as i64-w as i64*3/5).abs();let gy=(y as i64-h as i64/3).abs();let weight=(90-glow*80/w as i64-gy*160/h as i64).max(0)as u32;color=mix(color,if self.wallpaper==1{0xedb6a2}else{0xa2a3d6},weight);
            if y as i32>=ya{color=mix(far,mid,((y as i32-ya)*100/h as i32).min(100)as u32);let ridge=(y as i32-ya)*1280/w as i32;if ridge<12+(xx%19)&&ya<h as i32*48/100{color=mix(color,0xbbc6db,(100-ridge*4).max(0)as u32);}}
            if y as i32>=yb{color=mid;}if y as i32>=yd{color=near;}
            self.wall[y*w+x]=color;
        }}
    }
    fn open(&mut self,app:usize){
        self.launcher=false;self.query.clear();
        if let Some(i)=self.windows.iter().position(|w|w.app==app&&w.workspace==self.workspace){let mut window=self.windows.remove(i);window.minimized=false;self.windows.push(window);return;}
        let(w,h)=match app{0=>(780,500),5=>(400,540),_=>(860,540)};let w=w.min(self.canvas.w as i32-48);let h=h.min(self.canvas.h as i32-140);let offset=self.windows.len()as i32%4*22;
        let content=match app{0=>Content::Welcome,1=>Content::Files{selected:0},2=>Content::Terminal(Terminal::default()),3=>Content::Editor(Editor::default()),4=>Content::Monitor{tab:0},5=>Content::Calculator{expression:String::new(),result:"0".to_string()},6=>Content::Settings,_=>Content::Paint{strokes:Vec::new(),color:0x263246}};
        self.windows.push(Window{app,rect:rect(((self.canvas.w as i32-w)/2+offset).min(self.canvas.w as i32-w),((self.canvas.h as i32-h)/2-12+offset).max(48),w,h),restore:None,minimized:false,workspace:self.workspace,content});
    }
    fn active(&self)->Option<usize>{self.windows.iter().rposition(|w|!w.minimized&&w.workspace==self.workspace)}
    fn maximize(&mut self,i:usize){let win=&mut self.windows[i];if let Some(r)=win.restore.take(){win.rect=r;}else{win.restore=Some(win.rect);win.rect=rect(12,46,self.canvas.w as i32-24,self.canvas.h as i32-138);}}
    fn persist(&mut self){if let Some(disk)=&mut self.disk{match disk.save(&self.files){Ok(())=>self.notify("Saved to AvaFS"),Err(e)=>self.notify(e)}}else{self.notify("Saved in memory; no AvaFS disk is attached");}}
    fn save_editor(&mut self,index:usize){if let Content::Editor(e)=&mut self.windows[index].content{if let Err(error)=self.files.save(&e.name,&e.text){self.notify(error);return;}e.dirty=false;}self.persist();}
    fn load_document(&mut self,name:&str){if let Some(text)=self.files.get(name){let editor=Editor::new(name,text);self.open(3);if let Some(i)=self.active(){self.windows[i].content=Content::Editor(editor);}}}
    fn dock(&self)->Rect{rect((self.canvas.w as i32-532)/2,self.canvas.h as i32-80,532,64)}
    fn draw(&mut self){let begin=hardware::cycles();let uptime=self.elapsed();self.canvas.clip=None;self.canvas.pixels.copy_from_slice(&self.wall);let c=&mut self.canvas;let w=c.w as i32;let h=c.h as i32;
        c.rect(rect(0,0,w,36),0x0c1322,155);c.icon(18,9,0,1,0xddd8ff);c.text(43,6,"Avalanche",0,0xf2f3fb);c.text(140,6,"Shell",0,0xb0bbd1);
        for i in 0..3{c.round(rect(w/2-38+i*26,14,if i==self.workspace as i32{18}else{6},6),3,if i==self.workspace as i32{0xc1b7ff}else{0x67748b},255);}
        let(hour,min,_)=hardware::time();let clock=format!("{:02}:{:02}  UTC",hour,min);c.text(w-132,6,&clock,0,0xdfe6f3);c.icon(w-32,9,6,1,0xcbd5e7);
        c.text(34,74,"YOUR SPACE",0,0xabb9d1);c.text(32,96,"Make room for more.",2,0xe4e9f5);
        c.text(34,h-125,"AVALANCHE / 01",0,0x8598b2);
        let active=self.windows.iter().rposition(|win|!win.minimized&&win.workspace==self.workspace);
        for(i,window)in self.windows.iter().enumerate(){if window.minimized||window.workspace!=self.workspace{continue;}let r=window.rect;c.shadow(r);c.round(r,14,if self.light{0xeef1f7}else{0x151e2e},255);c.round(rect(r.x+1,r.y+1,r.w-2,46),13,if self.light{0xf7f8fc}else{0x202a3c},255);c.rect(rect(r.x+1,r.y+29,r.w-2,18),if self.light{0xf7f8fc}else{0x202a3c},255);
            c.icon(r.x+16,r.y+14,window.app,1,COLORS[window.app]);c.text(r.x+41,r.y+12,TITLES[window.app],0,if self.light{0x233148}else if Some(i)==active{0xf3f5fa}else{0x9ba9bf});
            for k in 0..3{let x=r.x+r.w-96+k*28;c.round(rect(x,r.y+10,22,25),6,if k==2&&Some(i)==active{0x463044}else if self.light{0xe7eaf1}else{0x273246},255);match k{0=>c.line(x+7,r.y+24,x+15,r.y+24,0xabb7cb),1=>{c.line(x+7,r.y+18,x+15,r.y+18,0xabb7cb);c.line(x+7,r.y+18,x+7,r.y+25,0xabb7cb);c.line(x+15,r.y+18,x+15,r.y+25,0xabb7cb);c.line(x+7,r.y+25,x+15,r.y+25,0xabb7cb);},_=>{c.line(x+7,r.y+18,x+15,r.y+25,0xe7acb8);c.line(x+15,r.y+18,x+7,r.y+25,0xe7acb8);}}}
            c.clip=Some(rect(r.x+1,r.y+47,r.w-2,r.h-49));Self::content(c,window,&self.files,&self.devices,self.total,self.free,self.light,self.wallpaper,self.frame_ms,self.frames,self.disk.is_some(),uptime,&self.platform);c.clip=None;
            c.line(r.x+r.w-12,r.y+r.h-5,r.x+r.w-5,r.y+r.h-12,0x66738b);
        }
        let dock=rect((w-532)/2,h-80,532,64);c.shadow(dock);c.glass(dock);c.round(rect(dock.x+1,dock.y+1,dock.w-2,1),0,0xbac8df,65);
        for app in 0..8{let x=dock.x+14+app as i32*64;let hovered=rect(x,dock.y+6,56,52).contains(self.mx,self.my);if hovered{c.round(rect(x,dock.y+6,56,49),10,0xc1cce0,25);}c.round(rect(x+11,dock.y+9,34,34),10,COLORS[app],30);c.icon(x+15,dock.y+11,app,2,COLORS[app]);if self.windows.iter().any(|win|win.app==app&&win.workspace==self.workspace){c.round(rect(x+25,dock.y+51,6,3),1,0xd2caff,255);}if hovered{let tw=Canvas::width(TITLES[app],0)+20;c.round(rect(x+28-tw/2,dock.y-35,tw,26),7,0x192335,255);c.text(x+28-tw/2+10,dock.y-32,TITLES[app],0,0xe8edf5);}}
        if self.launcher{let r=rect((w-560)/2,(h-420)/2,560,420);c.shadow(r);c.glass(r);c.text(r.x+26,r.y+21,"Find your next idea.",2,0xf0f2fb);c.round(rect(r.x+24,r.y+73,512,42),10,0x101a2c,255);c.text(r.x+40,r.y+83,&format!("> {}_",self.query),1,0xcac3ff);let mut n=0;for(app,title)in TITLES.iter().enumerate(){if title.to_ascii_lowercase().contains(&self.query.to_ascii_lowercase()){let x=r.x+24+(n%3)*174;let y=r.y+141+(n/3)*87;c.round(rect(x,y,164,75),10,0x6a7792,26);c.icon(x+14,y+14,app,2,COLORS[app]);c.text(x+14,y+49,title,0,0xe0e7f4);n+=1;}}}
        if self.quick{let r=rect(w-340,46,324,260);c.shadow(r);c.glass(r);c.text(r.x+22,r.y+18,"Control center",1,0xf1f3fb);button(c,rect(r.x+20,r.y+65,136,42),"Dark",!self.light,false);button(c,rect(r.x+168,r.y+65,136,42),"Light",self.light,false);c.text(r.x+22,r.y+130,&format!("Memory   {} MiB available",self.free>>20),0,0xb9c7dc);c.text(r.x+22,r.y+158,if self.disk.is_some(){"Storage   AvaFS connected"}else{"Storage   Memory session"},0,0xb9c7dc);c.text(r.x+22,r.y+204,"F1 launcher  /  Alt+Tab switch",0,0x919fb8);}
        if !self.toast.is_empty()&&uptime<self.toast_until{let tw=Canvas::width(&self.toast,0)+40;let r=rect(w-tw-20,h-134,tw,40);c.shadow(r);c.round(r,10,0x26374b,255);c.text(r.x+20,r.y+10,&self.toast,0,0xe3ecf7);}
        c.cursor(self.mx,self.my);c.present();self.frames+=1;self.frame_ms=(hardware::cycles()-begin)*1000/self.hz;
    }
    #[allow(clippy::too_many_arguments)]
    fn content(c:&mut Canvas,window:&Window,files:&Files,devices:&[hardware::Pci],total:u64,free:u64,light:bool,wallpaper:usize,frame_ms:u64,frames:u64,durable:bool,uptime:u64,platform:&crate::firmware::Platform){
        let r=window.rect;let x=r.x+24;let y=r.y+65;let text=if light{0x233148}else{0xe0e6f1};let muted=if light{0x69758b}else{0x8c9cb5};let panel=if light{0xe3e8f1}else{0x202b3d};
        match &window.content{
            Content::Welcome=>{c.round(rect(x,y,118,25),12,0xb2a5f7,24);c.text(x+12,y+3,"NATIVE / x86_64",0,0xbcaefb);c.text(x,y+49,"A little space.",3,text);c.text(x,y+103,"A lot of possibility.",3,text);c.text(x,y+177,"An original kernel. A desktop you can make your own.",1,muted);c.text(x,y+207,"Explore the shell, write a note, or create something new.",0,muted);
                button(c,rect(x,y+252,166,42),"Explore your files",true,light);button(c,rect(x+180,y+252,150,42),"Make it yours",false,light);
                for(i,(label,value))in [("MEMORY",format!("{} MiB",total>>20)),("ARCHITECTURE","64-bit Rust".to_string()),("SESSION",if durable{"AvaFS disk".to_string()}else{"Memory".to_string()})].iter().enumerate(){let xx=x+i as i32*(r.w-48)/3;c.round(rect(xx,y+330,(r.w-64)/3,70),9,panel,255);c.text(xx+15,y+341,label,0,muted);c.text(xx+15,y+365,value,1,text);}
            },
            Content::Files{selected}=>{c.rect(rect(r.x+1,r.y+47,170,r.h-48),if light{0xe4e9f2}else{0x192436},255);c.text(r.x+20,y,"PLACES",0,muted);c.round(rect(r.x+12,y+38,146,36),7,0x9c8fe1,32);c.icon(r.x+22,y+47,1,1,0xb7aaff);c.text(r.x+48,y+46,"Documents",0,text);c.text(r.x+20,r.y+r.h-62,if durable{"AvaFS volume"}else{"Memory session"},0,muted);let xx=r.x+194;c.text(xx,y,"Documents",2,text);button(c,rect(r.x+r.w-122,y,98,32),"New note",true,light);c.text(xx,y+48,"NAME",0,muted);c.text(r.x+r.w-110,y+48,"SIZE",0,muted);
                if files.entries.is_empty(){c.icon(xx+50,y+116,1,4,0x53657f);c.text(xx+50,y+203,"A clean slate.",2,text);c.text(xx+50,y+244,"Create a note to start your collection.",0,muted);}
                for(i,d)in files.entries.iter().enumerate(){let yy=y+78+i as i32*43;if i==*selected{c.round(rect(xx-8,yy-4,r.w-214,38),7,0xab9ff1,26);}c.icon(xx,yy+4,3,1,0xe1c794);c.text(xx+30,yy+2,&d.name,0,text);c.text(r.x+r.w-110,yy+2,&format!("{} B",d.text.len()),0,muted);}
                c.text(xx,r.y+r.h-34,&format!("{} items  /  Enter to open  /  Delete to remove",files.entries.len()),0,muted);
            },
            Content::Terminal(t)=>{c.rect(rect(r.x+1,r.y+47,r.w-2,r.h-48),0x0d1523,255);let max=((r.h-110)/22).max(1)as usize;let start=t.lines.len().saturating_sub(max);for(i,line)in t.lines[start..].iter().enumerate(){c.text(x,y+i as i32*22,line,0,if line.starts_with("guest@"){0xa9d5bc}else{0xb5c2d7});}let yy=y+(t.lines.len()-start)as i32*22;c.text(x,yy,&format!("guest@avalanche ~ $ {}_",t.input),0,0xc5b8ff);},
            Content::Editor(e)=>{c.rect(rect(r.x+1,r.y+47,r.w-2,37),panel,255);c.text(x,r.y+54,&format!("{}{}",e.name,if e.dirty{" *"}else{""}),0,text);c.text(r.x+r.w-230,r.y+54,"Ctrl+S save  /  Ctrl+F find",0,muted);let lines=((r.h-135)/23).max(1)as usize;let(row,col)=e.row_col();let first=if row<e.scroll{row}else if row>=e.scroll+lines{row-lines+1}else{e.scroll};let mut by=r.y+97;for(i,line)in e.text.split('\n').enumerate().skip(first).take(lines){c.text(r.x+15,by,&format!("{:>3}",i+1),0,muted);if i==row{c.rect(rect(r.x+58,by-1,r.w-70,23),panel,160);}let color=if line.trim_start().starts_with("//")||line.trim_start().starts_with('#'){0x7fa693}else if line.contains("fn ")||line.contains("let "){0xc3b0f4}else{text};c.text(r.x+66,by,line,0,color);if row==i{let prefix: String=line.chars().take(col).collect();let cx=r.x+66+Canvas::width(&prefix,0);c.rect(rect(cx,by+2,2,18),0xb9a8ff,255);}by+=23;}c.rect(rect(r.x+1,r.y+r.h-31,r.w-2,30),panel,255);c.text(x,r.y+r.h-25,&format!("Ln {}, Col {}    UTF-8    {} bytes",row+1,col+1,e.text.len()),0,muted);if e.searching{c.round(rect(r.x+r.w-330,r.y+87,310,40),8,0x354159,255);c.text(r.x+r.w-316,r.y+98,&format!("Find: {}_",e.find),0,0xf3f4fa);}},
            Content::Monitor{tab}=>{c.text(x,y,"Inside your Avalanche",2,text);button(c,rect(x,y+43,140,30),"Memory & PCI",*tab==0,light);button(c,rect(x+150,y+43,140,30),"Firmware",*tab==1,light);
                if *tab==1{
                    let acpi=&platform.acpi;let cpu_count=acpi.interrupts.as_ref().map_or(0,|t|t.processors.iter().filter(|p|p.enabled).count());
                    c.text(x,y+96,&format!("ACPI revision {}    /    {} processors    /    {} ECAM regions",acpi.revision,cpu_count,acpi.pci_windows.len()),0,text);
                    c.text(x,y+125,&format!("UEFI descriptors: {}    Runtime memory: {} KiB",platform.boot.efi_memory.len(),platform.boot.runtime_bytes()/1024),0,muted);
                    c.text(x,y+155,&format!("Boot: {}",platform.boot.command_line.raw),0,muted);
                    c.text(x,y+202,"TABLE       REV     BYTES          PHYSICAL ADDRESS",0,muted);
                    for(i,table)in acpi.tables.iter().enumerate(){let yy=y+233+i as i32*23;if yy>r.y+r.h-35{break;}c.text(x,yy,&format!("{}          {:>2}       {:>6}       {:#014x}",core::str::from_utf8(&table.signature).unwrap_or("????"),table.revision,table.length(),table.physical),0,text);}
                    return;
                }
let used=total-free;c.round(rect(x,y+85,r.w-48,110),10,panel,255);c.text(x+18,y+99,"PHYSICAL MEMORY",0,muted);c.text(x+18,y+126,&format!("{} MiB / {} MiB",used>>20,total>>20),2,text);c.round(rect(x+18,y+171,r.w-84,6),3,0x3c485c,255);c.round(rect(x+18,y+171,((r.w-84)as u64*used/total.max(1)).max(4)as i32,6),3,0xb5a7fc,255);
                c.text(x,y+218,&format!("Heap allocations   {} KiB",ALLOCATOR.used.load(Ordering::Relaxed)>>10),0,text);c.text(x+350,y+218,&format!("Uptime   {}m {}s",uptime/60,uptime%60),0,text);c.text(x,y+248,&format!("Last redraw   {} ms",frame_ms),0,text);c.text(x+350,y+248,&format!("Presented frames   {}",frames),0,text);c.text(x,y+291,"PCI DEVICES",0,muted);
                for(i,d)in devices.iter().take(6).enumerate(){c.text(x,y+318+i as i32*22,&format!("{:02x}:{:02x}.{}   {:04x}:{:04x}   class {:02x}:{:02x}",d.bus,d.slot,d.function,d.vendor,d.device,d.class,d.subclass),0,text);}
            },
            Content::Calculator{expression,result}=>{c.text(x,y,expression,1,muted);let shown=if result.len()>22{&result[..22]}else{result.as_str()};c.text(x,y+38,shown,2,text);let labels=["C","(",")","/","7","8","9","*","4","5","6","-","1","2","3","+","0",".","DEL","="];let bw=(r.w-60)/4;let bh=((r.h-195)/5).max(32);for(i,label)in labels.iter().enumerate(){button(c,rect(x+(i%4)as i32*(bw+4),y+109+(i/4)as i32*(bh+4),bw,bh),label,*label=="=",light);}},
            Content::Settings=>{c.text(x,y,"Make it yours.",2,text);c.text(x,y+47,"APPEARANCE",0,muted);button(c,rect(x,y+82,170,80),"Dark",!light,light);button(c,rect(x+188,y+82,170,80),"Light",light,light);c.text(x,y+194,"LANDSCAPES",0,muted);for(i,label)in ["Alpine dusk","Rose horizon","Quiet forest"].iter().enumerate(){let xx=x+i as i32*200;c.round(rect(xx,y+232,184,74),10,[0x354964,0x8a6572,0x306369][i],255);if i==wallpaper{c.text(xx+154,y+240,"+",1,0xffffff);}c.text(xx+12,y+273,label,0,0xf5f5fb);}c.text(x,y+348,"KEYBOARD SHORTCUTS",0,muted);c.text(x,y+381,"F1  Launcher       Alt+Tab  Switch windows       F11  Maximize",0,text);c.text(x,y+409,"Alt+F4  Close       Ctrl+S  Save       Ctrl+Z / Ctrl+Y  Undo / Redo",0,text);},
            Content::Paint{strokes,color}=>{let palette=[0x263246,0x7756ba,0x527ca6,0x47998c,0xd88574,0xffffff];for(i,col)in palette.iter().enumerate(){c.round(rect(x+i as i32*38,y,28,28),14,*col,255);if col==color{c.round(rect(x+i as i32*38+10,y+10,8,8),4,if *col==0xffffff{0x000000}else{0xffffff},255);}}button(c,rect(r.x+r.w-218,y,90,30),"Clear",false,light);button(c,rect(r.x+r.w-118,y,94,30),"Save SVG",true,light);let area=rect(x,y+50,r.w-48,r.h-140);c.rect(area,0xf6f3ed,255);c.clip=Some(area);for stroke in strokes{for pair in stroke.points.windows(2){let(a,b)=(pair[0],pair[1]);for offset in 0..3{c.line(area.x+a.0,area.y+a.1+offset,area.x+b.0,area.y+b.1+offset,stroke.color);}}}c.clip=Some(rect(r.x+1,r.y+47,r.w-2,r.h-49));c.text(x,r.y+r.h-30,"A small canvas for a big idea. Drag to draw.",0,muted);}
        }
    }
    fn click(&mut self){let(mx,my)=(self.mx,self.my);let w=self.canvas.w as i32;let h=self.canvas.h as i32;
        if my<36{if mx<190{self.launcher=!self.launcher;self.quick=false;}else if mx>w-160{self.quick=!self.quick;self.launcher=false;}else if (w/2-50..w/2+70).contains(&mx){self.workspace=((mx-(w/2-38)).max(0)/26).min(2)as usize;}return;}
        if self.quick{let r=rect(w-340,46,324,260);if r.contains(mx,my){if (r.y+65..r.y+107).contains(&my){self.light=mx>=r.x+162;}return;}self.quick=false;}
        let dock=self.dock();if dock.contains(mx,my){let app=((mx-dock.x-14).max(0)/64).min(7)as usize;self.open(app);return;}
        if self.launcher{let r=rect((w-560)/2,(h-420)/2,560,420);if !r.contains(mx,my){self.launcher=false;return;}let mut n=0;for(app,title)in TITLES.iter().enumerate(){if title.to_ascii_lowercase().contains(&self.query.to_ascii_lowercase()){let area=rect(r.x+24+(n%3)*174,r.y+141+(n/3)*87,164,75);if area.contains(mx,my){self.open(app);return;}n+=1;}}return;}
        let target=self.windows.iter().rposition(|win|!win.minimized&&win.workspace==self.workspace&&win.rect.contains(mx,my));let Some(i)=target else{return;};let win=self.windows.remove(i);self.windows.push(win);let i=self.windows.len()-1;let r=self.windows[i].rect;
        if my<r.y+46{if mx>=r.x+r.w-40{self.windows.pop();}else if mx>=r.x+r.w-68{self.maximize(i);}else if mx>=r.x+r.w-96{self.windows[i].minimized=true;}else{self.drag=Drag::Move(mx-r.x,my-r.y);}return;}
        if mx>r.x+r.w-18&&my>r.y+r.h-18{self.drag=Drag::Resize(r.x,r.y);return;}
        let x=r.x+24;let y=r.y+65;let mut open=None;let mut load=None;let mut save_paint=false;
        match &mut self.windows[i].content{
            Content::Welcome=>{if rect(x,y+252,166,42).contains(mx,my){open=Some(1);}if rect(x+180,y+252,150,42).contains(mx,my){open=Some(6);}},
            Content::Files{selected}=>{if rect(r.x+r.w-122,y,98,32).contains(mx,my){open=Some(3);}else if mx>r.x+180&&my>=y+74{let row=((my-y-74)/43)as usize;if row<self.files.entries.len(){if *selected==row{load=Some(self.files.entries[row].name.clone());}*selected=row;}}},
            Content::Calculator{expression,result}=>{let labels=["C","(",")","/","7","8","9","*","4","5","6","-","1","2","3","+","0",".","DEL","="];let bw=(r.w-60)/4;let bh=((r.h-195)/5).max(32);for(k,label)in labels.iter().enumerate(){if rect(x+(k%4)as i32*(bw+4),y+109+(k/4)as i32*(bh+4),bw,bh).contains(mx,my){match *label{"C"=>{expression.clear();*result="0".to_string();},"DEL"=>{expression.pop();},"="=>*result=match calculate(expression){Ok(v)=>format!("{}",v),Err(e)=>e.to_string()},_=>{if expression.len()<128{expression.push_str(label);}}}}}},
            Content::Monitor{tab}=>{if rect(x,y+43,140,30).contains(mx,my){*tab=0;}if rect(x+150,y+43,140,30).contains(mx,my){*tab=1;}},
            Content::Settings=>{if rect(x,y+82,170,80).contains(mx,my){self.light=false;}if rect(x+188,y+82,170,80).contains(mx,my){self.light=true;}for k in 0..3{if rect(x+k*200,y+232,184,74).contains(mx,my){self.wallpaper=k as usize;}}},
            Content::Paint{strokes,color}=>{for(k,col)in [0x263246,0x7756ba,0x527ca6,0x47998c,0xd88574,0xffffff].iter().enumerate(){if rect(x+k as i32*38,y,28,28).contains(mx,my){*color=*col;self.paint_color=k;}}if rect(r.x+r.w-218,y,90,30).contains(mx,my){strokes.clear();}if rect(r.x+r.w-118,y,94,30).contains(mx,my){save_paint=true;}if rect(x,y+50,r.w-48,r.h-140).contains(mx,my){strokes.push(Stroke{points:alloc::vec![(mx-x,my-y-50)],color:*color});self.drag=Drag::Paint;}},_=>{}
        }
        if self.windows[i].app==6{self.wallpaper();}
        if let Some(app)=open{self.open(app);}if let Some(name)=load{self.load_document(&name);}
        if save_paint{if let Content::Paint{strokes,..}=&self.windows[i].content{let mut svg=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",r.w-48,r.h-140,r.w-48,r.h-140);for stroke in strokes{svg.push_str(&format!("<polyline fill=\"none\" stroke=\"#{:06x}\" stroke-width=\"3\" points=\"",stroke.color));for(a,b)in &stroke.points{svg.push_str(&format!("{},{} ",a,b));}svg.push_str("\"/>\n");}svg.push_str("</svg>\n");let _=self.files.save("Drawing.svg",&svg);self.persist();}}
    }
    fn keyboard(&mut self,key:Key,ctrl:bool,alt:bool){
        if key==Key::Function(1){self.launcher=!self.launcher;self.query.clear();return;}
        if alt&&key==Key::Tab{if let Some(i)=self.active(){let w=self.windows.remove(i);self.windows.insert(0,w);}return;}
        if alt&&key==Key::Function(4){if let Some(i)=self.active(){self.windows.remove(i);}return;}
        if key==Key::Escape{self.launcher=false;self.quick=false;}
        if self.launcher{match key{Key::Char(c)if self.query.len()<80=>self.query.push(c),Key::Backspace=>{self.query.pop();},Key::Enter=>{if let Some(app)=TITLES.iter().position(|t|t.to_ascii_lowercase().contains(&self.query.to_ascii_lowercase())){self.open(app);}},_=>{}}return;}
        let Some(i)=self.active()else{return;};if key==Key::Function(11){self.maximize(i);return;}
        if ctrl&&key==Key::Char('s')&&self.windows[i].app==3{self.save_editor(i);return;}
        let uptime=self.elapsed();let mut load=None;let mut persist=false;
        match &mut self.windows[i].content{
            Content::Editor(e)=>{if ctrl{match key{Key::Char('z')=>e.undo(),Key::Char('y')=>e.redo(),Key::Char('f')=>{e.searching=true;e.find.clear();},Key::Char('n')=>*e=Editor::new(&format!("Note {}.txt",self.files.entries.len()+1),""),_=>{}}}else{e.key(key);}},
            Content::Terminal(t)=>match key{Key::Char('l')if ctrl=>t.lines.clear(),Key::Char(c)if !ctrl&&t.input.len()<512=>t.input.push(c),Key::Backspace=>{t.input.pop();},Key::Enter=>{let rev=self.files.revision;t.execute(&mut self.files,self.total,self.free,uptime);persist=self.files.revision!=rev;},Key::Up=>t.history(true),Key::Down=>t.history(false),_=>{}},
            Content::Calculator{expression,result}=>match key{Key::Char(c)if expression.len()<128=>expression.push(c),Key::Backspace=>{expression.pop();},Key::Enter=>*result=match calculate(expression){Ok(v)=>format!("{}",v),Err(e)=>e.to_string()},Key::Escape=>{expression.clear();*result="0".to_string();},_=>{}},
            Content::Files{selected}=>match key{Key::Up=>*selected=selected.saturating_sub(1),Key::Down=>*selected=(*selected+1).min(self.files.entries.len().saturating_sub(1)),Key::Enter=>{load=self.files.entries.get(*selected).map(|d|d.name.clone());},Key::Delete=>{if let Some(name)=self.files.entries.get(*selected).map(|d|d.name.clone()){self.files.remove(&name);persist=true;}},_=>{}},_=>{}
        }
        if persist{self.persist();}if let Some(name)=load{self.load_document(&name);}
    }
    pub fn run(mut self)->!{let mut input=Input::new();input.init();hardware::log(format_args!("Shell ready: {}x{}; mouse={}; PCI={}; AvaFS={}\n",self.canvas.w,self.canvas.h,input.mouse,self.devices.len(),self.disk.is_some()));self.mx=(self.canvas.w/2)as i32;self.my=(self.canvas.h*3/4)as i32;let mut dirty=true;let mut last_frame=0;let mut last_second=0;
        loop{let now=hardware::cycles();let second=self.elapsed();if second!=last_second{last_second=second;dirty=true;}
            for _ in 0..64{if let Some(event)=input.poll(){dirty=true;match event{Event::Keyboard{key,ctrl,alt}=>self.keyboard(key,ctrl,alt),Event::Pointer{i,j,buttons}=>{self.mx=(self.mx+i).clamp(0,self.canvas.w as i32-1);self.my=(self.my+j).clamp(0,self.canvas.h as i32-1);if buttons&1!=0&&self.buttons&1==0{self.click();}if buttons&1==0{self.drag=Drag::None;}else if let Some(index)=self.active(){match self.drag{Drag::Move(dx,dy)=>{let win=&mut self.windows[index];win.rect.x=(self.mx-dx).clamp(-win.rect.w+100,self.canvas.w as i32-100);win.rect.y=(self.my-dy).clamp(37,self.canvas.h as i32-100);},Drag::Resize(x,y)=>{let win=&mut self.windows[index];win.rect.w=(self.mx-x).clamp(360,(self.canvas.w as i32-x).max(360));win.rect.h=(self.my-y).clamp(280,(self.canvas.h as i32-y-85).max(280));},Drag::Paint=>{let r=self.windows[index].rect;if rect(r.x+24,r.y+115,r.w-48,r.h-140).contains(self.mx,self.my){if let Content::Paint{strokes,..}=&mut self.windows[index].content{if let Some(stroke)=strokes.last_mut(){if stroke.points.len()<100_000{stroke.points.push((self.mx-r.x-24,self.my-r.y-115));}}}}},Drag::None=>{}}}self.buttons=buttons;}}}}
            if dirty&&now.wrapping_sub(last_frame)>=self.hz/60{self.draw();last_frame=hardware::cycles();dirty=false;}core::hint::spin_loop();
        }
    }
}
