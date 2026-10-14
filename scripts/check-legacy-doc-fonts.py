"""Independent binary DOC font/size controls and final CLI/consumer checks.

The positive controls are authored here directly as CFB/FIB/STSH/FFN/CLX/FKP
bytes, never through the application parser or writer. Their typography oracle
is separately specified in the recipes. Genuine POI files are pinned, read-only.
Each execution makes a fresh directory; no consumer output is overwritten.
"""
import argparse, copy, hashlib, json, os, shutil, struct, subprocess, tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / 'tmp/legacy-doc-font-validation-20261014'
PINS = {'simple.doc':'4876e828e62d490fa188c7c4e47013e2e0dfdf02f5903dfd374bc0d2b2049ca1',
        'SampleDoc.doc':'717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833'}
INVEST = ROOT / 'tmp/legacy-doc-font-investigation-20261014'
INVEST_SHA = '624079b32ea9875b56c5b0626fe3ee1a68a46d6b4ebcfef772354b6d25864725'
W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
NS = {'w':W}
u16 = lambda b,p: struct.unpack_from('<H', b,p)[0]
u32 = lambda b,p: struct.unpack_from('<I', b,p)[0]
p16 = lambda n: struct.pack('<H',n)
p32 = lambda n: struct.pack('<I',n)
def set16(b,p,n): struct.pack_into('<H',b,p,n)
def set32(b,p,n): struct.pack_into('<I',b,p,n)
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def require(v, why):
    if not v: raise AssertionError(why)
def save(p, data):
    require(not p.exists(), 'Refuse overwrite '+str(p))
    if isinstance(data, bytes): p.write_bytes(data)
    else: p.write_text(json.dumps(data,indent=2,ensure_ascii=False)+'\n')

# Read-only source-container decoder, independent of office_oxide.
def streams(path):
    b=path.read_bytes(); require(b[:8]==bytes.fromhex('d0cf11e0a1b11ae1'),'CFB magic')
    require(u16(b,30)==9 and u16(b,32)==6,'Small CFB geometry')
    sec=lambda n:b[(n+1)*512:(n+2)*512]
    require(u32(b,72)==0,'No external DIFAT in independent subset')
    fat=[]
    for n in struct.unpack_from('<109I',b,76)[:u32(b,44)]: fat.extend(struct.unpack('<128I',sec(n)))
    def chain(n, table=fat, mini=None):
        out=bytearray(); seen=set()
        while n<0xfffffffa:
            require(n<len(table) and n not in seen,'CFB chain bounds/cycle');seen.add(n)
            piece=sec(n) if mini is None else mini[n*64:(n+1)*64]
            require(len(piece)==(512 if mini is None else 64),'CFB chain truncated')
            out.extend(piece);n=table[n]
        require(n==0xfffffffe,'CFB chain terminator');return bytes(out)
    entries=[]
    for d in [chain(u32(b,48))[p:p+128] for p in range(0,len(chain(u32(b,48))),128)]:
        if d[66] in (2,5):entries.append((d[:u16(d,64)-2].decode('utf-16le'),d[66],u32(d,116),struct.unpack_from('<Q',d,120)[0]))
    root=next(d for d in entries if d[1]==5)
    mini=chain(root[2])[:root[3]] if root[3] else b''
    mf=chain(u32(b,60)) if u32(b,64) else b''
    mft=struct.unpack('<%dI'%(len(mf)//4),mf)
    result={}
    for name,t,start,size in entries:
        if t!=2:continue
        require(name not in result,'Duplicate source stream')
        raw=chain(start,mft,mini) if size<4096 else chain(start)
        require(len(raw)>=size,'Declared stream truncation');result[name]=raw[:size]
    return result

def sprms(data):
    out=[];p=0
    while p<len(data):
        require(p+2<=len(data),'SPRMs opcode bounds')
        op=u16(data,p);p+=2;n=[1,1,2,4,2,2,None,3][op>>13]
        if n is None: n=data[p];p+=1
        require(p+n<=len(data),'SPRMs operand bounds');out.append((op,data[p:p+n]));p+=n
    return out

def source_oracle(path):
    ss=streams(path);wd=ss['WordDocument'];tab=ss['1Table' if u16(wd,10)&0x200 else '0Table']
    def field(p):
        off,n=u32(wd,p),u32(wd,p+4);require(off+n<=len(tab),'Table source bounds');return tab[off:off+n]
    f=field(0x112);fonts=[];p=4
    require(u16(f,2)==0,'FFN extra')
    for _ in range(u16(f,0)):
        n=f[p];d=f[p+1:p+1+n];require(len(d)==n,'FFN length')
        fonts.append({'name':d[39:].decode('utf-16le').split('\0')[0],'charset':d[3],'ffid':d[0]});p+=n+1
    require(p==len(f),'FFN tail')
    st=field(0xa2);base=u16(st,4);styles=[];p=2+u16(st,0)
    for i in range(u16(st,2)):
        n=u16(st,p);p+=2
        if not n:styles.append(None);continue
        d=st[p:p+n];require(len(d)==n,'STD length');p+=n
        q=base;nc=u16(d,q);q+=2;name=d[q:q+2*nc].decode('utf-16le');q+=2*nc+2;q+=q%2
        ups=[]
        for _ in range(u16(d,4)&15):
            z=u16(d,q);q+=2;ups.append(d[q:q+z]);q+=z;q+=q%2
        styles.append({'name':name,'sti':u16(d,0)&0xfff,'kind':u16(d,2)&15,'base':u16(d,2)>>4,'upx':[x.hex() for x in ups]})
    clx=field(0x1a2);require(clx[0]==2 and u32(clx,1)==16,'Single unmodified source piece')
    pc=clx[5:];require(u32(pc,0)==0,'CP begins 0');cnt=u32(pc,4);fcr=u32(pc,10);fc=(fcr&0x3fffffff)//2
    require(fcr&0x40000000 and u16(pc,14)==0,'Compressed no PRM')
    text=wd[fc:fc+cnt].decode('cp1252')
    initial={'font0':u16(st,14),'font2':u16(st,18),'halfPoints':20}
    def overlay(cur,data):
        v=dict(cur)
        for op,arg in sprms(data):
            if op==0x4a43:v['halfPoints']=u16(arg,0)
            if op==0x4a4f:v['font0']=u16(arg,0)
            if op==0x4a51:v['font2']=u16(arg,0)
        return v
    initial=overlay(initial,bytes.fromhex(styles[0]['upx'][1]));runs=[]
    ch=field(0xfa);n=(len(ch)-4)//8
    for i in range(n):
        pn=u32(ch,4*(n+1)+4*i)&0x3fffff;pg=wd[pn*512:(pn+1)*512];k=pg[511]
        for j in range(k):
            a,b=u32(pg,j*4),u32(pg,(j+1)*4);q=pg[4*(k+1)+j]*2;props=pg[q+1:q+1+pg[q]] if q else b''
            effective=overlay(initial,props);require(effective['font0']==effective['font2'],'Latin slots differ in positive source')
            runs.append({'start':a-fc,'end':b-fc,'text':text[a-fc:b-fc],'name':fonts[effective['font0']]['name'],'halfPoints':effective['halfPoints'],'sprms':[(hex(o),v.hex())for o,v in sprms(props)]})
    newpos=0x9a+u16(wd,0x98)*8
    return {'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size,'base_nFib':u16(wd,2),'effective_nFib':u16(wd,newpos+2) if u16(wd,newpos) else u16(wd,2),'text':text,'fonts':fonts,'styles':styles,'runs':runs}

# Independent encoder: regular-stream-only CFB v3, freshly authored FIB+tables.
def compound(word, table, duplicate=False):
    require(len(word)<=4096 and len(table)<=4096,'Small author control')
    data=[word.ljust(4096,b'\0'), table.ljust(4096,b'\0')]
    h=bytearray(512);h[:8]=bytes.fromhex('d0cf11e0a1b11ae1');set16(h,24,0x3e);set16(h,26,3);set16(h,28,0xfffe);set16(h,30,9);set16(h,32,6)
    set32(h,44,1);set32(h,48,0);set32(h,56,4096);set32(h,60,0xfffffffe);set32(h,68,0xfffffffe)
    for i in range(109):set32(h,76+4*i,17 if i==0 else 0xffffffff)
    dirs=bytearray(512)
    def entry(i,name,typ,start,size,left=0xffffffff,right=0xffffffff,child=0xffffffff):
        q=i*128;raw=(name+'\0').encode('utf-16le');dirs[q:q+len(raw)]=raw;set16(dirs,q+64,len(raw));dirs[q+66]=typ;dirs[q+67]=1
        set32(dirs,q+68,left);set32(dirs,q+72,right);set32(dirs,q+76,child);set32(dirs,q+116,start);struct.pack_into('<Q',dirs,q+120,size)
    entry(0,'Root Entry',5,0xfffffffe,0,child=1)
    entry(1,'WordDocument',2,1,4096,left=2,right=3 if duplicate else 0xffffffff)
    entry(2,'1Table',2,9,4096)
    if duplicate:entry(3,'WordDocument',2,1,4096)
    fat=[0xffffffff]*128;fat[0]=0xfffffffe
    for first in (1,9):
        for j in range(first,first+8):fat[j]=j+1 if j<first+7 else 0xfffffffe
    fat[17]=0xfffffffd
    return bytes(h+dirs)+b''.join(data)+struct.pack('<128I',*fat)

def spr(op,value):
    n=[1,1,2,4,2,2,None,3][op>>13]
    if isinstance(value,int):value=value.to_bytes(n,'little')
    return p16(op)+(bytes([len(value)]) if n is None else b'')+value

def std(name,kind=1,sti=0,props=b'',paps=b'',base=0xfff,basebytes=10,style_index=0):
    d=bytearray(basebytes);set16(d,0,sti);set16(d,2,(base<<4)|kind);set16(d,4,(0 if kind==1 else 10<<4)|(2 if kind==1 else 1))
    d+=p16(len(name))+name.encode('utf-16le')+b'\0\0'
    for x in ([p16(style_index)+paps,props] if kind==1 else [props]):
        if len(d)%2:d+=b'\0'
        d+=p16(len(x))+x
        if len(d)%2:d+=b'\0'
    set16(d,6,len(d))  # StdfBase.bchUpe: end of the unexpanded UPX data
    return p16(len(d))+d

def authored(text='This independently authored font control is ordinary prose.',fonts=('Times New Roman','Liberation Serif','Liberation Sans'),default=0,style=b'',runs=None,base=0xfff,basebytes=10,version=0xc1,extras=None,default_char=True):
    raw=(text+'\r').encode('cp1252');n=len(raw);require(n<500,'Control text fits authored sector');wd=bytearray(4096);tab=bytearray()
    set16(wd,0,0xa5ec);set16(wd,2,0xc1);set16(wd,6,0x409);set16(wd,10,0x1200);set16(wd,12,0xbf);set32(wd,24,1024);set32(wd,28,1024+n)
    set16(wd,32,14);set16(wd,62,22);set32(wd,64,4096);set32(wd,0x4c,n)
    paircount=93 if version==0xc1 else 183;set16(wd,0x98,paircount);end=0x9a+8*paircount
    if version!=0xc1:set16(wd,end,5);set16(wd,end+2,version)
    # Modern FIB requires text after the larger FIB. Keep FKP 5/6 for it.
    fc=1024 if version==0xc1 else 2048;chpg=3 if version==0xc1 else 5;papg=4 if version==0xc1 else 6
    set32(wd,24,fc);set32(wd,28,fc+n);wd[fc:fc+n]=raw
    st=bytearray(20);set16(st,0,18);set16(st,2,max(15,max(extras,default=0)+1) if extras else 15);set16(st,4,basebytes);set16(st,14,default);set16(st,16,default);set16(st,18,default)
    positions={}
    for i in range(u16(st,2)):
        positions['style'+str(i)]=len(st)
        if i==0:st+=std('Normal',props=spr(0x486d,0x409)+style,base=base,basebytes=basebytes)
        elif i==10 and default_char:st+=std('Default Paragraph Font',2,65,basebytes=basebytes)
        elif extras and i in extras:st+=extras[i]
        else:st+=b'\0\0'
    f=p16(len(fonts))+p16(0)
    for name in fonts:
        ff=bytearray(39);ff[0]=0x16;set16(ff,1,400);ff+=name.encode('utf-16le')+b'\0\0';f+=bytes([len(ff)])+ff
    def table(label,pos,value):
        positions[label]=len(tab);set32(wd,pos,len(tab));set32(wd,pos+4,len(value));tab.extend(value)
    table('stsh',0xa2,st);table('ffn',0x112,f)
    table('clx',0x1a2,b'\2'+p32(16)+p32(0)+p32(n)+p16(0)+p32(0x40000000|fc*2)+p16(0))
    table('chplc',0xfa,p32(fc)+p32(fc+n)+p32(chpg));table('paplc',0x102,p32(fc)+p32(fc+n)+p32(papg))
    # One complete section, no SEPX exceptions. Explicit metadata proves a
    # standalone 0x0C is a manual page break, not an unknown section boundary.
    table('section',0xca,p32(0)+p32(n)+p16(0)+p32(0xffffffff)+p16(0)+p32(0xffffffff))
    if runs is None:runs=[(n,b'')]
    if runs[-1][0]<n:runs=list(runs)+[(n,runs[-1][1])]
    pg=bytearray(512);pg[511]=len(runs);start=0;payload=510
    set32(pg,0,fc)
    for i,(stop,props) in enumerate(runs):
        set32(pg,4*(i+1),fc+stop)
        if props:
            payload=(payload-len(props)-1)&~1;require(payload>5*len(runs)+4,'CHPX author capacity');pg[payload]=len(props);pg[payload+1:payload+1+len(props)]=props;pg[4*(len(runs)+1)+i]=payload//2
        start=stop
    wd[chpg*512:(chpg+1)*512]=pg
    paras=[];start=0
    for i,c in enumerate(raw):
        if c==13:paras.append((start,i+1));start=i+1
    pg=bytearray(512);pg[511]=len(paras);set32(pg,0,fc)
    # Re-read form: cw=0, cb'=1, two-byte istd, no grpprl.
    pg[506:510]=b'\0\1\0\0'
    for i,(a,b) in enumerate(paras):set32(pg,(i+1)*4,fc+b);pg[4*(len(paras)+1)+13*i]=253
    wd[papg*512:(papg+1)*512]=pg
    positions.update(fc=fc,chpg=chpg,papg=papg,text_len=n)
    return wd,tab,positions

def expected_chars(text,name,hps):return [[c,name,hps] for c in text]
def expanded_model(doc):
    out=[]
    for b in doc['content']:
        if b['type']=='pageBreak':out.append({'type':'pageBreak'});continue
        chars=[]
        for node in b.get('content',[]):
            require(node['type']=='text','Text-only positive oracle')
            marks={m['type']:m.get('attrs',{}) for m in node.get('marks',[])}
            chars.extend([[c,marks.get('fontFamily',{}).get('name'),marks.get('fontSize',{}).get('halfPoints')] for c in node['text']])
        out.append({'type':b['type'],'chars':chars})
    return out

def expanded_source(oracle):
    chars=[]
    for r in oracle['runs']:chars.extend([[c,r['name'],r['halfPoints']] for c in r['text']])
    out=[];buf=[]
    for row in chars:
        if row[0]=='\r':
            out.append({'type':'pageBreak'} if ''.join(x[0] for x in buf)=='\f' else {'chars':buf});buf=[]
        else:buf.append(row)
    require(not buf,'End paragraph marker');return out

def owned_blank_fixture(write=False):
    path=ROOT/'tests/fixtures/doc-typography/blank.doc'
    word,table,_=authored(text='hello.\r\r\f\rworld.')
    data=compound(word,table)
    if write:
        path.parent.mkdir(parents=True,exist_ok=True);save(path,data)
    require(path.exists() and path.read_bytes()==data,'Owned typography blank fixture is not deterministic')
    oracle=source_oracle(path);require(oracle['text']=='hello.\r\r\f\rworld.\r','Owned blank exact source text')
    require({(r['name'],r['halfPoints'])for r in oracle['runs']}=={('Times New Roman',20)},'Owned blank source font')
    print(json.dumps({'path':str(path),'bytes':len(data),'sha256':sha(path),'text':oracle['text'],'font':'Times New Roman','halfPoints':20}))
    return path

def prepare(root):
    root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='fixtures-',dir=root));cases=[]
    require(sha(INVEST/'REPORT.md')==INVEST_SHA,'Investigation report changed')
    for name,digest in PINS.items():
        p=ROOT/'tmp/corpus'/name;require(sha(p)==digest,'Genuine source changed');o=source_oracle(p);save(dest/(name+'.source-oracle.json'),o)
        cases.append({'name':'genuine-'+p.stem,'path':str(p),'sha256':digest,'kind':'genuine-original','outcome':'preserved','expected':expanded_source(o)})
    simple=cases[0]['expected'];require({tuple(c[1:])for b in simple for c in b.get('chars',[])}=={('Times New Roman',20)},'simple fixed source oracle')
    require({tuple(c[1:])for b in cases[1]['expected'] for c in b.get('chars',[])}=={('Calibri',22),('Arial Black',32)},'SampleDoc fixed source oracle')
    def add(name,parts,classification='preserved',expect=None,mut=None):
        wd,tab,pos=parts
        if mut:mut(wd,tab,pos)
        p=dest/(name+'.doc');save(p,compound(wd,tab,name=='duplicate-word-stream'))
        d={'name':name,'path':str(p),'sha256':sha(p),'kind':'independently-authored-binary-control','outcome':classification}
        if expect is not None:d['expected']=expect
        if classification=='preserved':
            o=source_oracle(p);d['raw_oracle']=expanded_source(o)
            require(d['raw_oracle']==expect,'Independently encoded bytes differ from authored recipe: '+name)
        cases.append(d)
    add('owned-blank-pagination',authored(text='hello.\r\r\f\rworld.'),expect=[{'chars':expected_chars('hello.','Times New Roman',20)},{'chars':[]},{'type':'pageBreak'},{'chars':expected_chars('world.','Times New Roman',20)}])
    text='This independently authored font control is ordinary prose.'
    E=lambda name,hps:[{'chars':expected_chars(text,name,hps)}]
    add('normative-default-20',authored(),expect=E('Times New Roman',20))
    add('stshi-default-name',authored(default=2),expect=E('Liberation Sans',20))
    add('normal-style-halfpoint',authored(default=1,style=spr(0x4a43,27)),expect=E('Liberation Serif',27))
    add('normal-style-font-over-default',authored(style=spr(0x4a4f,2)+spr(0x4a51,2)+spr(0x4a43,25)),expect=E('Liberation Sans',25))
    props=spr(0x4a4f,2)+spr(0x4a51,2)+spr(0x4a43,31)
    add('direct-over-style',authored(default=1,style=spr(0x4a43,27),runs=[(len(text)+1,props)]),expect=E('Liberation Sans',31))
    for name,mid in [('font-only-run-split',spr(0x4a4f,2)+spr(0x4a51,2)),('font-size-run-split',props)]:
        add(name,authored(default=1,style=spr(0x4a43,24),runs=[(10,b''),(30,mid),(len(text)+1,b'')]),expect=[{'chars':expected_chars(text[:10],'Liberation Serif',24)+expected_chars(text[10:30],'Liberation Sans',24 if name=='font-only-run-split' else 31)+expected_chars(text[30:],'Liberation Serif',24)}])
    add('modern-effective-nfib',authored(default=1,version=0x112,basebytes=18,style=spr(0x4a43,24)),expect=E('Liberation Serif',24))
    add('inert-complex-size-language',authored(style=spr(0x4a61,32)+spr(0x485f,0x401)),expect=E('Times New Roman',20))
    add('unused-style-ignored',authored(extras={15:std('Unused complex font',2,0xffe,props=spr(0x4a43,46))}),expect=E('Times New Roman',20))
    add('unused-symbol-font',authored(fonts=('Times New Roman','Symbol')),expect=E('Times New Roman',20),mut=lambda w,t,p:t.__setitem__(p['ffn']+4+1+t[p['ffn']+4]+1+3,2))
    # Semantically valid but intentionally outside the new proof subset.
    for name,kwargs in [
        ('based-normal-style',{'base':15,'extras':{15:std('Base Paragraph',sti=0xffe,style_index=15)}}),
        ('active-character-style',{'runs':[(len(text)+1,spr(0x4a30,10))]}),
        ('plain-reset',{'runs':[(len(text)+1,spr(0x2a33,0))]}),
        ('relative-size',{'runs':[(len(text)+1,spr(0x2a44,1))]}),
        ('bidi-forcing',{'runs':[(len(text)+1,spr(0x085a,1))]}),
        ('complex-script-forcing',{'runs':[(len(text)+1,spr(0x0882,1))]}),
        ('symbol-selection',{'runs':[(len(text)+1,spr(0x6a09,b'\1\0A\0'))]}),
        ('differing-latin-slots',{'runs':[(len(text)+1,spr(0x4a4f,2))]}),
        ('non-english-lid',{'runs':[(len(text)+1,spr(0x486d,0x411))]}),
        ('unknown-character-modifier',{'runs':[(len(text)+1,spr(0x2aee,1))]}),
    ]:add(name,authored(**kwargs),'unsupported')
    # Fixed style slots MAY be empty. Absence is unsupported, not malformed;
    # explicitly selecting an absent character style is a malformed reference.
    add('absent-default-character-style',authored(default_char=False),'unsupported')
    add('selected-absent-character-style',authored(default_char=False,runs=[(len(text)+1,spr(0x4a30,10))]),'refused')
    add('locale-load-font-override',authored(),'unsupported',mut=lambda w,t,p:set16(w,10,u16(w,10)|0x2000))
    # A valid unsupported active inheritance path must not hide malformed fonts.
    baseprops=spr(0x4a4f,1)+spr(0x4a51,1)+spr(0x4a43,30)
    add('inherited-base-font-valid',authored(base=15,extras={15:std('Active Base Paragraph',sti=0xffe,props=baseprops,style_index=15)}),'unsupported')
    badbase=spr(0x4a4f,65535)+spr(0x4a51,65535)
    add('inherited-base-font-invalid',authored(base=15,extras={15:std('Active Base Paragraph',sti=0xffe,props=badbase,style_index=15)}),'refused')
    add('character-style-index-invalid',authored(runs=[(len(text)+1,spr(0x4a30,65535))]),'refused')
    add('complex-prm-missing-prc',authored(),'refused',mut=lambda w,t,p:set16(t,p['clx']+19,3))
    # An absent/zero-entry font table is valid only with zero effective indices;
    # undefined fc is ignored when lcb=0, and no literal family may be invented.
    def omit_fonts(w,t,p):set32(w,0x112,0xffffffff);set32(w,0x116,0)
    add('absent-font-table-undefined-offset',authored(),'unsupported',mut=omit_fonts)
    add('zero-entry-font-table',authored(),'unsupported',mut=lambda w,t,p:(set16(t,p['ffn'],0),set32(w,0x116,4)))
    add('absent-font-nonzero-default',authored(default=2),'refused',mut=omit_fonts)
    add('absent-font-nonzero-direct',authored(runs=[(len(text)+1,spr(0x4a4f,2)+spr(0x4a51,2))]),'refused',mut=omit_fonts)
    add('absent-font-nonzero-style',authored(style=spr(0x4a4f,2)+spr(0x4a51,2)),'refused',mut=omit_fonts)
    # These are malformed, not license to silently use historical defaults.
    mutations=[
      ('font-table-out-of-bounds',lambda w,t,p:set32(w,0x112,4090)),
      ('font-record-truncated',lambda w,t,p:t.__setitem__(p['ffn']+4,255)),
      ('font-count-mismatch',lambda w,t,p:set16(t,p['ffn'],4)),
      ('font-missing-null',lambda w,t,p:set16(t,p['ffn']+4+t[p['ffn']+4]-1,65)),
      ('font-alternate-index-invalid',lambda w,t,p:t.__setitem__(p['ffn']+9,100)),
      ('font-default-index-invalid',lambda w,t,p:set16(t,p['stsh']+14,99)),
      ('style-table-truncated',lambda w,t,p:set32(w,0xa6,25)),
      ('style-record-truncated',lambda w,t,p:set16(t,p['style0'],4000)),
      ('style-count-mismatch',lambda w,t,p:set16(t,p['stsh']+2,16)),
      ('style-active-index-invalid',lambda w,t,p:set16(w,p['papg']*512+508,99)),
      ('style-upx-identity-mismatch',lambda w,t,p:set16(t,p['style0']+30,3)),
      ('chpx-page-out-of-bounds',lambda w,t,p:set32(t,p['chplc']+8,100)),
      ('chpx-zero-count',lambda w,t,p:w.__setitem__(p['chpg']*512+511,0)),
      ('chpx-gap-start',lambda w,t,p:set32(w,p['chpg']*512,p['fc']+1)),
      ('chpx-gap-end',lambda w,t,p:set32(w,p['chpg']*512+4,p['fc']+p['text_len']-1)),
      ('chpx-desc-overlap',lambda w,t,p:w.__setitem__(p['chpg']*512+8,1)),
      ('chpx-plc-gap',lambda w,t,p:set32(t,p['chplc'],p['fc']+1)),
      ('papx-page-out-of-bounds',lambda w,t,p:set32(t,p['paplc']+8,100)),
      ('papx-gap-start',lambda w,t,p:set32(w,p['papg']*512,p['fc']+1)),
      ('papx-desc-overlap',lambda w,t,p:w.__setitem__(p['papg']*512+8,1)),
      ('papx-payload-truncated',lambda w,t,p:w.__setitem__(p['papg']*512+507,255)),
      ('fib-csw-invalid',lambda w,t,p:set16(w,32,13)),
      ('fib-cslw-invalid',lambda w,t,p:set16(w,62,21)),
      ('fib-paircount-invalid',lambda w,t,p:set16(w,0x98,94)),
      ('clx-cp-incomplete',lambda w,t,p:set32(t,p['clx']+9,p['text_len']-1)),
      ('clx-truncated',lambda w,t,p:set32(w,0x1a6,20)),
      ('resource-font-count',lambda w,t,p:set16(t,p['ffn'],257)),
      ('resource-style-count',lambda w,t,p:set16(t,p['stsh']+2,4095)),
      ('resource-source-cp',lambda w,t,p:set32(w,0x4c,1000001)),
      ('resource-property-bytes',lambda w,t,p:set32(w,0xa6,1048577)),
      ('duplicate-word-stream',lambda w,t,p:None),
    ]
    for name,mut in mutations:add(name,authored(),'refused',mut=mut)
    for name,prop in [('direct-font-index-invalid',spr(0x4a4f,99)+spr(0x4a51,99)),('size-below-model',spr(0x4a43,1)),('size-above-model',spr(0x4a43,3277)),('truncated-variable-operand',bytes.fromhex('3eca05aa'))]:
        add(name,authored(runs=[(len(text)+1,prop)]),'refused')
    edge='  This source has significant edge spaces.  '
    add('source-edge-space-identity',authored(text=edge),expect=[{'chars':expected_chars(edge,'Times New Roman',20)}])
    add('based-normal-wrong-kind',authored(base=10),'refused')
    add('clx-extra-terminal-cp',authored(),'unsupported',mut=lambda w,t,p:set32(t,p['clx']+9,p['text_len']+1))
    add('active-font-leading-space',authored(fonts=(' Times New Roman',)),'refused')
    add('active-font-too-long',authored(fonts=('A'*32,)),'refused')
    # CP1252 characters that are not the proven Latin text subset.
    def make_uncompressed(w,t,p):
        set32(t,p['clx']+15,p['fc']);w[p['fc']:p['fc']+2*p['text_len']]=(text+'\r').encode('utf-16le')
        for off in (28,p['chpg']*512+4,p['papg']*512+4):set32(w,off,p['fc']+2*p['text_len'])
        for off in (p['chplc']+4,p['paplc']+4):set32(t,off,p['fc']+2*p['text_len'])
    add('unsupported-uncompressed-piece',authored(),'unsupported',mut=make_uncompressed)
    manifest={'classification':'Independent author controls, not genuine source documents','source_oracle':'Independent Python CFB/STSH/FFN/CLX/FKP reader; product parser not used','checker_sha256':sha(__file__),'investigation_report_sha256':INVEST_SHA,'cases':cases}
    save(dest/'manifest.json',manifest);print(dest/'manifest.json');return dest/'manifest.json'

def normalize_nontypography(doc):
    out=copy.deepcopy(doc)
    for b in out['content']:
        if 'content' not in b:continue
        rows=[]
        for node in b['content']:
            if node['type']!='text':rows.append(node);continue
            m=[x for x in node.get('marks',[]) if x['type'] not in ('fontFamily','fontSize')]
            rows.extend({'text':c,'marks':m} for c in node['text'])
        b['content']=rows
    return out

def xml_chars(path):
    import zipfile
    with zipfile.ZipFile(path) as z:root=ET.fromstring(z.read('word/document.xml'))
    blocks=[]
    for p in root.findall('w:body/w:p',NS):
        if p.find('.//w:br[@w:type="page"]',NS) is not None:blocks.append({'type':'pageBreak'});continue
        rows=[]
        for r in p.findall('w:r',NS):
            text=''.join(n.text or '' for n in r.findall('w:t',NS))
            if not text:continue
            f=r.find('w:rPr/w:rFonts',NS);sz=r.find('w:rPr/w:sz',NS);cs=r.find('w:rPr/w:szCs',NS)
            require(f is not None and sz is not None and cs is not None,'Explicit DOCX font and sizes required')
            names=[f.get('{'+W+'}'+slot) for slot in ('ascii','hAnsi','eastAsia','cs')]
            require(len(set(names))==1 and names[0] is not None,'Four literal font slots must agree')
            require(sz.get('{'+W+'}val')==cs.get('{'+W+'}val'),'sz and szCs agree')
            rows.extend([[c,names[0],int(sz.get('{'+W+'}val'))] for c in text])
        blocks.append({'chars':rows})
    return blocks

def strip_type(rows):return [{k:v for k,v in b.items() if k!='type'} if b.get('type')!='pageBreak' else b for b in rows]

def patch_reference(old,path,expect):
    """Only font/size patch on frozen pre-feature output, authored from oracle."""
    import zipfile
    with zipfile.ZipFile(old) as zin,zipfile.ZipFile(path,'x',zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data=zin.read(item.filename)
            if item.filename=='word/document.xml':
                root=ET.fromstring(data);ps=root.findall('w:body/w:p',NS);require(len(ps)==len(expect),'Reference paragraph identity')
                for p,e in zip(ps,expect):
                    if e.get('type')=='pageBreak':continue
                    seq=e['chars'];off=0
                    for r in list(p.findall('w:r',NS)):
                        ts=r.findall('w:t',NS);text=''.join(t.text or '' for t in ts)
                        require(''.join(c[0] for c in seq[off:off+len(text)])==text,'Reference exact run text identity')
                        if not text:continue
                        chunks=[]
                        for row in seq[off:off+len(text)]:
                            if chunks and chunks[-1][1:]==row[1:]:chunks[-1][0]+=row[0]
                            else:chunks.append(list(row))
                        index=list(p).index(r);p.remove(r)
                        for j,(txt,name,hps) in enumerate(chunks):
                            nr=copy.deepcopy(r)
                            for t in list(nr.findall('w:t',NS)):nr.remove(t)
                            rp=nr.find('w:rPr',NS)
                            if rp is None:rp=ET.Element('{'+W+'}rPr');nr.insert(0,rp)
                            for tag in ('rFonts','sz','szCs'):
                                for el in list(rp.findall('w:'+tag,NS)):rp.remove(el)
                            ff=ET.Element('{'+W+'}rFonts',{'{'+W+'}'+slot:name for slot in ('ascii','hAnsi','eastAsia','cs')});rp.insert(0,ff)
                            for tag in ('sz','szCs'):ET.SubElement(rp,'{'+W+'}'+tag,{'{'+W+'}val':str(hps)})
                            te=ET.SubElement(nr,'{'+W+'}t',{'{http://www.w3.org/XML/1998/namespace}space':'preserve'});te.text=txt;p.insert(index+j,nr)
                        off+=len(text)
                    require(off==len(seq),'Reference full text coverage')
                data=ET.tostring(root,encoding='utf-8',xml_declaration=True)
            zout.writestr(item,data)

def run_checks(args):
    cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'CLI pin mismatch')
    baseline=ROOT/'tmp/paragraph-pagination-final-cli';require(sha(baseline)=='35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47','Baseline pin')
    manifest=json.loads(args.manifest.read_text());args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='run-',dir=args.root))
    report={'status':'running','checker_sha256':sha(__file__),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'cli':str(cli),'cli_sha256':sha(cli),'baseline_sha256':sha(baseline),'commands':[],'cases':[],'failures':[],'counts':{'preserved':0,'unsupported':0,'refused':0,'trusted_reopen':0,'no_overwrite':0,'new_pdfs':0,'fontonly_pdf_pairs':0,'source_output_metric_pairs':0,'trusted_tamper_refusals':0}}
    sources=[Path(c['path']) for c in manifest['cases']];protected=sources+[cli,baseline,Path(__file__),args.manifest,ROOT/'src/legacy_typography.rs',ROOT/'src/importing.rs',ROOT/'src/lib.rs'];before={str(p):sha(p)for p in protected}
    def run(argv,ok=None):
        p=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=120)
        report['commands'].append({'argv':list(map(str,argv)),'code':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
        if ok is not None:require((p.returncode==0)==ok,'Unexpected command result: '+str(argv)+' '+p.stderr)
        return p
    def flush(): (dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    for case in manifest['cases']:
        row={k:v for k,v in case.items() if k not in ('expected','raw_oracle')};report['cases'].append(row)
        try:
            source=Path(case['path']);require(sha(source)==case['sha256'],'Source pin changed')
            p=run([cli,'import-doc',source]);row['returncode']=p.returncode;row['stderr']=p.stderr
            if case['outcome']=='refused':require(p.returncode!=0,'Malformed/source-identity control was accepted');report['counts']['refused']+=1
            else:
                require(p.returncode==0,'Valid control rejected: '+p.stderr);j=json.loads(p.stdout);save(dest/(case['name']+'.import.json'),j)
                risks=j['compatibility']['risks'];row['risks']=risks
                preserved=any('Legacy DOC literal Latin font names and effective sizes are preserved' in r for r in risks)
                if case['outcome']=='unsupported':
                    require(not preserved,'Unsupported control falsely claimed preservation')
                    require(any('Legacy DOC font names and effective sizes are not preserved: ' in r for r in risks),'Unsupported control missing visible typography warning')
                    report['counts']['unsupported']+=1
                else:
                    require(preserved,'Positive control missing explicit preservation diagnostic')
                    actual=strip_type(expanded_model(j['document']));require(actual==case['expected'],'Imported character fonts/sizes disagree with independent source/author oracle')
                    old=run([baseline,'import-doc',source],True);oldj=json.loads(old.stdout)
                    require(normalize_nontypography(j['document'])==normalize_nontypography(oldj['document']),'Non-typography model changed')
                    model=dest/(case['name']+'.model.json');save(model,j['document']);out=dest/(case['name']+'.docx');run([cli,'create-docx',model,out],True)
                    require(xml_chars(out)==case['expected'],'DOCX XML literal slots/halfpoints differ from oracle')
                    reopened=json.loads(run([cli,'read-docx-model',out],True).stdout);require(reopened==j['document'],'Trusted reopen complete model mismatch');report['counts']['trusted_reopen']+=1
                    for existing in (source,out):
                        pin=sha(existing);run([cli,'create-docx',model,existing],False);require(sha(existing)==pin,'No-overwrite bytes changed');report['counts']['no_overwrite']+=1
                    oldmodel=dest/(case['name']+'.baseline-model.json');save(oldmodel,oldj['document']);oldout=dest/(case['name']+'.baseline.docx');run([baseline,'create-docx',oldmodel,oldout],True)
                    ref=dest/(case['name']+'.font-only-reference.docx');patch_reference(oldout,ref,case['expected']);require(xml_chars(ref)==case['expected'],'Reference XML typography differs')
                    row.update(model=str(model),docx=str(out),docx_sha256=sha(out),reference=str(ref),reference_sha256=sha(ref));report['counts']['preserved']+=1
            row['status']='passed'
        except Exception as e:
            row['status']='failed';row['error']=str(e);report['failures'].append({'name':case['name'],'error':str(e)})
        flush()
    # Newly explicit font declarations remain bound to the embedded trusted model.
    if not report['failures']:
        import zipfile
        source=Path(next(r['docx'] for r in report['cases'] if r['outcome']=='preserved'))
        for label in ('font-name','halfpoint-size','missing-font-slot','complex-size'):
            target=dest/('tampered-'+label+'.docx')
            with zipfile.ZipFile(source) as zi,zipfile.ZipFile(target,'x',zipfile.ZIP_DEFLATED) as zo:
                for item in zi.infolist():
                    data=zi.read(item.filename)
                    if item.filename=='word/document.xml':
                        root=ET.fromstring(data)
                        if label=='font-name':root.find('.//w:rFonts',NS).set('{'+W+'}ascii','Arial')
                        elif label=='halfpoint-size':root.find('.//w:rPr/w:sz',NS).set('{'+W+'}val','21')
                        elif label=='missing-font-slot':del root.find('.//w:rFonts',NS).attrib['{'+W+'}hAnsi']
                        else:root.find('.//w:rPr/w:szCs',NS).set('{'+W+'}val','22')
                        data=ET.tostring(root,encoding='utf-8',xml_declaration=True)
                    zo.writestr(item,data)
            run([cli,'read-docx-model',target],False);report['counts']['trusted_tamper_refusals']+=1
        flush()
    if args.render:
        require(not report['failures'],'Stop before consumers on source/CLI failure')
        try:
            consumer_checks(dest,report,run,flush)
        except Exception as e:
            report['failures'].append({'stage':'consumer','error':str(e)});flush()
    report['after']={str(p):sha(p)for p in protected};require(report['after']==before,'Protected source/CLI/checker changed')
    report['protected_before']=before;report['status']='passed-with-explicit-layout-heading-fallback-limits' if not report['failures'] else 'FAILED';flush()
    print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'counts':report['counts'],'failures':report['failures']},indent=2))
    if report['failures']:raise SystemExit(1)

def consumer_checks(dest,report,run,flush):
    import fitz
    lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice')
    lo_bin=Path('/opt/libreoffice/program/soffice.bin')
    # Resolve the actual fixed consumer binary from the launcher, not PATH guesses.
    candidates=list(Path('/opt').glob('**/program/soffice.bin'))
    require(len(candidates)==1,'Consumer executable identity must be unambiguous');lo_bin=candidates[0]
    report['consumer']={'launcher':str(lo),'binary':str(lo_bin),'sha256':sha(lo_bin),'version':run([lo,'--version'],True).stdout,'pymupdf':fitz.VersionBind,'poppler_sha256':sha('/usr/bin/pdftotext'),'runs':[],'comparisons':[],'known_limitations':[]}
    consumer_pin=sha(lo_bin)
    def render(path,label):
        require(shutil.disk_usage(dest).free>=650*1024*1024,'650 MiB free-space floor before independent consumer')
        d=dest/label;d.mkdir();profile=d/'profile';home=d/'home';home.mkdir()
        env=dict(os.environ,HOME=str(home),XDG_CACHE_HOME=str(d/'cache'),XDG_CONFIG_HOME=str(d/'config'))
        argv=[str(lo),'-env:UserInstallation='+profile.as_uri(),'--headless','--convert-to','pdf','--outdir',str(d),str(path)]
        pr=subprocess.run(argv,capture_output=True,text=True,timeout=120,env=env)
        command={'argv':argv,'HOME':str(home),'XDG_CACHE_HOME':str(d/'cache'),'returncode':pr.returncode,'stdout':pr.stdout,'stderr':pr.stderr}
        report['consumer']['runs'].append(command)
        pdf=d/(path.stem+'.pdf');require(pr.returncode==0 and pdf.exists(),'Fresh consumer PDF missing: '+pr.stderr)
        doc=fitz.open(pdf);pages=[]
        for page in doc:
            spans=[sp for block in page.get_text('dict')['blocks'] if 'lines' in block for line in block['lines'] for sp in line['spans']]
            raw=page.get_text('rawdict');chars=[]
            for block in raw['blocks']:
                if 'lines' not in block:continue
                for line in block['lines']:
                    for sp in line['spans']:
                        chars.extend({'c':c['c'],'font':sp['font'],'size':sp['size'],'origin':c['origin'],'bbox':c['bbox']} for c in sp['chars'])
            pix=page.get_pixmap(matrix=fitz.Matrix(1,1))
            pages.append({'text':page.get_text(),'width':page.rect.width,'height':page.rect.height,'fonts':sorted({(sp['font'],sp['size'])for sp in spans}),'spans':spans,'chars':chars,'words':page.get_text('words'),'raster_sha256':hashlib.sha256(pix.samples).hexdigest()})
        text=run(['/usr/bin/pdftotext','-layout',pdf,'-'],True).stdout;(d/'poppler.txt').write_text(text)
        # Actual inspection images only for selected representatives; every page still raster-hashed.
        if label in ('genuine-simple-output','genuine-simple-source','genuine-SampleDoc-output','genuine-SampleDoc-source','normal-style-halfpoint-source','normal-style-halfpoint-output'):
            for page in doc:page.get_pixmap(matrix=fitz.Matrix(1,1)).save(d/('page-'+str(page.number+1)+'.png'))
        row={'input':str(path),'input_sha256':sha(path),'pdf':str(pdf),'pdf_sha256':sha(pdf),'pages':pages,'profile':str(profile),'home':str(home)}
        report['counts']['new_pdfs']+=1;flush();return row
    rows={r['name']:r for r in report['cases']}
    for row in report['cases']:
        if row['outcome']!='preserved':continue
        name=row['name'];output=render(Path(row['docx']),name+'-output');reference=render(Path(row['reference']),name+'-reference');row['consumer_output']=output;row['consumer_reference']=reference
        # Strict no-tolerance comparison including all spans, full text and whole-page rasters.
        require(output['pages']==reference['pages'],'Font-only reference/output consumer mismatch: '+name)
        report['counts']['fontonly_pdf_pairs']+=1;report['consumer']['comparisons'].append({'name':name,'kind':'font-only-independent-XML-reference','full_page_text_spans_geometry_raster_equal':True});flush()
    for name in ('genuine-simple','genuine-SampleDoc','normative-default-20','normal-style-halfpoint','stshi-default-name'):
        row=rows[name];source=render(Path(row['path']),name+'-source');row['consumer_source']=source;out=row['consumer_output']
        require(len(source['pages'])==len(out['pages']),'Source/output page count changed for bounded sample '+name)
        if name.startswith('genuine-'):
            for a,b in zip(source['pages'],out['pages']):
                require(''.join(a['text'].split())==''.join(b['text'].split()),'Genuine per-page full text identity')
            if name=='genuine-simple':
                require(source['pages'][0]['fonts']==[('LiberationSerif',10.0)] and out['pages'][0]['fonts']==[('LiberationSerif',10.0)],'simple consumed source/output font and size')
                report['consumer']['known_limitations'].append({'name':name,'source_page':[source['pages'][0]['width'],source['pages'][0]['height']],'output_page':[out['pages'][0]['width'],out['pages'][0]['height']],'full_layout_equal':False,'note':'Source Letter/90pt margin versus output A4/72pt remains outside guarantee'})
            else:
                require(source['pages'][0]['fonts']==[('Carlito',11.0)],'SampleDoc source page1 expected font/size')
                require(out['pages'][0]['fonts']==[('Carlito',11.0),('Carlito-Bold',11.0)],'Known inferred heading weight remains visible')
                require(source['pages'][1]['fonts']==[('NotoSans-Regular',16.0)] and out['pages'][1]['fonts']==[('NotoSerif-Regular',16.0)],'Known unavailable Arial Black fallback difference remains visible')
                report['consumer']['known_limitations'].append({'name':name,'source_fonts':[p['fonts']for p in source['pages']],'output_fonts':[p['fonts']for p in out['pages']],'heading_weight_equal':False,'arial_black_substitute_equal':False,'full_layout_equal':False})
        else:
            a=source['pages'][0];b=out['pages'][0]
            # Paragraph geometry is intentionally not imported. Compare installed face,
            # physical halfpoint size and each corresponding word advance independently.
            require(a['fonts']==b['fonts'],'Installed source/output consumed font resources differ: '+name)
            wa=a['words'];wb=b['words'];require([w[4]for w in wa]==[w[4]for w in wb],'Installed source/output full words differ')
            diffs=[abs((x[2]-x[0])-(y[2]-y[0]))for x,y in zip(wa,wb)]
            require(max(diffs,default=0)<=0.001,'Installed glyph word advances differ: '+name)
            report['consumer']['comparisons'].append({'name':name,'kind':'independent-binary-source-installed-font','fonts':a['fonts'],'word_count':len(wa),'max_word_advance_difference_pt':max(diffs,default=0),'glyph_metric_tolerance_pt':0.001,'paragraph_geometry_claimed':False})
            report['counts']['source_output_metric_pairs']+=1
        flush()
    require(sha(lo_bin)==consumer_pin,'Consumer executable changed during run')

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--root',type=Path,default=DEFAULT);ap.add_argument('--prepare',action='store_true');ap.add_argument('--write-owned-fixture',action='store_true');ap.add_argument('--check-owned-fixture',action='store_true');ap.add_argument('--manifest',type=Path);ap.add_argument('--cli',type=Path);ap.add_argument('--expected-cli-sha256');ap.add_argument('--render',action='store_true');args=ap.parse_args()
    if args.write_owned_fixture or args.check_owned_fixture:owned_blank_fixture(args.write_owned_fixture);return
    if args.prepare:prepare(args.root);return
    require(args.manifest and args.cli and args.expected_cli_sha256,'Use --prepare or give manifest and pinned CLI')
    run_checks(args)

if __name__=='__main__':main()
