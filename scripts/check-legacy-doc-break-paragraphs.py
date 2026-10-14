"""Independent source-bound DOC break-paragraph validation.

Expected models are exact deltas from a SHA-pinned pre-feature CLI. Binary source
oracles and OOXML references use only independent Python decoders/encoders; no
candidate implementation is imported. Every invocation writes a fresh directory.
Historical reports, source files and the frozen four-cell experiment are immutable.
"""
import argparse, copy, hashlib, importlib.util, json, os, re, shutil, struct, subprocess, tempfile, zipfile
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT=Path(__file__).resolve().parent.parent
DEFAULT=ROOT/'tmp/doc-break-semantics-verify'
BASELINE=ROOT/'tmp/legacy-doc-page-geometry-final-cli'
BASELINE_SHA='af649bffb9a1adc82a42ae3ba7c006d159633a9cd126eb9b29777ca3ab9c2652'
HISTORY=ROOT/'tmp/legacy-doc-page-geometry-verify/geometry-run-y4vwa5nb/report.json'
HISTORY_SHA='2d04a340d85f9f41cea7bd1106d784b48fed3659b6380ee52ad0507181fddc0d'
EXPERIMENT=ROOT/'tmp/doc-break-semantics-experiment-20261014/report.json'
EXPERIMENT_SHA='91a714cc684511dc823553bcc705394aa47483c557930edb12859d5a9f24191d'
HELPERS={'check-legacy-doc-fonts.py':'18787d17213a10a7a7c2c43ec90f20a308c2a9cf5f011a7452db48df193776db','check-legacy-doc-normal-identity.py':'e59a6a2e90cc51b80814934ce5d4b2fcbaff01e11b7082cc257e8c70e6825894','check-legacy-doc-normal-spacing.py':'90398b500f62a57f5d8c9e354cff94ae5169553241a3a0a01ef255c2cd4dd429','check-legacy-doc-page-geometry.py':'bc06d2ff7b9a5fad7fbfe4fdeea25207fc6f3e0bb88cd8aedabab55c6db16d82'}

def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def require(value,reason):
    if not value:raise AssertionError(reason)
def save(p,data):
    raw=data if isinstance(data,bytes)else json.dumps(data,indent=2,ensure_ascii=False).encode()+b'\n'
    with Path(p).open('xb')as f:f.write(raw)
def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
for name,pin in HELPERS.items():require(sha(ROOT/'scripts'/name)==pin,'Independent helper changed: '+name)
c=module('break_source_fonts',ROOT/'scripts/check-legacy-doc-fonts.py')
n=module('break_source_normal',ROOT/'scripts/check-legacy-doc-normal-identity.py')
s=module('break_source_spacing',ROOT/'scripts/check-legacy-doc-normal-spacing.py')
g=module('break_source_geometry',ROOT/'scripts/check-legacy-doc-page-geometry.py')
W,NS=c.W,c.NS
ROOT_KEY='splitPageBreakAndParagraphMark';ATOM_KEY='breakParagraph'
PRESERVED='Legacy DOC split-page-break compatibility and complete Latin paragraph-mark metrics are preserved for {count} standalone manual break paragraph(s); browser printing does not reproduce the retained next-page mark line'
FALLBACK='Legacy DOC split-page-break paragraph semantics are not preserved for any source break: '


def raw_oracle(path):
    """Read explicit DOP, full Normal/PAPX identity, CHPX and exact break CPs."""
    font=c.source_oracle(path);spacing=s.spacing_oracle(path);ss=c.streams(path);w=ss['WordDocument'];t=ss['1Table'if c.u16(w,10)&512 else '0Table']
    off,size=struct.unpack_from('<II',w,0x192);dop=t[off:off+size]if size else b''
    require(not size or off+size<=len(t),'DOP bounds')
    version=font['effective_nFib'];supported_layout=version==0x112 and size in (674,690,694)
    enabled=supported_layout and bool(c.u32(dop,512)&(1<<27))
    po,pn=struct.unpack_from('<II',w,0xca);plc=t[po:po+pn]
    require(pn>=4 and (pn-4)%16==0 and po+pn<=len(t),'Section PLC framing')
    cnt=(pn-4)//16;cps=[c.u32(plc,4*i)for i in range(cnt+1)]
    breaks=[]
    for block_index,p in enumerate(spacing['paragraphs']):
        if p['type']!='pageBreak':continue
        a,b=p['start'],p['end'];require(b-a==2 and font['text'][a:b]=='\f\r','Standalone break/CR identity')
        metrics=[]
        for cp in (a,b-1):
            run=[r for r in font['runs']if r['start']<=cp<r['end']];require(len(run)==1,'Complete unique font run coverage')
            metrics.append({'family':run[0]['name'],'halfPoints':run[0]['halfPoints']})
        same=metrics[0]==metrics[1];manual=a+1 not in cps[1:]
        metric=None
        if same and manual and p['supported']:
            metric={'font':metrics[0],'spaceBefore':p['tuple'][0],'spaceAfter':p['tuple'][1],'lineSpacing':p['tuple'][2]}
        breaks.append({'block_index':block_index,'cp':[a,b],'raw_hex':'0c0d','papx_sprms':p['papx_sprms'],'break_font':metrics[0],'mark_font':metrics[1],'fonts_equal':same,'manual_nonsection':manual,'spacing_supported':p['supported'],'metrics':metric})
    return {'source_sha256':sha(path),'effective_nFib':version,'dop':{'offset':off,'length':size,'extended_supported':supported_layout,'copts2_bytes':dop[512:516].hex()if supported_layout else None,'split_bit27':bool(enabled)},'section_cps':cps,'source_text':font['text'],'normal':spacing['normal_style'],'breaks':breaks,'all_breaks_proven':bool(breaks)and all(b['metrics']is not None for b in breaks)}


def authored(text='First ordinary text.\r\f\rSecond ordinary text.',version=0x112,dop_length=None,flag=True,fonts=('Liberation Serif','Liberation Sans'),hps=22,spacing=None,direct=None,runs=None):
    """Independent binary author; preserves char UPX while setting Normal PAPX."""
    paps=s.before(0)+s.after(200)+s.line(276)if spacing is None else spacing
    w,t,p=c.authored(text=text,version=version,basebytes=18 if version==0x112 else 10,fonts=fonts,style=c.spr(0x4a43,hps),runs=runs)
    start=c.u32(w,0xa2);length=c.u32(w,0xa6);old=bytes(t[start:start+length]);count=c.u16(old,20)
    newnormal=c.std('Normal',paps=paps,props=c.spr(0x486d,0x409)+c.spr(0x4a43,hps),basebytes=c.u16(old,4))
    st=old[:20]+newnormal+old[22+count:];delta=len(st)-len(old);t[start:start+length]=st;c.set32(w,0xa6,len(st))
    for field in (0x112,0x1a2,0xfa,0x102,0xca):c.set32(w,field,c.u32(w,field)+delta)
    if direct is not None:w,t,p=n.with_papx((w,t,p),[(0,x)for x in direct])
    w,t,p=g.section((w,t,p),dop_length=dop_length)
    if flag and c.u32(w,0x196)>=674:c.set32(t,p['dop']+512,1<<27)
    return w,t,p


def prepare(args):
    require(sha(BASELINE)==BASELINE_SHA and sha(HISTORY)==HISTORY_SHA and sha(EXPERIMENT)==EXPERIMENT_SHA,'Frozen inputs changed')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='break-fixtures-',dir=args.root));cases=[]
    old=json.loads(HISTORY.read_text())
    for r in old['cases']:
        x={k:r[k]for k in ('name','path','sha256','kind')};x.update(historical=True,baseline_returncode=r['returncode'],outcome='preserved'if r['name']=='genuine-SampleDoc'else 'unchanged',render=r['name']=='genuine-SampleDoc')
        if x['outcome']=='preserved':x['oracle']=raw_oracle(Path(x['path']));require(x['oracle']['all_breaks_proven'],'Genuine break proof')
        cases.append(x)
    def add(name,parts,outcome='preserved',render=False,mutate=None):
        w,t,p=parts
        if mutate:mutate(w,t,p)
        path=dest/(name+'.doc');save(path,c.compound(w,t));row={'name':name,'path':str(path),'sha256':sha(path),'kind':'independently-authored-binary-break-control','historical':False,'outcome':outcome,'render':render}
        if outcome=='preserved':
            o=raw_oracle(path);require(o['dop']['split_bit27']and o['all_breaks_proven'],'Author recipe lacks complete positive proof: '+name);row['oracle']=o
        cases.append(row)
    for length in (674,690,694):add('break-dop-'+str(length),authored(dop_length=length),render=length==674)
    for name,text in [('leading','\f\rFirst visible paragraph.'),('trailing','Last visible paragraph.\r\f'),('consecutive','Before.\r\f\r\f\rAfter.'),('adjacent-empty','Before.\r\r\f\r\rAfter.'),('only','\f'),('multiple','One.\r\f\rTwo.\r\f\rThree.')]:add('break-'+name,authored(text=text),render=name in ('leading','trailing','consecutive'))
    text='Before.\r\f\rAfter.';cp=text.index('\f');total=len(text)+1
    add('break-split-chpx-equal',authored(text=text,runs=[(cp+1,b''),(cp+2,b''),(total,b'')]))
    add('break-split-chpx-equal-explicit',authored(text=text,runs=[(cp+1,b''),(cp+2,c.spr(0x4a43,22)),(total,b'')]))
    bothfont=c.spr(0x4a4f,1)+c.spr(0x4a51,1)+c.spr(0x4a43,27)
    add('break-font-independent-neighbor',authored(text=text,runs=[(cp,b''),(cp+2,bothfont),(total,b'')]))
    add('break-direct-spacing',authored(text=text,direct=[b'',s.before(40)+s.after(120)+s.line(360),b'']))
    add('break-partial-direct-zero',authored(text=text,direct=[b'',s.after(0),b'']))
    add('break-spacing-last-valid',authored(text=text,direct=[b'',s.after(999)+s.after(200)+s.line(360)+s.line(276),b'']))
    for name,props in [('font',c.spr(0x4a4f,1)+c.spr(0x4a51,1)),('size',c.spr(0x4a43,24)),('both',bothfont)]:
        add('break-mark-only-different-'+name,authored(text=text,runs=[(cp+1,b''),(cp+2,props),(total,b'')]),'fallback')
        add('break-only-different-'+name,authored(text=text,runs=[(cp,b''),(cp+1,props),(total,b'')]),'fallback')
    multi='One.\r\f\rTwo.\r\f\rThree.';second=multi.rindex('\f');total2=len(multi)+1
    add('break-late-font-mismatch',authored(text=multi,runs=[(second+1,b''),(second+2,bothfont),(total2,b'')]),'fallback')
    first=multi.index('\f')
    add('break-first-font-mismatch',authored(text=multi,runs=[(first+1,b''),(first+2,bothfont),(total2,b'')]),'fallback')
    different=c.spr(0x4a43,31)
    add('break-multiple-independent-metrics',authored(text=multi,runs=[(first,b''),(first+2,bothfont),(second,b''),(second+2,different),(total2,b'')],direct=[b'',s.after(80)+s.line(240),b'',s.before(60)+s.after(300)+s.line(360),b'']))
    add('break-late-spacing-unsupported',authored(text=multi,direct=[b'',b'',b'',s.line(240,0),b'']),'fallback')
    for name,ops in [('minimum',s.line(240,0)),('exact',s.line(-240,1)),('zero',s.line(0,1)),('too-small',s.line(119)),('too-large',s.line(2401))]:add('break-spacing-'+name,authored(text=text,direct=[b'',ops,b'']),'fallback')
    add('break-explicit-off',authored(flag=False),'unchanged')
    add('break-absent-dop',authored(),'unchanged',mutate=lambda w,t,p:(c.set32(w,0x192,0xffffffff),c.set32(w,0x196,0)))
    add('break-word97-dop500',authored(version=0xc1),'unchanged')
    add('break-no-breaks-bit-on',authored(text='Ordinary paragraph without a break.'),'unchanged')
    add('break-dop-unrelated-bit',authored(flag=False),'unchanged',mutate=lambda w,t,p:c.set32(t,p['dop']+512,1<<26))
    for length in (0,499,500,512,515,673,675,691,695):
        if length==0:continue
        add('break-malformed-dop-length-'+str(length),authored(),'refusal',mutate=lambda w,t,p,l=length:c.set32(w,0x196,l))
    add('break-malformed-dop-bounds',authored(),'refusal',mutate=lambda w,t,p:c.set32(w,0x192,0x7fffffff))
    for offset in (8,84,508):add('break-conflicting-dop-copy-'+str(offset),authored(),'refusal',mutate=lambda w,t,p,o=offset:c.set16(t,p['dop']+o,1))
    add('break-missing-section',authored(),'old-route',mutate=lambda w,t,p:(c.set32(w,0xca,0xffffffff),c.set32(w,0xce,0)))
    def section_end(w,t,p):
        off=c.u32(w,0xca);sed=bytes(t[off+8:off+20]);br=text.index('\f');new=c.p32(0)+c.p32(br+1)+c.p32(c.u32(w,76))+sed+sed;c.set32(w,0xca,len(t));c.set32(w,0xce,len(new));t.extend(new)
    add('break-section-ending',authored(text=text),'old-route',mutate=section_end)
    add('break-mixed-text-paragraph',authored(text='Before\fafter.\rLast.'),'old-route')
    # Frozen-old-only calibration proves the lowercase spaces are preserved.
    # Uppercase edge spaces trigger an existing end-to-end identity refusal;
    # mutable-model late failure/rollback remains a separately injected unit test.
    add('break-postbreak-whitespace',authored(text='First ordinary paragraph.\r\f\r  short edge  '))
    add('break-postbreak-uppercase-edge-refusal',authored(text='Before ordinary text.\r\f\r  SHORT TITLE  '),'refusal')
    add('break-malformed-mark-font',authored(text=text,runs=[(cp+1,b''),(cp+2,c.spr(0x4a4f,65535)+c.spr(0x4a51,65535)),(len(text)+1,b'')]),'refusal')
    add('break-malformed-mark-size',authored(text=text,runs=[(cp+1,b''),(cp+2,c.spr(0x4a43,0)),(len(text)+1,b'')]),'refusal')
    add('break-malformed-late-spacing',authored(text=text,direct=[b'',s.after(31681),b'']),'refusal')
    add('break-malformed-overwritten-spacing',authored(text=text,direct=[b'',s.after(31681)+s.after(200),b'']),'refusal')
    add('break-supported-overridden-spacing',authored(text=text,direct=[b'',s.line(-240,1)+s.line(276),b'']))
    add('break-unknown-character-proof',authored(text=text,runs=[(len(text)+1,c.spr(0x2aee,1))]),'unchanged')
    add('break-unknown-paragraph-proof',authored(text=text,direct=[b'',c.spr(0x2405,1),b'']),'unchanged')
    manifest={'checker_sha256':sha(__file__),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_report':str(HISTORY),'historical_report_sha256':HISTORY_SHA,'experiment':str(EXPERIMENT),'experiment_sha256':EXPERIMENT_SHA,'helper_pins':HELPERS,'cases':cases}
    save(dest/'manifest.json',manifest);print(dest/'manifest.json')


def expected_model(old,oracle):
    model=copy.deepcopy(old['document']);require(oracle['dop']['split_bit27']and oracle['all_breaks_proven'],'Positive source proof required')
    model.setdefault('attrs',{})[ROOT_KEY]=True
    for b in oracle['breaks']:
        target=model['content'][b['block_index']];require(target=={'type':'pageBreak'},'Frozen model/source break identity')
        target['attrs']={ATOM_KEY:copy.deepcopy(b['metrics'])}
    return model


def metrics_xml(value):
    font=value['font'];family=font['family'].replace('&','&amp;').replace('"','&quot;').replace('<','&lt;').replace('>','&gt;')
    rp='<w:rPr><w:rFonts w:ascii="'+family+'" w:hAnsi="'+family+'"/><w:sz w:val="'+str(font['halfPoints'])+'"/></w:rPr>'
    spacing='<w:spacing w:before="'+str(value['spaceBefore'])+'" w:after="'+str(value['spaceAfter'])+'" w:line="'+str(value['lineSpacing'])+'" w:lineRule="auto"/>'
    return '<w:p><w:pPr>'+spacing+rp+'</w:pPr><w:r>'+rp+'<w:br w:type="page"/></w:r></w:p>'


def xml_reference(base,out,model):
    """Construct direct source-controlled expected OOXML from old package bytes."""
    with zipfile.ZipFile(base)as z:parts={name:z.read(name)for name in z.namelist()}
    doc=parts['word/document.xml'].decode();bare='<w:p><w:r><w:br w:type="page"/></w:r></w:p>';breaks=[x for x in model['content']if x['type']=='pageBreak'];require(doc.count(bare)==len(breaks),'Frozen export break count')
    chunks=doc.split(bare);outdoc=chunks[0]
    for atom,chunk in zip(breaks,chunks[1:]):
        value=atom.get('attrs',{}).get(ATOM_KEY);outdoc+=(metrics_xml(value)if value else bare)+chunk
    parts['word/document.xml']=outdoc.encode()
    enabled=model.get('attrs',{}).get(ROOT_KEY)
    needs_settings=enabled or any(ATOM_KEY in atom.get('attrs',{})for atom in breaks)
    if needs_settings and 'word/settings.xml'not in parts:
        parts['word/settings.xml']=('<w:settings xmlns:w="'+W+'"/>').encode()
        ct=parts['[Content_Types].xml'].decode();require('/word/settings.xml'not in ct,'No dangling settings content type')
        parts['[Content_Types].xml']=ct.replace('</Types>','<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>').encode()
        rel=parts['word/_rels/document.xml.rels'].decode();require('relationships/settings'not in rel,'No dangling settings relationship')
        parts['word/_rels/document.xml.rels']=rel.replace('</Relationships>','<Relationship Id="settings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>').encode()
    if enabled:parts['word/settings.xml']=('<w:settings xmlns:w="'+W+'"><w:compat><w:splitPgBreakAndParaMark/></w:compat></w:settings>').encode()
    with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
        for name,data in parts.items():z.writestr(name,data)
    return parts


def xml_assert(output,reference,model):
    with zipfile.ZipFile(output)as z:parts={name:z.read(name)for name in z.namelist()}
    with zipfile.ZipFile(reference)as z:refs={name:z.read(name)for name in z.namelist()}
    require(set(parts)==set(refs),'Complete expected part set')
    for name in parts:
        if name=='rust-wps/model-v2.json':continue
        require(parts[name]==refs[name],'Independent exact XML payload mismatch: '+name)
    doc=ET.fromstring(parts['word/document.xml']);paras=doc.findall('w:body/w:p',NS);require(len(paras)==len(model['content']),'No extra paragraph')
    for atom,p in zip(model['content'],paras):
        if atom['type']!='pageBreak':continue
        require(len(p.findall('w:r',NS))==1 and len(p.findall('w:r/w:br',NS))==1 and not p.findall('.//w:t',NS),'Exact one-run break paragraph without text')
        value=atom.get('attrs',{}).get(ATOM_KEY)
        if value:
            require(p.find('w:pPr/w:spacing',NS) is not None,'Break paragraph spacing')
            for rp in ('w:pPr/w:rPr','w:r/w:rPr'):
                r=p.find(rp,NS);require(r is not None and r.find('w:rFonts',NS) is not None and r.find('w:sz',NS) is not None,'Both mark and run Latin font/size')
    return {'complete_part_set':True,'exact_nonmodel_payloads':True,'paragraph_count':len(paras),'break_count':sum(a['type']=='pageBreak'for a in model['content'])}


def stripped_model(model):
    base=copy.deepcopy(model);base.get('attrs',{}).pop(ROOT_KEY,None)
    for a in base['content']:
        if a['type']=='pageBreak':
            a.get('attrs',{}).pop(ATOM_KEY,None)
            if not a.get('attrs'):a.pop('attrs',None)
    return base


def model_checks(dest,report,run):
    """Authored model validity is deliberately independent of import provenance."""
    old=json.loads((EXPERIMENT.parent/'model.json').read_text());metric={'font':{'family':'Calibri','halfPoints':22},'spaceBefore':0,'spaceAfter':200,'lineSpacing':276}
    valid=[]
    for flag,metrics,label in [(False,False,'bare'),(True,False,'flag-only'),(False,True,'metrics-only'),(True,True,'both')]:
        model=copy.deepcopy(old)
        if flag:model.setdefault('attrs',{})[ROOT_KEY]=True
        if metrics:model['content'][3]['attrs']={ATOM_KEY:copy.deepcopy(metric)}
        valid.append((label,model))
    mixed=copy.deepcopy(valid[-1][1]);mixed['content'].insert(5,{'type':'pageBreak'});valid.append(('mixed-atoms',mixed))
    zero=copy.deepcopy(valid[1][1]);zero['content']=[x for x in zero['content']if x['type']!='pageBreak'];valid.append(('zero-break-root',zero))
    for label,index in [('bare-onlybreak',0),('flag-onlybreak',1),('metrics-onlybreak',2),('both-onlybreak',3)]:
        model=copy.deepcopy(valid[index][1]);model['content']=[model['content'][3]];valid.append((label,model))
    for label,field,value in [('font-min','halfPoints',2),('font-max','halfPoints',3276),('font-escape','family','A&B "Face"'),('font-31','family','A'*31),('font-utf16-31','family','😀'*15+'A')]:
        model=copy.deepcopy(valid[3][1]);model['content'][3]['attrs'][ATOM_KEY]['font'][field]=value;valid.append((label,model))
    for name,value in [('spaceBefore',0),('spaceBefore',31680),('spaceAfter',0),('spaceAfter',31680),('lineSpacing',120),('lineSpacing',2400)]:
        model=copy.deepcopy(valid[3][1]);model['content'][3]['attrs'][ATOM_KEY][name]=value;valid.append((name+'-'+str(value),model))
    for label,model in valid:
        stem='model-valid-'+label;mp=dest/(stem+'.json');save(mp,model);out=dest/(stem+'.docx');run([report['cli'],'create-docx',mp,out],True)
        require(json.loads(run([report['cli'],'read-docx-model',out],True).stdout)==model,'Authored complete model reopen: '+label)
        bp=dest/(stem+'.old.json');save(bp,stripped_model(model));bo=dest/(stem+'.old.docx');run([BASELINE,'create-docx',bp,bo],True);ref=dest/(stem+'.reference.docx');xml_reference(bo,ref,model);xml_assert(out,ref,model)
        if label=='bare':require(out.read_bytes()==bo.read_bytes(),'Absent authored state must remain byte exact')
        report['counts']['model_valid']+=1;report['counts']['trusted_reopen']+=1;report['counts']['xml_oracles']+=1
        report.setdefault('authored_models',[]).append({'name':label,'model':str(mp),'docx':str(out),'sha256':sha(out),'reference':str(ref),'reference_sha256':sha(ref)})
    bad=[]
    def invalid(label,fn):
        model=copy.deepcopy(valid[3][1]);fn(model);bad.append((label,model))
    for v in (False,None,0,1,'true',{},[]):invalid('root-'+repr(v),lambda m,v=v:m['attrs'].__setitem__(ROOT_KEY,v))
    for v in (None,False,[],0,'Calibri',{}):invalid('atom-'+repr(v),lambda m,v=v:m['content'][3]['attrs'].__setitem__(ATOM_KEY,v))
    for field in ('font','spaceBefore','spaceAfter','lineSpacing'):invalid('missing-'+field,lambda m,f=field:m['content'][3]['attrs'][ATOM_KEY].pop(f))
    for field in ('family','halfPoints'):invalid('missing-font-'+field,lambda m,f=field:m['content'][3]['attrs'][ATOM_KEY]['font'].pop(f))
    for field in ('spaceBefore','spaceAfter'):
        for v in (-1,31681,1.5,200.0,'200',True,None):invalid(field+'-'+repr(v),lambda m,f=field,v=v:m['content'][3]['attrs'][ATOM_KEY].__setitem__(f,v))
    for v in (0,119,2401,240.5,276.0,'276',False,None):invalid('line-'+repr(v),lambda m,v=v:m['content'][3]['attrs'][ATOM_KEY].__setitem__('lineSpacing',v))
    for v in (0,1,3277,22.5,22.0,'22',True,None):invalid('hps-'+repr(v),lambda m,v=v:m['content'][3]['attrs'][ATOM_KEY]['font'].__setitem__('halfPoints',v))
    for v in ('',' leading','trailing ','A'*32,'😀'*16,'bad\nfont','bad\x00font','\ufeffFace',[],None):invalid('family-'+repr(v),lambda m,v=v:m['content'][3]['attrs'][ATOM_KEY]['font'].__setitem__('family',v))
    invalid('unknown-metric',lambda m:m['content'][3]['attrs'][ATOM_KEY].__setitem__('typo',1))
    invalid('unknown-font',lambda m:m['content'][3]['attrs'][ATOM_KEY]['font'].__setitem__('typo',1))
    invalid('wrong-node',lambda m:m['content'][0].setdefault('attrs',{}).__setitem__(ATOM_KEY,copy.deepcopy(metric)))
    invalid('root-on-paragraph',lambda m:m['content'][0].setdefault('attrs',{}).__setitem__(ROOT_KEY,True))
    for index,(label,model)in enumerate(bad):
        mp=dest/('model-invalid-'+str(index)+'.json');save(mp,model);out=dest/('model-invalid-'+str(index)+'.docx');p=run([report['cli'],'create-docx',mp,out],False);require(not out.exists()and not p.stdout,'Invalid model cannot create a partial package: '+label);report['counts']['model_invalid']+=1
    report['invalid_model_labels']=[x[0]for x in bad]


def tamper_checks(dest,report,run):
    row=next(x for x in report['cases']if x['name']=='genuine-SampleDoc');source=Path(row['docx'])
    with zipfile.ZipFile(source)as z:original={n:z.read(n)for n in z.namelist()}
    scenarios=[]
    def alter(label,part,fn):
        parts=dict(original);parts[part]=fn(parts[part]);require(parts[part]!=original[part],'Tamper target unchanged: '+label);scenarios.append((label,parts,part))
    alter('remove-flag','word/settings.xml',lambda b:b.replace(b'<w:splitPgBreakAndParaMark/>',b''))
    alter('flag-false','word/settings.xml',lambda b:b.replace(b'<w:splitPgBreakAndParaMark/>',b'<w:splitPgBreakAndParaMark w:val="0"/>'))
    # Select only the unique break paragraph; never tamper an ordinary text run.
    def break_xml(data,part,change):
        text=data.decode()
        # Safer structural boundary selection: locate break then last paragraph start.
        end=text.index('<w:br w:type="page"/>')+len('<w:br w:type="page"/></w:r></w:p>');start=text.rfind('<w:p>',0,end);frag=text[start:end]
        if part=='mark':a=frag.index('<w:pPr>');b=frag.index('</w:pPr>')+len('</w:pPr>')
        elif part=='run':a=frag.index('<w:r>',frag.index('</w:pPr>'));b=len(frag)
        else:a=0;b=len(frag)
        target=frag[a:b];updated=change(target);require(target!=updated,'Exact break field target');frag=frag[:a]+updated+frag[b:];return (text[:start]+frag+text[end:]).encode()
    for side in ('mark','run'):
        alter(side+'-font','word/document.xml',lambda b,side=side:break_xml(b,side,lambda x:x.replace('Calibri','Liberation Serif')))
        alter(side+'-size','word/document.xml',lambda b,side=side:break_xml(b,side,lambda x:x.replace('w:val="22"','w:val="24"')))
        alter(side+'-remove-rpr','word/document.xml',lambda b,side=side:break_xml(b,side,lambda x:re.sub(r'<w:rPr>.*?</w:rPr>','',x)))
    for attr in ('before','after','line'):
        alter('spacing-'+attr,'word/document.xml',lambda b,attr=attr:break_xml(b,'mark',lambda x:re.sub('w:'+attr+'="([0-9]+)"',lambda m:'w:'+attr+'="'+str(int(m[1])+1)+'"',x)))
    alter('remove-spacing','word/document.xml',lambda b:break_xml(b,'mark',lambda x:re.sub(r'<w:spacing[^>]*/>','',x)))
    for label,parts,part in scenarios:
        for sync in (False,True):
            current=dict(parts)
            if sync:
                embedded=json.loads(current['rust-wps/model-v2.json']);embedded['hashes']={n:hashlib.sha256(d).hexdigest()for n,d in current.items()if n!='rust-wps/model-v2.json'};current['rust-wps/model-v2.json']=json.dumps(embedded,separators=(',',':'),ensure_ascii=False).encode()
            out=dest/('tamper-'+label+('-synchronized'if sync else '')+'.docx')
            with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
                for name,data in current.items():z.writestr(name,data)
            p=run([report['cli'],'read-docx-model',out],False);require(not p.stdout,'Tamper cannot return partial model')
            if sync:require('Embedded model disagrees with DOCX content: '+part in p.stderr,'Synchronized semantic rejection must name exact part')
            report['counts']['tamper_refusals']+=1
    for label,fn in [('root-model-only',lambda m:m['document']['attrs'].pop(ROOT_KEY)),('metrics-model-only',lambda m:m['document']['content'][3]['attrs'][ATOM_KEY]['font'].__setitem__('halfPoints',24))]:
        current=dict(original);embedded=json.loads(current['rust-wps/model-v2.json']);fn(embedded);current['rust-wps/model-v2.json']=json.dumps(embedded).encode();out=dest/('tamper-'+label+'.docx')
        with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
            for name,data in current.items():z.writestr(name,data)
        run([report['cli'],'read-docx-model',out],False);report['counts']['tamper_refusals']+=1
    for label in ('extra-part','removed-settings'):
        current=dict(original)
        if label=='extra-part':current['word/unexpected.xml']=b'<unexpected/>'
        else:current.pop('word/settings.xml')
        out=dest/('tamper-'+label+'.docx')
        with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
            for name,data in current.items():z.writestr(name,data)
        run([report['cli'],'read-docx-model',out],False);report['counts']['tamper_refusals']+=1


def fallback_reason(case):
    name=case['name']
    if name=='break-section-ending':return 'a source break is a section-ending character, not a manual page break'
    if name=='break-missing-section':return 'exact source section boundaries are not proven'
    if name=='break-mixed-text-paragraph':return None  # Existing full-proof Unsupported route; no partial new import.
    if 'different-'in name or name in ('break-late-font-mismatch','break-first-font-mismatch'):return 'page-break character and trailing paragraph mark have different Latin font metrics'
    oracle=s.spacing_oracle(Path(case['path']))
    for row in oracle['paragraphs']:
        if row['type']!='pageBreak' or row['supported']:continue
        line=row['spacing']['line'];value=line['value']
        if line['kind']=='multiple':return f'line multiple {value}/240 is outside supported 120–2400 units'
        return f'{line["kind"]} line spacing of {value} twips is outside the supported multiple-line subset'
    raise AssertionError('No independent fallback reason for '+name)


def run_checks(args):
    require(sha(BASELINE)==BASELINE_SHA and sha(HISTORY)==HISTORY_SHA and sha(EXPERIMENT)==EXPERIMENT_SHA,'Frozen baseline/history/experiment changed')
    cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'Candidate CLI identity')
    manifest=json.loads(args.manifest.read_text());args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='break-run-',dir=args.root))
    protected={BASELINE,cli,HISTORY,EXPERIMENT,args.manifest.resolve(),Path(__file__).resolve()}
    protected.update(ROOT/'scripts'/name for name in HELPERS);protected.update(ROOT.glob('src/*.rs'));protected.update(Path(x['path'])for x in manifest['cases'])
    before={str(p):sha(p)for p in sorted(protected)}
    countnames=('preserved','fallback','unchanged','refusals','inspect_exact','trusted_reopen','no_overwrite','xml_oracles','model_valid','model_invalid','tamper_refusals','new_pdfs','reused_source_pdfs','pdf_pairs')
    report={'status':'RUNNING','cli':str(cli),'cli_sha256':sha(cli),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'checker_sha256':sha(__file__),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'historical_report':str(HISTORY),'historical_report_sha256':HISTORY_SHA,'experiment':str(EXPERIMENT),'experiment_sha256':EXPERIMENT_SHA,'protected_before':before,'commands':[],'cases':[],'failures':[],'counts':{k:0 for k in countnames}}
    def flush():(dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    def run(argv,ok=None):
        require(shutil.disk_usage(dest).free>=600*1024**2,'600 MiB disk floor before subprocess')
        p=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=120)
        report['commands'].append({'argv':list(map(str,argv)),'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
        if ok is not None:require((p.returncode==0)==ok,'Unexpected command result: '+str(argv)+' '+p.stderr)
        return p
    flush()
    for case in manifest['cases']:
        row={k:v for k,v in case.items()if k!='oracle'};report['cases'].append(row)
        try:
            path=Path(case['path']);require(sha(path)==case['sha256'],'Source hash changed')
            old=run([BASELINE,'import-doc',path]);new=run([cli,'import-doc',path]);row.update(returncode=new.returncode,stderr=new.stderr)
            oldi=run([BASELINE,'inspect',path]);newi=run([cli,'inspect',path])
            if oldi.returncode==0:require(newi.returncode==0 and s.normalized_inspect(json.loads(oldi.stdout))==s.normalized_inspect(json.loads(newi.stdout)),'Read-only inspect changed')
            else:require((oldi.returncode,oldi.stdout,oldi.stderr)==(newi.returncode,newi.stdout,newi.stderr),'Read-only inspect refusal changed')
            report['counts']['inspect_exact']+=1
            outcome=case['outcome']
            if case['historical']:require(old.returncode==case['baseline_returncode'],'Historical baseline disposition differs from frozen report')
            if outcome in ('preserved','fallback')or(not case['historical']and outcome=='unchanged'):
                require(old.returncode==0,'Declared valid control is rejected by the frozen baseline; cannot silently downgrade coverage: '+old.stderr)
            if old.returncode!=0 or outcome=='refusal':
                require(old.returncode!=0 and new.returncode!=0 and not new.stdout,'Expected refusal returned partial/valid model')
                require((old.returncode,old.stdout,old.stderr)==(new.returncode,new.stdout,new.stderr),'Existing refusal response changed')
                report['counts']['refusals']+=1;row['observed']='refusal'
            else:
                require(new.returncode==0,'Valid source rejected: '+new.stderr);oldj,newj=json.loads(old.stdout),json.loads(new.stdout)
                if outcome=='preserved':
                    oracle=raw_oracle(path);require(oracle==case['oracle'],'Independent raw source oracle changed');expected=expected_model(oldj,oracle);row['raw_oracle']=oracle
                    require(newj['document']==expected,'Complete model differs from exact independent root/atom delta')
                    count=len(oracle['breaks']);oldwarning=f'Legacy DOC break-only paragraph spacing and baseline fonts are not preserved ({count} paragraph(s)); standalone page-break nodes are retained'
                    risks=set(oldj['compatibility']['risks']);require(oldwarning in risks,'Old break limitation exists');risks.remove(oldwarning)
                    risks={x.replace('source layout, grids and compatibility settings are not proven','source layout, grids and other compatibility settings are not proven')for x in risks};risks.add(PRESERVED.format(count=count))
                    require(set(newj['compatibility']['risks'])==risks,'Exact success diagnostics delta')
                    report['counts']['preserved']+=1;row['observed']='preserved'
                elif outcome in ('fallback','old-route'):
                    expected=oldj['document'];require(newj['document']==expected,'Unproved source must retain complete old model')
                    oldrisks=set(oldj['compatibility']['risks']);fullproof=any(x.startswith('Legacy DOC literal Latin font names and effective sizes are preserved')for x in oldrisks)
                    if fullproof:
                        reason=fallback_reason(case);require(reason is not None,'Unsupported source unexpectedly has full typography proof');expectedrisks=oldrisks|{FALLBACK+reason};require(set(newj['compatibility']['risks'])==expectedrisks,'Exact bounded fallback diagnostic')
                        report['counts']['fallback']+=1;row['observed']='fallback'
                    else:require(old.stdout==new.stdout and old.stderr==new.stderr,'Whole-proof unsupported route must be exact');report['counts']['unchanged']+=1;row['observed']='unchanged-unsupported'
                else:
                    require(old.stdout==new.stdout and old.stderr==new.stderr,'Absent/off/old route response changed');expected=oldj['document'];report['counts']['unchanged']+=1;row['observed']='unchanged'
                restored=copy.deepcopy(newj);restored['document']=oldj['document'];restored['compatibility']['risks']=oldj['compatibility']['risks'];require(restored==oldj,'Unrelated compatibility/response state changed')
                if outcome!='preserved':
                    require(ROOT_KEY not in expected.get('attrs',{})and all(ATOM_KEY not in b.get('attrs',{})for b in expected['content']),'Fallback/absent import cannot attach partial new semantics')
                for suffix,model,tool in [('baseline',oldj['document'],BASELINE),('output',expected,cli)]:
                    mp=dest/(case['name']+'.'+suffix+'.json');save(mp,model);out=dest/(case['name']+'.'+suffix+'.docx');run([tool,'create-docx',mp,out],True)
                    require(json.loads(run([cli,'read-docx-model',out],True).stdout)==model,'Full trusted model reopen');report['counts']['trusted_reopen']+=1
                    if suffix=='output':
                        for target in (path,out):
                            pin=sha(target);denied=run([cli,'create-docx',mp,target],False);require(('Output already exists'if target.suffix=='.docx'else 'Output must be DOCX')in denied.stderr,'Wrong no-overwrite reason');require(sha(target)==pin,'Existing source/output overwritten');report['counts']['no_overwrite']+=1
                output=dest/(case['name']+'.output.docx');baseline=dest/(case['name']+'.baseline.docx');row.update(model=str(dest/(case['name']+'.output.json')),docx=str(output),docx_sha256=sha(output),baseline_docx=str(baseline),baseline_docx_sha256=sha(baseline))
                save(dest/(case['name']+'.import.json'),newj);save(dest/(case['name']+'.baseline-import.json'),oldj)
                if outcome=='preserved':
                    ref=dest/(case['name']+'.break-reference.docx');xml_reference(baseline,ref,expected);row['xml_checks']=xml_assert(output,ref,expected);row.update(reference=str(ref),reference_sha256=sha(ref));report['counts']['xml_oracles']+=1
                else:require(output.read_bytes()==baseline.read_bytes(),'Fallback/absent-state DOCX bytes changed')
            row['declared_outcome']=case['outcome']
            row['outcome']='unchanged'if row['observed']=='unchanged-unsupported'else row['observed']
            require(sha(path)==case['sha256'],'Source altered during check');row['status']='passed'
        except Exception as error:row.update(status='failed',error=str(error));report['failures'].append({'name':case['name'],'error':str(error)})
        flush()
    if not report['failures']:
        for label,fn in [('model-states',model_checks),('trusted-tampering',tamper_checks)]:
            try:fn(dest,report,run)
            except Exception as error:report['failures'].append({'stage':label,'error':str(error)})
            flush()
    if args.render and not report['failures']:
        try:consumer_checks(dest,report,run,flush)
        except Exception as error:report['failures'].append({'stage':'consumer','error':str(error)})
    report['protected_after']={str(p):sha(p)for p in sorted(protected)};require(report['protected_after']==before,'Protected files changed during verification')
    report['status']='PASS-with-explicit-font-substitution-and-platform-limits'if not report['failures']else 'FAILED';flush()
    print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'counts':report['counts'],'failures':report['failures']},indent=2))
    if report['failures']:raise SystemExit(1)


def consumer_checks(dest,report,run,flush):
    import fitz
    lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice');binary=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/native/libreoffice-headless/libreoffice/program/soffice.bin')
    pin='d905a74dc92eaccb22481ba2de1345666a074a5f0d48322aaad1b6dc8a9e1e40';require(sha(binary)==pin,'Fixed independent consumer')
    report['consumer']={'binary':str(binary),'sha256':pin,'version':run([lo,'--version'],True).stdout,'pymupdf':fitz.VersionBind,'runs':[],'comparisons':[],'known_limitations':[]}
    def inspect(pdf,label):
        doc=fitz.open(pdf);pages=[]
        for page in doc:
            spans=[sp for block in page.get_text('dict')['blocks']if 'lines'in block for line in block['lines']for sp in line['spans']];chars=[]
            for block in page.get_text('rawdict')['blocks']:
                for line in block.get('lines',[]):
                    for sp in line['spans']:chars.extend({'c':ch['c'],'font':sp['font'],'size':sp['size'],'origin':ch['origin'],'bbox':ch['bbox']}for ch in sp['chars'])
            pix=page.get_pixmap(matrix=fitz.Matrix(1,1));pix.save(dest/(label+'-page-'+str(page.number+1)+'.png'))
            pages.append({'text':page.get_text(),'width':page.rect.width,'height':page.rect.height,'mediabox':list(page.mediabox),'cropbox':list(page.cropbox),'rotation':page.rotation,'fonts':sorted({(sp['font'],sp['size'])for sp in spans}),'spans':spans,'chars':chars,'words':page.get_text('words'),'raster_sha256':hashlib.sha256(pix.samples).hexdigest()})
        return {'pdf':str(pdf),'pdf_sha256':sha(pdf),'pages':pages}
    def render(path,label):
        require(shutil.disk_usage(dest).free>=600*1024**2,'600 MiB disk floor before consumer');d=dest/label;d.mkdir();home=d/'home';home.mkdir();env=dict(os.environ,HOME=str(home),XDG_CACHE_HOME=str(d/'cache'),XDG_CONFIG_HOME=str(d/'config'))
        argv=[str(lo),'-env:UserInstallation='+(d/'profile').as_uri(),'--headless','--convert-to','pdf','--outdir',str(d),str(path)]
        p=subprocess.run(argv,capture_output=True,text=True,timeout=120,env=env);report['consumer']['runs'].append({'argv':argv,'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'HOME':env['HOME'],'XDG_CACHE_HOME':env['XDG_CACHE_HOME'],'XDG_CONFIG_HOME':env['XDG_CONFIG_HOME']})
        pdf=d/(path.stem+'.pdf');require(p.returncode==0 and pdf.exists(),'Independent render failed');report['counts']['new_pdfs']+=1;return inspect(pdf,label)
    targets=[row for row in report['cases']if row.get('render')]
    require(targets and all(row.get('status')=='passed'and row.get('outcome')=='preserved'and 'reference'in row for row in targets),'Every declared consumer control must remain a proven successful source and have an independent reference')
    for row in targets:
        name=row['name'];a=render(Path(row['docx']),name+'-output');b=render(Path(row['reference']),name+'-reference');row.update(consumer_output=a,consumer_reference=b)
        require(a['pages']==b['pages'],'Candidate/reference full text/font/geometry/raster mismatch');report['counts']['pdf_pairs']+=1
        report['consumer']['comparisons'].append({'name':name,'kind':'independent-XML-reference','all_pages_exact':True})
        if name=='genuine-SampleDoc':
            old=json.loads(HISTORY.read_text());oldrow=next(x for x in old['cases']if x['name']==name);oldsource=oldrow['consumer_source'];path=Path(oldsource['pdf']);require(sha(path)==oldsource['pdf_sha256'],'Pinned reused genuine source PDF');source=inspect(path,name+'-reused-source');source.update(provenance='PINNED REUSED; not a new source render',prior_report_sha256=HISTORY_SHA);report['counts']['reused_source_pdfs']+=1
            base=oldrow['consumer_output'];require(sha(base['pdf'])==base['pdf_sha256'],'Pinned reused baseline PDF');require(len(a['pages'])==len(source['pages'])==2,'Genuine source two pages')
            require(a['pages'][0]['chars']==source['pages'][0]['chars']and a['pages'][0]['raster_sha256']==source['pages'][0]['raster_sha256'],'Genuine page1 invariance')
            require(a['pages'][0]['raster_sha256']==base['pages'][0]['raster_sha256'],'Frozen baseline page1 invariance')
            ac,sc=a['pages'][1]['chars'],source['pages'][1]['chars'];require(len(ac)==len(sc)==61 and all(x['c']==y['c']and x['origin'][1]==y['origin'][1]for x,y in zip(ac,sc)),'Every genuine page2 y origin restored')
            require(a['pages'][1]['fonts']==[('NotoSerif-Regular',16.0)]and source['pages'][1]['fonts']==[('NotoSans-Regular',16.0)],'Unresolved substitute-font distinction remains')
            report['consumer']['comparisons'].append({'name':name,'kind':'untouched-genuine-source','source_render':'pinned reused','page1_chars_raster_exact':True,'all_61_page2_y_origins_exact':True,'source_page2_first_y':sc[0]['origin'][1],'output_page2_first_y':ac[0]['origin'][1],'baseline_page2_first_y':base['pages'][1]['chars'][0]['origin'][1],'full_source_page2_match':False});row['consumer_source']=source
            report['consumer']['known_limitations'].append('Genuine SampleDoc source is NotoSans-Regular and output NotoSerif-Regular on page 2; baseline y equality does not establish font, horizontal advance or raster fidelity')
        else:
            source=render(Path(row['path']),name+'-source');row['consumer_source']=source
            require(source['pages']==a['pages'],'Installed-font authored DOC/output source comparison differs: '+name)
            report['consumer']['comparisons'].append({'name':name,'kind':'independently-authored-installed-font-DOC-source','all_pages_exact':True})
        flush()
    report['consumer']['known_limitations'].append('No Microsoft Word, WPS, Windows WebView2, browser or native-print validation is performed by this checker. New PDFs and pinned reused PDFs are counted separately. The earlier 31-PDF/306-case report remains unchanged.')
    require(sha(binary)==pin,'Consumer binary changed')


if __name__=='__main__':
    ap=argparse.ArgumentParser();ap.add_argument('--prepare',action='store_true');ap.add_argument('--root',type=Path,default=DEFAULT);ap.add_argument('--cli',type=Path);ap.add_argument('--expected-cli-sha256');ap.add_argument('--manifest',type=Path);ap.add_argument('--render',action='store_true');args=ap.parse_args()
    if args.prepare:prepare(args)
    else:
        require(args.cli and args.manifest and args.expected_cli_sha256,'Verification requires final CLI, manifest and CLI SHA-256')
        run_checks(args)
