use alloc::{string::{String,ToString},vec::Vec,format};
use crate::input::Key;
#[derive(Clone)] pub struct Document {pub name:String,pub text:String}
pub struct Files {pub entries:Vec<Document>,pub revision:u64}
impl Default for Files {fn default()->Self{Self::new()}}
impl Files {
    pub fn new()->Self{Self{entries:Vec::new(),revision:0}}
    pub fn save(&mut self,name:&str,text:&str)->Result<(),&'static str>{
        if name.is_empty()||name.len()>80||name.chars().any(|c|c=='/'||c.is_control()){return Err("Invalid filename");}
        if text.len()>1024*1024{return Err("Document exceeds 1 MiB");}
        if let Some(d)=self.entries.iter_mut().find(|d|d.name==name){d.text=text.to_string();}else{self.entries.push(Document{name:name.to_string(),text:text.to_string()});}
        self.revision+=1;Ok(())
    }
    pub fn remove(&mut self,name:&str)->bool{if let Some(i)=self.entries.iter().position(|d|d.name==name){self.entries.remove(i);self.revision+=1;true}else{false}}
    pub fn get(&self,name:&str)->Option<&str>{self.entries.iter().find(|d|d.name==name).map(|d|d.text.as_str())}
}
#[derive(Clone)]struct EditState{text:String,cursor:usize}
pub struct Editor {pub name:String,pub text:String,pub cursor:usize,pub scroll:usize,pub dirty:bool,undo:Vec<EditState>,redo:Vec<EditState>,pub find:String,pub searching:bool}
impl Default for Editor{fn default()->Self{Self::new("Untitled.txt","")}}
impl Editor {
    pub fn new(name:&str,text:&str)->Self{Self{name:name.to_string(),text:text.to_string(),cursor:0,scroll:0,dirty:false,undo:Vec::new(),redo:Vec::new(),find:String::new(),searching:false}}
    fn snapshot(&self)->EditState{EditState{text:self.text.clone(),cursor:self.cursor}}
    fn checkpoint(&mut self){if self.undo.len()==64{self.undo.remove(0);}self.undo.push(self.snapshot());self.redo.clear();self.dirty=true;}
    pub fn row_col(&self)->(usize,usize){let before=&self.text[..self.cursor];(before.bytes().filter(|&b|b==b'\n').count(),before.rsplit('\n').next().unwrap_or("").chars().count())}
    pub fn undo(&mut self){if let Some(s)=self.undo.pop(){self.redo.push(self.snapshot());self.text=s.text;self.cursor=s.cursor;self.dirty=true;}}
    pub fn redo(&mut self){if let Some(s)=self.redo.pop(){self.undo.push(self.snapshot());self.text=s.text;self.cursor=s.cursor;self.dirty=true;}}
    pub fn find_next(&mut self){if self.find.is_empty(){return;}let next=self.text[self.cursor..].find(&self.find).map(|i|i+self.cursor).or_else(||self.text[..self.cursor].find(&self.find));if let Some(i)=next{self.cursor=i+self.find.len();self.scroll=self.text[..i].bytes().filter(|&b|b==b'\n').count().saturating_sub(3);}}
    pub fn key(&mut self,key:Key){
        if self.searching {match key{Key::Char(c)=>self.find.push(c),Key::Backspace=>{self.find.pop();},Key::Enter=>{self.find_next();self.searching=false;},Key::Escape=>self.searching=false,_=>{}}return;}
        match key{
            Key::Char(c)=>{if self.text.len()<1024*1024{self.checkpoint();self.text.insert(self.cursor,c);self.cursor+=c.len_utf8();}},
            Key::Enter=>{self.checkpoint();self.text.insert(self.cursor,'\n');self.cursor+=1;},
            Key::Tab=>{self.checkpoint();self.text.insert_str(self.cursor,"    ");self.cursor+=4;},
            Key::Backspace if self.cursor>0=>{self.checkpoint();let p=self.text[..self.cursor].char_indices().next_back().unwrap().0;self.text.drain(p..self.cursor);self.cursor=p;},
            Key::Delete if self.cursor<self.text.len()=>{self.checkpoint();self.text.remove(self.cursor);},
            Key::Left if self.cursor>0=>self.cursor=self.text[..self.cursor].char_indices().next_back().unwrap().0,
            Key::Right if self.cursor<self.text.len()=>self.cursor+=self.text[self.cursor..].chars().next().unwrap().len_utf8(),
            Key::Home=>self.cursor=self.text[..self.cursor].rfind('\n').map_or(0,|i|i+1),
            Key::End=>self.cursor=self.text[self.cursor..].find('\n').map_or(self.text.len(),|i|i+self.cursor),
            Key::Up|Key::Down=>{let(row,col)=self.row_col();let target=if key==Key::Up{row.saturating_sub(1)}else{row+1};let mut at=0;for(i,line)in self.text.split('\n').enumerate(){if i==target{self.cursor=at+line.char_indices().nth(col).map_or(line.len(),|(j,_)|j);break;}at+=line.len()+1;}},
            _=>{}
        }
    }
}
pub fn calculate(s:&str)->Result<f64,&'static str>{
    struct Parser<'a>{s:&'a[u8],i:usize,depth:u32}
    impl Parser<'_>{
        fn ws(&mut self){while self.i<self.s.len()&&self.s[self.i].is_ascii_whitespace(){self.i+=1;}}
        fn expr(&mut self,min:u8)->Result<f64,&'static str>{
            self.depth+=1;if self.depth>64{return Err("Expression too deeply nested");}self.ws();let mut value;
            if self.i>=self.s.len(){return Err("Expected a number");}
            match self.s[self.i]{
                b'('=>{self.i+=1;value=self.expr(0)?;self.ws();if self.s.get(self.i)!=Some(&b')'){return Err("Missing closing parenthesis");}self.i+=1;},
                b'-'|b'+'=>{let negative=self.s[self.i]==b'-';self.i+=1;value=self.expr(3)?;if negative{value=-value;}},
                _=>{let start=self.i;while self.i<self.s.len()&&(self.s[self.i].is_ascii_digit()||self.s[self.i]==b'.'){self.i+=1;}value=core::str::from_utf8(&self.s[start..self.i]).map_err(|_|"Invalid input")?.parse::<f64>().map_err(|_|"Invalid number")?;}
            }
            loop{self.ws();let op=match self.s.get(self.i){Some(&op)=>op,None=>break};let prec=match op{b'+'|b'-'=>1,b'*'|b'/'|b'%'=>2,_=>break};if prec<min{break;}self.i+=1;let right=self.expr(prec+1)?;value=match op{b'+'=>value+right,b'-'=>value-right,b'*'=>value*right,b'/' if right!=0.0=>value/right,b'%' if right!=0.0=>value%right,_=>return Err("Division by zero")};}
            self.depth-=1;if value.is_finite(){Ok(value)}else{Err("Result overflow")}
        }
    }
    let mut p=Parser{s:s.as_bytes(),i:0,depth:0};let v=p.expr(0)?;p.ws();if p.i!=p.s.len(){Err("Unexpected character")}else{Ok(v)}
}
pub struct Terminal{pub lines:Vec<String>,pub input:String,history:Vec<String>,history_index:usize}
impl Default for Terminal{fn default()->Self{Self{lines:alloc::vec!["Avalanche shell / native x86_64".to_string(),"Type help for commands. Ctrl+L clears this view.".to_string(),String::new()],input:String::new(),history:Vec::new(),history_index:0}}}
impl Terminal {
    pub fn print(&mut self,s:&str){for line in s.lines(){self.lines.push(line.to_string());}while self.lines.len()>300{self.lines.remove(0);}}
    pub fn history(&mut self,up:bool){if up{self.history_index=self.history_index.saturating_sub(1);}else{self.history_index=(self.history_index+1).min(self.history.len());}self.input=self.history.get(self.history_index).cloned().unwrap_or_default();}
    pub fn execute(&mut self,files:&mut Files,total:u64,free:u64,uptime:u64){
        let command=core::mem::take(&mut self.input);self.print(&format!("guest@avalanche ~ $ {}",command));self.history.push(command.clone());self.history_index=self.history.len();
        let(c,arg)=command.trim().split_once(' ').unwrap_or((command.trim(),""));
        match c{
            ""=>{},"help"=>self.print("help  clear  uname  uptime  date  free\nls  cat NAME  touch NAME  rm NAME\necho TEXT  calc EXPRESSION  reboot"),
            "clear"=>self.lines.clear(),"uname"=>self.print("AvalancheOS 0.1.0 x86_64 / original Rust kernel"),
            "uptime"=>self.print(&format!("{}m {}s",uptime/60,uptime%60)),
            "date"=>{let(h,m,s)=crate::hardware::time();self.print(&format!("{:02}:{:02}:{:02} RTC",h,m,s));},
            "free"=>self.print(&format!("RAM total {} MiB / available {} MiB",total>>20,free>>20)),
            "ls"=>{for d in &files.entries{self.print(&format!("{:>8}  {}",d.text.len(),d.name));}},
            "cat"=>self.print(files.get(arg).unwrap_or("File not found")),
            "touch"=>{if files.get(arg).is_none(){if let Err(e)=files.save(arg,""){self.print(e);}}},
            "rm"=>{if !files.remove(arg){self.print("File not found");}},
            "echo"=>self.print(arg),"calc"=>match calculate(arg){Ok(n)=>self.print(&format!("{}",n)),Err(e)=>self.print(e)},
            "reboot"=>crate::hardware::reboot(),_=>self.print("Unknown command; type help")
        }
    }
}
