"""Independent raw-FFN font-family proof and genuine product-output checks.

Preparation never invokes a product binary or consumer. Execution requires an
explicit serialized slot and a SHA-pinned final CLI. Expected metadata is decoded
from immutable binary source and the frozen pre-feature model; candidate source
code is never imported. Every attempt retains a new evidence directory.
"""
import argparse, copy, hashlib, importlib.util, json, os, shutil, signal, struct, subprocess, sys, tempfile, time, traceback, zipfile
from pathlib import Path
from xml.etree import ElementTree as ET
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parent.parent
DEFAULT=ROOT/'tmp/doc-font-family-verify'
BASELINE=ROOT/'tmp/legacy-doc-break-paragraphs-final-cli'
BASELINE_SHA='a3cb41b089460be59712c7fa70858a8a4ed5afd8e32f796cc44ddd06324ffe83'
HISTORY=ROOT/'tmp/doc-break-semantics-verify/break-run-bdz2jxlj/report.json'
HISTORY_SHA='65e62f6b9fee9361282d58b536108f5009e5cdc4a09a7565209ed273364a9165'
EXPERIMENT=ROOT/'tmp/doc-font-family-experiment'
PINS={str(HISTORY):HISTORY_SHA,str(BASELINE):BASELINE_SHA,
 str(EXPERIMENT/'manifest.json'):'dfaaf99c96c176c1144f55a03e030cfeb35f3fcb97eba715f343891ea687311a',
 str(EXPERIMENT/'fresh-run/report.json'):'191d5fd75fd37016f778f628c87433758500c9a4507345b230f733575d6bf8f1',
 str(EXPERIMENT/'analysis.json'):'f2f1134f8d2ce2e04fc43cf249ee90c0e7595be247f093bd4b3488df30f61237',
 str(ROOT/'scripts/check-legacy-doc-break-paragraphs.py'):'e78dc945916d7dbd32b5ed79adc07514214ee7a6c5736920746b58d745e217b8'}
KEY='fontFamilyMetadata';W='http://schemas.openxmlformats.org/wordprocessingml/2006/main';R='http://schemas.openxmlformats.org/package/2006/relationships';CT='http://schemas.openxmlformats.org/package/2006/content-types'
FONT='word/fontTable.xml';RELS='word/_rels/document.xml.rels';TYPES='[Content_Types].xml';FREL='http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable';FCT='application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml'
SUCCESS='Legacy DOC source Roman/Swiss font family metadata is preserved for {count} emitted literal font name(s); font availability, browser printing and other consumers may differ'
FALLBACK='Legacy DOC source font family metadata is not preserved for any font: '
OTHER='an emitted Latin font has a valid family outside the supported Roman/Swiss subset'
SLOT='effective Latin slots or retained break characters disagree on font family'
ALIAS='emitted font names contain distinct ASCII-case aliases'
CONFLICT='one emitted literal font name has conflicting source families'
FLOOR=600*1024**2

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def require(v,msg):
 if not v:raise AssertionError(msg)
def save(path,data):
 raw=data if isinstance(data,bytes)else(json.dumps(data,indent=2,ensure_ascii=False)+'\n').encode()
 with Path(path).open('xb')as f:f.write(raw)
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
for path,pin in PINS.items():require(sha(path)==pin,'Frozen evidence identity: '+path)
b=module('independent_family_breaks',ROOT/'scripts/check-legacy-doc-break-paragraphs.py');c=b.c

def ascii_fold(value):return value.translate(str.maketrans('ABCDEFGHIJKLMNOPQRSTUVWXYZ','abcdefghijklmnopqrstuvwxyz'))

def source_oracle(path):
 """Decode independent CFB/FFN/STSH/CHPX bytes, retaining both Latin slots."""
 path=Path(path);ss=c.streams(path);wd=ss['WordDocument'];table_name='1Table'if c.u16(wd,10)&512 else'0Table';tab=ss[table_name]
 def field(at):
  off,n=struct.unpack_from('<II',wd,at);require(off+n<=len(tab),'Independent source table bounds');return tab[off:off+n]
 f=field(0x112);require(len(f)>=4 and c.u16(f,2)==0,'FFN header');fonts=[];p=4
 for idx in range(c.u16(f,0)):
  length=f[p];d=f[p+1:p+1+length];require(len(d)==length and length>=41,'FFN record bounds')
  fonts.append({'index':idx,'ffid':d[0],'family':(d[0]>>4)&7,'name':d[39:].decode('utf-16le').split('\0')[0],'weight':c.u16(d,1),'charset':d[3],'raw_hex':d.hex()});p+=length+1
 require(p==len(f),'FFN exact tail')
 st=field(0xa2);base=c.u16(st,4);p=2+c.u16(st,0);size=c.u16(st,p);d=st[p+2:p+2+size];q=base;chars=c.u16(d,q);q+=2+chars*2+2;q+=q%2;upx=[]
 for _ in range(c.u16(d,4)&15):
  n=c.u16(d,q);q+=2;upx.append(d[q:q+n]);q+=n;q+=q%2
 initial={'font0':c.u16(st,14),'font2':c.u16(st,18),'halfPoints':20}
 def overlay(current,props):
  v=dict(current)
  for op,arg in c.sprms(props):
   if op==0x4a4f:v['font0']=c.u16(arg,0)
   elif op==0x4a51:v['font2']=c.u16(arg,0)
   elif op==0x4a43:v['halfPoints']=c.u16(arg,0)
  return v
 require(len(upx)>1,'Normal character UPX');initial=overlay(initial,upx[1]);clx=field(0x1a2)
 require(clx[0]==2 and c.u32(clx,1)==16,'Independent single piece source');pc=clx[5:];n=c.u32(pc,4);fcr=c.u32(pc,10);fc=(fcr&0x3fffffff)//2;require(fcr&0x40000000 and c.u16(pc,14)==0,'Compressed source without PRM');text=wd[fc:fc+n].decode('cp1252')
 ch=field(0xfa);nr=(len(ch)-4)//8;runs=[]
 for i in range(nr):
  pn=c.u32(ch,4*(nr+1)+4*i)&0x3fffff;page=wd[pn*512:(pn+1)*512];k=page[511]
  for j in range(k):
   start,end=c.u32(page,j*4)-fc,c.u32(page,(j+1)*4)-fc;q=page[4*(k+1)+j]*2;props=page[q+1:q+1+page[q]]if q else b'';effective=overlay(initial,props)
   slots=[fonts[effective['font0']],fonts[effective['font2']]]
   runs.append({'start':start,'end':end,'text':text[start:end],'slots':slots,'halfPoints':effective['halfPoints'],'sprms':[[hex(op),arg.hex()]for op,arg in c.sprms(props)]})
 return {'source_sha256':sha(path),'table_stream':table_name,'font_table_sha256':hashlib.sha256(f).hexdigest(),'fonts':fonts,'text':text,'runs':runs}

def family_oracle(path,model):
 raw=source_oracle(path);runs=raw['runs'];text=raw['text'];groups=[];positions=[];start=0;blocks=model['content'];pi=0
 for end,char in enumerate(text):
  if char!='\r':continue
  fragment=text[start:end];node=blocks[pi];pi+=1
  if fragment=='\f':
   require(node['type']=='pageBreak','Source/model break identity')
   if node.get('attrs',{}).get('breakParagraph'):
    positions.append({'kind':'retained-break-and-mark','cp':[start,end+1]});fonts=[]
    for cp in (start,end):
     active=[r for r in runs if r['start']<=cp<r['end']];require(len(active)==1,'Break CP covered exactly');fonts.extend(active[0]['slots'])
    groups.append(fonts)
  else:
   emitted=''.join(n.get('text','')for n in node.get('content',[]));require(emitted==fragment,'Exact source/model emitted text identity')
   for r in runs:
    a,z=max(start,r['start']),min(end,r['end'])
    if a<z:positions.append({'kind':'emitted-text','cp':[a,z],'text':text[a:z]});groups.append(r['slots'])
  start=end+1
 require(start==len(text)and pi==len(blocks),'Complete source/model block binding')
 values={};reasons=set()
 for fonts in groups:
  families={f['family']for f in fonts}
  if not families<={1,2}:reasons.add(OTHER);continue
  if len(families)!=1:reasons.add(SLOT);continue
  for f in fonts:
   name,family=f['name'],f['family'];aliases=[old for old in values if ascii_fold(old)==ascii_fold(name)and old!=name]
   if aliases:reasons.add(ALIAS)
   if name in values and values[name]!=family:reasons.add(CONFLICT)
   values[name]=family
 metadata=[]if reasons else[{'name':name,'family':{1:'roman',2:'swiss'}[value]}for name,value in sorted(values.items())]
 raw.update(emitted_positions=positions,emitted_font_groups=groups,font_family_metadata=metadata,fallback_reasons=sorted(reasons))
 return raw

def mutate_families(parts,families):
 w,t,p=parts;at=c.u32(w,0x112)+4
 for family in families:
  t[at+1]=(t[at+1]&0x8f)|(family<<4);at+=t[at]+1
 return w,t,p

def prepare(args):
 require(shutil.disk_usage(ROOT).free>=FLOOR,'600 MiB floor before prepare');args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='family-fixtures-',dir=args.root));history=json.loads(HISTORY.read_text());cases=[]
 for row in history['cases']:
  case={k:row[k]for k in ('name','path','sha256','kind')};case.update(historical=True,baseline_returncode=row['returncode'],outcome='refusal'if row['returncode']else'unchanged')
  if row['returncode']==0:
   old=json.loads((HISTORY.parent/(row['name']+'.import.json')).read_text());case['baseline_import']=str(HISTORY.parent/(row['name']+'.import.json'));case['baseline_docx']=row['docx'];case['baseline_docx_sha256']=row['docx_sha256']
   if any(r.startswith('Legacy DOC literal Latin font names and effective sizes are preserved')for r in old['compatibility']['risks']):
    oracle=family_oracle(case['path'],old['document']);case['oracle']=oracle;case['outcome']='fallback'if oracle['fallback_reasons']else'preserved'if oracle['font_family_metadata']else'unchanged'
  cases.append(case)
 def add(name,parts,outcome='preserved'):
  w,t,p=parts;path=dest/(name+'.doc');save(path,c.compound(w,t));case={'name':name,'path':str(path),'sha256':sha(path),'kind':'independently-authored-binary-family-control','historical':False,'outcome':outcome}
  if outcome!='refusal':case['raw_source']=source_oracle(path)
  cases.append(case)
 text='First font family text.\r\f\rSecond font family text.';cp=text.index('\f');total=len(text)+1
 def base(fonts=('FamilyProbeA','FamilyProbeB'),families=(1,2),runs=None,flag=True,body=text):return mutate_families(b.authored(text=body,fonts=fonts,runs=runs,flag=flag),families)
 def idx(i):return c.spr(0x4a4f,i)+c.spr(0x4a51,i)
 add('family-active-roman',base(fonts=('FamilyProbeA',),families=(1,)))
 add('family-active-swiss',base(fonts=('FamilyProbeA',),families=(2,)))
 add('family-two-active',base(runs=[(10,b''),(total,idx(1))]))
 add('family-break-only-swiss',base(runs=[(cp,b''),(cp+2,idx(1)),(total,b'')]))
 add('family-break-only-document',base(fonts=('FamilyProbeA',),families=(2,),body='\f'))
 add('family-no-emitted-fonts',base(fonts=('FamilyProbeA',),families=(2,),body='\f',flag=False),'unchanged')
 for fam in (0,3,4,5):
  add('family-active-unsupported-'+str(fam),base(fonts=('FamilyProbeA',),families=(fam,)),'fallback')
  add('family-unused-unsupported-'+str(fam),base(families=(1,fam)))
  add('family-late-unsupported-'+str(fam),base(families=(1,fam),runs=[(10,b''),(total,idx(1))]),'fallback')
  add('family-unretained-break-unsupported-'+str(fam),base(families=(1,fam),runs=[(cp,b''),(cp+2,idx(1)),(total,b'')],flag=False))
 for fam in (6,7):
  add('family-invalid-active-'+str(fam),base(fonts=('FamilyProbeA',),families=(fam,)),'refusal')
  add('family-invalid-unused-'+str(fam),base(families=(1,fam)),'refusal')
 for label,names,fams,outcome in [('duplicate',('FamilyProbeA','FamilyProbeA'),(1,1),'preserved'),('conflict',('FamilyProbeA','FamilyProbeA'),(1,2),'fallback'),('alias',('FamilyProbeA','familyprobea'),(1,1),'fallback')]:
  add('family-'+label+'-separate-runs',base(fonts=names,families=fams,runs=[(10,b''),(total,idx(1))]),outcome)
  add('family-'+label+'-same-slots',base(fonts=names,families=fams,runs=[(total,c.spr(0x4a51,1))]),'unchanged'if label=='alias'else outcome)
  add('family-unused-'+label,base(fonts=names,families=fams))
  add('family-'+label+'-break-and-mark',base(fonts=names,families=fams,runs=[(cp+1,b''),(cp+2,idx(1)),(total,b'')]),'preserved'if label=='alias'else outcome)
 for label,indices in [('forward',[0,1,2]),('reverse',[2,1,0])]:
  add('family-combined-alias-conflict-'+label,base(fonts=('FamilyProbeA','familyprobea','familyprobea'),families=(1,2,1),runs=[(7,idx(indices[0])),(16,idx(indices[1])),(total,idx(indices[2]))]),'fallback')
 add('family-combined-four-fallback-reasons',base(fonts=('FamilyProbeA','FamilyProbeA','familyprobea','familyprobea','OtherUnsupported'),families=(1,2,2,1,3),runs=[(5,idx(0)),(10,idx(2)),(15,idx(3)),(20,idx(4)),(total,c.spr(0x4a4f,0)+c.spr(0x4a51,1))]),'fallback')
 add('family-combined-unsupported-slot-alias-conflict',base(fonts=('FamilyProbeA','familyprobea','familyprobea','FamilyOther'),families=(1,2,1,3),runs=[(5,idx(0)),(10,idx(1)),(15,idx(2)),(20,idx(3)),(total,c.spr(0x4a4f,0)+c.spr(0x4a51,1))]),'unchanged')
 add('family-blank-paragraph-unused',base(families=(1,3),body='One.\r\rTwo.',runs=[(5,b''),(6,idx(1)),(len('One.\r\rTwo.')+1,b'')]))
 add('family-trailing-mark-unused',base(families=(1,4),body='One.',runs=[(4,b''),(5,idx(1))]))
 for control in json.loads((EXPERIMENT/'manifest.json').read_text())['synthetic_controls']:
  cases.append({'name':control['label'],'path':control['path'],'sha256':control['sha256'],'kind':'reused-independent-family-causal-source','historical':False,'outcome':'preserved','raw_source':source_oracle(control['path'])})
 for case in cases:
  if case['historical']or case['outcome']in ('refusal','unchanged'):continue
  raw=case['raw_source'];model,breaks=preparation_model(raw);ss=c.streams(Path(case['path']));w=ss['WordDocument'];tab=ss['1Table'if c.u16(w,10)&512 else'0Table'];off,length=struct.unpack_from('<II',w,0x192)
  flag=length>=674 and bool(c.u32(tab,off+512)&(1<<27))
  if flag and breaks and all(ok for _,ok in breaks):
   for node,_ in breaks:node['attrs']={'breakParagraph':{'source_binding_skeleton':True}}
  case['oracle']=family_oracle(case['path'],model)
  observed='fallback'if case['oracle']['fallback_reasons']else'preserved'if case['oracle']['font_family_metadata']else'unchanged'
  require(observed==case['outcome'],'Independent recipe declared outcome: '+case['name'])
 protected=dict(PINS);protected.update({x['path']:x['sha256']for x in cases});protected.update({str(ROOT/'scripts'/name):pin for name,pin in b.HELPERS.items()})
 manifest={'checker_sha256':sha(__file__),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_report':str(HISTORY),'historical_report_sha256':HISTORY_SHA,'protected':protected,'cases':cases,'counts':{'historical_sources':365,'newly_authored_sources':sum(x['kind']=='independently-authored-binary-family-control'for x in cases),'reused_causal_sources':2}}
 save(dest/'manifest.json',manifest);print(dest/'manifest.json')

def without_metadata(model):
 result=copy.deepcopy(model);result.get('attrs',{}).pop(KEY,None)
 if result.get('attrs')=={}:result.pop('attrs')
 return result

def parts(path):
 with zipfile.ZipFile(path)as z:
  require(z.testzip()is None and len(z.namelist())==len(set(z.namelist())),'Unique ZIP entries/CRC');return {n:z.read(n)for n in z.namelist()}

def xml_assert(output,baseline,model):
 now,old=parts(output),parts(baseline);metadata=model.get('attrs',{}).get(KEY,[]);mp='rust-wps/model-v2.json'if model.get('attrs',{}).get('styleProfile')=='standard-v2'else'rust-wps/model-v1.json'
 if not metadata:require(Path(output).read_bytes()==Path(baseline).read_bytes(),'Absent metadata old package bytes');return {'absent_whole_package_exact':True}
 require(set(now)==set(old)|{FONT},'Only font table added')
 require(all(now[n]==old[n]for n in old if n not in (mp,RELS,TYPES)),'All prior nonmodel/non-OPC bytes unchanged')
 root=ET.fromstring(now[FONT]);require(root.tag=='{'+W+'}fonts'and not root.attrib,'Font table root');observed=[]
 for f in root:
  require(f.tag=='{'+W+'}font'and set(f.attrib)=={'{'+W+'}name'}and len(f)==1,'Exact font record fields');ch=f[0];require(ch.tag=='{'+W+'}family'and set(ch.attrib)=={'{'+W+'}val'}and len(ch)==0,'Only family, no inferred weight/charset/pitch/signature');observed.append({'name':f.get('{'+W+'}name'),'family':ch.get('{'+W+'}val')})
 require(observed==sorted(metadata,key=lambda x:x['name']),'Deterministic exact-name XML family records')
 rels=ET.fromstring(now[RELS]);oldrels=ET.fromstring(old[RELS]);fr=[x for x in rels if x.get('Type')==FREL];require(len(fr)==1 and fr[0].get('Target')=='fontTable.xml'and fr[0].get('TargetMode')is None,'One internal fontTable relationship');require(len({x.get('Id')for x in rels})==len(rels),'Relationship IDs unique');require([x.attrib for x in rels if x.get('Type')!=FREL]==[x.attrib for x in oldrels],'All prior relationships preserved')
 types=ET.fromstring(now[TYPES]);ft=[x for x in types if x.get('PartName')=='/'+FONT];require(len(ft)==1 and ft[0].get('ContentType')==FCT,'Exact fontTable content type');require([x.attrib for x in types if x.get('PartName')!='/'+FONT]==[x.attrib for x in ET.fromstring(old[TYPES])],'All prior content types preserved')
 embedded=json.loads(now[mp]);require(set(embedded['hashes'])==set(now)-{mp},'All nonmodel parts covered');require(all(embedded['hashes'][n]==hashlib.sha256(data).hexdigest()for n,data in now.items()if n!=mp),'All hashes match actual payload bytes')
 return {'exact_font_records':observed,'complete_part_set':True,'all_old_payloads_exact':True,'opc_wiring':True,'all_part_hashes':True}

def model_checks(dest,report,run):
 base={'type':'doc','attrs':{'styleProfile':'standard-v2'},'content':[{'type':'paragraph','content':[{'type':'text','text':'Independent authored metadata.'}]}]}
 def withmeta(value):
  m=copy.deepcopy(base);m['attrs'][KEY]=value;return m
 valid=[('absent',base),('roman',withmeta([{'name':'FontOnly','family':'roman'}])),('swiss-unused',withmeta([{'name':'AbsentUnused','family':'swiss'}])),('unsorted',withmeta([{'name':'Zulu','family':'swiss'},{'name':'Alpha','family':'roman'}])),('escape',withmeta([{'name':'A&B "Font" <Face>','family':'roman'}])),('name31',withmeta([{'name':'A'*31,'family':'swiss'}])),('utf16-31',withmeta([{'name':'😀'*15+'A','family':'roman'}])),('non-ascii-fold',withmeta([{'name':'ÄFont','family':'roman'},{'name':'äFont','family':'swiss'}])),('max256',withmeta([{'name':'Meta'+str(i),'family':'roman'if i%2 else'swiss'}for i in range(256)]))]
 for label,value in [('empty-text',[]),('changed-text',[{'type':'paragraph','content':[{'type':'text','text':'Entirely replaced body'}]}]),('unused-heading',[{'type':'heading','attrs':{'level':2},'content':[{'type':'text','text':'Heading'}]}])]:
  m=withmeta([{'name':'FormerTextFont','family':'swiss'}]);m['content']=value if value else[{'type':'paragraph'}];valid.append((label,m))
 legacy=copy.deepcopy(valid[1][1]);legacy['attrs'].pop('styleProfile');valid.append(('legacy-v1',legacy))
 flag=copy.deepcopy(valid[1][1]);flag['attrs']['splitPageBreakAndParagraphMark']=True;flag['content'].append({'type':'pageBreak'});valid.append(('independent-break',flag))
 for label,model in valid:
  stem='author-valid-'+label;mp=dest/(stem+'.json');save(mp,model);out=dest/(stem+'.docx');run([report['cli'],'create-docx',mp,out],True);reopen=run([report['cli'],'read-docx-model',out],True);require(json.loads(reopen.stdout)==model,'Complete authored model reopen: '+label)
  raw=dest/(stem+'.bare.json');save(raw,without_metadata(model));bo=dest/(stem+'.bare.docx');run([BASELINE,'create-docx',raw,bo],True);check=xml_assert(out,bo,model)
  twin=dest/(stem+'.repeat.docx');run([report['cli'],'create-docx',mp,twin],True);require(out.read_bytes()==twin.read_bytes(),'Deterministic package byte output')
  before=sha(out);run([report['cli'],'create-docx',mp,out],False);require(sha(out)==before,'Author output not overwritten')
  report['counts']['model_valid']+=1;report['counts']['trusted_reopen']+=1;report['counts']['no_overwrite']+=1;report['counts']['xml_oracles']+=1
  report.setdefault('authored_models',[]).append({'name':label,'model':str(mp),'docx':str(out),'sha256':sha(out),'xml_checks':check})
 invalid=[]
 def bad(label,value):invalid.append((label,withmeta(value)))
 for i,value in enumerate((None,False,True,0,1,'roman',{},[])):bad('root-type-'+str(i),value)
 for field in ('name','family'):bad('missing-'+field,[{k:v for k,v in {'name':'FontOnly','family':'roman'}.items()if k!=field}])
 for i,name in enumerate(('', ' A','A ','A'*32,'😀'*16,'bad\nname','bad\0name','\ufeffFace',None,False,0,{},[])):bad('invalid-name-'+str(i),[{'name':name,'family':'roman'}])
 for i,family in enumerate(('Roman','Swiss','ROMAN','script','decorative','modern','auto','',1,None,False,[],{})):bad('invalid-family-'+str(i),[{'name':'FontOnly','family':family}])
 for label,entries in [('dup-equal',[('Face','roman'),('Face','roman')]),('dup-conflict',[('Face','roman'),('Face','swiss')]),('alias-equal',[('Face','roman'),('face','roman')]),('alias-conflict',[('Face','roman'),('fACE','swiss')])]:bad(label,[{'name':n,'family':f}for n,f in entries])
 bad('over256',[{'name':'Font'+str(i),'family':'roman'}for i in range(257)]);bad('unknown-field',[{'name':'Face','family':'roman','weight':400}]);bad('record-null',[None]);bad('record-list',[[]]);bad('record-string',['Face'])
 for target in ('paragraph','text'):
  m=copy.deepcopy(base);node=m['content'][0]if target=='paragraph'else m['content'][0]['content'][0];node.setdefault('attrs',{})[KEY]=[{'name':'Face','family':'roman'}];invalid.append(('nonroot-'+target,m))
 for i,(label,model)in enumerate(invalid):
  mp=dest/('author-invalid-'+str(i)+'.json');save(mp,model);out=dest/('author-invalid-'+str(i)+'.docx');r=run([report['cli'],'create-docx',mp,out],False);require(not out.exists()and not r.stdout,'Invalid author creates no file/model: '+label);report['counts']['model_invalid']+=1
 report['invalid_model_labels']=[x[0]for x in invalid]

def tamper_checks(dest,report,run):
 row=next(x for x in report['cases']if x['name']=='genuine-SampleDoc');original=parts(row['docx']);mp='rust-wps/model-v2.json';changes=[]
 def change(label,part,fn):
  new=dict(original);new[part]=fn(new[part]);require(new!=original,'Tamper actually changes bytes');changes.append((label,new,part))
 change('family-value',FONT,lambda v:v.replace(b'val="swiss"',b'val="roman"'))
 change('font-name',FONT,lambda v:v.replace(b'Arial Black',b'Arial Black Changed'))
 change('font-extra-weight',FONT,lambda v:v.replace(b'</w:font>',b'<w:weight w:val="700"/></w:font>',1))
 change('font-remove-family',FONT,lambda v:v.replace(b'<w:family w:val="swiss"/>',b''))
 change('font-record-duplicate',FONT,lambda v:v.replace(b'</w:fonts>',b'<w:font w:name="Arial Black"><w:family w:val="swiss"/></w:font></w:fonts>'))
 change('font-xml-malformed',FONT,lambda v:v[:-1])
 change('font-target',RELS,lambda v:v.replace(b'Target="fontTable.xml"',b'Target="fonts-other.xml"'))
 change('font-reltype',RELS,lambda v:v.replace(FREL.encode(),(FREL+'Wrong').encode()))
 change('font-contenttype',TYPES,lambda v:v.replace(FCT.encode(),(FCT+'Wrong').encode()))
 change('font-contentpath',TYPES,lambda v:v.replace(b'/word/fontTable.xml',b'/word/fonts-other.xml'))
 for label,new,part in changes:
  for sync in (False,True):
   current=dict(new)
   if sync:
    embedded=json.loads(current[mp]);embedded['hashes']={n:hashlib.sha256(v).hexdigest()for n,v in current.items()if n!=mp};current[mp]=json.dumps(embedded,separators=(',',':'),ensure_ascii=False).encode()
   out=dest/('tamper-'+label+('-repaired-hashes'if sync else'')+'.docx')
   with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
    for n,v in current.items():z.writestr(n,v)
   r=run([report['cli'],'read-docx-model',out],False);require(not r.stdout,'Tamper no partial model');report['counts']['tamper_refusals']+=1
 for label in ('missing-table','extra-part','model-only-family','model-only-remove','model-invalid-alias','repaired-model-font-extra'):
  current=dict(original);embedded=json.loads(current[mp])
  if label=='missing-table':current.pop(FONT)
  elif label=='extra-part':current['word/font-extra.xml']=b'<unexpected/>'
  elif label=='model-only-family':embedded['document']['attrs'][KEY][0]['family']='roman'if embedded['document']['attrs'][KEY][0]['family']=='swiss'else'swiss'
  elif label=='model-only-remove':embedded['document']['attrs'].pop(KEY)
  elif label=='model-invalid-alias':
   rec=copy.deepcopy(embedded['document']['attrs'][KEY][0]);rec['name']=rec['name'].swapcase();embedded['document']['attrs'][KEY].append(rec)
  else:
   current[FONT]=current[FONT].replace(b'</w:font>',b'<w:panose1 w:val="00000000000000000000"/></w:font>',1);embedded['hashes'][FONT]=hashlib.sha256(current[FONT]).hexdigest()
  current[mp]=json.dumps(embedded,separators=(',',':'),ensure_ascii=False).encode();out=dest/('tamper-'+label+'.docx')
  with zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as z:
   for n,v in current.items():z.writestr(n,v)
  r=run([report['cli'],'read-docx-model',out],False);require(not r.stdout,'Tamper no partial model');report['counts']['tamper_refusals']+=1

def run_checks(args):
 require(args.serialized_slot,'Explicit serialized execution slot required');cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'Final candidate CLI identity');manifest=json.loads(args.manifest.read_text());args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='family-run-',dir=args.root));protected=dict(manifest['protected']);protected.update({str(cli):sha(cli),str(args.manifest.resolve()):sha(args.manifest),str(Path(__file__).resolve()):sha(__file__)});protected.update({str(p):sha(p)for p in ROOT.glob('src/*.rs')})
 require({p:sha(p)for p in protected}==protected,'Protected identities at start');counts=('preserved','fallback','unchanged','refusals','inspect_exact','trusted_reopen','no_overwrite','xml_oracles','model_valid','model_invalid','tamper_refusals','new_pdfs','reused_source_pdfs','pdf_pairs')
 report={'status':'RUNNING','cli':str(cli),'cli_sha256':sha(cli),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'checker_sha256':sha(__file__),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'historical_report':str(HISTORY),'historical_report_sha256':HISTORY_SHA,'commands':[],'cases':[],'failures':[],'counts':{k:0 for k in counts},'protected_before':protected,'min_free_bytes':shutil.disk_usage(dest).free,'source_counts':manifest['counts'],'stages':{'sources':'pending','authored-models':'pending','trusted-tampering':'pending','consumer':'pending'if args.render else'not-requested'},'execution_complete':False}
 def flush():(dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
 def run(argv,ok=None,env=None,timeout=120):
  require(shutil.disk_usage(dest).free>=FLOOR,'600 MiB floor before command');argv=list(map(str,argv));start=time.monotonic();proc=subprocess.Popen(argv,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True);termination=None
  # communicate drains stdout, while short timed waits monitor disk and time.
  while True:
   try:out,err=proc.communicate(timeout=0.15);break
   except (KeyboardInterrupt,SystemExit):
    os.killpg(proc.pid,signal.SIGTERM)
    try:out,err=proc.communicate(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);out,err=proc.communicate()
    report['commands'].append({'argv':argv,'returncode':proc.returncode,'stdout':out,'stderr':err,'terminated':'interrupted'});raise
   except subprocess.TimeoutExpired:
    free=shutil.disk_usage(dest).free;report['min_free_bytes']=min(report['min_free_bytes'],free)
    if free<FLOOR or time.monotonic()-start>timeout:
     termination='disk-floor'if free<FLOOR else'timeout';os.killpg(proc.pid,signal.SIGTERM)
     try:out,err=proc.communicate(timeout=5)
     except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);out,err=proc.communicate()
     break
  rec={'argv':argv,'returncode':proc.returncode,'stdout':out,'stderr':err,'seconds':time.monotonic()-start,'terminated':termination}
  if env is not None:rec['environment']={k:env.get(k)for k in ('HOME','XDG_CACHE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_RUNTIME_DIR','TMPDIR','FONTCONFIG_FILE','FONTCONFIG_PATH','SAL_USE_VCLPLUGIN','LANG','LC_ALL','DISPLAY')}
  report['commands'].append(rec);require(termination is None,'Command terminated: '+str(rec))
  if ok is not None:require((proc.returncode==0)==ok,'Unexpected command disposition: '+str(rec))
  return subprocess.CompletedProcess(argv,proc.returncode,out,err)
 flush()
 try:
  for case in manifest['cases']:
   row={k:v for k,v in case.items()if k not in ('raw_source','oracle')};report['cases'].append(row)
   try:
    source=Path(case['path']);require(sha(source)==case['sha256'],'Source unchanged');old=run([BASELINE,'import-doc',source]);new=run([cli,'import-doc',source]);row.update(returncode=new.returncode,stderr=new.stderr)
    oldi=run([BASELINE,'inspect',source]);newi=run([cli,'inspect',source])
    if oldi.returncode==0:require(newi.returncode==0 and b.s.normalized_inspect(json.loads(oldi.stdout))==b.s.normalized_inspect(json.loads(newi.stdout)),'Read-only inspect equivalence')
    else:require((oldi.returncode,oldi.stdout,oldi.stderr)==(newi.returncode,newi.stdout,newi.stderr),'Read-only refusal exact')
    report['counts']['inspect_exact']+=1
    if case['historical']:require(old.returncode==case['baseline_returncode'],'Historical disposition unchanged')
    if case['outcome']=='refusal':
     require(old.returncode!=0 and (old.returncode,old.stdout,old.stderr)==(new.returncode,new.stdout,new.stderr)and not new.stdout,'Existing refusal exact/no model');report['counts']['refusals']+=1;row['observed']='refusal'
    else:
     require(old.returncode==0 and new.returncode==0,'Declared successful source rejected');oldj,newj=json.loads(old.stdout),json.loads(new.stdout);expected=copy.deepcopy(oldj['document']);risks=set(oldj['compatibility']['risks']);proof=any(x.startswith('Legacy DOC literal Latin font names and effective sizes are preserved')for x in risks)
     if proof:
      oracle=family_oracle(source,oldj['document']);row['oracle']=oracle
      if 'oracle'in case:require(oracle==case['oracle'],'Prepared independent oracle stable')
      metadata=oracle['font_family_metadata'];reasons=oracle['fallback_reasons']
      if metadata:expected.setdefault('attrs',{})[KEY]=metadata;risks.add(SUCCESS.format(count=len(metadata)));observed='preserved'
      elif reasons:risks.update(FALLBACK+x for x in reasons);observed='fallback'
      else:observed='unchanged'
     else:observed='unchanged'
     require(observed==case['outcome'],'Declared source outcome '+case['outcome']+' differs from independent actual emitted proof '+observed);row['observed']=observed
     require(newj['document']==expected,'Full model differs from exact source family-only delta');require(set(newj['compatibility']['risks'])==risks,'Exact source success/fallback diagnostic delta')
     restored=copy.deepcopy(newj);restored['document']=oldj['document'];restored['compatibility']['risks']=oldj['compatibility']['risks'];require(restored==oldj,'Unrelated complete response state unchanged');report['counts'][observed]+=1
     if case['historical']:require(json.loads(Path(case['baseline_import']).read_text())==oldj,'Frozen baseline still reproduces saved complete import')
     for suffix,model,tool in [('baseline',oldj['document'],BASELINE),('output',newj['document'],cli)]:
      mp=dest/(case['name']+'.'+suffix+'.json');save(mp,model);out=dest/(case['name']+'.'+suffix+'.docx');run([tool,'create-docx',mp,out],True);reopen=run([cli,'read-docx-model',out],True);require(json.loads(reopen.stdout)==model,'Full trusted model reopen');report['counts']['trusted_reopen']+=1
     output=dest/(case['name']+'.output.docx');baseline=dest/(case['name']+'.baseline.docx');mp=dest/(case['name']+'.output.json')
     for target in (source,output):
      pin=sha(target);r=run([cli,'create-docx',mp,target],False);require(('Output already exists'if target.suffix=='.docx'else'Output must be DOCX')in r.stderr,'Exact no-clobber reason');require(sha(target)==pin,'Source/output no overwrite');report['counts']['no_overwrite']+=1
     if case['historical']:require(sha(baseline)==case['baseline_docx_sha256'],'Frozen baseline whole bytes reproduce historical output')
     row.update(model=str(mp),docx=str(output),docx_sha256=sha(output),baseline_docx=str(baseline),baseline_docx_sha256=sha(baseline),xml_checks=xml_assert(output,baseline,newj['document']));report['counts']['xml_oracles']+=1
     save(dest/(case['name']+'.import.json'),newj);save(dest/(case['name']+'.baseline-import.json'),oldj)
    row['status']='passed'
   except Exception as error:row.update(status='failed',error=str(error));report['failures'].append({'name':case['name'],'error':str(error),'traceback':traceback.format_exc()})
   flush()
  report['stages']['sources']='complete'
  if not report['failures']:
   for label,fn in [('authored-models',model_checks),('trusted-tampering',tamper_checks)]:
    try:fn(dest,report,run);report['stages'][label]='complete'
    except Exception as error:report['failures'].append({'stage':label,'error':str(error),'traceback':traceback.format_exc()})
    flush()
  if args.render and not report['failures']:
   try:consumer_checks(dest,report,run,flush);report['stages']['consumer']='complete'
   except Exception as error:report['failures'].append({'stage':'consumer','error':str(error),'traceback':traceback.format_exc()})
  report['execution_complete']=len(report['cases'])==len(manifest['cases'])and all(x.get('status')=='passed'for x in report['cases'])and all(v in ('complete','not-requested')for v in report['stages'].values())
 except BaseException as error:
  report['failures'].append({'stage':'interrupted-execution','type':type(error).__name__,'error':str(error),'traceback':traceback.format_exc()})
  raise
 finally:
  report['protected_after']={p:sha(p)for p in protected}
  if report['protected_after']!=protected:report['failures'].append({'stage':'identity','error':'Protected files changed'})
  report['status']='PASS-with-explicit-consumer-and-platform-limits'if report['execution_complete']and not report['failures']else'FAILED-INCOMPLETE'if not report['execution_complete']else'FAILED';flush();print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'status':report['status'],'counts':report['counts'],'failures':report['failures']},indent=2),flush=True)
 if report['failures']:raise SystemExit(1)

def consumer_checks(dest,report,run,flush):
 """All consumer files are fresh actual CLI packages, never ZIP interventions."""
 import fitz
 lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice');binary=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/native/libreoffice-headless/libreoffice/program/soffice.bin');pin='d905a74dc92eaccb22481ba2de1345666a074a5f0d48322aaad1b6dc8a9e1e40';require(sha(binary)==pin,'Pinned consumer binary');consumer={'version':run([lo,'--version'],True).stdout,'launcher':str(lo),'launcher_sha256':sha(lo),'binary':str(binary),'binary_sha256':pin,'pymupdf':fitz.VersionBind,'renders':{},'comparisons':{},'limits':['Exact text/glyph/metrics/raster equality is limited to this pinned Linux LibreOffice environment.','Arial Black remains absent; this preserves source substitution intent, not original font appearance.','Genuine SampleDoc alone cannot establish source FFN causality because this consumer special-cases Arial-prefixed names.','Independent absent-font sources differ solely in family bits and provide the family-only causal control.','Raw PDF font object numbers are retained and compared separately; equal bytes do not imply object-number equality.','No Microsoft Word, WPS, browser, Windows or native-print validation.']};report['consumer']=consumer;inventory_pin=None
 def snapshot(path,label):
  doc=fitz.open(path);pages=[]
  for pg in doc:
   raw=pg.get_text('rawdict');lines=[line for block in raw['blocks']for line in block.get('lines',[])];chars=[]
   for li,line in enumerate(lines):
    for span in line['spans']:
     chars.extend({'c':ch['c'],'font':span['font'],'size':span['size'],'flags':span['flags'],'color':span['color'],'origin':list(ch['origin']),'bbox':list(ch['bbox']),'line':li}for ch in span['chars'])
   resources=[]
   for font in pg.get_fonts(full=True):
    name,extension,font_type,data=doc.extract_font(font[0]);resources.append({'pdf_font':list(font),'name':name,'extension':extension,'font_type':font_type,'embedded_bytes':len(data),'embedded_sha256':hashlib.sha256(data).hexdigest()})
   raster={}
   for dpi in (72,144):
    pix=pg.get_pixmap(matrix=fitz.Matrix(dpi/72,dpi/72),alpha=False);out=dest/(label+'-page-'+str(pg.number+1)+'-'+str(dpi)+'dpi.png');pix.save(out);raster[str(dpi)]={'width':pix.width,'height':pix.height,'components':pix.n,'samples_sha256':hashlib.sha256(pix.samples).hexdigest(),'png':str(out),'png_sha256':sha(out)}
   pages.append({'text':pg.get_text(),'mediabox':list(pg.mediabox),'cropbox':list(pg.cropbox),'rotation':pg.rotation,'line_count':len(lines),'chars':chars,'rawdict':raw,'texttrace':pg.get_texttrace(),'resources':resources,'raster':raster})
  result={'pdf':str(path),'pdf_sha256':sha(path),'page_count':len(pages),'pages':pages};doc.close();return result
 def render(path,label):
  nonlocal inventory_pin
  path=Path(path);sub=dest/label;sub.mkdir()
  for name in ('home','cache','config','data','runtime','temp'):(sub/name).mkdir(mode=0o700)
  env=dict(os.environ,HOME=str(sub/'home'),XDG_CACHE_HOME=str(sub/'cache'),XDG_CONFIG_HOME=str(sub/'config'),XDG_DATA_HOME=str(sub/'data'),XDG_RUNTIME_DIR=str(sub/'runtime'),TMPDIR=str(sub/'temp'))
  inventory=run(['fc-list','--format=%{file}\t%{family}\t%{style}\n'],True,env=env).stdout;canonical='\n'.join(sorted(inventory.splitlines()));current=hashlib.sha256(canonical.encode()).hexdigest()
  if inventory_pin is None:
   inventory_pin=current;save(dest/'font-inventory.txt',canonical.encode());families={x.strip()for line in canonical.splitlines()for x in line.split('\t')[1].split(',')};require('Arial Black'not in families and'JwAbsentFamilyProbe62'not in families,'Genuine and causal fonts absent');require({'Liberation Serif','Liberation Sans'}<=families,'Installed font controls verified');consumer['font_environment']={'inventory_sha256':current,'font_files':{str(fp):sha(fp)for fp in sorted({Path(line.split('\t')[0])for line in canonical.splitlines()})if fp.exists()},'config_files':{str(fp):sha(fp)for fp in sorted(Path('/etc/fonts').rglob('*'))if fp.is_file()}}
  require(current==inventory_pin,'Exact inventory in each consumer environment');before=sha(path);run([lo,'-env:UserInstallation='+(sub/'profile').as_uri(),'--headless','--convert-to','pdf','--outdir',sub,path],True,env=env);pdf=sub/(path.stem+'.pdf');require(pdf.exists()and sha(path)==before,'Fresh PDF, input unchanged');snap=snapshot(pdf,label);snap.update(input=str(path),input_sha256=before,font_inventory_sha256=current,fresh_process_profile_home_xdg=True);consumer['renders'][label]=snap;report['counts']['new_pdfs']+=1;flush();print('RENDERED '+label,flush=True)
 def resource_identity(page):
  result=copy.deepcopy(page['resources'])
  for row in result:row['pdf_font'][0]='<document-local-font-object>'
  return sorted(result,key=lambda x:json.dumps(x,sort_keys=True))
 def compare(left,right):
  a,z=consumer['renders'][left],consumer['renders'][right];checks=[]
  for x,y in zip(a['pages'],z['pages']):
   adv=lambda p:[[i,[d['origin'][0]-c['origin'][0],d['origin'][1]-c['origin'][1]]]for i,(c,d)in enumerate(zip(p['chars'],p['chars'][1:]))if c['line']==d['line']]
   same=[c['c']for c in x['chars']]==[c['c']for c in y['chars']];dx=max((abs(c['origin'][0]-d['origin'][0])for c,d in zip(x['chars'],y['chars'])),default=0)if same else None;dy=max((abs(c['origin'][1]-d['origin'][1])for c,d in zip(x['chars'],y['chars'])),default=0)if same else None
   check={'text_exact':x['text']==y['text'],'page_geometry_exact':all(x[k]==y[k]for k in ('mediabox','cropbox','rotation')),'line_count_exact':x['line_count']==y['line_count'],'characters_exact':x['chars']==y['chars'],'glyph_traces_exact':x['texttrace']==y['texttrace'],'rawdict_exact':x['rawdict']==y['rawdict'],'adjacent_advances_exact':adv(x)==adv(y),'font_resource_identity_exact':resource_identity(x)==resource_identity(y),'raw_font_resource_records_exact':x['resources']==y['resources'],'raster72_exact':x['raster']['72']['samples_sha256']==y['raster']['72']['samples_sha256'],'raster144_exact':x['raster']['144']['samples_sha256']==y['raster']['144']['samples_sha256'],'max_origin_x_delta_pt':dx,'max_origin_y_delta_pt':dy};checks.append(check)
  keys=[k for k in checks[0]if k.endswith('_exact')and k!='raw_font_resource_records_exact']if checks else[];value={'left':left,'right':right,'page_count_exact':a['page_count']==z['page_count'],'pages':checks,'all_consumed_properties_exact':a['page_count']==z['page_count']and all(all(c[k]for k in keys)for c in checks),'all_raw_resource_records_exact':all(c['raw_font_resource_records_exact']for c in checks)};consumer['comparisons'][left+'__'+right]=value;return value
 selected=[('genuine-SampleDoc','sample'),('genuine-simple','simple'),('break-dop-674','installed'),('authored-absent-swiss','absent-swiss'),('authored-absent-roman','absent-roman'),('break-leading','leading'),('break-trailing','trailing'),('break-consecutive','consecutive')]
 for name,label in selected:
  row=next(x for x in report['cases']if x['name']==name);render(row['path'],label+'-source');render(row['docx'],label+'-product');require(compare(label+'-source',label+'-product')['all_consumed_properties_exact'],'Source/actual product exact consumer equality: '+label);report['counts']['pdf_pairs']+=1
  if label in ('sample','simple','installed','absent-swiss'):
   render(row['baseline_docx'],label+'-baseline');pair=compare(label+'-baseline',label+'-product')
   if label in ('simple','installed'):require(pair['all_consumed_properties_exact'],'Unchanged installed/simple consumer regression')
   else:require(not pair['all_consumed_properties_exact'],'Family metadata causally changes absent font fallback')
 require(not compare('absent-swiss-product','absent-roman-product')['all_consumed_properties_exact'],'Causal opposite source family affects product output')
 require(compare('absent-swiss-baseline','absent-roman-product')['all_consumed_properties_exact'],'Causal old baseline Roman fallback')
 sample=consumer['comparisons']['sample-source__sample-product'];require(len(sample['pages'])==2 and all(x['max_origin_y_delta_pt']==0 for x in sample['pages']),'Retained break and page continuity exact')
 pairs=json.loads((EXPERIMENT/'manifest.json').read_text())['synthetic_controls'];aa,zz=(Path(x['path']).read_bytes()for x in pairs);diff=[{'offset':i,'left':v,'right':w,'xor':v^w}for i,(v,w)in enumerate(zip(aa,zz))if v!=w];require(len(aa)==len(zz)and len(diff)==1 and diff[0]['xor']==0x30,'Causal source files differ solely in family bits');consumer['causal_source_byte_differences']=diff
 groups={}
 for label,r in consumer['renders'].items():
  for i,page in enumerate(r['pages'],1):groups.setdefault(page['raster']['144']['samples_sha256'],[]).append({'label':label,'page':i,'png':page['raster']['144']['png'],'png_sha256':page['raster']['144']['png_sha256']})
 consumer['visual_groups']=[{'raster144_sha256':digest,'representative':members[0],'members':members,'review':'pending actual view_image inspection'}for digest,members in groups.items()];consumer['counts']={'fresh_pdfs':len(consumer['renders']),'reused_pdfs':0,'pdf_pages':sum(x['page_count']for x in consumer['renders'].values()),'png_files':2*sum(x['page_count']for x in consumer['renders'].values()),'unique_visual_groups':len(groups)}

def preparation_model(raw):
 """Only an independent binding skeleton, never a candidate expected body model."""
 text=raw['text'];runs=raw['runs'];content=[];breaks=[];start=0
 for end,ch in enumerate(text):
  if ch!='\r':continue
  fragment=text[start:end]
  if fragment=='\f':
   node={'type':'pageBreak'};pair=[]
   for cp in (start,end):
    rr=[r for r in runs if r['start']<=cp<r['end']];require(len(rr)==1,'Independent break CP coverage');pair.append((rr[0]['slots'][0]['name'],rr[0]['halfPoints']))
   breaks.append((node,pair[0]==pair[1]))
  else:node={'type':'paragraph','content':[{'type':'text','text':fragment}]}if fragment else{'type':'paragraph'}
  content.append(node);start=end+1
 return {'type':'doc','content':content},breaks

if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('--prepare',action='store_true');ap.add_argument('--root',type=Path,default=DEFAULT);ap.add_argument('--cli',type=Path);ap.add_argument('--expected-cli-sha256');ap.add_argument('--manifest',type=Path);ap.add_argument('--render',action='store_true');ap.add_argument('--serialized-slot',action='store_true');args=ap.parse_args()
 if args.prepare:prepare(args)
 else:
  require(args.cli and args.expected_cli_sha256 and args.manifest,'Pinned CLI and manifest required');run_checks(args)
