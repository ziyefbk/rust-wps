#!/usr/bin/env python3
"""Independent, source-bound external DOCX family metadata regression checks.

`prepare` reads immutable OPC/XML sources and authors isolated test packages; it
never invokes the product, a renderer, a compiler, or network access. Historical
expectations are frozen before candidate execution. `verify` requires an explicit
serialized resource slot and SHA-pinned candidate. It compares complete responses
and packages, never strips arbitrary metadata or diagnostics. Evidence is noclobber.
"""
import argparse
import collections
import copy
import datetime
import hashlib
import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback
from urllib.parse import unquote_to_bytes
from xml.etree import ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / 'tmp/docx-font-family-verify'
BASELINE = ROOT / 'tmp/legacy-doc-font-metadata-final-cli'
BASELINE_SHA = '9d789ea59a9a42bfc588bba48a1fd445e0da8b0d8d247c680c6927c92a581c5b'
HISTORY = ROOT / 'tmp/doc-font-family-regression/historical-w_nlm0h7/report.json'
HISTORY_SHA = '0c9f135670991156dca9e26ccef78ef79fe0aaa68cd7bcae42d76788cbc4441d'
EXPERIMENT = ROOT / 'tmp/docx-font-family-experiment-20261014'
PINS = {str(BASELINE): BASELINE_SHA, str(HISTORY): HISTORY_SHA,
        str(EXPERIMENT/'manifest.json'): '16cc3ca0749b39fc9cf8c25dddc5ba7e200c488a5041cab087ae4bc962b9be48',
        str(EXPERIMENT/'fresh-run-zh3uvhsv/report.json'): 'b3e619aff0d187dbb60dd26cb2215cf956107fa52a328218933cafc8ed0e5828',
        str(EXPERIMENT/'analysis.json'): '56f77c4a660d3d889c7356b4a1632d95e41d62ae484661601d7fff4f0f78dc31'}
W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
WS = 'http://purl.oclc.org/ooxml/wordprocessingml/main'
A = 'http://schemas.openxmlformats.org/drawingml/2006/main'
R = 'http://schemas.openxmlformats.org/package/2006/relationships'
CT = 'http://schemas.openxmlformats.org/package/2006/content-types'
REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/'
SREL = 'http://purl.oclc.org/ooxml/officeDocument/relationships/'
MIME = {'officeDocument':'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml',
        **{n:'application/vnd.openxmlformats-officedocument.wordprocessingml.'+n+'+xml' for n in ('fontTable','styles','settings')},
        'theme':'application/vnd.openxmlformats-officedocument.theme+xml'}
KEY = 'fontFamilyMetadata'
SUCCESS = 'Source-declared Roman/Swiss classifications preserved for proved Latin text and empty-paragraph fonts; other font metadata, bare-break defaults and local substitution remain unproved'
REASONS = {'package':'package identity is unproved', 'table':'font-table declarations are malformed or ambiguous',
           'body':'body content is outside the supported Latin subset', 'font':'effective font or style/theme identity is unproved',
           'unsupported':'a retained font has no supported source classification', 'alias':'a retained name has duplicate or ASCII-case-alias definitions',
           'binding':'source and imported font ranges differ', 'limit':'proof resource limits exceeded', 'candidate':'candidate metadata failed validation'}
FLOOR = 600*1024**2
MC = 'http://schemas.openxmlformats.org/markup-compatibility/2006'
CP1252 = set('€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ')


def sha(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''): h.update(chunk)
    return h.hexdigest()

def digest(b): return hashlib.sha256(b).hexdigest()
def require(ok,why):
    if not ok: raise AssertionError(why)
def save(path,obj):
    raw=obj if isinstance(obj,bytes) else (json.dumps(obj,indent=2,ensure_ascii=False)+'\n').encode()
    with Path(path).open('xb') as f: f.write(raw)
    return Path(path)
def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def fold(x): return x.translate(str.maketrans('ABCDEFGHIJKLMNOPQRSTUVWXYZ','abcdefghijklmnopqrstuvwxyz'))
def q(name,ns=W): return '{'+ns+'}'+name
def local(tag): return tag.rsplit('}',1)[-1]
def ns(tag): return tag[1:].split('}',1)[0] if tag.startswith('{') else ''
def children(e,tag): return [] if e is None else [c for c in e if c.tag==tag]
def one(e,tag,stage='font'):
    values=children(e,tag); proof(len(values)<=1,stage,'Repeated '+local(tag)); return values[0] if values else None
def val(e,name='val',namespace=W): return None if e is None else e.get(q(name,namespace))
def boolean(e): return e is not None and val(e) not in ('false','off','0')
def message(reason): return 'DOCX font-family metadata not preserved: '+reason+'; existing imported fonts and layout are unchanged'
def disk(extra=0): require(shutil.disk_usage(ROOT).free>=FLOOR+extra,'600 MiB observed-free-space floor plus reserved work')
def verify_ids(protected):
    for p,h in protected.items(): require(sha(p)==h,'Protected bytes changed: '+p)

class Unavailable(Exception):
    def __init__(self,stage,evidence): self.stage,self.evidence=stage,evidence

def proof(ok,stage,evidence):
    if not ok: raise Unavailable(stage,evidence)

class Budget:
    def __init__(self): self.nodes=0; self.ancillary=0; self.links=0; self.visits=0; self.text=0
    def xml(self,raw,stage,main=False):
        if main: proof(len(raw)<=16*1024**2,'limit','Main XML exceeds 16 MiB')
        else: self.ancillary+=len(raw); proof(self.ancillary<=1024**2,'limit','Cumulative ancillary XML exceeds 1 MiB')
        proof(not re.search(br'<!\s*(DOCTYPE|ENTITY)\b',raw,re.I),stage,'DTD/entity declaration')
        parser=ET.XMLPullParser(events=('start','end'));root=None;depth=0
        try:
            for at in range(0,len(raw),4096):
                parser.feed(raw[at:at+4096])
                for event,e in parser.read_events():
                    if event=='end':depth-=1;continue
                    if root is None:root=e
                    depth+=1;self.nodes+=1
                    proof(depth<=128 and self.nodes<=100000,'limit','XML depth/node budget')
                    proof(len(e.attrib)<=64 and all(len(v.encode())<=16384 for v in e.attrib.values()),'limit','XML attribute budget')
            parser.close()
            # Expat may defer reparsing a long token until close; drain those
            # final events under the same counters instead of accepting them.
            for event,e in parser.read_events():
                if event=='end':depth-=1;continue
                if root is None:root=e
                depth+=1;self.nodes+=1
                proof(depth<=128 and self.nodes<=100000,'limit','XML depth/node budget')
                proof(len(e.attrib)<=64 and all(len(v.encode())<=16384 for v in e.attrib.values()),'limit','XML attribute budget')
        except (ET.ParseError,ValueError) as e: raise Unavailable(stage,'Malformed XML: '+str(e).split(':')[0]) from e
        proof(root is not None,stage,'Missing XML root');return root
    def scan(self,count): self.visits+=count; proof(self.visits<=5000000,'limit','Aggregate helper-tree visits')


def canonical(path,relative=False):
    proof(isinstance(path,str) and path!='','package','Missing part identity')
    proof(not re.search(r'%(?![0-9A-Fa-f]{2})',path),'package','Malformed percent escape')
    proof(not any(x in path for x in ('\\','?','#',':')) and not path.startswith('//'),'package','Ambiguous URI/separator')
    try: decoded=unquote_to_bytes(path).decode('utf-8')
    except UnicodeError as e: raise Unavailable('package','Invalid percent-encoded UTF-8') from e
    proof(not re.search(r'%(?:2f|5c|2e)',path,re.I),'package','Encoded separator/dot identity')
    proof(not any(x in decoded for x in ('\\','?','#',':')) and not any(ord(c)<32 or ord(c)==127 for c in decoded),'package','Decoded URI/separator/control')
    if not relative: proof(not any(x in ('.','..','') for x in decoded.lstrip('/').split('/')),'package','Raw member traversal/empty segment')
    return decoded

class Package:
    def __init__(self,path,budget):
        self.budget=budget; self.payload={}; self.identities=collections.defaultdict(list); self.selected={}; self.controls={}
        proof(Path(path).stat().st_size<=64*1024**2,'limit','Compressed input bound')
        try:
            with zipfile.ZipFile(path) as z:
                infos=z.infolist(); proof(len(infos)<=4096,'limit','ZIP entry count'); proof(sum(i.file_size for i in infos)<=128*1024**2,'limit','ZIP expansion bound')
                for i in infos:
                    if i.is_dir(): continue
                    name=canonical(i.filename).lstrip('/');self.identities[fold(name)].append(i.filename)
                    self.payload[i.filename]=z.read(i)
        except (zipfile.BadZipFile,RuntimeError) as e: raise Unavailable('package','ZIP identity/CRC') from e
    def get(self,path):
        name=canonical(path).lstrip('/'); names=self.identities.get(fold(name),[])
        proof(len(names)==1,'package','Selected part absent/physical alias: '+name)
        proof(canonical(names[0]).lstrip('/')==name and names[0].lstrip('/')==name,'package','Selected physical member differs from reader-decoded identity: '+name)
        self.selected[name]=digest(self.payload[names[0]])
        return self.payload[names[0]]
    def relationships(self,path,optional=False):
        if optional and fold(path) not in self.identities: return []
        raw=self.get(path);root=self.budget.xml(raw,'package');proof(root.tag==q('Relationships',R) and not root.attrib,'package','Raw relationships root')
        ids=set();rows=[]
        for e in root:
            proof(e.tag==q('Relationship',R) and not len(e) and not (e.text or '').strip(),'package','Wrapped/foreign relationship')
            proof(set(e.attrib)<={'Id','Type','Target','TargetMode'} and {'Id','Type','Target'}<=set(e.attrib),'package','Relationship attributes')
            proof(all(e.get(k) for k in ('Id','Type','Target')) and e.get('Id') not in ids,'package','Relationship ID uniqueness')
            proof(e.get('TargetMode') in (None,'Internal','External'),'package','Unknown TargetMode')
            ids.add(e.get('Id'));rows.append(dict(e.attrib))
        proof(not (root.text or '').strip() and all(not(e.tail or '').strip() for e in root),'package','Unexpected relationship text')
        self.controls[path]=rows;return rows
    def target(self,row,base):
        proof(row.get('TargetMode')!='External','package','External selected role')
        target=canonical(row['Target'],relative=True)
        resolved=posixpath.normpath(target.lstrip('/') if target.startswith('/') else posixpath.join(base,target))
        proof(not resolved.startswith('../') and resolved!='..','package','Relationship escaped package')
        return resolved
    def role(self,rows,role,base):
        matching=[x for x in rows if x['Type'] in (REL+role,SREL+role)]
        proof(not any(x['Type'].endswith('/'+role) and x not in matching for x in rows),'package','Lookalike role URI: '+role)
        proof(len(matching)<=1,'package','Repeated relationship role: '+role)
        return None if not matching else self.target(matching[0],base)
    def types(self):
        root=self.budget.xml(self.get('[Content_Types].xml'),'package');proof(root.tag==q('Types',CT) and not root.attrib,'package','Content type root')
        defaults={};overrides={}
        for e in root:
            proof(not len(e) and not(e.text or '').strip(),'package','Wrapped content type')
            if e.tag==q('Default',CT):
                proof(set(e.attrib)=={'Extension','ContentType'} and all(e.attrib.values()),'package','Default attributes'); key=fold(e.get('Extension'));proof(key not in defaults,'package','Duplicate Default extension');defaults[key]=e.get('ContentType')
            elif e.tag==q('Override',CT):
                proof(set(e.attrib)=={'PartName','ContentType'} and all(e.attrib.values()),'package','Override attributes');key=fold(canonical(e.get('PartName')).lstrip('/'));proof(key not in overrides,'package','Duplicate normalized Override');overrides[key]=e.get('ContentType')
            else: raise Unavailable('package','Foreign content type declaration')
        return defaults,overrides
    def authority(self):
        rootrels=self.relationships('_rels/.rels');main=self.role(rootrels,'officeDocument','')
        proof(main=='word/document.xml','package','Unique actual conventional main role')
        rows=self.relationships('word/_rels/document.xml.rels',True)
        font=self.role(rows,'fontTable','word')
        # Absence is only established after strict raw relationship authority.
        if font is None: return None
        roles={'officeDocument':main, **{k:self.role(rows,k,'word') for k in ('fontTable','styles','theme','settings')}}
        active=[p for p in roles.values() if p is not None]
        proof(len({fold(p) for p in active})==len(active),'package','Selected roles alias one part')
        proof(not any(p.startswith('_rels/') or '/_rels/' in p or p=='[Content_Types].xml' for p in active),'package','Selected role aliases control part')
        defaults,overrides=self.types()
        for role,path in roles.items():
            if path is None: continue
            self.get(path); content_type=overrides.get(fold(path),defaults.get(fold(path.rsplit('.',1)[-1])))
            proof(content_type==MIME[role],'package','Role MIME differs: '+role)
        return roles


def valid_name(name):
    return isinstance(name,str) and name and name.strip()==name and len(name.encode('utf-16-le'))//2<=31 and not any(ord(c)<32 or 127<=ord(c)<160 or c in '\ufeff\ufffe\uffff' for c in name)

def font_records(raw,budget):
    root=budget.xml(raw,'table');w=ns(root.tag);proof(w in (W,WS) and root.tag==q('fonts',w),'table','Font table root namespace')
    proof(all(k==q('Ignorable',MC) for k in root.attrib),'table','Unknown font root attribute')
    proof(not(root.text or '').strip(),'table','Unexpected root text'); records=[]
    for e in root:
        proof(e.tag==q('font',w) and set(e.attrib)=={q('name',w)} and val(e,'name',w),'table','Font path/name')
        proof(not(e.text or '').strip() and not(e.tail or '').strip(),'table','Unexpected font text')
        seen=set();record={'name':val(e,'name',w),'family':'auto','charset':None,'unsupported_active':[]}
        for child in e:
            tag=local(child.tag);proof(ns(child.tag)==w and tag not in seen and not len(child),'table','Duplicate/foreign/wrapped font metadata');seen.add(tag)
            proof(not(child.text or '').strip() and not(child.tail or '').strip(),'table','Unexpected metadata text')
            if tag in ('family','panose1','charset','pitch','altName'):
                proof(set(child.attrib)<={q('val',w),*([q('characterSet',w)] if tag=='charset' else [])} and val(child,namespace=w) is not None,'table','Ancillary value path')
                v=val(child,namespace=w)
                if tag=='family':proof(v in ('auto','roman','swiss','modern','script','decorative'),'table','Unknown family enum');record['family']=v
                elif tag=='charset':proof(re.fullmatch('[0-9A-Fa-f]{2}',v) is not None,'table','Invalid charset');record['charset']=v.lower()
                elif tag=='panose1':proof(re.fullmatch('[0-9A-Fa-f]{20}',v) is not None,'table','Invalid PANOSE')
                elif tag=='pitch':proof(v in ('default','fixed','variable'),'table','Unknown pitch')
                else:record['unsupported_active'].append('altName')
            elif tag=='sig':proof(set(child.attrib)=={q(n,w) for n in ('usb0','usb1','usb2','usb3','csb0','csb1')} and all(re.fullmatch('[0-9A-Fa-f]{8}',v) for v in child.attrib.values()),'table','Invalid signature')
            elif tag=='notTrueType':proof(set(child.attrib)<={q('val',w)},'table','notTrueType attributes')
            elif tag in ('embedRegular','embedBold','embedItalic','embedBoldItalic'):record['unsupported_active'].append(tag)
            else:raise Unavailable('table','Unknown font metadata path: '+tag)
        records.append(record)
    proof(len(records)<=256,'limit','Font definition count exceeds 256')
    return records


def ensure_reader_prefix(raw):
    # The legacy reader consumes literal w: for typography. Expanded identity alone
    # must not bless a foreign/aliased property the reader interprets differently.
    active={'rFonts','rStyle','pStyle','rPr','pPr','style','basedOn','docDefaults','rPrDefault','themeFontLang','cs','rtl','lang','sz','bidi'}
    for m in re.finditer(rb'<(?![!?/])([A-Za-z_][\w.-]*:)?([A-Za-z_][\w.-]*)(\s[^<>]*?|\s*)/?>',raw):
        if m.group(2).decode() in active:
            proof(m.group(1)==b'w:','font','Reader typography property prefix differs')
            text=m.group(3)
            for a in re.finditer(rb'([A-Za-z_][\w.-]*:)?([A-Za-z_][\w.-]*)\s*=',text):
                if a.group(2).decode() in {'ascii','hAnsi','asciiTheme','hAnsiTheme','val','styleId','type','default','hint'}:
                    proof(a.group(1)==b'w:','font','Reader typography attribute prefix differs')


def body_blocks(raw,budget):
    doc=budget.xml(raw,'body',main=True);w=ns(doc.tag);proof(w in (W,WS) and doc.tag==q('document',w),'body','Main root');body=one(doc,q('body',w),'body');proof(body is not None,'body','Missing body')
    proof(len(children(doc,q('body',w)))==1,'body','Unique body')
    result=[];allowed_inert={q(n,w) for n in ('bookmarkStart','bookmarkEnd','proofErr')}
    for index,p in enumerate(body):
        if p.tag==q('sectPr',w):proof(index==len(body)-1,'body','Only trailing section');continue
        proof(p.tag==q('p',w),'body','Nonparagraph top-level body: '+local(p.tag));ppr=one(p,q('pPr',w),'body')
        if ppr is not None:proof(not any(e.tag==q('numPr',w) for e in ppr.iter()),'body','Direct numbering context')
        runs=[];bare=[]
        for r in p:
            if r is ppr or r.tag in allowed_inert:continue
            proof(r.tag==q('r',w),'body','Wrapped/unsupported paragraph content: '+local(r.tag));rpr=one(r,q('rPr',w),'body');text='';br=[]
            for child in r:
                if child is rpr or child.tag==q('lastRenderedPageBreak',w):continue
                if child.tag==q('t',w):
                    proof(not len(child),'body','Wrapped text');fragment=child.text or '';proof(fragment!='','body','Empty text node')
                    proof(all(32<=ord(c)<=126 or (160<=ord(c)<=255 and c!='µ') or c in CP1252 for c in fragment),'body','Nonadmitted scalar');text+=fragment
                elif child.tag==q('br',w):proof(child.attrib=={q('type',w):'page'} and not len(child),'body','Non-page break');br.append(child)
                else:raise Unavailable('body','Unsupported run content: '+local(child.tag))
            if br:
                proof(not text and rpr is None and not r.attrib and len(r)==1 and len(br)==1,'body','Styled or mixed manual break');bare.extend(br)
            elif text:runs.append((text,rpr))
            else:proof(rpr is None and not len(r),'body','Unproved empty property run')
        if bare:
            proof(len(bare)==1 and not runs and ppr is None and len(p)==1 and all(ns(k)==w and local(k).startswith('rsid') for k in p.attrib),'body','Nonbare break paragraph');kind='pageBreak'
        else:kind='paragraph' if runs else 'empty'
        budget.text+=sum(len(t) for t,_ in runs);proof(budget.text<=1000000,'limit','Source scalar count');result.append({'kind':kind,'ppr':ppr,'runs':runs})
    proof(len(result)+sum(len(x['runs']) for x in result)<=10000,'limit','Body/range request bound')
    return result,w,doc


def reader_property_shape(props,w,budget):
    # Reader-consumed structural identity precedes numbering. This deliberately
    # does not impose the later font helper's property-name allowlist.
    if props is None:return
    budget.scan(1+len(props))
    proof(props.tag==q('rPr',w) and all(ns(k)==w for k in props.attrib),'font','Unproved reader property identity')
    for child in props:
        proof(ns(child.tag)==w and all(ns(k)==w for k in child.attrib) and not len(child),'font','Foreign or wrapped reader property')


def property_grammar(props,w,budget,paragraph=False):
    """Independent strict grammar for the contract's preexisting bounded helper.

    Neutral-looking excluded properties are still unproved. Apply only to
    consulted layers; unused template styles do not acquire authority here.
    """
    if props is None:return
    budget.scan(1+len(props))
    run_tags=set('rStyle rFonts b bCs i iCs caps smallCaps strike dstrike noProof snapToGrid color spacing kern position sz szCs highlight u rtl cs em lang shd'.split())
    paragraph_tags=set('pStyle keepNext keepLines pageBreakBefore widowControl suppressLineNumbers suppressAutoHyphens kinsoku wordWrap overflowPunct topLinePunct autoSpaceDE autoSpaceDN bidi adjustRightInd snapToGrid spacing ind contextualSpacing mirrorIndents suppressOverlap jc textAlignment textboxTightWrap outlineLvl shd'.split())
    proof(props.tag==q('pPr' if paragraph else 'rPr',w) and not props.attrib and not (props.text or '').strip(),'font','Unproved typography property container')
    seen=set()
    for child in props:
        tag=local(child.tag)
        proof(ns(child.tag)==w and tag not in seen,'font','Foreign/repeated typography property');seen.add(tag)
        if paragraph and tag=='rPr':property_grammar(child,w,budget);continue
        proof(tag in (paragraph_tags if paragraph else run_tags),'font','Outside bounded '+('paragraph' if paragraph else 'run')+' property grammar: '+tag)
        proof(not len(child) and not (child.text or '').strip() and not (child.tail or '').strip(),'font','Wrapped/textual typography property')
        proof(all(ns(key)==w for key in child.attrib),'font','Foreign/unqualified typography property attribute')


def section_font_grammar(document,w,budget):
    # The existing helper proves no explicit direction declaration, even lrTb.
    # This guard runs after complete body grammar, before any effective font or
    # family join, and never runs on absent/empty-table/no-retained no-op paths.
    budget.scan(sum(1 for _ in document.iter()))
    for section in (e for e in document.iter() if local(e.tag)=='sectPr'):
        budget.scan(sum(1 for _ in section.iter()))
        proof(not any(local(e.tag)=='textDirection' for e in section.iter()),'font','Explicit section textDirection is outside bounded font proof')
        grids=[e for e in section if local(e.tag)=='docGrid']
        proof(len(grids)<=1 and all(e.tag==q('docGrid',w) for e in grids),'font','Ambiguous section document grid')
        for grid in grids:
            proof(not len(grid) and not (grid.text or '').strip() and set(grid.attrib)<={q(k,w) for k in ('type','linePitch','charSpace')},'font','Unproved section document grid shape')
            proof(val(grid,'type',w) in (None,'default'),'font','Active section document grid')


def resolve_body(package,roles,blocks,w,budget,document):
    stylesraw=package.get(roles['styles']) if roles['styles'] else b'<w:styles xmlns:w="'+w.encode()+b'"/>'
    ensure_reader_prefix(stylesraw);styles=budget.xml(stylesraw,'font') if roles['styles'] else ET.fromstring(stylesraw);proof(styles.tag==q('styles',w),'font','Styles root');stylelist=children(styles,q('style',w));proof(len(stylelist)<=4093,'limit','Styles count')
    byid={};default={}
    for st in stylelist:
        sid=val(st,'styleId',w);proof(sid and sid not in byid,'font','Duplicate/missing style ID');byid[sid]=st
        if val(st,'default',w) in ('1','true','on'):
            role=val(st,'type',w);proof(role not in default,'font','Duplicate default style role');default[role]=sid
    dd=one(styles,q('docDefaults',w));dr=one(dd,q('rPrDefault',w));baseprops=one(dr,q('rPr',w));chaincache={}
    dp=one(dd,q('pPrDefault',w));defaultp=one(dp,q('pPr',w))
    def chain(sid,kind):
        if sid is None:return []
        cachekey=(sid,kind)
        if cachekey in chaincache:return chaincache[cachekey]
        result=[];seen=set()
        while sid:
            budget.links+=1;proof(budget.links<=10000 and len(result)<64,'limit','Style chain traversal budget')
            proof(sid in byid and sid not in seen,'font','Missing/cyclic active style');seen.add(sid);st=byid[sid];proof(val(st,'type',w)==kind,'font','Wrong active style role');result.append(st)
            proof(sum(1 for e in st.iter() if e.tag==q('basedOn',w))==len(children(st,q('basedOn',w))),'font','Nested basedOn')
            sid=val(one(st,q('basedOn',w)),'val',w)
        result.reverse();chaincache[cachekey]=result;return result
    theme=None;settings=None;theme_cache={}
    def theme_name(token):
        nonlocal theme,settings
        proof(token in ('majorAscii','majorHAnsi','minorAscii','minorHAnsi'),'font','Unsupported theme slot');key=token[:5]
        if key in theme_cache:return theme_cache[key]
        if theme is None:
            proof(roles['theme'] is not None,'font','Missing theme relationship');theme=budget.xml(package.get(roles['theme']),'font')
            if roles['settings']:
                raw=package.get(roles['settings']);ensure_reader_prefix(raw);settings=budget.xml(raw,'font');proof(settings.tag==q('settings',w),'font','Settings root');ls=[e for e in settings.iter() if local(e.tag)=='themeFontLang'];proof(len(ls)<=1 and all(e in list(settings) and e.tag==q('themeFontLang',w) for e in ls),'font','Ambiguous themeFontLang')
                if ls:
                    lang=val(ls[0],namespace=w);proof(lang is None or re.fullmatch(r'(en|es|fr|de|it|pt|nl)(?:-Latn)?(?:-[A-Za-z]{2}|-[0-9]{3})?',lang,re.I),'font','Unsupported theme language')
        schemes=[e for e in theme.iter() if local(e.tag)=='fontScheme'];proof(len(schemes)==1,'font','Unique theme fontScheme');scheme=schemes[0]
        sets=children(scheme,q(key+'Font',A));proof(len(sets)==1,'font','Unique Latin font collection');collection=sets[0];latin=children(collection,q('latin',A));proof(len(latin)==1 and latin[0].get('typeface'),'font','Unique theme Latin face')
        proof(not any(e.tag==q('font',A) and e.get('script')=='Latn' for e in collection),'font','Supplemental Latn mapping')
        name=latin[0].get('typeface');proof(valid_name(name),'font','Invalid theme literal name');theme_cache[key]=name;return name
    def layer(state,rpr,toggle=False):
        if rpr is None:return
        budget.scan(len(list(rpr.iter())));property_grammar(rpr,w,budget)
        for e in rpr.iter():
            if local(e.tag) in ('rFonts','rStyle','sz','cs','rtl','lang'):
                proof(e in list(rpr) and e.tag==q(local(e.tag),w),'font','Nested/foreign active typography property')
        language=one(rpr,q('lang',w))
        if language is not None:
            used=val(language,namespace=w)
            proof(used is None or re.fullmatch(r'(en|es|fr|de|it|pt|nl)(?:-Latn)?(?:-[A-Za-z]{2}|-[0-9]{3})?',used,re.I),'font','Active non-Latin or unproved source language')
        for prop in ('cs','rtl'):
            e=one(rpr,q(prop,w));proof(not boolean(e),'font','Complex script forcing property')
        for property_name,key in (('b','bold'),('i','italic')):
            flag=one(rpr,q(property_name,w))
            if flag is not None:
                value=val(flag,namespace=w);proof(value in (None,'0','1','true','false','on','off'),'font','Unknown style toggle')
                enabled=value not in ('0','false','off')
                state[key]=(not state.get(key,False)) if toggle and enabled else state.get(key,False) if toggle else enabled
        fs=one(rpr,q('rFonts',w))
        if fs is not None:
            proof(val(fs,'hint',w) in (None,'default'),'font','Complex font hint')
            proof(set(fs.attrib)<={q(a,w) for a in ('ascii','hAnsi','eastAsia','cs','asciiTheme','hAnsiTheme','eastAsiaTheme','cstheme','csTheme','hint')},'font','Unknown/foreign/unqualified font attributes')
            for slot in ('ascii','hAnsi'):
                themed=val(fs,slot+'Theme',w);literal=val(fs,slot,w)
                if themed is not None:state[slot]=('theme',themed)
                elif literal is not None:state[slot]=('literal',literal)
        size=one(rpr,q('sz',w))
        if size is not None:
            value=val(size,namespace=w);proof(value is not None and value.isdigit() and 2<=int(value)<=3276,'font','Invalid baseline size');state['size']=int(value)
    def name(state):
        names=[]
        for slot in ('ascii','hAnsi'):
            proof(slot in state,'font','Missing effective Latin slot');kind,face=state[slot];face=theme_name(face) if kind=='theme' else face;proof(valid_name(face),'font','Unsupported literal name');names.append(face)
        proof(names[0]==names[1],'font','Unequal effective Latin slots');return names[0]
    # Establish active numbering before the newly explicit typography guards.
    # Reuse source-derived chains, never scan dormant template style properties.
    reader_property_shape(baseprops,w,budget)
    if defaultp is not None:budget.scan(sum(1 for _ in defaultp.iter()))
    proof(defaultp is None or not any(e.tag==q('numPr',w) for e in defaultp.iter()),'body','Default paragraph numbering context')
    numbering_checked=set()
    for block in blocks:
        if block['kind']=='pageBreak':continue
        reader_property_shape(one(block['ppr'],q('rPr',w)),w,budget)
        ps=one(block['ppr'],q('pStyle',w));sid=val(ps,namespace=w) if ps is not None else default.get('paragraph')
        if sid in numbering_checked:continue
        numbering_checked.add(sid)
        for st in chain(sid,'paragraph'):
            reader_property_shape(one(st,q('rPr',w)),w,budget)
            spr=one(st,q('pPr',w))
            if spr is not None:budget.scan(sum(1 for _ in spr.iter()))
            proof(spr is None or not any(e.tag==q('numPr',w) for e in spr.iter()),'body','Active style numbering context')
    section_font_grammar(document,w,budget);property_grammar(defaultp,w,budget,True)
    resolved=[];active=set()
    for block in blocks:
        if block['kind']=='pageBreak':resolved.append({'kind':'pageBreak'});continue
        ppr=block['ppr'];ps=one(ppr,q('pStyle',w));sid=val(ps,namespace=w) if ps is not None else default.get('paragraph');state={};layer(state,baseprops)
        pc=chain(sid,'paragraph')
        for st in pc:
            spr=one(st,q('pPr',w));proof(not boolean(one(spr,q('bidi',w))),'font','Paragraph bidi forcing');proof(one(spr,q('numPr',w)) is None,'body','Active style numbering context');property_grammar(spr,w,budget,True);layer(state,one(st,q('rPr',w)),True)
        property_grammar(ppr,w,budget,True);proof(not boolean(one(ppr,q('bidi',w))),'font','Direct paragraph bidi forcing')
        ranges=[]
        for text,rpr in block['runs']:
            rs=dict(state);cs=one(rpr,q('rStyle',w))
            if cs is not None:
                for st in chain(val(cs,namespace=w),'character'):layer(rs,one(st,q('rPr',w)),True)
            layer(rs,rpr);face=name(rs);active.add(face)
            if ranges and ranges[-1]['name']==face:ranges[-1]['text']+=text
            else:ranges.append({'text':text,'name':face})
        row={'kind':block['kind'],'ranges':ranges,'paragraph_style':sid}
        if block['kind']=='empty':
            layer(state,one(ppr,q('rPr',w)));face=name(state);proof('size' in state,'font','Missing empty baseline size');active.add(face);row['empty_font']={'family':face,'halfPoints':state['size'],'bold':state.get('bold',False),'italic':state.get('italic',False)}
        resolved.append(row)
    proof(len(active)<=256,'limit','Retained name count')
    return resolved,active


def bind_model(model,blocks):
    content=model.get('content',[]);proof(len(content)==len(blocks),'binding','Full block count')
    for actual,expected in zip(content,blocks):
        if expected['kind']=='pageBreak':proof(actual=={'type':'pageBreak'},'binding','Exact bare page break identity');continue
        proof(actual.get('type') in ('paragraph','heading'),'binding','Paragraph kind');ranges=[]
        for node in actual.get('content',[]):
            proof(node.get('type')=='text' and isinstance(node.get('text'),str),'binding','Additional nontext node')
            fonts=[m.get('attrs',{}).get('name') for m in node.get('marks',[]) if m.get('type')=='fontFamily'];proof(len(fonts)==1,'binding','Missing/duplicate actual font mark')
            if ranges and ranges[-1]['name']==fonts[0]:ranges[-1]['text']+=node['text']
            else:ranges.append({'text':node['text'],'name':fonts[0]})
        proof(ranges==expected['ranges'],'binding','Complete ordered text/font ranges')
        if expected['kind']=='empty':proof(actual.get('attrs',{}).get('emptyParagraphFont')==expected['empty_font'],'binding','Complete empty baseline object')


def source_oracle(path,model=None):
    b=Budget();evidence={'source_sha256':sha(path),'method':'Independent stdlib ZIP/XML, exact role identity, source cascade and complete ordered ranges'}
    try:
        package=Package(path,b);roles=package.authority();evidence['raw_relationships']=package.controls
        if roles is None:return dict(evidence,outcome='exact',absence='No authoritative main-document fontTable relationship')
        evidence['roles']=roles;records=font_records(package.get(roles['fontTable']),b);evidence['records']=records
        if not records:return dict(evidence,outcome='exact',absence='Valid empty authoritative font table')
        relpath=posixpath.join(posixpath.dirname(roles['fontTable']),'_rels',posixpath.basename(roles['fontTable'])+'.rels')
        fr=package.relationships(relpath,True);proof(not any(x['Type'] in (REL+'font',SREL+'font') for x in fr),'table','Embedded font-program relationship')
        raw=package.get(roles['officeDocument']);blocks,w,document=body_blocks(raw,b)
        if all(block['kind']=='pageBreak' for block in blocks):return dict(evidence,outcome='exact',absence='Admitted body has no retained text/blank font')
        ensure_reader_prefix(raw);resolved,active=resolve_body(package,roles,blocks,w,b,document);evidence['blocks']=resolved
        if not active:return dict(evidence,outcome='exact',absence='Admitted body has no retained text/blank font')
        metadata=[]
        for name in sorted(active):
            definitions=[r for r in records if fold(r['name'])==fold(name)]
            proof(len(definitions)<=1 and (not definitions or definitions[0]['name']==name),'alias','Retained ASCII-fold equivalence class: '+name)
            proof(len(definitions)==1 and definitions[0]['family'] in ('roman','swiss'),'unsupported','Missing/unsupported source family: '+name)
            r=definitions[0];proof(not r['unsupported_active'] and r['charset'] in (None,'00'),'font','Active alternate/embedded/non-Latin font record: '+name)
            metadata.append({'name':name,'family':r['family']})
        if model is not None:bind_model(model,resolved)
        evidence['selected_part_sha256']=package.selected
        return dict(evidence,outcome='positive',metadata=metadata,model_bound=model is not None)
    except Unavailable as error:
        return dict(evidence,outcome='fallback',reason=REASONS[error.stage],stage=error.stage,reason_evidence=error.evidence)


def response(row): return {k:row[k] for k in ('returncode','stdout','stderr')}
def expected_response(old,oracle):
    expected=copy.deepcopy(old)
    if old['returncode'] or oracle['outcome']=='exact':return expected
    obj=json.loads(old['stdout']);require(obj.get('document') is not None,'Import success has actual document')
    require(json.dumps(obj,indent=2,ensure_ascii=False)+'\n'==old['stdout'],'Baseline JSON serialization independently reproducible')
    if oracle['outcome']=='positive':
        attrs=obj['document'].setdefault('attrs',{});require(KEY not in attrs,'Old generic response unexpectedly has family metadata');attrs[KEY]=[{k:r[k] for k in ('family','name')} for r in oracle['metadata']];obj['document']['attrs']=dict(sorted(attrs.items()))
        added=SUCCESS
    else:added=message(oracle['reason'])
    risks=obj['compatibility']['risks'];require(added not in risks,'Baseline unexpectedly has new message');risks.append(added);risks.sort()
    expected['stdout']=json.dumps(obj,indent=2,ensure_ascii=False)+'\n';return expected


def esc(s): return s.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;').replace('"','&quot;')
def xml(tag,body='',attrs='',namespace=W,prefix='w'):
    return ('<?xml version="1.0" encoding="UTF-8"?><'+prefix+':'+tag+' xmlns:'+prefix+'="'+namespace+'"'+(' '+attrs if attrs else '')+'>'+body+'</'+prefix+':'+tag+'>').encode()
def rfonts(name,other=None): return '<w:rFonts w:ascii="'+esc(name)+'" w:hAnsi="'+esc(other if other is not None else name)+'"/>'
def runxml(text='Independent family proof.',name='FamilyProbeA',extra='',props=None):
    rp=rfonts(name)+extra if props is None else props
    return '<w:r>'+('<w:rPr>'+rp+'</w:rPr>' if rp else '')+'<w:t xml:space="preserve">'+esc(text)+'</w:t></w:r>'
def pxml(runs=None,ppr=''):
    return '<w:p>'+('<w:pPr>'+ppr+'</w:pPr>' if ppr else '')+(runxml() if runs is None else runs)+'</w:p>'
def tablexml(records):
    return xml('fonts',''.join('<w:font w:name="'+esc(n)+'">'+('' if f is None else '<w:family w:val="'+f+'"/>')+'</w:font>' for n,f in records))
def relxml(rows):
    return ('<?xml version="1.0"?><Relationships xmlns="'+R+'">'+''.join('<Relationship '+ ' '.join(k+'="'+esc(v)+'"' for k,v in r.items())+'/>' for r in rows)+'</Relationships>').encode()
def source_parts(body=None,records=None,styles=None,theme=None,settings=None):
    p={'word/document.xml':xml('document','<w:body>'+(pxml() if body is None else body)+'</w:body>'),
       'word/fontTable.xml':tablexml([('FamilyProbeA','swiss')] if records is None else records),
       '_rels/.rels':relxml([{'Id':'main','Type':REL+'officeDocument','Target':'word/document.xml'}])}
    rows=[{'Id':'font','Type':REL+'fontTable','Target':'fontTable.xml'}]
    for role,value in [('styles',styles),('theme',theme),('settings',settings)]:
        if value is not None:p['word/'+role+'.xml']=value;rows.append({'Id':role,'Type':REL+role,'Target':role+'.xml'})
    p['word/_rels/document.xml.rels']=relxml(rows)
    overrides=''.join('<Override PartName="/'+path+'" ContentType="'+MIME['officeDocument' if path=='word/document.xml' else path.split('/')[-1][:-4]]+'"/>' for path in p if path.startswith('word/') and '/_rels/' not in path)
    p['[Content_Types].xml']=('<Types xmlns="'+CT+'"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'+overrides+'</Types>').encode()
    return p

def write_zip(path,parts,extra=()):
    with zipfile.ZipFile(path,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for n,b in list(parts.items())+list(extra):
            i=zipfile.ZipInfo(n,(1980,1,1,0,0,0));i.compress_type=zipfile.ZIP_DEFLATED;z.writestr(i,b)

def make_fixtures(dest):
    cases=[]
    def add(label,parts,expect=None,stage=None,extra=(),coverage=None):
        disk(20*1024**2);path=dest/(label+'.docx');write_zip(path,parts,extra);oracle=source_oracle(path)
        if expect is not None:require(oracle['outcome']==expect,label+': independently authored expected outcome '+str(oracle))
        if stage is not None:require(oracle.get('stage')==stage,label+': independently authored reason stage '+str(oracle))
        cases.append({'name':label,'source':str(path),'source_sha256':sha(path),'operation':'import-docx','kind':'independently-authored-opc-xml','source_evidence':oracle,'coverage':coverage or label})
    base=source_parts();add('direct-swiss',base,'positive')
    add('direct-roman-same-absent-name',source_parts(records=[('FamilyProbeA','roman')]),'positive')
    two=pxml(runxml('First range. ','FamilyProbeA')+runxml('Second range.','FamilyProbeB'))
    pair=[('FamilyProbeA','swiss'),('FamilyProbeB','roman')]
    add('two-literal-ranges',source_parts(body=two,records=pair),'positive')
    add('repeated-font-and-format-splits',source_parts(body=pxml(runxml('First ','FamilyProbeA')+runxml('bold','FamilyProbeA','<w:b/>')+runxml(' and final.','FamilyProbeA'))),'positive')
    name='A&B "Face" <Z>';add('escaped-name-and-text',source_parts(body=pxml(runxml('Fish & chips <fine> “yes”.',name)),records=[(name,'roman')]),'positive')
    add('printable-latin-and-cp1252',source_parts(body=pxml(runxml('Café Àÿ € Œ Š Ž it’s fine.'))),'positive')
    for f in ('auto','modern','script','decorative',None):
        label=f or 'omitted'
        add('late-family-'+label,source_parts(body=two,records=[pair[0],('FamilyProbeB',f)]),'fallback','unsupported')
        add('inactive-family-'+label,source_parts(records=[('FamilyProbeA','swiss'),('Dormant',f)]),'positive')
    add('late-record-missing',source_parts(body=two),'fallback','unsupported')
    add('known-name-no-record',source_parts(body=pxml(runxml(name='Cambria'))),'fallback','unsupported')
    for label,name,family in [('duplicate-equal','FamilyProbeA','swiss'),('duplicate-conflict','FamilyProbeA','roman'),('ascii-alias','familyprobea','swiss')]:
        add(label,source_parts(records=[('FamilyProbeA','swiss'),(name,family)]),'fallback','alias')
    add('differently-cased-only-definition',source_parts(records=[('familyprobea','swiss')]),'fallback','alias')
    add('inactive-equal-duplicates',source_parts(records=[('FamilyProbeA','swiss'),('Dormant','auto'),('Dormant','modern')]),'positive')
    add('family-unknown-enum',source_parts(records=[('FamilyProbeA','serif')]),'fallback','table')
    for label,old,new in [('family-child-duplicate',b'<w:family w:val="swiss"/>',b'<w:family w:val="swiss"/><w:family w:val="swiss"/>'),
                          ('family-wrapped',b'<w:family w:val="swiss"/>',b'<w:wrap><w:family w:val="swiss"/></w:wrap>'),
                          ('family-foreign-shadow',b'<w:family w:val="swiss"/>',b'<x:family xmlns:x="urn:foreign" w:val="swiss"/>'),
                          ('font-wrapped',b'<w:font ',b'<w:wrap/><w:font '),
                          ('font-name-duplicate-attribute',b'w:name="FamilyProbeA"',b'w:name="FamilyProbeA" w:name="FamilyProbeA"'),
                          ('font-unexpected-text',b'<w:family',b'unexpected<w:family')]:
        parts=dict(base);parts['word/fontTable.xml']=parts['word/fontTable.xml'].replace(old,new);add(label,parts,'fallback','table')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'w:',b'ft:').replace(b'xmlns:w=',b'xmlns:ft=');add('fonttable-expanded-prefix',p,'positive')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(W.encode(),WS.encode());add('fonttable-strict-namespace',p,'positive')
    for tag in ('altName','charset','embedRegular'):
        child={'altName':'<w:altName w:val="Alternate"/>','charset':'<w:charset w:val="02"/>','embedRegular':'<w:embedRegular xmlns:r="'+REL[:-1]+'" r:id="embedded"/>'}[tag]
        p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'</w:font>',child.encode()+b'</w:font>');add('active-'+tag,p,'fallback','font')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'</w:font>',b'<w:charset w:val="01"/></w:font>');add('active-default-charset-unproved',p,'fallback','font')
    p=dict(base);p['word/_rels/fontTable.xml.rels']=relxml([{'Id':'embedded','Type':REL+'font','Target':'fonts/a.odttf'}]);add('font-program-relationship',p,'fallback','table')
    p=dict(base);p['word/fonts/unrelated.odttf']=b'Not an installed or opened font';add('unrelated-font-program-inert',p,'positive')
    for body,label in [(pxml(runxml('alpha中')),'mixed-cjk'),(pxml(runxml('alphaµ')),'micro-sign'),(pxml(runxml('alphaΩ')),'greek'),(pxml(runxml('alpha\t')),'embedded-tab-scalar')]:add(label,source_parts(body=body),'fallback','body')
    for extra,label in [('<w:cs/>','forcing-cs'),('<w:rtl/>','forcing-rtl'),('<w:rFonts w:ascii="FamilyProbeA" w:hAnsi="FamilyProbeA"/>','duplicate-rfonts')]:add(label,source_parts(body=pxml(runxml(extra=extra))),'fallback','font')
    add('whitespace-text-range',source_parts(body=pxml(runxml(' '))),'positive')
    add('source-latin-lang-dormant-bidi',source_parts(body=pxml(runxml(extra='<w:lang w:val="en-US" w:bidi="ar-SA"/>'))),'positive')
    add('source-active-nonlatin-lang',source_parts(body=pxml(runxml(extra='<w:lang w:val="zh-CN"/>'))),'fallback','font')
    add('explicit-cs-off',source_parts(body=pxml(runxml(extra='<w:cs w:val="0"/><w:rtl w:val="false"/>'))),'positive')
    for label,ordinary,explicit in [
        ('vanish',pxml(runxml('Run property control.')),pxml(runxml('Run property control.',extra='<w:vanish w:val="0"/>'))),
        ('paragraph-tabs',pxml(runxml('Paragraph property control.')),pxml(runxml('Paragraph property control.'),ppr='<w:tabs/>')),
        ('section-direction',pxml(runxml('Section direction control.'))+'<w:sectPr/>',pxml(runxml('Section direction control.'))+'<w:sectPr><w:textDirection w:val="lrTb"/></w:sectPr>')]:
        add('bounded-grammar-'+label+'-omitted',source_parts(body=ordinary),'positive')
        add('bounded-grammar-'+label+'-explicit-neutral',source_parts(body=explicit),'fallback','font')
    add('unequal-latin-slots',source_parts(body=pxml(runxml(props=rfonts('FamilyProbeA','FamilyProbeB'))),records=pair),'fallback','font')
    add('one-missing-latin-slot',source_parts(body=pxml(runxml(props='<w:rFonts w:ascii="FamilyProbeA"/>'))),'fallback','font')
    add('paragraph-mark-not-body-font',source_parts(body=pxml(runxml(props=''),ppr='<w:rPr>'+rfonts('FamilyProbeA')+'</w:rPr>')),'fallback','font')
    for label,props in [('hint-eastasia',rfonts('FamilyProbeA').replace('/>',' w:hint="eastAsia"/>')),('hint-cs',rfonts('FamilyProbeA').replace('/>',' w:hint="cs"/>')),('nested-fonts','<w:wrap>'+rfonts('FamilyProbeA')+'</w:wrap>'),('foreign-fonts','<x:rFonts xmlns:x="urn:other" w:ascii="FamilyProbeA" w:hAnsi="FamilyProbeA"/>')]:add(label,source_parts(body=pxml(runxml(props=props))),'fallback','font')
    p=dict(base);p['word/document.xml']=p['word/document.xml'].replace(b'w:',b'x:').replace(b'xmlns:w=',b'xmlns:x=');add('reader-prefix-alias',p,'fallback','font')
    for label,body in [('tab-run',pxml('<w:r><w:tab/></w:r>')),('linebreak-run',pxml('<w:r><w:br/></w:r>')),('hyperlink-wrapper',pxml('<w:hyperlink>'+runxml()+'</w:hyperlink>')),('symbol-run',pxml('<w:r><w:sym w:font="FamilyProbeA" w:char="0041"/></w:r>')),('empty-run-properties',pxml('<w:r><w:rPr>'+rfonts('FamilyProbeA')+'</w:rPr></w:r>')),('table-context','<w:tbl><w:tr><w:tc>'+pxml()+'</w:tc></w:tr></w:tbl>'),('direct-numbering',pxml(ppr='<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>'))]:add(label,source_parts(body=body),'fallback','body')
    bare='<w:p><w:r><w:br w:type="page"/></w:r></w:p>'
    add('start-end-consecutive-bare-breaks',source_parts(body=bare+bare+pxml()+bare),'positive')
    add('bare-break-only-no-retained-fonts',source_parts(body=bare+bare),'exact')
    for label,body in [('styled-bare-break',pxml('<w:r><w:br w:type="page"/></w:r>',ppr='<w:rPr>'+rfonts('FamilyProbeA')+'</w:rPr>')),('mixed-pagebreak',pxml(runxml()+'<w:r><w:br w:type="page"/></w:r>'))]:add(label,source_parts(body=body),'fallback','body')
    numbered_defaults=xml('styles','<w:docDefaults><w:pPrDefault><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr></w:pPrDefault></w:docDefaults>')
    add('bounded-grammar-numbering-before-section',source_parts(body=pxml()+'<w:sectPr><w:textDirection w:val="lrTb"/></w:sectPr>',styles=numbered_defaults),'fallback','body')
    add('bounded-grammar-numbering-before-run',source_parts(body=pxml(runxml(extra='<w:vanish w:val="0"/>')),styles=numbered_defaults),'fallback','body')
    wrapped_defaults=numbered_defaults.replace(b'<w:docDefaults>',('<w:docDefaults><w:rPrDefault><w:rPr><w:wrap>'+rfonts('FamilyProbeA')+'</w:wrap></w:rPr></w:rPrDefault>').encode())
    add('bounded-grammar-reader-shape-before-numbering',source_parts(styles=wrapped_defaults),'fallback','font')
    defaults='<w:docDefaults><w:rPrDefault><w:rPr>'+rfonts('FamilyProbeA')+'<w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>'
    add('defaults-literal',source_parts(body=pxml(runxml(props='')),styles=xml('styles',defaults)),'positive')
    add('true-empty-baseline',source_parts(body=pxml()+pxml('')+pxml(),styles=xml('styles',defaults)),'positive')
    add('true-empty-mark-override',source_parts(body=pxml()+pxml('',ppr='<w:rPr>'+rfonts('FamilyProbeB')+'<w:sz w:val="26"/></w:rPr>'),records=pair,styles=xml('styles',defaults)),'positive')
    add('unproved-empty-baseline',source_parts(body=pxml()+pxml('')),'fallback','font')
    add('late-empty-family-missing',source_parts(body=pxml()+pxml('',ppr='<w:rPr>'+rfonts('FamilyProbeB')+'<w:sz w:val="26"/></w:rPr>'),styles=xml('styles',defaults)),'fallback','unsupported')
    def style(sid,kind='paragraph',based=None,props='',ppr='',default=False):
        return '<w:style w:type="'+kind+'" w:styleId="'+sid+'"'+(' w:default="1"' if default else '')+'>'+('<w:basedOn w:val="'+based+'"/>' if based else '')+('<w:pPr>'+ppr+'</w:pPr>' if ppr else '')+('<w:rPr>'+props+'</w:rPr>' if props else '')+'</w:style>'
    add('paragraph-style-cascade',source_parts(body=pxml(runxml(props=''),'<w:pStyle w:val="Leaf"/>'),styles=xml('styles',style('Base',props=rfonts('FamilyProbeA'))+style('Leaf',based='Base'))),'positive')
    add('explicit-character-style',source_parts(body=pxml(runxml(props='<w:rStyle w:val="Character"/>')),styles=xml('styles',style('Character','character',props=rfonts('FamilyProbeA')))),'positive')
    add('default-character-not-applied',source_parts(body=pxml(runxml(props='')),styles=xml('styles',style('DefaultChar','character',props=rfonts('FamilyProbeA'),default=True))),'fallback','font')
    add('localized-style-id',source_parts(body=pxml(runxml(props=''),'<w:pStyle w:val="标题一"/>'),styles=xml('styles',style('标题一',props=rfonts('FamilyProbeA')))),'positive')
    for length in (24,64,65):
        sts=''.join(style('S'+str(i),based='S'+str(i-1) if i else None,props=rfonts('FamilyProbeA') if i in (0,length-1) else '') for i in range(length))
        add('style-chain-'+str(length),source_parts(body=pxml(runxml(props=''),'<w:pStyle w:val="S'+str(length-1)+'"/>'),styles=xml('styles',sts)),'fallback' if length==65 else 'positive','limit' if length==65 else None)
    for label,sts,sid in [('cycle',style('A',based='B')+style('B',based='A'),'A'),('missing',style('A',based='Missing'),'A'),('wrong-kind',style('A','character',props=rfonts('FamilyProbeA')),'A'),('duplicate',style('A',props=rfonts('FamilyProbeA'))+style('A',props=rfonts('FamilyProbeA')),'A')]:add('style-'+label,source_parts(body=pxml(runxml(props=''),'<w:pStyle w:val="'+sid+'"/>'),styles=xml('styles',sts)),'fallback','font')
    themed='<w:rFonts w:asciiTheme="minorAscii" w:hAnsiTheme="minorHAnsi"/>'
    def theme_xml(extra='',include_colors=True):
        colors='<a:clrScheme name="Independent">'+''.join('<a:'+slot+'><a:srgbClr val="000000"/></a:'+slot+'>' for slot in ('dk1','lt1','dk2','lt2','accent1','accent2','accent3','accent4','accent5','accent6','hlink','folHlink'))+'</a:clrScheme>' if include_colors else ''
        return xml('theme','<a:themeElements>'+colors+'<a:fontScheme name="Independent"><a:majorFont><a:latin typeface="FamilyProbeB"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="FamilyProbeA"/><a:ea typeface=""/><a:cs typeface=""/>'+extra+'</a:minorFont></a:fontScheme></a:themeElements>',namespace=A,prefix='a')
    tp=source_parts(body=pxml(runxml(props=themed)),records=pair,theme=theme_xml())
    add('theme-latin',tp,'positive')
    add('missing-color-scheme-reader-binding',source_parts(body=pxml(runxml(props=themed)),records=pair,theme=theme_xml(include_colors=False)),'positive')
    p=source_parts(body=pxml(runxml(props=rfonts('FamilyProbeA').replace('/>',' w:asciiTheme="minorAscii" w:hAnsiTheme="minorHAnsi"/>'))),records=pair,theme=theme_xml());add('same-layer-theme-precedence',p,'positive')
    tdefaults='<w:docDefaults><w:rPrDefault><w:rPr>'+themed+'</w:rPr></w:rPrDefault></w:docDefaults>'
    add('direct-literal-replaces-inherited-theme',source_parts(body=pxml(runxml(name='FamilyProbeB')),records=pair,styles=xml('styles',tdefaults),theme=theme_xml()),'positive')
    for language,stage in [('en-US',None),('fr-Latn-FR',None),('zh-CN','font'),('en-Arab-US','font'),('xx-XX','font')]:
        p=dict(tp);p['word/settings.xml']=xml('settings','<w:themeFontLang w:val="'+language+'"/>');p['word/_rels/document.xml.rels']=p['word/_rels/document.xml.rels'].replace(b'</Relationships>',('<Relationship Id="settings" Type="'+REL+'settings" Target="settings.xml"/></Relationships>').encode());p['[Content_Types].xml']=p['[Content_Types].xml'].replace(b'</Types>',('<Override PartName="/word/settings.xml" ContentType="'+MIME['settings']+'"/></Types>').encode());add('theme-language-'+language,p,'fallback' if stage else 'positive',stage)
    p=dict(tp);p['word/theme.xml']=theme_xml('<a:font script="Latn" typeface="FamilyProbeB"/>');add('theme-supplemental-latn',p,'fallback','font')
    p=dict(tp);p.pop('word/theme.xml');add('declared-theme-missing',p,'fallback','package')
    for role in ('fontTable','styles','theme','settings'):
        p=source_parts(styles=xml('styles',defaults),theme=theme_xml(),settings=xml('settings'))
        target='word/'+role+'.xml';alternative='word/alternate/'+role+'-proof.xml';p[alternative]=p.pop(target)
        p['word/_rels/document.xml.rels']=p['word/_rels/document.xml.rels'].replace(('Target="'+role+'.xml"').encode(),('Target="alternate/'+role+'-proof.xml"').encode());p['[Content_Types].xml']=p['[Content_Types].xml'].replace(('/'+target).encode(),('/'+alternative).encode());add('shifted-'+role+'-authority',p,'positive')
    p=dict(base);p['word/_rels/document.xml.rels']=relxml([]);add('absent-relation-orphan-table',p,'exact')
    p=dict(base);p['word/_rels/document.xml.rels']=relxml([]);p['word/fontTable.xml']=b'not XML';add('absent-authority-malformed-orphan-inert',p,'exact')
    p=dict(base);p.pop('word/fontTable.xml');p['word/_rels/document.xml.rels']=relxml([]);add('genuine-absent-table',p,'exact')
    p=dict(base);p.pop('word/fontTable.xml');add('declared-table-missing',p,'fallback','package')
    add('valid-empty-fonttable',source_parts(records=[]),'exact')
    for label,key,old,new in [('relationship-duplicate-id','word/_rels/document.xml.rels',b'</Relationships>',('<Relationship Id="font" Type="'+REL+'styles" Target="fontTable.xml"/></Relationships>').encode()),('relationship-duplicate-font-role','word/_rels/document.xml.rels',b'</Relationships>',('<Relationship Id="font2" Type="'+REL+'fontTable" Target="fontTable.xml"/></Relationships>').encode()),('relationship-unknown-mode','word/_rels/document.xml.rels',b'Target="fontTable.xml"',b'Target="fontTable.xml" TargetMode="internal"'),('relationship-external-font','word/_rels/document.xml.rels',b'Target="fontTable.xml"',b'Target="https://example.invalid/fontTable.xml" TargetMode="External"'),('relationship-lookalike-type','word/_rels/document.xml.rels',(REL+'fontTable').encode(),b'urn:lookalike/fontTable'),('relationship-wrapped','word/_rels/document.xml.rels',b'<Relationship Id=',b'<Wrapper/><Relationship Id='),('relationship-foreign-root','word/_rels/document.xml.rels',R.encode(),b'urn:foreign'),('wrong-font-mime','[Content_Types].xml',MIME['fontTable'].encode(),b'application/xml'),('duplicate-content-type','[Content_Types].xml',b'</Types>',('<Override PartName="/word/fontTable.xml" ContentType="'+MIME['fontTable']+'"/></Types>').encode()),('duplicate-default','[Content_Types].xml',b'</Types>',b'<Default Extension="RELS" ContentType="application/xml"/></Types>'),('main-role-absent','_rels/.rels',(REL+'officeDocument').encode(),b'urn:unrelated'),('main-role-lookalike','_rels/.rels',(REL+'officeDocument').encode(),b'urn:lookalike/officeDocument')]:
        p=dict(base);p[key]=p[key].replace(old,new);add(label,p,'fallback','package')
    for label,target in [('target-query','fontTable.xml?q=1'),('target-fragment','fontTable.xml#x'),('target-percent-slash','fonts%2ffontTable.xml'),('target-percent-dot','%2e/fontTable.xml'),('target-bad-percent','font%ZZTable.xml'),('target-authority','//other/fontTable.xml'),('target-backslash','folder\\fontTable.xml')]:
        p=dict(base);p['word/_rels/document.xml.rels']=p['word/_rels/document.xml.rels'].replace(b'Target="fontTable.xml"',('Target="'+target+'"').encode());add(label,p,'fallback','package')
    p=dict(base);p['word/_rels/document.xml.rels']=p['word/_rels/document.xml.rels'].replace(b'Target="fontTable.xml"',b'Target="sub/../fontTable.xml"');add('relative-dot-target',p,'positive')
    for label,target in [('selected-invalid-percent-utf8','font%FFTable.xml'),('selected-encoded-control','font%01Table.xml'),('physical-percent-only-unreadable','font%54able.xml')]:
        p=dict(base);p['word/'+target]=p.pop('word/fontTable.xml');p['word/_rels/document.xml.rels']=p['word/_rels/document.xml.rels'].replace(b'Target="fontTable.xml"',('Target="'+target+'"').encode());p['[Content_Types].xml']=p['[Content_Types].xml'].replace(b'/word/fontTable.xml',('/word/'+target).encode());add(label,p,'fallback','package')
    for label,name in [('physical-case-alias','word/FontTable.xml'),('physical-percent-alias','word/font%54able.xml'),('physical-exact-duplicate','word/fontTable.xml')]:add(label,base,'fallback','package',extra=[(name,base['word/fontTable.xml'])])
    p=dict(base);p['word/_rels/document.xml.rels']=b'<Relationships xmlns="'+R.encode()+b'"><Wrapper><Relationship Id="font" Type="'+(REL+'fontTable').encode()+b'" Target="fontTable.xml"/></Wrapper></Relationships>';add('wrapped-candidate-not-absence',p,'fallback','package')
    p=dict(base);p['word/_rels/document.xml.rels']=b'<Relationships xmlns="'+R.encode()+b'"><Relationship Id="bad" Type="urn:none" Target="none" TargetMode="bogus"/></Relationships>';add('malformed-tree-not-absence',p,'fallback','package')
    p=dict(base);p['word/fontTable.xml']=b'<!DOCTYPE w:fonts [<!ENTITY x "swiss">]>'+p['word/fontTable.xml'].split(b'?>',1)[1].replace(b'"swiss"',b'"&x;"');add('fonttable-custom-entity',p,'fallback','table')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'</w:fonts>',b' '*(1024**2)+b'</w:fonts>');add('ancillary-byte-limit',p,'fallback','limit')
    add('font-definition-limit',source_parts(records=[('Face'+str(i),'swiss') for i in range(257)]),'fallback','limit')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'</w:fonts>',b'<w:wrap>'*128+b'</w:wrap>'*128+b'</w:fonts>');add('xml-depth-limit',p,'fallback','limit')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'FamilyProbeA',b'Z'*16385);add('attribute-byte-limit',p,'fallback','limit')
    p=dict(base);p['word/fontTable.xml']=p['word/fontTable.xml'].replace(b'<w:font ',('<w:font '+' '.join('a'+str(i)+'="x"' for i in range(65))+' ').encode());add('attribute-count-limit',p,'fallback','limit')
    p=dict(base);p['word/fontTable.xml']=xml('fonts','<w:x/>'*100001);add('aggregate-xml-node-limit',p,'fallback','limit')
    add('font-definition-boundary',source_parts(records=[('FamilyProbeA','swiss')]+[('Inactive'+str(i),'auto') for i in range(255)]),'positive')
    add('inactive-name-attribute-boundary',source_parts(records=[('FamilyProbeA','swiss'),('X'*16384,'auto')]),'positive')
    add('style-count-limit',source_parts(styles=xml('styles',''.join(style('Inactive'+str(i)) for i in range(4094)))),'fallback','limit')
    shared=''.join(style('Shared'+str(i),based='Shared'+str(i-1) if i else None,props=rfonts('FamilyProbeA') if i==0 else '') for i in range(63))
    leaves=''.join(style('Leaf'+str(i),based='Shared62') for i in range(157));body=''.join(pxml(runxml(props=''),'<w:pStyle w:val="Leaf'+str(i)+'"/>') for i in range(157))
    add('aggregate-chain-link-limit',source_parts(body=body,styles=xml('styles',shared+leaves)),'fallback','limit')
    add('source-scalar-limit',source_parts(body=pxml(runxml('X'*1000001))),'fallback','limit')
    add('body-range-limit',source_parts(body=pxml()*5001),'fallback','limit')
    p=dict(base);p['word/document.xml']=p['word/document.xml'].replace(b'</w:document>',b' '*(16*1024**2)+b'</w:document>');add('main-xml-byte-limit',p,'fallback','limit')
    return cases


def prepare(args):
    disk(50*1024**2);verify_ids(PINS);args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='prepared-',dir=args.root));history=json.loads(HISTORY.read_text());require(history['status']=='PASS' and history['cli_sha256']==BASELINE_SHA and len(history['read_commands'])==1520,'Pinned historical full report')
    protected=dict(PINS);historical=[]
    for i,row in enumerate(history['read_commands']):
        path=row['source'];require(sha(path)==row['source_sha256'],'Historical immutable source');protected[path]=row['source_sha256'];old=response(row['command'])
        if row['operation']=='import-docx' and old['returncode']==0:oracle=source_oracle(path,json.loads(old['stdout'])['document'])
        else:oracle={'outcome':'exact','reason_evidence':'Existing hard refusal unchanged' if old['returncode'] else 'Read-only audit/trusted model contract unchanged'}
        historical.append({'historical_index':i,'source':path,'source_sha256':row['source_sha256'],'operation':row['operation'],'baseline_response':old,'outcome':oracle['outcome'],'reason':oracle.get('reason'),'metadata':oracle.get('metadata'),'expected_response':expected_response(old,oracle),'source_evidence':oracle})
    cases=make_fixtures(dest)
    experiment=json.loads((EXPERIMENT/'manifest.json').read_text())
    prior_experiment=json.loads((EXPERIMENT/'fresh-run-zh3uvhsv/report.json').read_text())
    def prior_response(path):
        matches=[r for r in prior_experiment['commands'] if r['argv']==[str(BASELINE),'import-docx',path]]
        require(len(matches)==1,'Unique frozen genuine baseline response');return response(matches[0])
    for x in experiment['positives']:
        path=x['oracle']['source'];require(sha(path)==x['oracle']['sha256'],'Genuine unchanged witness');oracle=source_oracle(path);require(oracle['outcome']=='positive' and oracle['metadata']==x['oracle']['active_metadata'],'Independent genuine source proof')
        baseline=json.loads((EXPERIMENT/'fresh-run-zh3uvhsv'/(x['label']+'-import.json')).read_text());bind_model(baseline['document'],oracle['blocks'])
        cases.append({'name':x['label'],'source':path,'source_sha256':sha(path),'operation':'import-docx','kind':'genuine-unchanged-source','source_evidence':oracle})
    for x in experiment['real_diagnostics']:
        path=str(ROOT/x['path']);require(sha(path)==x['sha256'],'Genuine immutable control');cases.append({'name':Path(path).stem,'source':path,'source_sha256':sha(path),'operation':'import-docx','kind':'genuine-unchanged-control','source_evidence':source_oracle(path)})
    correction_dir=ROOT/'tmp/docx-font-family-design-20261014/integration-fixture-corrections'
    correction_path=correction_dir/'corrections.json'
    if correction_path.exists():
        correction=json.loads(correction_path.read_text());protected[str(correction_path)]=sha(correction_path)
        for name in ('dtd-related.docx','dtd-orphan.docx','rtl-original-related.docx','rtl-original-absent.docx'):
            path=correction_dir/name;require(sha(path)==correction['files'][name],'Exact reconstructed integration source identity')
            cases.append({'name':'integration-original-'+path.stem,'source':str(path),'source_sha256':sha(path),'operation':'import-docx','kind':'independently-authored-integration-diagnostic','provenance':correction['provenance'],'source_evidence':source_oracle(path)})
    for case in cases:
        if case['kind'].startswith('genuine-'):
            old=prior_response(case['source']);oracle=case['source_evidence'] if old['returncode']==0 else {'outcome':'exact','reason_evidence':'Existing exact hard refusal'}
            case.update(baseline_response=old,expected_response=expected_response(old,oracle),outcome=oracle['outcome'],reason=oracle.get('reason'),metadata=oracle.get('metadata'),baseline_reused_from=str(EXPERIMENT/'fresh-run-zh3uvhsv/report.json'))
    protected.update({r['source']:r['source_sha256'] for r in cases})
    counts={'historical_total':len(historical),'historical_outcomes':dict(collections.Counter(r['outcome'] for r in historical)), 'historical_fallback_reasons':dict(collections.Counter(r['reason'] for r in historical if r['outcome']=='fallback')),'historical_hard_refusals':sum(r['baseline_response']['returncode']!=0 for r in historical),'source_cases':len(cases),'synthetic_sources':sum(r['kind']=='independently-authored-opc-xml' for r in cases)}
    manifest={'status':'PREPARED_SOURCE_ONLY','version':1,'created_at':now(),'checker':str(Path(__file__).resolve()),'checker_sha256':sha(__file__),'baseline_cli':str(BASELINE),'baseline_cli_sha256':BASELINE_SHA,'history_report':str(HISTORY),'history_report_sha256':HISTORY_SHA,'protected':protected,'counts':counts,'historical_rows':historical,'source_rows':cases,'stages':{'source_oracles_completed':True,'fixture_authoring_completed':True,'historical_classification_completed':True,'fresh_baseline_binding_completed':False,'fresh_candidate_checks_completed':False,'fresh_consumer_checks_completed':False},'resource_policy':{'minimum_free_disk_bytes':FLOOR,'per_command_timeout_seconds':120,'maximum_captured_stream_bytes':16*1024**2,'serial_product_execution':True},'old_render_failures_retained':str(EXPERIMENT/'fresh-run-zh3uvhsv/report.json'),'execution_permission':'No product/build/render until parent grants serialized slot; source expectations authored before execution'}
    save(dest/'genuine-frontend-sources.json',{'sources':[{'name':r['name'],'path':r['source'],'sha256':r['source_sha256'],'metadata':r['metadata'],'baseline_response':r['baseline_response'],'expected_response':r['expected_response']} for r in cases if r['kind']=='genuine-unchanged-source'],'baseline_read_method':'Reused exact raw commands from SHA-pinned prior consumer report; no fresh baseline commands during source-only preparation'})
    save(dest/'manifest.json',manifest);print(json.dumps({'manifest':str(dest/'manifest.json'),'sha256':sha(dest/'manifest.json'),'counts':counts,'stages':manifest['stages']},indent=2))

class Runner:
    """Serial execution with bounded wall time/output and retained full responses."""
    def __init__(self,dest,report): self.dest=dest;self.report=report;self.n=0
    def __call__(self,argv,expected=None,env=None):
        disk();self.n+=1;start=time.monotonic();minimum=shutil.disk_usage(ROOT).free
        stdout=self.dest/f'command-{self.n:05d}.stdout';stderr=self.dest/f'command-{self.n:05d}.stderr';command=list(map(str,argv));reason=None
        with stdout.open('xb') as out,stderr.open('xb') as err:
            proc=subprocess.Popen(command,cwd=ROOT,stdout=out,stderr=err,env=env,start_new_session=True)
            try:
                while proc.poll() is None:
                    time.sleep(.05);minimum=min(minimum,shutil.disk_usage(ROOT).free)
                    if minimum<FLOOR:reason='Observed free disk below 600 MiB'
                    if time.monotonic()-start>120:reason='Command timeout exceeded 120 seconds'
                    if max(stdout.stat().st_size,stderr.stat().st_size)>16*1024**2:reason='Captured stream exceeds 16 MiB'
                    if reason:raise RuntimeError(reason)
            except BaseException:
                if proc.poll() is None:
                    os.killpg(proc.pid,signal.SIGTERM)
                    try:proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
                raise
            finally:
                out.flush();err.flush()
                row={'argv':command,'returncode':proc.returncode,'stdout':stdout.read_text(),'stderr':stderr.read_text(),'stdout_path':str(stdout),'stderr_path':str(stderr),'seconds':time.monotonic()-start,'minimum_observed_free_disk_bytes':minimum,'termination_reason':reason}
                self.report['commands'].append(row)
                with (self.dest/'progress.jsonl').open('a') as f:f.write(json.dumps({'at':now(),'kind':'command','value':row},ensure_ascii=False)+'\n');f.flush();os.fsync(f.fileno())
        if expected is not None:require((row['returncode']==0)==expected,'Unexpected command status: '+str(command))
        return row


def package_parts(path):
    with zipfile.ZipFile(path) as z:
        require(len(z.namelist())==len(set(z.namelist())) and z.testzip() is None,'Unique package entries and CRC');return {n:z.read(n) for n in z.namelist()}

def package_assert(current,old,model):
    new,before=package_parts(current),package_parts(old);metadata=model.get('attrs',{}).get(KEY)
    if metadata is None:require(Path(current).read_bytes()==Path(old).read_bytes(),'Fallback/absence complete output package exact');return {'whole_package_exact':True}
    mps=[n for n in new if n.startswith('rust-wps/model-v') and n.endswith('.json')];require(len(mps)==1,'Single embedded integrity model');mp=mps[0]
    require(set(new)==set(before)|{'word/fontTable.xml'},'Only font table added')
    permitted={mp,'[Content_Types].xml','word/_rels/document.xml.rels'}
    require(all(new[n]==b for n,b in before.items() if n not in permitted),'Every old rendered payload exactly retained')
    root=ET.fromstring(new['word/fontTable.xml']);require(root.tag==q('fonts') and not root.attrib,'Export font root');records=[]
    for f in root:
        require(f.tag==q('font') and set(f.attrib)=={q('name')} and len(f)==1,'Export exact font path');family=f[0];require(family.tag==q('family') and set(family.attrib)=={q('val')} and not len(family),'Export only family metadata');records.append({'name':val(f,'name'),'family':val(family)})
    require(records==sorted(metadata,key=lambda r:r['name']),'Export independent family records exactly')
    r=ET.fromstring(new['word/_rels/document.xml.rels']);r0=ET.fromstring(before['word/_rels/document.xml.rels']);f=[e for e in r if e.get('Type')==REL+'fontTable'];require(len(f)==1 and f[0].get('Target')=='fontTable.xml' and f[0].get('TargetMode') is None,'Unique internal exact font relationship');require(len({e.get('Id') for e in r})==len(r),'Unique relationship IDs');require([e.attrib for e in r if e not in f]==[e.attrib for e in r0],'All earlier relationship entries exact')
    t=ET.fromstring(new['[Content_Types].xml']);t0=ET.fromstring(before['[Content_Types].xml']);f=[e for e in t if e.get('PartName')=='/word/fontTable.xml'];require(len(f)==1 and f[0].get('ContentType')==MIME['fontTable'],'Font MIME exact');require([e.attrib for e in t if e not in f]==[e.attrib for e in t0],'All earlier content types exact')
    embedded=json.loads(new[mp]);require(embedded['document']==model,'Full embedded model');require(set(embedded['hashes'])==set(new)-{mp},'Complete integrity part set');require(all(embedded['hashes'][n]==digest(raw) for n,raw in new.items() if n!=mp),'Exact embedded hashes for every part')
    return {'exact_font_records':records,'all_previous_rendered_payloads_exact':True,'integrity_and_wiring_exact':True}


def trusted_tamper_checks(path,model,dest,run,cli):
    original=package_parts(path);mp=next(n for n in original if n.startswith('rust-wps/model-v') and n.endswith('.json'));variants=[]
    for label,change,sync in [('stale-table',lambda p:p.__setitem__('word/fontTable.xml',p['word/fontTable.xml'].replace(b'"swiss"',b'"roman"',1)),False),('synchronized-table',lambda p:p.__setitem__('word/fontTable.xml',p['word/fontTable.xml'].replace(b'"swiss"',b'"roman"',1)),True),('synchronized-font-name',lambda p:p.__setitem__('word/fontTable.xml',p['word/fontTable.xml'].replace(b'w:name="',b'w:name="Tampered ',1)),True),('synchronized-family-omission',lambda p:p.__setitem__('word/fontTable.xml',re.sub(br'<w:family\b[^>]*/>',b'',p['word/fontTable.xml'],count=1)),True)]:
        parts=dict(original);change(parts)
        if parts==original:continue
        if sync:
            embedded=json.loads(parts[mp]);embedded['hashes']={n:digest(b) for n,b in parts.items() if n!=mp};parts[mp]=json.dumps(embedded,ensure_ascii=False).encode()
        target=dest/(path.stem+'-'+label+'.docx');write_zip(target,parts);answer=run([cli,'read-docx-model',target],False);variants.append({'name':label,'path':str(target),'sha256':sha(target),'response':response(answer)})
    for label in ('removed-model-family','changed-model-family'):
        parts=dict(original);embedded=json.loads(parts[mp])
        if label=='removed-model-family':embedded['document']['attrs'].pop(KEY)
        else:embedded['document']['attrs'][KEY][0]['family']='roman' if embedded['document']['attrs'][KEY][0]['family']=='swiss' else 'swiss'
        parts[mp]=json.dumps(embedded,ensure_ascii=False).encode();target=dest/(path.stem+'-'+label+'.docx');write_zip(target,parts);answer=run([cli,'read-docx-model',target],False);variants.append({'name':label,'path':str(target),'sha256':sha(target),'response':response(answer)})
    embedded=json.loads(original[mp]);embedded['document']['attrs'][KEY][0]['family']='decorative';parts=dict(original);parts[mp]=json.dumps(embedded,ensure_ascii=False).encode();target=dest/(path.stem+'-invalid-model-family.docx');write_zip(target,parts);answer=run([cli,'read-docx-model',target],False);variants.append({'name':'invalid-model-family','path':str(target),'sha256':sha(target),'response':response(answer)})
    return variants


def verify(args):
    require(args.slot=='parent-granted','Serialized parent slot required');disk(100*1024**2);require(sha(args.manifest)==args.manifest_sha256,'Pinned prepared manifest');manifest=json.loads(args.manifest.read_text());verify_ids(manifest['protected']);require(sha(args.cli)==args.sha256,'Pinned final CLI');require(sha(__file__)==manifest['checker_sha256'],'Checker changed after preparation')
    dest=Path(tempfile.mkdtemp(prefix='verified-',dir=args.root));report={'status':'RUNNING','started_at':now(),'manifest':str(args.manifest),'manifest_sha256':args.manifest_sha256,'cli':str(args.cli),'cli_sha256':args.sha256,'baseline_cli':str(BASELINE),'baseline_cli_sha256':BASELINE_SHA,'commands':[],'source_rows':[],'stages':{'baseline_bindings_completed':False,'candidate_source_checks_completed':False,'package_and_trusted_integrity_completed':False,'source_immutability_completed':False},'consumer_execution':False,'historical_execution':'Owned by independent full regression harness using prepared historical_rows','source_expectations':'Source proof fixed before candidate; prior baseline binds only exact source ranges, no product resolver imports'};run=Runner(dest,report)
    try:
        # Resolve baseline old refusals and complete range binding before any candidate.
        bound=[]
        for case in manifest['source_rows']:
            old=run([BASELINE,'import-docx',case['source']]);oracle=copy.deepcopy(case['source_evidence'])
            if old['returncode']!=0:oracle={'outcome':'exact','reason_evidence':'Full old hard refusal remains exact','independent_source_evidence':oracle}
            elif oracle['outcome']=='positive':
                try:bind_model(json.loads(old['stdout'])['document'],oracle['blocks'])
                except Unavailable as e:oracle=dict(oracle,outcome='fallback',stage=e.stage,reason=REASONS[e.stage],reason_evidence=e.evidence)
            row=dict(case,baseline_response=response(old),outcome=oracle['outcome'],reason=oracle.get('reason'),metadata=oracle.get('metadata') if oracle['outcome']=='positive' else None,source_evidence=oracle,expected_response=expected_response(response(old),oracle));bound.append(row)
        save(dest/'baseline-bound-source-manifest.json',{'status':'BASELINE_BOUND_BEFORE_CANDIDATE','prepared_manifest':str(args.manifest),'prepared_manifest_sha256':args.manifest_sha256,'source_rows':bound})
        report['stages']['baseline_bindings_completed']=True
        frontend={'sources':[{'name':r['name'],'path':r['source'],'sha256':r['source_sha256'],'metadata':r['metadata'],'baseline_response':r['baseline_response'],'expected_response':r['expected_response']} for r in bound if r['kind']=='genuine-unchanged-source']};save(dest/'genuine-frontend-sources.json',frontend)
        for i,row in enumerate(bound):
            actual=run([args.cli,'import-docx',row['source']]);observed=response(actual);require(observed==row['expected_response'],'Complete source response differs from independent exact expectation: '+row['name']);record=dict(row,observed_response=observed,expected_full_response_exact=True)
            if not actual['returncode']:
                model=json.loads(actual['stdout'])['document'];oldmodel=json.loads(row['baseline_response']['stdout'])['document'];modelpath=save(dest/f'{i:03d}-candidate-model.json',model);oldpath=save(dest/f'{i:03d}-baseline-model.json',oldmodel);output=dest/f'{i:03d}-candidate.docx';baseline=dest/f'{i:03d}-baseline.docx';run([BASELINE,'create-docx',oldpath,baseline],True);run([args.cli,'create-docx',modelpath,output],True)
                require(json.loads(run([args.cli,'read-docx-model',output],True)['stdout'])==model,'Complete trusted reopen');record['package']=package_assert(output,baseline,model)
                for target in (Path(row['source']),output):
                    before=sha(target);blocked=run([args.cli,'create-docx',modelpath,target],False);require('Output already exists' in blocked['stderr'] and sha(target)==before,'Source/output noclobber')
                record.update(output=str(output),output_sha256=sha(output),baseline_output=str(baseline),baseline_output_sha256=sha(baseline),model=str(modelpath),complete_trusted_reopen=True,no_overwrite_checks=2)
                if row['kind']=='genuine-unchanged-source' or row['name']=='direct-swiss':
                    generic_old=run([BASELINE,'import-docx',output]);generic_oracle=source_oracle(output,json.loads(generic_old['stdout'])['document']) if not generic_old['returncode'] else {'outcome':'exact'}
                    generic_new=run([args.cli,'import-docx',output]);require(response(generic_new)==expected_response(response(generic_old),generic_oracle),'Generic product reimport requires independent source proof');record['generic_reimport']={'oracle':generic_oracle,'baseline':response(generic_old),'candidate':response(generic_new)}
                    orphan=package_parts(output);orphan.pop('word/fontTable.xml');orphan['word/_rels/document.xml.rels']=re.sub(br'<Relationship\b[^>]*Type="[^"]*/fontTable"[^>]*/>',b'',orphan['word/_rels/document.xml.rels']);orphan['[Content_Types].xml']=re.sub(br'<Override\b[^>]*PartName="/word/fontTable.xml"[^>]*/>',b'',orphan['[Content_Types].xml']);orphanpath=dest/f'{i:03d}-embedded-model-is-not-source-authority.docx';write_zip(orphanpath,orphan)
                    o=source_oracle(orphanpath);require(o['outcome']=='exact','Orphan embedded model never supplies authority');old=run([BASELINE,'import-docx',orphanpath]);fresh=run([args.cli,'import-docx',orphanpath]);require(response(old)==response(fresh),'Embedded author root without relationship is exact source absence');require(not old['returncode'] and KEY not in json.loads(fresh['stdout'])['document'].get('attrs',{}),'No metadata inferred from embedded model');run([args.cli,'read-docx-model',orphanpath],False);record['embedded_authority_ignored']=True
                if row['kind']=='genuine-unchanged-source':record['tamper_checks']=trusted_tamper_checks(output,model,dest,run,args.cli)
            report['source_rows'].append(record)
        report['counts']={'source_cases':len(bound),'outcomes':dict(collections.Counter(x['outcome'] for x in bound)),'old_hard_refusals':sum(x['baseline_response']['returncode']!=0 for x in bound),'successful_imports':sum(x['baseline_response']['returncode']==0 for x in bound),'complete_trusted_reopens':sum(x.get('complete_trusted_reopen',False) for x in report['source_rows']),'no_overwrite_checks':sum(x.get('no_overwrite_checks',0) for x in report['source_rows'])}
        report['stages']['candidate_source_checks_completed']=True;report['stages']['package_and_trusted_integrity_completed']=True;report['status']='PASS'
    except BaseException as error:report.update(status='FAIL',error=repr(error),traceback=traceback.format_exc());raise
    finally:
        try:verify_ids(manifest['protected']);require(sha(args.cli)==args.sha256,'Candidate changed during verification');report['stages']['source_immutability_completed']=True
        except BaseException as e:report.update(status='FAIL',identity_error=repr(e))
        report.update(finished_at=now(),free_disk_bytes=shutil.disk_usage(ROOT).free);save(dest/'report.json',report);print('RESULT',report['status'],dest/'report.json',sha(dest/'report.json'),flush=True)


def main():
    ap=argparse.ArgumentParser(description=__doc__);sp=ap.add_subparsers(dest='action',required=True)
    prep=sp.add_parser('prepare');prep.add_argument('--root',type=Path,default=DEFAULT)
    check=sp.add_parser('verify');check.add_argument('--root',type=Path,default=DEFAULT);check.add_argument('--manifest',type=Path,required=True);check.add_argument('--manifest-sha256',required=True);check.add_argument('--cli',type=Path,required=True);check.add_argument('--sha256',required=True);check.add_argument('--slot',choices=['parent-granted'],required=True)
    args=ap.parse_args();args.root.mkdir(parents=True,exist_ok=True)
    attempt=Path(tempfile.mkdtemp(prefix='attempt-'+args.action+'-',dir=args.root))
    record={'status':'RUNNING','operation':args.action,'argv':sys.argv,'started_at':now(),'checker':str(Path(__file__).resolve()),'checker_sha256':sha(__file__),'counted_as_acceptance_report':False,'files_removed':0,'operation_completed':False}
    try:
        if args.action=='prepare':prepare(args)
        else:verify(args)
        record.update(status='COMPLETED',operation_completed=True)
    except BaseException as error:
        record.update(status='FAILED_OR_INTERRUPTED',error=repr(error),traceback=traceback.format_exc());raise
    finally:
        record.update(finished_at=now(),free_disk_bytes=shutil.disk_usage(ROOT).free);save(attempt/'report.json',record)
        print('ATTEMPT',record['status'],attempt/'report.json',sha(attempt/'report.json'),flush=True)
if __name__=='__main__':main()
