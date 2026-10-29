use alloc::vec::Vec;
use crate::boot::Framebuffer;
static FONT_RLE:&[u8]=include_bytes!("../assets/font.rle");
static WIDTHS:&[u8]=include_bytes!("../assets/widths.bin");
#[derive(Clone,Copy,Debug)] pub struct Rect{pub x:i32,pub y:i32,pub w:i32,pub h:i32}
impl Rect {pub fn contains(self,x:i32,y:i32)->bool{x>=self.x&&y>=self.y&&x<self.x+self.w&&y<self.y+self.h}}
pub struct Canvas {pub w:usize,pub h:usize,pub pixels:Vec<u32>,front:Vec<u32>,fb:*mut u8,pitch:usize,shifts:[u8;3],pub clip:Option<Rect>,font:Vec<u8>}
pub fn mix(a:u32,b:u32,t:u32)->u32 {let t=t.min(255);let mut out=0;for s in [0,8,16]{out|=((((a>>s)&255)*(255-t)+((b>>s)&255)*t)/255)<<s;}out}
impl Canvas {
    pub fn new(fb:&Framebuffer)->Self {let w=fb.width as usize;let h=fb.height as usize;let mut font=Vec::with_capacity(4*224*48*56);for pair in FONT_RLE.chunks_exact(2){font.resize(font.len()+pair[0]as usize,pair[1]);}assert_eq!(font.len(),4*224*48*56);Self{w,h,pixels:alloc::vec![0;w*h],front:alloc::vec![u32::MAX;w*h],fb:fb.address,pitch:fb.pitch as usize,shifts:[fb.red_shift,fb.green_shift,fb.blue_shift],clip:None,font}}
    pub fn pixel(&mut self,x:i32,y:i32,c:u32,a:u32){if self.clip.is_some_and(|r|!r.contains(x,y)){return;}if x>=0&&y>=0&&(x as usize)<self.w&&(y as usize)<self.h{let i=y as usize*self.w+x as usize;self.pixels[i]=if a==255{c}else{mix(self.pixels[i],c,a)};}}
    pub fn rect(&mut self,r:Rect,c:u32,a:u32){for y in r.y.max(0)..(r.y+r.h).min(self.h as i32){for x in r.x.max(0)..(r.x+r.w).min(self.w as i32){self.pixel(x,y,c,a);}}}
    pub fn round(&mut self,r:Rect,radius:i32,c:u32,alpha:u32){
        let radius=radius.min(r.w/2).min(r.h/2).max(0);
        for y in r.y.max(0)..(r.y+r.h).min(self.h as i32){for x in r.x.max(0)..(r.x+r.w).min(self.w as i32){
            let dx=if x<r.x+radius{r.x+radius-x}else if x>=r.x+r.w-radius{x-(r.x+r.w-radius-1)}else{0};
            let dy=if y<r.y+radius{r.y+radius-y}else if y>=r.y+r.h-radius{y-(r.y+r.h-radius-1)}else{0};
            let a=if dx==0||dy==0{alpha}else{let mut n=0;for sy in [1,3]{for sx in [1,3]{let xx=dx*4-sx;let yy=dy*4-sy;if xx*xx+yy*yy<=radius*radius*16{n+=1;}}}alpha*n/4};
            if a>0{self.pixel(x,y,c,a);}
        }}
    }
    pub fn line(&mut self,x0:i32,y0:i32,x1:i32,y1:i32,c:u32){let(mut x,mut y)=(x0,y0);let dx=(x1-x0).abs();let sx=if x0<x1{1}else{-1};let dy=-(y1-y0).abs();let sy=if y0<y1{1}else{-1};let mut e=dx+dy;loop{self.pixel(x,y,c,255);if x==x1&&y==y1{break;}let q=2*e;if q>=dy{e+=dy;x+=sx;}if q<=dx{e+=dx;y+=sy;}}}
    pub fn text(&mut self,x:i32,y:i32,text:&str,size:usize,c:u32){let mut px=x;let size=size.min(3);let height=[19,24,34,53][size];for ch in text.chars(){let cp=if (32..256).contains(&(ch as u32)){ch as usize}else{63};let index=size*224+cp-32;let advance=WIDTHS[index] as usize;let offset=index*48*56;for yy in 0..height{for xx in 0..advance{let a=self.font[offset+yy*48+xx]as u32;if a!=0{self.pixel(px+xx as i32,y+yy as i32,c,a);}}}px+=advance as i32;}}
    pub fn width(text:&str,size:usize)->i32{text.chars().map(|ch|{let cp=if (32..256).contains(&(ch as u32)){ch as usize}else{63};WIDTHS[size.min(3)*224+cp-32]as i32}).sum()}
    pub fn shadow(&mut self,r:Rect){for i in (1..=10).rev(){self.round(Rect{x:r.x-i,y:r.y-i+8,w:r.w+2*i,h:r.h+2*i},16+i,0,5);}}
    pub fn glass(&mut self,r:Rect){
        let x0=r.x.max(0)as usize;let y0=r.y.max(0)as usize;let x1=(r.x+r.w).min(self.w as i32).max(0)as usize;let y1=(r.y+r.h).min(self.h as i32).max(0)as usize;
        if x1<=x0||y1<=y0{return;}
        let width=x1-x0;let height=y1-y0;let mut temp=alloc::vec![0;width*height];
        for y in 0..height{let mut sums=[0u32;3];for dx in 0..=10 {let p=self.pixels[(y+y0)*self.w+x0+dx.min(width-1)];for(s,sum)in sums.iter_mut().enumerate(){*sum+=(p>>(s*8))&255;}}for x in 0..width{let l=x.saturating_sub(10);let h=(x+10).min(width-1);let n=(h-l+1)as u32;temp[y*width+x]=(sums[0]/n)|((sums[1]/n)<<8)|((sums[2]/n)<<16);if x>=10{let p=self.pixels[(y+y0)*self.w+x0+x-10];for(s,sum)in sums.iter_mut().enumerate(){*sum-=p>>(s*8)&255;}}if x+11<width{let p=self.pixels[(y+y0)*self.w+x0+x+11];for(s,sum)in sums.iter_mut().enumerate(){*sum+=p>>(s*8)&255;}}}}
        for x in 0..width{let mut sums=[0u32;3];for dy in 0..=10.min(height-1){let p=temp[dy*width+x];for(s,sum)in sums.iter_mut().enumerate(){*sum+=p>>(s*8)&255;}}for y in 0..height{let l=y.saturating_sub(10);let h=(y+10).min(height-1);let n=(h-l+1)as u32;self.pixels[(y+y0)*self.w+x0+x]=(sums[0]/n)|((sums[1]/n)<<8)|((sums[2]/n)<<16);if y>=10{let p=temp[(y-10)*width+x];for(s,sum)in sums.iter_mut().enumerate(){*sum-=p>>(s*8)&255;}}if y+11<height{let p=temp[(y+11)*width+x];for(s,sum)in sums.iter_mut().enumerate(){*sum+=p>>(s*8)&255;}}}}
        self.round(r,16,0x172132,215);
    }
    pub fn present(&mut self){for y in 0..self.h{for x in 0..self.w{let i=y*self.w+x;let c=self.pixels[i];if c!=self.front[i]{let encoded=(((c>>16)&255)<<self.shifts[0])|(((c>>8)&255)<<self.shifts[1])|((c&255)<<self.shifts[2]);unsafe{(self.fb.add(y*self.pitch+x*4)as *mut u32).write_volatile(encoded);}self.front[i]=c;}}}}
    pub fn icon(&mut self,x:i32,y:i32,id:usize,scale:i32,color:u32){
        let s=scale;match id {
            0=>{self.line(x+2*s,y+12*s,x+8*s,y+2*s,color);self.line(x+8*s,y+2*s,x+14*s,y+12*s,color);self.line(x+5*s,y+9*s,x+11*s,y+9*s,color);},
            1=>{self.round(Rect{x:x+s,y:y+4*s,w:14*s,h:10*s},2*s,color,255);self.round(Rect{x:x+s,y:y+2*s,w:6*s,h:5*s},s,color,255);},
            2=>{self.line(x+2*s,y+4*s,x+6*s,y+8*s,color);self.line(x+6*s,y+8*s,x+2*s,y+12*s,color);self.line(x+8*s,y+12*s,x+13*s,y+12*s,color);},
            3=>{self.round(Rect{x:x+3*s,y:y+s,w:10*s,h:14*s},s,color,255);for k in 0..3 {self.line(x+5*s,y+(5+k*3)*s,x+11*s,y+(5+k*3)*s,0x263246);}},
            4=>{self.round(Rect{x:x+s,y:y+2*s,w:14*s,h:12*s},2*s,color,255);self.line(x+3*s,y+11*s,x+6*s,y+7*s,0x203448);self.line(x+6*s,y+7*s,x+9*s,y+10*s,0x203448);self.line(x+9*s,y+10*s,x+12*s,y+5*s,0x203448);},
            5=>{self.round(Rect{x:x+2*s,y:y+s,w:12*s,h:14*s},2*s,color,255);self.rect(Rect{x:x+4*s,y:y+3*s,w:8*s,h:3*s},0x203448,255);for k in 0..6{self.rect(Rect{x:x+(4+k%3*3)*s,y:y+(8+k/3*3)*s,w:s,h:s},0x203448,255);}},
            _=>{for k in 0..3{self.line(x+2*s,y+(4+k*4)*s,x+14*s,y+(4+k*4)*s,color);self.round(Rect{x:x+(4+(k%2)*6)*s,y:y+(3+k*4)*s,w:2*s,h:3*s},s,color,255);}}
    }
    }
    pub fn cursor(&mut self,x:i32,y:i32){for row in 0..22{let w=row*3/5;for col in 0..=w{self.pixel(x+col,y+row,if col==0||col==w||row==21{0x10151f}else{0xf6f9ff},255);}}self.line(x+6,y+16,x+11,y+26,0xf6f9ff);self.line(x+7,y+16,x+12,y+26,0xf6f9ff);}
}
