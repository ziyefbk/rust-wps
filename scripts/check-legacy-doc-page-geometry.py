"""Independent raw-DOC page geometry oracle, controls, XML and consumer checks.

Never use the candidate parser/writer to derive expected geometry. The raw
source oracle below reads CFB/FIB/PlcfSed/SEPX/DOP directly; controls are authored
as binary bytes. Expected models add only the six independently specified twips
to the pinned spacing CLI model. XML references modify that old export directly.
Real upstream originals, authored controls and old regression inputs stay distinct.
"""
import argparse, copy, hashlib, importlib.util, json, os, re, shutil, struct, subprocess, tempfile, zipfile
from pathlib import Path
import xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parent.parent

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
c=module('geometry_font_controls',ROOT/'scripts/check-legacy-doc-fonts.py')
s=module('geometry_spacing_controls',ROOT/'scripts/check-legacy-doc-normal-spacing.py')
require,sha,save=c.require,c.sha,c.save
W,NS=c.W,c.NS
BASELINE=ROOT/'tmp/legacy-doc-spacing-final-cli'
BASELINE_SHA='b2c7d87e448397b6fac61892c97b9cee32d1662212a2851ff877a60f9742c6b3'
HISTORY=ROOT/'tmp/legacy-doc-normal-spacing-verify/spacing-fixtures-afdxojzt/manifest.json'
HISTORY_SHA='ea9df47749c4e200b65cb0eb17e99dd102d4fe03f620263541ff2261a45524f2'
PRIOR=ROOT/'tmp/legacy-doc-normal-spacing-verify/spacing-run-hqcqfwf5/report.json'
PRIOR_SHA='fa0390c419f8b6340766b8dea6e28f348e4382a293c6789f66857f3d09d93e7f'
PRESERVED='Legacy DOC single-section page size and four body margins are preserved as exact encoded twips; source layout and pagination require visual verification'
FALLBACK='Legacy DOC page geometry is not preserved: '
KEYS=('widthTwips','heightTwips','marginLeftTwips','marginRightTwips','marginTopTwips','marginBottomTwips')
OPS=(0xb01f,0xb020,0xb021,0xb022,0x9023,0x9024)
SPECS={
 'fib':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/26fb6c06-4e5c-4778-ab4e-edbf26a545bb',
 'pairs':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/0c9df81f-98d0-454e-ad84-b612cd05b1a4',
 'plc':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/68a959a6-11e0-4f3e-9a99-76ca8cc4dddc',
 'sed':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ae1ec5fc-e3c0-4e27-956e-9ceedc41cc2a',
 'sepx':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/1cc8d6e1-17e2-4667-99b6-39b2c70a0ebe',
 'properties':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/46c3ec54-53ff-4c0a-b0d6-07ad15d2546e',
 'order':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/9ac56e29-8488-4b0a-a009-86a26e2f175e',
 'dop':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/841b5f72-487e-4fe7-8657-ec90d5af8750',
 'dopbase':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f885b87a-15b3-460e-aecc-213bd17f960e',
 'dop97':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/2feeb178-f901-4151-bf5c-8496d5a0700b',
 'typography':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/d6676a31-1afb-40c8-a5c1-745f4e632c87',
 'dop2002':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/4ba592e9-dff6-466e-b13d-257c325c4d8a',
 'dop2003':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/8783a51d-da22-4e5f-a8f0-d98ae25215c5',
 'orientation':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f35a0396-e4d0-4d8e-8b7f-6114d01b864a',
 'grid':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/06301327-0f19-454e-a117-552725a86c1d'}


def geometry(values=(12240,15840,1800,1800,1440,1440)):
    return dict(zip(KEYS,values))


def raw_oracle(path):
    """Positive-source structural provenance; returns encoded integers only."""
    streams=c.streams(path);w=streams['WordDocument'];require(c.u16(w,0)==0xa5ec,'FIB identity')
    table_name='1Table' if c.u16(w,10)&512 else '0Table';t=streams[table_name]
    csw=c.u16(w,32);lw=34+2*csw;cslw=c.u16(w,lw);fc=lw+2+4*cslw
    require(csw==14 and cslw==22 and fc==0x98,'FIB variable section identity')
    count=c.u16(w,fc);end=fc+2+8*count;new=c.u16(w,end);version=c.u16(w,end+2)if new else c.u16(w,2)
    require((version,count,new)in ((0xc1,93,0),(0x112,183,5)),'FIB version identity')
    require(end+2+new*2<=len(w),'FIB extension bounds')
    out={'sha256':sha(path),'stream_sizes':{k:len(v)for k,v in streams.items()},'table_stream':table_name,'base_nFib':c.u16(w,2),'effective_nFib':version,'cswNew':new,'pair_count':count,'main_cp':c.u32(w,76),'fLoadOverridePage':bool(w[19]&4),'tables':{}}
    def part(label,index):
        off,n=c.u32(w,0x9a+8*index),c.u32(w,0x9e+8*index)
        require(not n or off+n<=len(t),label+' table bounds')
        data=t[off:off+n] if n else b''
        out['tables'][label]={'offset':off,'length':n,'sha256':hashlib.sha256(data).hexdigest()};return data
    dop=part('DOP',31);require(len(dop)==500 if version==0xc1 else len(dop)in (674,690,694),'DOP version/length')
    out['dop_flags']={'fFacingPages':bool(dop[0]&1),'fMirrorMargins':bool(dop[6]&0x20),'iGutterPos':bool(dop[83]&0x80),'f2on1':bool(dop[90]&0x20),'fFolioPrint':bool(dop[548]&0x40)if len(dop)>548 else False,'fReverseFolio':bool(dop[548]&0x80)if len(dop)>548 else False,'fReadingModeInkLockDown':bool(dop[598]&1)if len(dop)>598 else False}
    for name,index in [('PlcfHdd',11),('PrDrvr',27),('PrEnvPort',28),('PrEnvLand',29)]:require(not part(name,index),'Positive source has '+name)
    require(not out['fLoadOverridePage']and not any(out['dop_flags'].values()),'Positive source has document override')
    plc=part('PlcfSed',6);require(len(plc)==20,'Single-section PLC length');cps=struct.unpack_from('<2I',plc)
    require(cps==(0,out['main_cp']),'Exact main story section identity');sed=plc[8:20];pos=struct.unpack_from('<i',sed,2)[0]
    require(0<=pos<=len(w)-2,'SEPX pointer bounds');size=struct.unpack_from('<h',w,pos)[0];require(size>=0 and pos+2+size<=len(w),'Signed SEPX bounds')
    props=c.sprms(w[pos+2:pos+2+size]);values={};neutral={}
    for op,arg in props:
        if op in OPS:values[KEYS[OPS.index(op)]]=int.from_bytes(arg,'little',signed=op in (0x9023,0x9024))
        else:neutral[f'{op:04x}']=int.from_bytes(arg,'little')
    require(set(values)==set(KEYS),'Positive source explicit six-component geometry')
    out.update(cps=list(cps),sed_raw=sed.hex(),fcSepx=pos,sepx_cb=size,sepx_raw=w[pos+2:pos+2+size].hex(),sprms=[{'opcode':f'{op:04x}','raw':arg.hex()}for op,arg in props],geometry=values,other_properties=neutral)
    return out


def section(parts,props=None,dop_length=None):
    w,t,p=parts;version=c.u16(w,0x9c+8*c.u16(w,0x98))if c.u16(w,0x9a+8*c.u16(w,0x98))else 0xc1
    props=b''.join(c.spr(op,v)for op,v in zip(OPS,(12240,15840,1800,1800,1440,1440))) if props is None else props
    pos=3584;require(len(props)<510,'Authored SEPX fits free sector');w[pos:pos+2+len(props)]=c.p16(len(props))+props
    sed=c.u32(w,0xca);c.set32(t,sed+10,pos)
    n=(500 if version==0xc1 else 674)if dop_length is None else dop_length
    dop=bytearray(n)
    if n>=12:c.set16(dop,10,720)
    c.set32(w,0x192,len(t));c.set32(w,0x196,n);p['dop']=len(t);t.extend(dop);p['sepx']=pos
    return w,t,p


def fixture(values=(12240,15840,1800,1800,1440,1440),extra=b'',omit=(),prefix=b'',**kwargs):
    props=prefix+b''.join(c.spr(op,v&65535)for op,v in zip(OPS,values)if op not in omit)+extra
    return section(c.authored(**kwargs),props)


def prepare(args):
    require(sha(BASELINE)==BASELINE_SHA and sha(HISTORY)==HISTORY_SHA and sha(PRIOR)==PRIOR_SHA,'Pinned baseline/evidence changed')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='geometry-fixtures-',dir=args.root));cases=[]
    old=json.loads(HISTORY.read_text())
    for row in old['cases']:
        case={k:row[k]for k in ('name','path','sha256','kind')};case['historical']=True
        case['outcome']='unchanged-refusal'if row['outcome']in ('refused','new-refusal')else 'unchanged-unsupported'if row['outcome']=='unsupported'else 'fallback'
        if row['kind']=='genuine-original':case.update(outcome='preserved',oracle=raw_oracle(Path(row['path'])),render=True)
        cases.append(case)
    def add(name,parts,outcome='preserved',expected=None,render=False,mutate=None,reason=None):
        w,t,p=parts
        if mutate:mutate(w,t,p)
        path=dest/(name+'.doc');save(path,c.compound(w,t));row={'name':name,'path':str(path),'sha256':sha(path),'kind':'independently-authored-binary-page-control','historical':False,'outcome':outcome,'render':render}
        if outcome=='preserved':
            row['oracle']=raw_oracle(path);require(row['oracle']['geometry']==(expected or geometry()),'Binary source differs from explicit author recipe: '+name)
        if reason:row['reason']=reason
        cases.append(row)
    add('page-letter',fixture(),render=True)
    for name,values in [('a4',(11906,16838,1440,1440,1440,1440)),('a4-raw',(11907,16840,1440,1440,1440,1440)),('a5',(8391,11906,720,720,900,1200)),('asymmetric',(12240,15840,1080,2160,900,1800)),('zero-margins',(12240,15840,0,0,0,0)),('max-margins',(12240,15840,4320,4320,4320,4320))]:add('page-'+name,fixture(values),expected=geometry(values),render=name in ('a4-raw','a5','asymmetric','zero-margins'))
    for name,values in [('letter',(15840,12240,900,1200,1800,1440)),('a4',(16840,11907,1440,1440,1440,1440)),('a5',(11906,8391,720,720,720,720))]:add('page-landscape-'+name,fixture(values,extra=c.spr(0x301d,2)),expected=geometry(values),render=True)
    for delta in (-2,2):
        values=(12240+delta,15840+delta,1800,1800,1440,1440)
        add('page-nominal-tolerance-'+str(delta),fixture(values),expected=geometry(values))
    add('page-nominal-outside-tolerance',fixture((12243,15840,1800,1800,1440,1440)),outcome='fallback')
    add('page-insufficient-body-width',fixture((8391,11906,4000,4000,1440,1440)),outcome='fallback')
    add('page-explicit-portrait',fixture(extra=c.spr(0x301d,1)))
    add('page-stored-order',fixture(prefix=c.spr(0xb021,2500),extra=c.spr(0xb021,1800)+c.spr(0x9023,1440)))
    add('page-valid-overwritten',fixture(prefix=c.spr(0xb01f,16000)+c.spr(0x9023,6000)))
    add('page-neutral-extra',fixture(extra=c.spr(0xb025,0)+c.spr(0x500b,0)+c.spr(0x5032,0)+c.spr(0x9031,360)+c.spr(0xb017,709)+c.spr(0xb018,709)+c.spr(0x900c,708)+c.spr(0x5026,9)+c.spr(0x703a,4594569)),render=True)
    add('page-ignored-sed-fields',fixture(),mutate=lambda w,t,p:(c.set16(t,c.u32(w,0xca)+8,65535),c.set16(t,c.u32(w,0xca)+14,65535),c.set32(t,c.u32(w,0xca)+16,0x12345678)))
    add('page-ignored-zero-pointer',fixture(),mutate=lambda w,t,p:[c.set32(w,0x9a+8*k,0xffffffff)for k in (11,27,28,29)])
    for n in (674,690,694):add('page-dop-'+str(n),section(c.authored(version=0x112,basebytes=18),dop_length=n))
    text='A long line of independently authored ordinary prose crosses the narrow physical body area. '*4
    add('page-installed-wrap',fixture((8391,11906,1440,1440,1440,1440),text=text,fonts=('Liberation Serif',)),expected=geometry((8391,11906,1440,1440,1440,1440)),render=True)
    # Legacy absence never invents page geometry or erases typography/spacing.
    add('page-missing-dop',fixture(),outcome='fallback',mutate=lambda w,t,p:c.set32(w,0x196,0))
    add('page-missing-sepx',fixture(),outcome='fallback',mutate=lambda w,t,p:c.set32(t,c.u32(w,0xca)+10,0xffffffff))
    for op in OPS:add('page-missing-'+hex(op),fixture(omit=(op,)),outcome='fallback')
    add('page-empty-sepx',section(c.authored(),b''),outcome='fallback')
    for name,offset,mask,modern in [('facing',0,1,False),('mirror',6,32,False),('top-gutter',83,128,False),('two-on-one',90,32,False),('book-fold',548,64,True),('reverse-book-fold',548,192,True),('reading-lock',598,1,True)]:
        add('page-dop-'+name,fixture(version=0x112,basebytes=18)if modern else fixture(),outcome='fallback',mutate=lambda w,t,p,o=offset,m=mask:t.__setitem__(p['dop']+o,t[p['dop']+o]|m))
    add('page-loading-override',fixture(),outcome='fallback',mutate=lambda w,t,p:w.__setitem__(19,w[19]|4))
    for name,extra in [('gutter',c.spr(0xb025,200)),('columns',c.spr(0x500b,1)),('grid',c.spr(0x5032,1)+c.spr(0x9031,360)),('vertical',c.spr(0x5033,1)),('title-page',c.spr(0x300a,1)),('center',c.spr(0x301a,1)),('unknown',c.spr(0x30fe,1)),('landscape-conflict',c.spr(0x301d,2))]:add('page-active-'+name,fixture(extra=extra),outcome='fallback')
    add('page-negative-fixed-margin',fixture((12240,15840,1800,1800,-1440,1440)),outcome='fallback')
    add('page-unmodeled-paper',fixture((16000,20000,1800,1800,1440,1440)),outcome='fallback')
    add('page-too-large-margin',fixture((12240,15840,5000,1800,1440,1440)),outcome='fallback')
    for name,op,value in [('width-small',0xb01f,143),('width-large',0xb01f,31681),('height-small',0xb020,0),('left-large',0xb021,31681),('right-negative',0xb022,65535),('top-large',0x9023,31666),('bottom-large',0x9024,(-31666)&65535),('orientation-zero',0x301d,0),('orientation-three',0x301d,3),('columns-large',0x500b,44),('grid-invalid',0x5032,4),('linepitch-zero',0x9031,0),('linepitch-large',0x9031,31681)]:
        add('page-invalid-'+name,fixture(extra=c.spr(op,value)),outcome='new-refusal')
        add('page-invalid-overwritten-'+name,fixture(prefix=c.spr(op,value),extra=b'' if op in OPS else c.spr(op,{0x301d:1,0x500b:0,0x5032:0,0x9031:360}[op])),outcome='new-refusal')
    add('page-truncated-fixed',fixture(extra=c.p16(0xb01f)+b'\0'),outcome='new-refusal')
    add('page-truncated-variable',fixture(extra=c.p16(0xd2fe)+b'\x08\x00'),outcome='new-refusal')
    add('page-trailing-byte',fixture(extra=b'\0'),outcome='new-refusal')
    add('page-malformed-after-unknown',fixture(extra=c.spr(0x30fe,1)+c.spr(0xb01f,0)),outcome='new-refusal')
    add('page-malformed-with-missing-dop',fixture(extra=c.spr(0xb01f,0)),outcome='new-refusal',mutate=lambda w,t,p:c.set32(w,0x196,0))
    add('page-malformed-with-loading-override',fixture(extra=c.spr(0xb01f,0)),outcome='new-refusal',mutate=lambda w,t,p:w.__setitem__(19,w[19]|4))
    for name,mut in [('dop-truncated',lambda w,t,p:c.set32(w,0x196,499)),('dop-too-long',lambda w,t,p:c.set32(w,0x196,501)),('dop-out-of-bounds',lambda w,t,p:c.set32(w,0x192,0x7fffffff)),('sepx-negative-size',lambda w,t,p:c.set16(w,p['sepx'],65535)),('sepx-truncated-size',lambda w,t,p:c.set16(w,p['sepx'],512)),('sepx-out-of-bounds',lambda w,t,p:c.set32(t,c.u32(w,0xca)+10,4095)),('sepx-negative-pointer',lambda w,t,p:c.set32(t,c.u32(w,0xca)+10,0xfffffffe)),('printer-out-of-bounds',lambda w,t,p:(c.set32(w,0x9a+27*8,0x7fffffff),c.set32(w,0x9e+27*8,10)))]:add('page-'+name,fixture(),outcome='new-refusal',mutate=mut)
    add('page-section-table-absent',fixture(),outcome='fallback',mutate=lambda w,t,p:c.set32(w,0xce,0))
    for name,mut in [('plc-short',lambda w,t,p:c.set32(w,0xce,19)),('plc-long-invalid',lambda w,t,p:c.set32(w,0xce,21)),('plc-out-of-bounds',lambda w,t,p:c.set32(w,0xca,0x7fffffff)),('plc-nonzero-start',lambda w,t,p:c.set32(t,c.u32(w,0xca),1)),('plc-duplicate-cps',lambda w,t,p:c.set32(t,c.u32(w,0xca)+4,0)),('plc-end-before-main',lambda w,t,p:c.set32(t,c.u32(w,0xca)+4,c.u32(w,76)-1)),('plc-negative-end',lambda w,t,p:c.set32(t,c.u32(w,0xca)+4,0x80000000))]:
        add('page-'+name,fixture(),outcome='new-refusal',mutate=mut)
    def invalid_multisection(w,t,p):
        n=c.u32(w,76);sed=bytes(t[c.u32(w,0xca)+8:c.u32(w,0xca)+20]);c.set32(w,0xca,len(t));c.set32(w,0xce,36);t.extend(c.p32(0)+c.p32(1)+c.p32(n)+sed+sed)
    add('page-invalid-multisection-marker',fixture(),outcome='new-refusal',mutate=invalid_multisection)
    add('page-end-beyond-main',fixture(),outcome='fallback',mutate=lambda w,t,p:c.set32(t,c.u32(w,0xca)+4,c.u32(w,76)+1))
    add('page-inactive-actual-page-lock',fixture(version=0x112,basebytes=18),mutate=lambda w,t,p:c.set16(t,p['dop']+620,0x200))
    add('page-printer-metrics',fixture(),outcome='fallback',mutate=lambda w,t,p:c.set32(t,p['dop']+84,0x80000000))
    add('page-modern-printer-metrics',fixture(version=0x112,basebytes=18),outcome='fallback',mutate=lambda w,t,p:(c.set32(t,p['dop']+84,0x80000000),c.set32(t,p['dop']+508,0x80000000)))
    for modern,offset in ((False,8),(False,84),(True,8),(True,84),(True,508)):
        add('page-dop-ignored-copts-bit-'+str(modern)+'-'+str(offset),fixture(version=0x112,basebytes=18)if modern else fixture(),mutate=lambda w,t,p,o=offset:c.set16(t,p['dop']+o,0x200))
    add('page-dop-copts60-conflict',fixture(),outcome='new-refusal',mutate=lambda w,t,p:c.set16(t,p['dop']+8,1))
    add('page-dop-copts80-conflict',fixture(version=0x112,basebytes=18),outcome='new-refusal',mutate=lambda w,t,p:c.set32(t,p['dop']+508,1))
    add('page-dop-reverse-without-folio',fixture(version=0x112,basebytes=18),outcome='new-refusal',mutate=lambda w,t,p:t.__setitem__(p['dop']+548,128))
    add('page-disabled-grid-after-active',fixture(extra=c.spr(0x5032,1)+c.spr(0x9031,360)+c.spr(0x5032,0)))
    add('page-single-column-after-multi',fixture(extra=c.spr(0x500b,2)+c.spr(0x500b,0)))
    add('page-zero-gutter-after-nonzero',fixture(extra=c.spr(0xb025,120)+c.spr(0xb025,0)))
    add('page-disabled-revision-wall',fixture(extra=c.spr(0x3239,1)+c.spr(0x3239,0)))
    add('page-printer-record-present',fixture(),outcome='fallback',mutate=lambda w,t,p:(c.set32(w,0x9a+27*8,p['dop']),c.set32(w,0x9e+27*8,10)))
    add('page-header-record-without-text',fixture(),outcome='new-refusal',mutate=lambda w,t,p:(c.set32(w,0x9a+11*8,p['dop']),c.set32(w,0x9e+11*8,12)))
    for n in (0,143,31681,65535):
        add('page-invalid-early-with-unknown-'+str(n),fixture(prefix=c.spr(0xb01f,n),extra=c.spr(0x30fe,1)),outcome='new-refusal')
    # Full typography Unsupported is deliberately unchanged, even with unrelated
    # page metadata: it never receives a misleading partial geometry guarantee.
    add('page-font-unproved',fixture(runs=[(59,c.spr(0x2aee,1))]),outcome='unchanged-unsupported')
    manifest={'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_manifest_sha256':HISTORY_SHA,'specifications':SPECS,'cases':cases}
    save(dest/'manifest.json',manifest);print(json.dumps({'manifest':str(dest/'manifest.json'),'sha256':sha(dest/'manifest.json'),'count':len(cases)},indent=2))


def fallback_reasons(case):
    """Declared public diagnostic contract, keyed by independent author recipe."""
    if case['historical']:return ['document page settings are absent','section properties are absent']
    name=case['name']
    if name.startswith('page-missing-0x') or name=='page-empty-sepx':return ['explicit page dimensions and all four margins are required']
    reasons={
        'page-missing-dop':'document page settings are absent',
        'page-missing-sepx':'section properties are absent',
        'page-section-table-absent':'section table is absent',
        'page-dop-facing':'facing-page headers are outside the audited page subset',
        'page-dop-mirror':'mirrored margins are outside the audited page subset',
        'page-dop-top-gutter':'top gutter mode is outside the audited page subset',
        'page-dop-two-on-one':'two-pages-per-sheet mode is outside the audited page subset',
        'page-dop-book-fold':'bookfold mode is outside the audited page subset',
        'page-dop-reverse-book-fold':'bookfold mode is outside the audited page subset',
        'page-dop-reading-lock':'reading-layout page lock is outside the audited page subset',
        'page-loading-override':'FIB installation-language page override is active',
        'page-active-gutter':'nonzero gutter is outside the audited page subset',
        'page-active-columns':'multiple columns are outside the audited page subset',
        'page-active-grid':'active document grid is outside the audited page subset',
        'page-active-vertical':'non-default text flow is outside the audited page subset',
        'page-active-title-page':'first-page special layout is outside the audited page subset',
        'page-active-center':'non-top vertical alignment is outside the audited page subset',
        'page-active-unknown':'unproven section property 0x30FE',
        'page-active-landscape-conflict':'page dimensions disagree with section orientation',
        'page-negative-fixed-margin':'negative fixed margins are outside the audited page subset',
        'page-unmodeled-paper':'encoded page is outside the editor subset: Page size is outside supported A4, Letter and A5 bounds',
        'page-too-large-margin':'encoded page is outside the editor subset: Page margins must be integer twips in 0..=4320',
        'page-nominal-outside-tolerance':'encoded page is outside the editor subset: Page size is outside supported A4, Letter and A5 bounds',
        'page-insufficient-body-width':'encoded page is outside the editor subset: Page content area must be at least 1440 twips in each direction',
        'page-end-beyond-main':'section end is not exactly the proven body end',
        'page-printer-metrics':'printer metrics are outside the audited page subset',
        'page-modern-printer-metrics':'printer metrics are outside the audited page subset',
        'page-printer-record-present':'printer driver settings are outside the audited page subset',
    }
    require(name in reasons,'Missing independently specified fallback diagnostic: '+name)
    return [reasons[name]]


def expected_document(old,case):
    result=copy.deepcopy(old['document'])
    if case['outcome']=='preserved':result.setdefault('attrs',{})['pageGeometry']=case['oracle']['geometry']
    return result


def xml_reference(baseline,out,values):
    """Independent direct package edit, not a candidate writer round trip."""
    orientation=' w:orient="landscape"'if values['widthTwips']>values['heightTwips']else ''
    size=f'<w:pgSz w:w="{values["widthTwips"]}" w:h="{values["heightTwips"]}"{orientation}/>'
    margins='<w:pgMar '+ ' '.join(f'w:{edge}="{values["margin"+edge.title()+"Twips"]}"'for edge in ('top','right','bottom','left'))+' w:header="720" w:footer="720" w:gutter="0"/>'
    with zipfile.ZipFile(baseline)as zi,zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as zo:
        for item in zi.infolist():
            data=zi.read(item.filename)
            if item.filename=='word/document.xml':
                text=data.decode();text,ns=re.subn(r'<w:pgSz\b[^>]*/>',size,text);text,nm=re.subn(r'<w:pgMar\b[^>]*/>',margins,text)
                require(ns==nm==1,'Exactly one baseline page geometry pair');data=text.encode()
            zo.writestr(item,data)


def xml_assert(output,reference,values):
    from docx import Document
    with zipfile.ZipFile(output)as a,zipfile.ZipFile(reference)as b:
        require(set(a.namelist())==set(b.namelist()),'Independent exact package set')
        for name in a.namelist():
            if name=='rust-wps/model-v2.json':continue
            require(a.read(name)==b.read(name),'Unexpected package content delta: '+name)
        doc=ET.fromstring(a.read('word/document.xml'));sections=doc.findall('.//w:sectPr',NS);require(len(sections)==1 and doc.find('w:body',NS)[-1]is sections[0],'One terminal body section')
        pg,mar=sections[0].find('w:pgSz',NS),sections[0].find('w:pgMar',NS)
        decoded={'widthTwips':int(pg.get('{'+W+'}w')),'heightTwips':int(pg.get('{'+W+'}h'))}
        decoded.update({'margin'+edge.title()+'Twips':int(mar.get('{'+W+'}'+edge))for edge in ('top','right','bottom','left')})
        require(decoded==values,'Raw six twips in actual DOCX')
        require(pg.get('{'+W+'}orient')==('landscape'if values['widthTwips']>values['heightTwips']else None),'Actual XML orientation')
    sections=list(Document(output).sections);require(len(sections)==1,'python-docx single section');p=sections[0]
    independent={'widthTwips':p.page_width.twips,'heightTwips':p.page_height.twips,'marginTopTwips':p.top_margin.twips,'marginRightTwips':p.right_margin.twips,'marginBottomTwips':p.bottom_margin.twips,'marginLeftTwips':p.left_margin.twips}
    require(independent==values,'Independent python-docx exact geometry')
    return decoded


def run_checks(args):
    require(sha(BASELINE)==BASELINE_SHA and sha(HISTORY)==HISTORY_SHA and sha(PRIOR)==PRIOR_SHA,'Pinned baseline/historical evidence changed')
    cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'Candidate identity differs from release')
    manifest=json.loads(args.manifest.read_text());args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='geometry-run-',dir=args.root))
    protected=set([BASELINE,cli,HISTORY,PRIOR,args.manifest.resolve(),Path(__file__).resolve(),ROOT/'scripts/check-legacy-doc-fonts.py',ROOT/'scripts/check-legacy-doc-normal-identity.py',ROOT/'scripts/check-legacy-doc-normal-spacing.py'])
    protected.update(ROOT.glob('src/*.rs'));protected.update(Path(row['path'])for row in manifest['cases'])
    before={str(p):sha(p)for p in sorted(protected)}
    report={'status':'RUNNING','cli':str(cli),'cli_sha256':sha(cli),'baseline':str(BASELINE),'baseline_sha256':sha(BASELINE),'checker_sha256':sha(__file__),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'specification_urls':SPECS,'protected_before':before,'commands':[],'cases':[],'failures':[],'counts':{k:0 for k in ('preserved','fallback','unchanged_unsupported','unchanged_refusals','new_refusals','inspect_exact','trusted_reopen','no_overwrite','xml_oracles','tamper_refusals','new_pdfs','pdf_pairs')}}
    def flush():(dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    def run(argv,ok=None):
        require(shutil.disk_usage(dest).free>=600*1024*1024,'600MiB free-space floor before subprocess')
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
            if oldi.returncode==0:
                require(newi.returncode==0 and s.normalized_inspect(json.loads(oldi.stdout))==s.normalized_inspect(json.loads(newi.stdout)),'Read-only inspect changed')
            else:require((oldi.returncode,oldi.stdout,oldi.stderr)==(newi.returncode,newi.stdout,newi.stderr),'Read-only refusal changed')
            report['counts']['inspect_exact']+=1
            outcome=case['outcome']
            if outcome.endswith('refusal'):
                require(new.returncode!=0 and not new.stdout,'Malformed/refused input produced a partial model')
                if outcome=='unchanged-refusal':require((old.returncode,old.stdout,old.stderr)==(new.returncode,new.stdout,new.stderr),'Historical refusal changed');report['counts']['unchanged_refusals']+=1
                else:
                    require(old.returncode==0,'New page control already rejected by frozen baseline: '+old.stderr)
                    require('DOC'in new.stderr and any(x in new.stderr.lower()for x in ('page','section','sepx','dop')),'Failure is not attributable to page proof: '+new.stderr)
                    report['counts']['new_refusals']+=1
            else:
                require(old.returncode==new.returncode==0,'Valid input rejected: '+old.stderr+' / '+new.stderr)
                oldj,newj=json.loads(old.stdout),json.loads(new.stdout)
                save(dest/(case['name']+'.baseline-import.json'),oldj);save(dest/(case['name']+'.import.json'),newj)
                if outcome=='unchanged-unsupported':
                    require(old.stdout==new.stdout and old.stderr==new.stderr,'Whole-proof Unsupported response changed');report['counts']['unchanged_unsupported']+=1
                else:
                    expected=expected_document(oldj,case);require(newj['document']==expected,'Only exact independently expected root geometry may change')
                    copy_response=copy.deepcopy(newj);copy_response['document']=oldj['document']
                    oldrisks=set(oldj['compatibility']['risks']);newrisks=set(newj['compatibility']['risks'])
                    if outcome=='preserved':
                        oracle=raw_oracle(path);require(oracle==case['oracle'],'Raw source oracle changed');row['raw_oracle']=oracle
                        expected_risks=set(oldrisks);expected_risks.discard('Legacy DOC body manual page breaks are preserved; section layout is not preserved');expected_risks.add('Legacy DOC body manual page breaks are preserved; only audited single-section page geometry is preserved');expected_risks.add(PRESERVED)
                        require(newrisks==expected_risks,'Exact preservation diagnostic delta')
                    else:
                        added=newrisks-oldrisks;require(oldrisks<=newrisks and added=={FALLBACK+r for r in fallback_reasons(case)},'Exact independently specified geometry-only fallback diagnostics: '+repr(added))
                        require(PRESERVED not in newrisks,'Unproved geometry receives preservation claim')
                    copy_response['compatibility']['risks']=oldj['compatibility']['risks'];require(copy_response==oldj,'Nongeometry response changed')
                    models=[('baseline',oldj['document'],BASELINE),('output',expected,cli)]
                    if outcome=='preserved':models.append(('expected',expected,BASELINE))
                    for suffix,model,tool in models:
                        mp=dest/(case['name']+'.'+suffix+'.json');save(mp,model);out=dest/(case['name']+'.'+suffix+'.docx');run([tool,'create-docx',mp,out],True)
                        require(json.loads(run([cli,'read-docx-model',out],True).stdout)==model,'Full trusted model reopen');report['counts']['trusted_reopen']+=1
                        for target in (path,out):
                            pin=sha(target);denied=run([cli,'create-docx',mp,target],False);require(('Output already exists'if target.suffix=='.docx'else 'Output must be DOCX')in denied.stderr,'Wrong no-overwrite reason');require(sha(target)==pin,'Source or existing output was overwritten');report['counts']['no_overwrite']+=1
                    output=dest/(case['name']+'.output.docx');baseline=dest/(case['name']+'.baseline.docx')
                    row.update(docx=str(output),docx_sha256=sha(output),baseline_docx=str(baseline),model=str(dest/(case['name']+'.output.json')))
                    if outcome=='preserved':
                        require(output.read_bytes()==(dest/(case['name']+'.expected.docx')).read_bytes(),'Candidate export differs from old writer of independent expected model')
                        ref=dest/(case['name']+'.geometry-reference.docx');xml_reference(baseline,ref,case['oracle']['geometry']);row['xml_geometry']=xml_assert(output,ref,case['oracle']['geometry']);row.update(reference=str(ref),reference_sha256=sha(ref));report['counts']['xml_oracles']+=1
                    else:require(output.read_bytes()==baseline.read_bytes(),'Geometry fallback changed DOCX bytes')
                    report['counts'][outcome]+=1
            row['status']='passed'
        except Exception as error:row.update(status='failed',error=str(error));report['failures'].append({'name':case['name'],'error':str(error)})
        flush()
    if not report['failures']:
        try:tamper_checks(dest,report,run)
        except Exception as error:report['failures'].append({'stage':'tamper','error':str(error)})
        flush()
    if args.render and not report['failures']:
        try:consumer_checks(dest,report,run,flush)
        except Exception as error:report['failures'].append({'stage':'consumer','error':str(error)})
        flush()
    report['protected_after']={str(p):sha(p)for p in sorted(protected)}
    require(report['protected_after']==before,'Protected source/checker/evidence/binary changed')
    report['status']='PASS-with-explicit-source-layout-break-font-and-compatibility-limits'if not report['failures']else 'FAILED';flush()
    print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'counts':report['counts'],'failures':report['failures']},indent=2))
    if report['failures']:raise SystemExit(1)


def tamper_checks(dest,report,run):
    source=Path(next(row['docx']for row in report['cases']if row['name']=='genuine-simple'))
    for label in ('width','height','top','right','bottom','left','orientation','remove-size','remove-margins','mirror-settings'):
        with zipfile.ZipFile(source)as zi:parts={name:zi.read(name)for name in zi.namelist()}
        target='word/settings.xml'if label=='mirror-settings'else 'word/document.xml';old=parts[target].decode()
        if label=='mirror-settings':
            changed,count=re.subn(r'<w:settings\b([^>]*)/>',r'<w:settings\1><w:mirrorMargins/></w:settings>',old);require(count==1,'Exact minimal settings tamper target')
        else:
            tag='pgSz'if label in ('width','height','orientation','remove-size')else 'pgMar';m=re.search(r'<w:'+tag+r'\b[^>]*/>',old);require(m,'Exact literal page target');token=m.group()
            if label.startswith('remove-'):token=''
            elif label=='orientation':token=token[:-2]+' w:orient="landscape"/>'
            else:
                attr={'width':'w','height':'h'}.get(label,label);token,count=re.subn('w:'+attr+'="([0-9]+)"',lambda m:'w:'+attr+'="'+str(int(m[1])+1)+'"',token);require(count==1,'Exact page attribute target')
            changed=old[:m.start()]+token+old[m.end():]
        require(changed!=old,'Tamper changes only intended target');parts[target]=changed.encode()
        for synchronized in (False,True):
            current=dict(parts)
            if synchronized:
                embedded=json.loads(current['rust-wps/model-v2.json']);embedded['hashes']={name:hashlib.sha256(data).hexdigest()for name,data in current.items()if name!='rust-wps/model-v2.json'};current['rust-wps/model-v2.json']=json.dumps(embedded,separators=(',',':'),ensure_ascii=False).encode()
            out=dest/('tamper-'+label+('-synchronized'if synchronized else '')+'.docx')
            with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as zo:
                for name,data in current.items():zo.writestr(name,data)
            rejected=run([report['cli'],'read-docx-model',out],False)
            if synchronized:require('Embedded model disagrees with DOCX content: '+target in rejected.stderr,'Synchronized rejection must name exact semantic target: '+rejected.stderr)
            report['counts']['tamper_refusals']+=1


def consumer_checks(dest,report,run,flush):
    import fitz
    lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice')
    binaries=list(Path('/opt').glob('**/program/soffice.bin'));require(len(binaries)==1,'Unique independent consumer binary');binary=binaries[0];pin=sha(binary)
    require(pin=='d905a74dc92eaccb22481ba2de1345666a074a5f0d48322aaad1b6dc8a9e1e40','Fixed LibreOffice identity')
    report['consumer']={'binary':str(binary),'sha256':pin,'version':run([lo,'--version'],True).stdout,'pymupdf':fitz.VersionBind,'poppler_sha256':sha('/usr/bin/pdftotext'),'runs':[],'comparisons':[],'known_limitations':[]}
    def render(path,label):
        require(shutil.disk_usage(dest).free>=650*1024*1024,'650MiB free-space floor before consumer')
        d=dest/label;d.mkdir();home=d/'home';home.mkdir();profile=d/'profile';env=dict(os.environ,HOME=str(home),XDG_CACHE_HOME=str(d/'cache'),XDG_CONFIG_HOME=str(d/'config'))
        argv=[str(lo),'-env:UserInstallation='+profile.as_uri(),'--headless','--convert-to','pdf','--outdir',str(d),str(path)]
        result=subprocess.run(argv,capture_output=True,text=True,timeout=120,env=env)
        report['consumer']['runs'].append({'argv':argv,'HOME':str(home),'XDG_CACHE_HOME':str(d/'cache'),'XDG_CONFIG_HOME':str(d/'config'),'returncode':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
        pdf=d/(path.stem+'.pdf');require(result.returncode==0 and pdf.exists(),'Independent consumer failed: '+result.stdout+' / '+result.stderr);doc=fitz.open(pdf);pages=[]
        for page in doc:
            spans=[sp for block in page.get_text('dict')['blocks']if 'lines'in block for line in block['lines']for sp in line['spans']];chars=[]
            for block in page.get_text('rawdict')['blocks']:
                if 'lines'not in block:continue
                for line in block['lines']:
                    for sp in line['spans']:chars.extend({'c':ch['c'],'font':sp['font'],'size':sp['size'],'origin':ch['origin'],'bbox':ch['bbox']}for ch in sp['chars'])
            pix=page.get_pixmap(matrix=fitz.Matrix(1,1));pages.append({'text':page.get_text(),'width':page.rect.width,'height':page.rect.height,'mediabox':list(page.mediabox),'cropbox':list(page.cropbox),'rotation':page.rotation,'fonts':sorted({(sp['font'],sp['size'])for sp in spans}),'spans':spans,'chars':chars,'words':page.get_text('words'),'raster_sha256':hashlib.sha256(pix.samples).hexdigest()})
            if label.startswith(('genuine-','page-installed-wrap')):pix.save(d/('page-'+str(page.number+1)+'.png'))
        text=run(['/usr/bin/pdftotext','-layout',pdf,'-'],True).stdout;(d/'poppler.txt').write_text(text)
        require(len(pages)>0,'PDF has pages');report['counts']['new_pdfs']+=1
        return {'input':str(path),'input_sha256':sha(path),'pdf':str(pdf),'pdf_sha256':sha(pdf),'pages':pages,'profile':str(profile),'home':str(home),'poppler_text':text}
    rows={row['name']:row for row in report['cases']}
    for row in report['cases']:
        if not row.get('render')or 'reference'not in row:continue
        name=row['name'];output=render(Path(row['docx']),name+'-output');reference=render(Path(row['reference']),name+'-reference');row.update(consumer_output=output,consumer_reference=reference)
        require(output['pages']==reference['pages']and output['poppler_text']==reference['poppler_text'],'Strict independent XML reference/output text, fonts, coordinates or raster differ: '+name)
        values=row['raw_oracle']['geometry']
        for page in output['pages']:
            require(page['rotation']==0 and page['mediabox']==page['cropbox'],'No unexpected physical page transformation')
            # PDF exporters round nominal international paper sizes. Keep exact
            # source/model/XML twips separate; tolerate at most0.15pt in physical
            # MediaBox only, never text coordinates or strict reference equality.
            require(abs(page['width']-values['widthTwips']/20)<=0.15 and abs(page['height']-values['heightTwips']/20)<=0.15,'PDF physical page size disagrees with raw twips')
        report['counts']['pdf_pairs']+=1;report['consumer']['comparisons'].append({'name':name,'kind':'independent-root-geometry-only-reference','exact_text_fonts_spans_chars_page_raster':True,'max_coordinate_difference_pt':0,'changed_pixels':0,'encoded_geometry_twips':values,'consumed_page_pt':[[p['width'],p['height']]for p in output['pages']],'mediabox_minus_raw_twips_pt':[[p['width']-values['widthTwips']/20,p['height']-values['heightTwips']/20]for p in output['pages']]});flush()
    for name in ('genuine-simple','genuine-SampleDoc'):
        row=rows[name];source=render(Path(row['path']),name+'-source');base=render(Path(row['baseline_docx']),name+'-baseline');out=row['consumer_output'];row.update(consumer_source=source,consumer_baseline=base)
        require(len(source['pages'])==len(out['pages'])==len(base['pages']),'Genuine page count changed')
        for a,b in zip(source['pages'],out['pages']):require(''.join(a['text'].split())==''.join(b['text'].split()),'Genuine whitespace-normalized per-page text differs')
        a,b,old=source['pages'][0],out['pages'][0],base['pages'][0]
        if name=='genuine-simple':
            require(a['fonts']==b['fonts']==old['fonts']==[('LiberationSerif',10.0)],'simple source/output/baseline physical font-size')
            require(a['width']==b['width']==612 and a['height']==b['height']==792,'Genuine Letter MediaBox restored exactly')
            require(a['chars']==b['chars']and a['spans']==b['spans']and a['raster_sha256']==b['raster_sha256'],'simple source/output whole-page character geometry and raster must match')
            require(old['width']!=b['width']and abs(old['chars'][0]['origin'][0]-b['chars'][0]['origin'][0]+18)<0.001,'Known A4 /18pt baseline defect is isolated and repaired')
            report['consumer']['comparisons'].append({'name':name,'kind':'untouched-real-source','whitespace_normalized_per_page_text_equal':True,'source_page_pt':[a['width'],a['height']],'baseline_page_pt':[old['width'],old['height']],'output_page_pt':[b['width'],b['height']],'source_first_origin':a['chars'][0]['origin'],'baseline_first_origin':old['chars'][0]['origin'],'output_first_origin':b['chars'][0]['origin'],'exact_page1_chars_spans_raster':True})
        else:
            require(a['fonts']==b['fonts']==[('Carlito',11.0)],'SampleDoc page1 remains regular11pt')
            require(a['chars']==b['chars'],'SampleDoc page1 exact source/output character geometry')
            require(source['pages'][1]['fonts']==[('NotoSans-Regular',16.0)]and out['pages'][1]['fonts']==[('NotoSerif-Regular',16.0)],'Prior Arial Black missing-font substitution distinction remains')
            report['consumer']['comparisons'].append({'name':name,'kind':'untouched-real-source-page1','raw_page_twips':row['raw_oracle']['geometry'],'whitespace_normalized_per_page_text_equal':True,'source_page_pt':[a['width'],a['height']],'output_page_pt':[b['width'],b['height']],'page1_chars_exact':a['chars']==b['chars'],'page1_raster_exact':a['raster_sha256']==b['raster_sha256']})
            report['consumer']['known_limitations'].append({'name':name,'source_fonts':[p['fonts']for p in source['pages']],'output_fonts':[p['fonts']for p in out['pages']],'source_page2_first_origin':source['pages'][1]['chars'][0]['origin'],'output_page2_first_origin':out['pages'][1]['chars'][0]['origin'],'full_layout_equal':False,'note':'Break-only paragraph spacing and baseline fonts, empty baseline fonts, missing Arial Black substitution and compatibility differences remain outside page geometry proof'})
        flush()
    for name in ('page-asymmetric','page-landscape-letter','page-installed-wrap'):
        row=rows[name];source=render(Path(row['path']),name+'-source');out=row['consumer_output'];row['consumer_source']=source
        require(len(source['pages'])==len(out['pages']),'Authored source/output page count')
        # These use zero paragraph gaps and installed fonts or identical fixed
        # substitution; expected equality must be measured, never assumed.
        require(source['pages']==out['pages'],'Authored raw DOC/output full measured page differs: '+name)
        report['consumer']['comparisons'].append({'name':name,'kind':'independently-authored-binary-source','full_text_font_geometry_raster_exact':True});flush()
    report['consumer']['known_limitations'].append({'scope':'all results','note':'No application browser, Windows WebView2, native printing, Microsoft Word or WPS validation. Single-section encoded page geometry is not general DOC layout fidelity. Historical12pt adjacent-paragraph sum/max difference is retained; these controls do not alter compatibility settings.'})
    require(sha(binary)==pin,'Independent consumer binary changed');flush()


def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,default=ROOT/'tmp/legacy-doc-page-geometry-verify');p.add_argument('--prepare',action='store_true');p.add_argument('--manifest',type=Path);p.add_argument('--cli',type=Path);p.add_argument('--expected-cli-sha256');p.add_argument('--render',action='store_true');args=p.parse_args()
    if args.prepare:prepare(args);return
    require(args.manifest and args.cli and args.expected_cli_sha256,'Provide --prepare or pinned candidate and manifest');run_checks(args)

if __name__=='__main__':main()
