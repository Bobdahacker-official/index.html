#!/usr/bin/env python3
"""Build a FAT32 EFI system partition, GPT disk and UEFI El Torito ISO."""
import struct, pathlib, uuid, zlib
ROOT = pathlib.Path(__file__).resolve().parents[1]
S=512
SECTORS=131072
FAT_SECTORS=1009
RESERVED=32
DATA=RESERVED+2*FAT_SECTORS

def guid(s): return uuid.UUID(s).bytes_le
def fat_image():
    image=bytearray(SECTORS*S)
    image[:3]=b'\xeb\x58\x90'; image[3:11]=b'AVALOS  '
    struct.pack_into('<HBHBHHBHHHII', image,11,512,1,32,2,0,0,0xf8,0,63,255,2048,SECTORS)
    struct.pack_into('<IHHIHH',image,36,FAT_SECTORS,0,0,2,1,6)
    struct.pack_into('<BBBI',image,64,0x80,0,0x29,0x41564131)
    image[71:82]=b'AVALANCHE  ';image[82:90]=b'FAT32   ';image[510:512]=b'\x55\xaa'
    image[6*S:7*S]=image[:S]
    fat=bytearray(FAT_SECTORS*S)
    struct.pack_into('<II',fat,0,0xffffff8,0xffffffff)
    next_cluster=2
    def allocate(payload):
        nonlocal next_cluster
        n=max(1,(len(payload)+S-1)//S); first=next_cluster;next_cluster+=n
        for c in range(first,first+n):struct.pack_into('<I',fat,c*4,0xfffffff if c==first+n-1 else c+1)
        off=(DATA+first-2)*S;image[off:off+len(payload)]=payload
        return first
    def entry(name,attr,cluster,size):
        e=bytearray(32);e[:11]=name.encode('ascii');e[11]=attr
        # Stable timestamp: 2026-01-01 00:00:00.
        struct.pack_into('<H',e,16,23585);struct.pack_into('<H',e,18,23585)
        struct.pack_into('<H',e,20,cluster>>16);struct.pack_into('<HHHI',e,22,0,23585,cluster&65535,size)
        return e
    root=allocate(bytes(S));efi=allocate(bytes(S));boot=allocate(bytes(S))
    files=[('BOOTX64 EFI',ROOT/'vendor/limine/BOOTX64.EFI'),('KERNEL  ELF',ROOT/'build/avalanche.elf'),('LIMINE  CON',ROOT/'limine.conf')]
    # Long file names are unnecessary: use DOS-compatible kernel/config names.
    entries=[]
    for name,path in files:
        content=path.read_bytes();cluster=allocate(content);entries.append(entry(name,0x20,cluster,len(content)))
    def directory(cluster,content):
        start=(DATA+cluster-2)*S;image[start:start+len(content)]=content
    # limine.conf requires a VFAT name since CONF has four characters.
    short=b'LIMINE~1CON'
    checksum=0
    for c in short:checksum=(((checksum&1)<<7)+(checksum>>1)+c)&255
    lfn=bytearray(b'\xff'*32);lfn[0]=0x41;lfn[11]=15;lfn[12]=0;lfn[13]=checksum;lfn[26:28]=b'\0\0'
    chars=[ord(c) for c in 'limine.conf']+[0,0xffff]
    for off,c in zip([1,3,5,7,9,14,16,18,20,22,24,28,30],chars):struct.pack_into('<H',lfn,off,c)
    entries[2][:11]=short
    directory(root,entry('EFI        ',16,efi,0)+entries[1]+lfn+entries[2])
    directory(efi,entry('.          ',16,efi,0)+entry('..         ',16,0,0)+entry('BOOT       ',16,boot,0))
    directory(boot,entry('.          ',16,boot,0)+entry('..         ',16,efi,0)+entries[0])
    for offset in [RESERVED*S,(RESERVED+FAT_SECTORS)*S]:image[offset:offset+len(fat)]=fat
    struct.pack_into('<I',image,S,0x41615252);struct.pack_into('<IIII',image,S+484,0x61417272,SECTORS-DATA-(next_cluster-2),next_cluster,0)
    image[S+510:S+512]=b'\x55\xaa';image[7*S:8*S]=image[S:2*S]
    return image

def disk_image(esp):
    start=2048;total=start+SECTORS+4096+2048;last=total-1
    disk=bytearray(total*S);disk[start*S:(start+SECTORS)*S]=esp
    disk[446:462]=struct.pack('<B3sB3sII',0,b'\0\2\0',0xee,b'\xff\xff\xff',1,total-1);disk[510:512]=b'\x55\xaa'
    table=bytearray(128*128)
    struct.pack_into('<16s16sQQQ',table,0,guid('c12a7328-f81f-11d2-ba4b-00a0c93ec93b'),guid('4176616c-616e-4368-ae00-000000000001'),start,start+SECTORS-1,0)
    label='Avalanche EFI'.encode('utf-16le');table[56:56+len(label)]=label
    struct.pack_into('<16s16sQQQ',table,128,guid('d5c1b355-6cb1-4d64-a6d5-415641465330'),guid('4176616c-616e-4368-ae00-000000000003'),start+SECTORS,start+SECTORS+4095,0)
    table[184:194]='AvaFS'.encode('utf-16le')
    def header(current,backup,table_lba):
        h=bytearray(S)
        struct.pack_into('<8sIIIIQQQQ16sQIII',h,0,b'EFI PART',0x10000,92,0,0,current,backup,34,last-33,guid('4176616c-616e-4368-ae00-000000000002'),table_lba,128,128,zlib.crc32(table))
        struct.pack_into('<I',h,16,zlib.crc32(h[:92]));return h
    disk[2*S:2*S+len(table)]=table;disk[(last-32)*S:(last-32)*S+len(table)]=table
    disk[S:2*S]=header(1,last,2);disk[last*S:(last+1)*S]=header(last,1,last-32)
    return disk

def iso_esp(kernel,config):
    # Keep the optical EFI image within El Torito's 16-bit sector count.
    sectors=32768; fat_sectors=128; data_start=289
    image=bytearray(sectors*512);image[:3]=b'\xeb\x3c\x90';image[3:11]=b'AVALOS  '
    struct.pack_into('<HBHBHHBHHHII',image,11,512,1,1,2,512,sectors,0xf8,fat_sectors,63,255,0,0)
    struct.pack_into('<BBBI',image,36,0x80,0,0x29,0x41564132);image[43:54]=b'AVALANCHE  ';image[54:62]=b'FAT16   ';image[510:512]=b'\x55\xaa'
    fat=bytearray(fat_sectors*512);struct.pack_into('<HH',fat,0,0xfff8,0xffff);cursor=2
    def allocate(payload):
        nonlocal cursor
        first=cursor;n=max(1,(len(payload)+511)//512);cursor+=n
        if cursor-2>sectors-data_start:raise ValueError('EFI optical image is full')
        for c in range(first,first+n):struct.pack_into('<H',fat,c*2,0xffff if c==first+n-1 else c+1)
        at=(data_start+first-2)*512;image[at:at+len(payload)]=payload;return first
    def entry(name,attr,cluster,size):
        e=bytearray(32);e[:11]=name.encode();e[11]=attr;struct.pack_into('<HI',e,26,cluster,size);return e
    efi=allocate(bytes(512));boot=allocate(bytes(512));exe=(ROOT/'vendor/limine/BOOTX64.EFI').read_bytes();ec=allocate(exe);kc=allocate(kernel);cc=allocate(config)
    short=b'LIMINE~1CON';checksum=0
    for c in short:checksum=(((checksum&1)<<7)+(checksum>>1)+c)&255
    lfn=bytearray(b'\xff'*32);lfn[0]=0x41;lfn[11]=15;lfn[12]=0;lfn[13]=checksum;lfn[26:28]=b'\0\0'
    for off,ch in zip([1,3,5,7,9,14,16,18,20,22,24,28,30],[ord(c)for c in 'limine.conf']+[0,0xffff]):struct.pack_into('<H',lfn,off,ch)
    root=entry('EFI        ',16,efi,0)+entry('KERNEL  ELF',32,kc,len(kernel))+lfn+entry('LIMINE~1CON',32,cc,len(config))
    image[257*512:257*512+len(root)]=root
    for cluster,parent,child in [(efi,0,entry('BOOT       ',16,boot,0)),(boot,efi,entry('BOOTX64 EFI',32,ec,len(exe)))]:
        directory=entry('.          ',16,cluster,0)+entry('..         ',16,parent,0)+child;at=(data_start+cluster-2)*512;image[at:at+len(directory)]=directory
    image[512:129*512]=fat;image[129*512:257*512]=fat
    return image

def iso_image(path,esp,kernel,config):
    """ISO 9660 level 2 with an EFI no-emulation El Torito boot catalog."""
    block=2048
    files=[(b'EFI.IMG;1',esp),(b'KERNEL.ELF;1',kernel),(b'LIMINE.CONF;1',config)]
    cursor=24; extents=[]
    for name,data in files:
        extents.append((name,cursor,len(data),data));cursor+=(len(data)+block-1)//block
    image=bytearray(cursor*block)
    def both16(v):return struct.pack('<H',v)+struct.pack('>H',v)
    def both32(v):return struct.pack('<I',v)+struct.pack('>I',v)
    def record(name,extent,size,directory=False):
        length=33+len(name)+(len(name)%2==0)
        r=bytearray(length);r[0]=length;r[2:10]=both32(extent);r[10:18]=both32(size)
        r[18:25]=bytes([126,1,1,0,0,0,0]);r[25]=2 if directory else 0
        r[28:32]=both16(1);r[32]=len(name);r[33:33+len(name)]=name;return r
    p=bytearray(block);p[:7]=b'\x01CD001\x01';p[8:40]=b'AVALANCHE'.ljust(32);p[40:72]=b'AVALANCHEOS'.ljust(32)
    p[80:88]=both32(cursor);p[120:124]=both16(1);p[124:128]=both16(1);p[128:132]=both16(block)
    p[132:140]=both32(10);struct.pack_into('<I',p,140,20);struct.pack_into('>I',p,148,21)
    p[156:190]=record(b'\0',22,block,True);p[190:813]=b' '*623
    for off in (813,830,847,864):p[off:off+17]=b'2026010100000000\0'
    p[881]=1;image[16*block:17*block]=p
    boot=bytearray(block);boot[:7]=b'\0CD001\x01';boot[7:39]=b'EL TORITO SPECIFICATION'.ljust(32,b'\0');struct.pack_into('<I',boot,71,23);image[17*block:18*block]=boot
    image[18*block:18*block+7]=b'\xffCD001\x01'
    image[20*block:20*block+10]=struct.pack('<BBIHH',1,0,22,1,0)
    image[21*block:21*block+10]=struct.pack('>BBIHH',1,0,22,1,0)
    directory=record(b'\0',22,block,True)+record(b'\1',22,block,True)+record(b'BOOT.CAT;1',23,block)
    for name,extent,size,data in extents:
        directory+=record(name,extent,size);image[extent*block:extent*block+size]=data
    image[22*block:22*block+len(directory)]=directory
    catalog=bytearray(block);catalog[0]=1;catalog[1]=0xef;catalog[4:28]=b'AVALANCHEOS'.ljust(24,b'\0');catalog[30:32]=b'\x55\xaa'
    checksum=(-sum(struct.unpack('<16H',catalog[:32])))&65535;struct.pack_into('<H',catalog,28,checksum)
    catalog[32]=0x88;struct.pack_into('<H',catalog,38,len(esp)//512);struct.pack_into('<I',catalog,40,24)
    image[23*block:24*block]=catalog;path.write_bytes(image)

if __name__=='__main__':
    out=ROOT/'build';out.mkdir(exist_ok=True)
    esp=fat_image();(out/'efi.img').write_bytes(esp);(out/'avalancheos.img').write_bytes(disk_image(esp))
    kernel=(out/'avalanche.elf').read_bytes();config=(ROOT/'limine.conf').read_bytes()
    iso_image(out/'avalancheos.iso',iso_esp(kernel,config),kernel,config)
    print('Built build/avalancheos.iso and build/avalancheos.img')
