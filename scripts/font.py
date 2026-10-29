#!/usr/bin/env python3
"""Generate the kernel's antialiased Latin font atlas from the vendored font."""
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
root=Path(__file__).resolve().parents[1]
data=bytearray(); widths=[]
for size in (14,18,26,42):
    font=ImageFont.truetype(str(root/'vendor/fonts/DejaVuSans.ttf'),size,layout_engine=ImageFont.Layout.BASIC)
    for cp in range(32,256):
        image=Image.new('L',(48,56));draw=ImageDraw.Draw(image)
        draw.text((0,size+2),chr(cp),font=font,fill=255,anchor='ls')
        widths.append(min(48,round(font.getlength(chr(cp)))))
        data.extend(image.tobytes())
encoded=bytearray()
i=0
while i<len(data):
    j=i+1
    while j<len(data) and data[j]==data[i] and j-i<255:j+=1
    encoded.extend((j-i,data[i]));i=j
(root/'assets/font.rle').write_bytes(encoded)
(root/'assets/widths.bin').write_bytes(bytes(widths))
