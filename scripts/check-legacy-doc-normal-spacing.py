"""Independent legacy DOC Normal-spacing increment oracle.

Source values are read as raw CFB/FIB/STSH/PAPX bytes. The candidate parser is
never used to create expectations. Expected models start with the SHA-pinned
Normal-identity CLI and receive only independently decoded supported tuples.
OOXML references edit that frozen export directly, including the existing
minimal settings relationship that gives explicit spacing its consumer meaning.
Every invocation creates new evidence; historical reports remain immutable.
"""
import argparse, copy, hashlib, importlib.util, json, os, re, shutil, struct, subprocess, tempfile
import xml.etree.ElementTree as ET
from pathlib import Path
import zipfile

ROOT=Path(__file__).resolve().parent.parent

def module(name, path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

c=module('font_controls',ROOT/'scripts/check-legacy-doc-fonts.py')
n=module('normal_controls',ROOT/'scripts/check-legacy-doc-normal-identity.py')
require,sha,save=c.require,c.sha,c.save
W,NS=c.W,c.NS
REL='http://schemas.openxmlformats.org/package/2006/relationships'
CT='http://schemas.openxmlformats.org/package/2006/content-types'
BASELINE=ROOT/'tmp/legacy-doc-normal-final-cli'
BASELINE_SHA='a4414930e0b4049dd781b733b4893b23c91b17d5d7afee402c6ab1eb3b9ab441'
PREVIOUS=ROOT/'tmp/legacy-doc-normal-independent/normal-run-dxabllcc/report.json'
PREVIOUS_SHA='e2da47e8698270b9514d453d1d2bb059eed890d5816cfcbcff3b763d054ed7c8'
OLD_MANIFEST=ROOT/'tmp/legacy-doc-normal-independent/normal-fixtures-3wegn_1j/manifest.json'
OLD_MANIFEST_SHA='3baba34439f42143cbb0bba6cb29203d47fc3a1c134c4f96496f62c976f40af9'
KEYS=('spaceBefore','spaceAfter','lineSpacing')
SPECS={
 'paragraph':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/484822ee-a9d9-4af4-8423-29fda67a6a58',
 'lspd':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/fcdd2b79-9d47-4e0c-b1fd-705aeab88e41',
 'order':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/d8b66123-1a3d-4e06-94c5-5ab16e9b6417',
 'duplicates':'https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/9ac56e29-8488-4b0a-a009-86a26e2f175e'}


def decode_spacing(operations, state=None):
    """Normative raw decoding, deliberately independent of vendor LSPD code."""
    out=copy.deepcopy(state or {'before':0,'after':0,'line':{'kind':'multiple','value':240}})
    for op,value in operations:
        if isinstance(op,str):op=int(op,16);value=bytes.fromhex(value)
        if op in (0xa413,0xa414):
            v=c.u16(value,0);require(v<=31680,'Invalid before/after twips')
            out['before' if op==0xa413 else 'after']=v
        elif op==0x6412:
            raw,flag=struct.unpack('<HH',value)
            require(flag in (0,1),'Invalid LSPD flag')
            require(raw<=0x7bc0 or raw>=0x8440,'Invalid LSPD signed gap')
            if raw>=0x8440:out['line']={'kind':'exact','value':65536-raw}
            elif flag==1:out['line']={'kind':'multiple','value':raw}
            elif raw==0:out['line']={'kind':'multiple','value':240}
            else:out['line']={'kind':'minimum','value':raw}
    return out


def spacing_oracle(path):
    identity=n.normal_identity_oracle(path)
    style=decode_spacing(n.pap_properties(bytes.fromhex(identity['normal_style']['upx'][0])[2:]))
    for row in identity['paragraphs']:
        value=decode_spacing(row['papx_sprms'],style)
        supported=value['line']['kind']=='multiple' and 120<=value['line']['value']<=2400
        row['spacing']=value;row['supported']=supported
        row['tuple']=[value['before'],value['after'],value['line']['value']] if supported else None
    return json.loads(json.dumps(identity))


def with_style(parts,paps):
    w,t,pos=parts;start=c.u32(w,0xa2);length=c.u32(w,0xa6);old=bytes(t[start:start+length]);count=c.u16(old,20)
    replacement=c.std('Normal',paps=paps,props=c.spr(0x486d,0x409),basebytes=c.u16(old,4))
    st=old[:20]+replacement+old[22+count:];delta=len(st)-len(old)
    t[start:start+length]=st;c.set32(w,0xa6,len(st))
    for field in (0x112,0x1a2,0xfa,0x102,0xca):c.set32(w,field,c.u32(w,field)+delta)
    return w,t,pos


def line(value,flag=1):return c.spr(0x6412,struct.pack('<HH',value&65535,flag))
def before(value):return c.spr(0xa413,value)
def after(value):return c.spr(0xa414,value)
def fixture(text='A spacing control with ordinary text.',style=b'',direct=None,**kw):
    parts=c.authored(text=text,**kw)
    if style:parts=with_style(parts,style)
    if direct is not None:parts=n.with_papx(parts,[(0,props)for props in direct])
    return parts


def prepare(args):
    require(sha(BASELINE)==BASELINE_SHA,'Pinned baseline CLI changed')
    require(sha(PREVIOUS)==PREVIOUS_SHA and sha(OLD_MANIFEST)==OLD_MANIFEST_SHA,'Historical evidence changed')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='spacing-fixtures-',dir=args.root))
    cases=copy.deepcopy(json.loads(OLD_MANIFEST.read_text())['cases'])
    for row in cases:
        row['classification']='historical-'+row['outcome'];row['render']=row['outcome']=='preserved'
        if row['outcome']=='preserved':row['spacing_oracle']=spacing_oracle(Path(row['path']))
    def add(name,parts,outcome='preserved',render=False,recipes=None):
        path=dest/(name+'.doc');save(path,c.compound(parts[0],parts[1]))
        row={'name':name,'path':str(path),'sha256':sha(path),'kind':'independently-authored-binary-spacing-control','outcome':outcome,'classification':'new-'+outcome,'render':render}
        if outcome in ('preserved','spacing-unsupported'):
            row['spacing_oracle']=spacing_oracle(path);row['expected']=c.expanded_source(c.source_oracle(path))
            if recipes is not None:require([p['tuple']for p in row['spacing_oracle']['paragraphs']]==recipes,'Author recipe disagrees with raw decoder: '+name)
        cases.append(row)
    add('spacing-style',fixture(style=before(80)+after(200)+line(276)),render=True,recipes=[[80,200,276]])
    add('spacing-direct',fixture(direct=[before(240)+after(400)+line(360)]),render=True,recipes=[[240,400,360]])
    add('spacing-partial-zero',fixture(text='First ordinary paragraph.\rSecond ordinary paragraph.\rThird ordinary paragraph.',style=before(160)+after(200)+line(276),direct=[after(0),before(0)+line(240),b'']),render=True,recipes=[[160,0,276],[0,200,240],[160,200,276]])
    add('spacing-stored-order',fixture(style=before(10)+before(20)+after(30)+after(40)+line(120)+line(276),direct=[before(80)+before(0)+after(60)+after(140)+line(360)+line(241)]),render=True,recipes=[[0,140,241]])
    add('spacing-explicit-zero',fixture(style=before(240)+after(240)+line(480),direct=[before(0)+after(0)+line(0,0)]),render=True,recipes=[[0,0,240]])
    for value in (120,121,239,240,241,276,360,2399,2400):
        add('spacing-multiple-'+str(value),fixture(direct=[line(value)]),render=value in (120,240,241,360),recipes=[[0,0,value]])
    for value in (0,1,31680):
        add('spacing-before-after-'+str(value),fixture(direct=[before(value)+after(value)]),render=value==1,recipes=[[value,value,240]])
    for kind,ops in [('exact0',line(-240,0)),('exact1',line(-240,1)),('minimum',line(240,0)),('zero-multiple',line(0,1)),('large-multiple',line(31680,1))]:
        add('spacing-style-overridden-'+kind,fixture(style=ops+before(60),direct=[line(276)]),render=kind=='exact1',recipes=[[60,0,276]])
        add('spacing-direct-overridden-'+kind,fixture(direct=[ops+after(200)+line(360)]),recipes=[[0,200,360]])
    for raw,flag in [(0,1),(1,1),(119,1),(2401,1),(31680,1),(1,0),(240,0),(31680,0),(-1,0),(-240,0),(-31680,0),(-1,1),(-240,1),(-31680,1)]:
        add('spacing-unsupported-'+str(raw)+'-'+str(flag),fixture(style=before(80)+after(200),direct=[line(raw,flag)]),'spacing-unsupported',render=raw in (0,-240) and flag==1,recipes=[None])
    add('spacing-mixed-tuples',fixture(text='Supported first.\rUnsupported second.\rExact third.\rSingle fourth.\rZero fifth.',style=before(80)+after(200),direct=[line(276),line(240,0),line(-200,1),line(0,0),line(0,1)]),'spacing-unsupported',True,[[80,200,276],None,None,[80,200,240],None])
    add('spacing-adjacent-margin-max',fixture(text='First paragraph.\rSecond paragraph.\rThird paragraph.',direct=[after(240),before(240),before(80)+after(160)]),render=True,recipes=[[0,240,240],[240,0,240],[80,160,240]])
    add('spacing-whitespace-blanks-breaks',fixture(text='\r  leading and trailing ordinary paragraph.  \r \r\xa0\r\f\r\f\rLast.\r',style=after(200)+line(276)),render=True)
    add('spacing-short-edge-space-identity',fixture(text='  leading and trailing  ',style=after(200)+line(276)),'refused')
    # Long ordinary Latin prose produces multiple physical lines; explicit font
    # is actually installed, avoiding a source-vs-output substitute confounder.
    long='One line of ordinary prose to measure supported line spacing. '*5
    for value in (240,360,480):
        add('spacing-installed-multiline-'+str(value),fixture(text=long,style=line(value),fonts=('Liberation Serif',)),render=True)
    add('spacing-installed-margins',fixture(text='First ordinary paragraph.\rSecond ordinary paragraph.\rThird ordinary paragraph.',fonts=('Liberation Serif',),direct=[after(240),before(240),before(80)+after(160)]),render=True)
    malformed=[('before31681',before(31681)),('before65535',before(65535)),('after31681',after(31681)),('after65535',after(65535))]
    malformed += [('line-gap-'+str(raw),line(raw))for raw in (31681,32767,32768,33855)]
    malformed += [('flag-'+str(raw)+'-'+str(flag),line(raw,flag))for raw in (0,240,-240)for flag in (2,65535)]
    malformed += [('truncated-line',c.p16(0x6412)+c.p16(240))]
    for label,ops in malformed:
        add('spacing-malformed-direct-'+label,fixture(direct=[ops]),'refused' if label=='truncated-line' else 'new-refusal')
        add('spacing-malformed-style-'+label,fixture(style=ops,direct=[line(240)+before(0)+after(0)]),'refused' if label=='truncated-line' else 'new-refusal')
    add('spacing-malformed-overwritten',fixture(direct=[line(240,2)+line(240)]),'new-refusal')
    add('spacing-malformed-late',fixture(text='First valid.\rSecond invalid.',direct=[line(276),after(31681)]),'new-refusal')
    for order in (False,True):
        malformed=before(31681);unknown=c.spr(0x2405,1)
        add('spacing-malformed-unknown-pap-'+str(order),fixture(direct=[unknown+malformed if order else malformed+unknown]),'new-refusal')
    add('spacing-malformed-unknown-char',fixture(direct=[after(31681)],runs=[(37,c.spr(0x2aee,1))]),'new-refusal')
    add('spacing-malformed-inherited-normal',fixture(direct=[line(240,2)],base=15,extras={15:c.std('Active Base Paragraph',sti=0xffe,style_index=15)}),'new-refusal')
    add('spacing-malformed-missing-char-style',fixture(direct=[before(31681)],default_char=False),'new-refusal')
    add('spacing-malformed-style-unknown-pap',fixture(style=c.spr(0x2405,1)+after(31681)),'new-refusal')
    add('spacing-unproved-paragraph',fixture(direct=[line(276)+c.spr(0x2405,1)]),'unsupported')
    add('spacing-unproved-character',fixture(direct=[line(276)],runs=[(37,c.spr(0x2aee,1))]),'unsupported')
    for row in cases:
        if row['kind']!='independently-authored-binary-spacing-control' or row['outcome']not in ('new-refusal','refused'):continue
        name=row['name']
        if name=='spacing-short-edge-space-identity':row['expected_error']='DOC typography source-to-editor text identity mismatch; source remains read-only'
        elif 'truncated-line'in name:row['expected_error']='DOC typography structure is truncated'
        elif 'line-gap'in name:row['expected_error']='DOC LSPD dyaLine is outside the valid signed twip range'
        elif 'flag-'in name or name in ('spacing-malformed-overwritten','spacing-malformed-inherited-normal'):row['expected_error']='DOC LSPD fMultLinespace must be 0 or 1'
        elif 'before'in name or 'unknown-pap-False'in name or 'unknown-pap-True'in name or 'missing-char-style'in name:row['expected_error']='DOC paragraph spacing 0xA413 is outside valid 0–31680 twips'
        else:row['expected_error']='DOC paragraph spacing 0xA414 is outside valid 0–31680 twips'
    manifest={'classification':'Independent encoded Normal spacing values; not whole-source layout fidelity','checker_sha256':sha(__file__),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_report':str(PREVIOUS),'historical_report_sha256':PREVIOUS_SHA,'historical_manifest':str(OLD_MANIFEST),'historical_manifest_sha256':OLD_MANIFEST_SHA,'specification_urls':SPECS,'cases':cases}
    save(dest/'manifest.json',manifest);print(dest/'manifest.json')


def xml_tree(node):return (node.tag,tuple(sorted(node.attrib.items())),node.text or '',tuple(xml_tree(child)for child in node))
def normalized_inspect(value):return n.normalized_inspect(value)


def model_expected(old,oracle):
    expected=copy.deepcopy(old['document']);blocks=expected['content'];require(len(blocks)==len(oracle['paragraphs']),'Exact block count')
    for block,p in zip(blocks,oracle['paragraphs']):
        if p['type']=='pageBreak':require(block=={'type':'pageBreak'},'Page break remains bare');continue
        require(block['type']=='paragraph','Proven Normal baseline identity')
        require(''.join(v['text']for v in block.get('content',[]))==p['text'],'Exact full text identity')
        if p['supported']:block.setdefault('attrs',{}).update(dict(zip(KEYS,p['tuple'])))
    return expected


def xml_reference(baseline,out,oracle):
    explicit=any(p['type']!='pageBreak' and p['supported']for p in oracle['paragraphs'])
    with zipfile.ZipFile(baseline)as zi,zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as zo:
        for item in zi.infolist():
            data=zi.read(item.filename)
            if item.filename=='word/document.xml':
                root=ET.fromstring(data);ps=root.findall('w:body/w:p',NS);require(len(ps)==len(oracle['paragraphs']),'XML paragraph count')
                for p,proof in zip(ps,oracle['paragraphs']):
                    if proof['type']=='pageBreak' or not proof['supported']:continue
                    pp=p.find('w:pPr',NS)
                    if pp is None:pp=ET.Element('{'+W+'}pPr');p.insert(0,pp)
                    spacing=pp.find('w:spacing',NS)
                    if spacing is None:spacing=ET.SubElement(pp,'{'+W+'}spacing')
                    spacing.attrib={('{'+W+'}'+k):str(v)for k,v in zip(('before','after','line'),proof['tuple'])};spacing.set('{'+W+'}lineRule','auto')
                data=ET.tostring(root,encoding='utf-8',xml_declaration=True)
            elif item.filename=='word/_rels/document.xml.rels' and explicit and 'word/settings.xml' not in zi.namelist():
                require(data.count(b'</Relationships>')==1,'Baseline relationships closing tag');data=data.replace(b'</Relationships>',b'<Relationship Id="settings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>')
            elif item.filename=='[Content_Types].xml' and explicit and 'word/settings.xml' not in zi.namelist():
                require(data.count(b'</Types>')==1,'Baseline Content Types closing tag');data=data.replace(b'</Types>',b'<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>')
            zo.writestr(item,data)
        if explicit and 'word/settings.xml' not in zi.namelist():zo.writestr('word/settings.xml','<w:settings xmlns:w="'+W+'"/>')


def assert_xml(output,reference,oracle):
    with zipfile.ZipFile(output)as a,zipfile.ZipFile(reference)as b:
        require(set(a.namelist())==set(b.namelist()),'Independent package part set')
        for name in a.namelist():
            if name=='rust-wps/model-v2.json':continue
            if name.endswith(('.xml','.rels')):require(xml_tree(ET.fromstring(a.read(name)))==xml_tree(ET.fromstring(b.read(name))),'Independent exact XML mismatch: '+name)
            else:require(a.read(name)==b.read(name),'Non-XML output part changed: '+name)
        ps=ET.fromstring(a.read('word/document.xml')).findall('w:body/w:p',NS)
        for p,o in zip(ps,oracle['paragraphs']):
            spacing=p.find('w:pPr/w:spacing',NS)
            if o['type']=='pageBreak':require(spacing is None,'Page-break atom has no spacing');continue
            if o['supported']:require(spacing is not None and tuple(int(spacing.get('{'+W+'}'+k))for k in ('before','after','line'))==tuple(o['tuple']) and spacing.get('{'+W+'}lineRule')=='auto','All explicit supported XML attributes')


# Exact diagnostic oracle is supplied here from the intended public contract,
# never learned by sampling candidate output.
def expected_risks(old,oracle):
    risks=set(old['compatibility']['risks']);counts={};supported=0;breaks=0;empty=0
    for row in oracle['paragraphs']:
        if row['type']=='pageBreak':breaks+=1;continue
        if row['text']=='':empty+=1
        if row['supported']:supported+=1;continue
        line=row['spacing']['line'];kind,value=line['kind'],line['value']
        reason=(f"line multiple {value}/240 is outside supported 120–2400 units" if kind=='multiple' else f"{kind} line spacing of {value} twips is outside the supported multiple-line subset")
        counts[reason]=counts.get(reason,0)+1
    unsupported=sum(counts.values())
    if supported+unsupported:risks.add(f"Legacy DOC encoded paragraph spacing values preserved for {supported} audited Normal paragraph(s); {unsupported} paragraph(s) retain normalized spacing; source layout, grids and compatibility settings are not proven")
    for reason,count in counts.items():risks.add(f"Legacy DOC encoded paragraph spacing not preserved for {count} audited Normal paragraph(s): {reason}")
    if breaks:risks.add(f"Legacy DOC break-only paragraph spacing and baseline fonts are not preserved ({breaks} paragraph(s)); standalone page-break nodes are retained")
    if empty:risks.add(f"Legacy DOC empty-paragraph baseline fonts are not preserved ({empty} paragraph(s)); encoded spacing does not establish their line geometry")
    return sorted(risks)


def run_checks(args):
    cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'Candidate CLI identity');require(sha(BASELINE)==BASELINE_SHA,'Baseline identity')
    manifest=json.loads(args.manifest.read_text());require(manifest['checker_sha256']==sha(__file__),'Frozen manifest/checker identity')
    require(sha(PREVIOUS)==PREVIOUS_SHA and sha(OLD_MANIFEST)==OLD_MANIFEST_SHA,'Historical evidence unchanged')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='spacing-run-',dir=args.root))
    report={'status':'running','checker_sha256':sha(__file__),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'cli':str(cli),'cli_sha256':sha(cli),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'specification_urls':SPECS,'commands':[],'cases':[],'failures':[],'counts':{'preserved':0,'spacing_unsupported':0,'unsupported_exact_responses':0,'unchanged_refusals':0,'new_refusals':0,'inspect_exact':0,'trusted_reopen':0,'no_overwrite':0,'trusted_tamper_refusals':0,'new_pdfs':0,'spacing_pdf_pairs':0}}
    protected={Path(r['path'])for r in manifest['cases']}|{cli,BASELINE,PREVIOUS,OLD_MANIFEST,args.manifest,Path(__file__),ROOT/'scripts/check-legacy-doc-fonts.py',ROOT/'scripts/check-legacy-doc-normal-identity.py',ROOT/'src/legacy_typography.rs',ROOT/'src/importing.rs',ROOT/'src/document.rs',ROOT/'tests/importing.rs'}
    for directory in (PREVIOUS.parent,ROOT/'tmp/legacy-doc-font-investigation-20261014'):
        protected.update(p for p in directory.rglob('*')if p.is_file() and not {'profile','home','cache','config'}.intersection(p.parts))
    protected_before={str(p):sha(p)for p in sorted(protected)};report['protected_before']=protected_before
    def flush(): (dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    def run(argv,ok=None):
        require(shutil.disk_usage(dest).free>=600*1024*1024,'600MiB free-space floor')
        p=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=120)
        report['commands'].append({'argv':list(map(str,argv)),'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
        if ok is not None:require((p.returncode==0)==ok,'Unexpected command result: '+str(argv)+' '+p.stderr)
        return p
    for case in manifest['cases']:
        row={k:v for k,v in case.items()if k not in ('expected','spacing_oracle','normal_identity_oracle','raw_oracle')};report['cases'].append(row)
        try:
            source=Path(case['path']);require(sha(source)==case['sha256'],'Source pin');old=run([BASELINE,'import-doc',source]);new=run([cli,'import-doc',source]);row.update(returncode=new.returncode,stderr=new.stderr)
            oldi=run([BASELINE,'inspect',source],True);newi=run([cli,'inspect',source],True)
            require(normalized_inspect(json.loads(oldi.stdout))==normalized_inspect(json.loads(newi.stdout)),'Read-only inspect changed');report['counts']['inspect_exact']+=1
            if case['outcome']in ('refused','new-refusal'):
                require(new.returncode!=0 and not new.stdout,'Refused input produced partial model')
                if case['outcome']=='refused':require(old.returncode==new.returncode and old.stderr==new.stderr,'Historical refusal changed');report['counts']['unchanged_refusals']+=1
                else:
                    require(old.returncode==0,'New malformed-spacing control already refused by baseline: '+old.stderr)
                    require(case['expected_error']in new.stderr,'Malformed control failed for wrong reason: '+new.stderr)
                    report['counts']['new_refusals']+=1
                if case.get('expected_error'):require(case['expected_error']in new.stderr,'Required refusal reason missing')
            else:
                require(old.returncode==new.returncode==0,'Valid input rejected: '+old.stderr+' / '+new.stderr);oldj=json.loads(old.stdout);newj=json.loads(new.stdout)
                save(dest/(case['name']+'.baseline-import.json'),oldj);save(dest/(case['name']+'.import.json'),newj)
                if case['outcome']=='unsupported':require(oldj==newj,'Whole-proof unsupported response changed');report['counts']['unsupported_exact_responses']+=1
                else:
                    oracle=spacing_oracle(source);require(oracle==case['spacing_oracle'],'Raw source oracle changed');expected=copy.deepcopy(oldj);expected['document']=model_expected(oldj,oracle);expected['compatibility']['risks']=expected_risks(oldj,oracle)
                    require(newj==expected,'Exact intended model/diagnostic delta failed')
                    require(c.strip_type(c.expanded_model(newj['document']))==case['expected'],'Independent original typography/text oracle changed')
                    for suffix,model,tool in [('baseline',oldj['document'],BASELINE),('output',newj['document'],cli),('expected',expected['document'],BASELINE)]:
                        mp=dest/(case['name']+'.'+suffix+'.json');save(mp,model);out=dest/(case['name']+'.'+suffix+'.docx');run([tool,'create-docx',mp,out],True)
                        require(json.loads(run([cli,'read-docx-model',out],True).stdout)==model,'Full trusted model reopen');report['counts']['trusted_reopen']+=1
                        for target in (source,out):
                            pin=sha(target);denied=run([cli,'create-docx',mp,target],False);require(('Output already exists' if target.suffix=='.docx' else 'Output must be DOCX')in denied.stderr,'Wrong no-overwrite reason');require(sha(target)==pin,'Source/target overwritten');report['counts']['no_overwrite']+=1
                    output=dest/(case['name']+'.output.docx');base=dest/(case['name']+'.baseline.docx');ref=dest/(case['name']+'.spacing-reference.docx');xml_reference(base,ref,oracle);assert_xml(output,ref,oracle)
                    require(output.read_bytes()==(dest/(case['name']+'.expected.docx')).read_bytes(),'Old CLI export of independently expected model differs')
                    require(c.xml_chars(output)==case['expected'],'Four font slots/halfpoints/text changed')
                    report['counts']['spacing_unsupported' if case['outcome']=='spacing-unsupported' else 'preserved']+=1
                    row.update(model=str(dest/(case['name']+'.output.json')),docx=str(output),docx_sha256=sha(output),baseline_docx=str(base),reference=str(ref),reference_sha256=sha(ref),raw_spacing=oracle)
            row['status']='passed'
        except Exception as error:row['status']='failed';row['error']=str(error);report['failures'].append({'name':case['name'],'error':str(error)})
        flush()
    if not report['failures']:
        tamper_checks(dest,report,run);flush()
    if args.render and not report['failures']:
        try:consumer_checks(dest,report,run,flush)
        except Exception as error:report['failures'].append({'stage':'consumer','error':str(error)});flush()
    report['protected_after']={str(p):sha(p)for p in sorted(protected)}
    require(report['protected_after']==protected_before,'Protected evidence/source/checker/CLI changed')
    report['status']='PASS-with-explicit-page-break-empty-font-grid-compatibility-page-and-substitution-limits' if not report['failures']else 'FAILED';flush()
    print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'counts':report['counts'],'failures':report['failures']},indent=2))
    if report['failures']:raise SystemExit(1)


def tamper_checks(dest,report,run):
    source=Path(next(r for r in report['cases']if r['name']=='genuine-SampleDoc')['docx'])
    for label in ('before','after','line','lineRule','remove-spacing','remove-settings','change-settings','reinsert-heading','change-font'):
        out=dest/('tamper-'+label+'.docx')
        with zipfile.ZipFile(source)as zi,zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED)as zo:
            for item in zi.infolist():
                data=zi.read(item.filename)
                if label=='remove-settings' and item.filename=='word/settings.xml':continue
                if label=='change-settings' and item.filename=='word/settings.xml':data=('<w:settings xmlns:w="'+W+'"><w:compat><w:doNotUseHTMLParagraphAutoSpacing/></w:compat></w:settings>').encode()
                if item.filename=='word/document.xml' and label not in ('remove-settings','change-settings'):
                    original=data.decode();changed=original
                    spacing=re.search(r'<w:spacing\b[^>]*/>',original);require(spacing is not None,'Literal spacing control target')
                    token=spacing.group(0)
                    if label in ('before','after','line'):
                        pattern='w:'+label+'="([0-9]+)"';match=re.search(pattern,token);require(match is not None,'Literal spacing attribute target')
                        replacement=re.sub(pattern,lambda x:'w:'+label+'="'+str(int(x.group(1))+1)+'"',token,count=1)
                        changed=original[:spacing.start()]+replacement+original[spacing.end():]
                    elif label=='lineRule':
                        require('w:lineRule="auto"'in token,'Literal auto-line rule target');replacement=token.replace('w:lineRule="auto"','w:lineRule="exact"',1);changed=original[:spacing.start()]+replacement+original[spacing.end():]
                    elif label=='remove-spacing':changed=original[:spacing.start()]+original[spacing.end():]
                    elif label=='reinsert-heading':
                        require('<w:pPr>'in original,'Literal paragraph properties target');changed=original.replace('<w:pPr>','<w:pPr><w:pStyle w:val="Heading1"/>',1)
                    elif label=='change-font':
                        require('<w:rFonts w:ascii="Calibri"'in original,'Literal font target');changed=original.replace('<w:rFonts w:ascii="Calibri"','<w:rFonts w:ascii="Arial"',1)
                    require(changed!=original,'Tamper must change only its target');data=changed.encode()
                zo.writestr(item,data)
        run([report['cli'],'read-docx-model',out],False);report['counts']['trusted_tamper_refusals']+=1
        # A hash mismatch alone is insufficient to prove trusted reencoding.
        # Recompute every changed package-part digest while retaining the same
        # embedded document; the semantic/canonical consistency gate must fail.
        synchronized=dest/('tamper-'+label+'-synchronized-hashes.docx')
        with zipfile.ZipFile(out)as zi:
            parts={name:zi.read(name)for name in zi.namelist()}
        embedded=json.loads(parts['rust-wps/model-v2.json'])
        embedded['hashes']={name:hashlib.sha256(data).hexdigest()for name,data in parts.items()if name!='rust-wps/model-v2.json'}
        parts['rust-wps/model-v2.json']=json.dumps(embedded,separators=(',',':'),ensure_ascii=False).encode()
        with zipfile.ZipFile(synchronized,'x',zipfile.ZIP_DEFLATED)as zo:
            for name,data in parts.items():zo.writestr(name,data)
        rejected=run([report['cli'],'read-docx-model',synchronized],False)
        reason='Embedded model disagrees with DOCX part set' if label=='remove-settings' else 'Embedded model disagrees with DOCX content: '+('word/settings.xml' if label=='change-settings' else 'word/document.xml')
        require(reason in rejected.stderr,'Synchronized tamper rejected for wrong target: '+rejected.stderr)
        report['counts']['trusted_tamper_refusals']+=1


def consumer_checks(dest,report,run,flush):
    import fitz
    lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice')
    binaries=list(Path('/opt').glob('**/program/soffice.bin'));require(len(binaries)==1,'Unique fixed independent consumer');binary=binaries[0];pin=sha(binary)
    report['consumer']={'binary':str(binary),'sha256':pin,'version':run([lo,'--version'],True).stdout,'pymupdf':fitz.VersionBind,'poppler_sha256':sha('/usr/bin/pdftotext'),'runs':[],'comparisons':[],'known_limitations':[]}
    def render(path,label):
        require(shutil.disk_usage(dest).free>=650*1024*1024,'650MiB free-space floor before consumer')
        d=dest/label;d.mkdir();profile=d/'profile';home=d/'home';home.mkdir();env=dict(os.environ,HOME=str(home),XDG_CACHE_HOME=str(d/'cache'),XDG_CONFIG_HOME=str(d/'config'))
        argv=[str(lo),'-env:UserInstallation='+profile.as_uri(),'--headless','--convert-to','pdf','--outdir',str(d),str(path)]
        pr=subprocess.run(argv,capture_output=True,text=True,timeout=120,env=env)
        report['consumer']['runs'].append({'argv':argv,'HOME':str(home),'XDG_CACHE_HOME':str(d/'cache'),'returncode':pr.returncode,'stdout':pr.stdout,'stderr':pr.stderr})
        pdf=d/(path.stem+'.pdf');require(pr.returncode==0 and pdf.exists(),'Fresh consumer PDF missing: '+pr.stderr);doc=fitz.open(pdf);pages=[]
        for page in doc:
            spans=[sp for block in page.get_text('dict')['blocks']if 'lines'in block for line in block['lines']for sp in line['spans']];chars=[]
            for block in page.get_text('rawdict')['blocks']:
                if 'lines'not in block:continue
                for line in block['lines']:
                    for sp in line['spans']:chars.extend({'c':ch['c'],'font':sp['font'],'size':sp['size'],'origin':ch['origin'],'bbox':ch['bbox']}for ch in sp['chars'])
            pix=page.get_pixmap(matrix=fitz.Matrix(1,1));pages.append({'text':page.get_text(),'width':page.rect.width,'height':page.rect.height,'fonts':sorted({(sp['font'],sp['size'])for sp in spans}),'spans':spans,'chars':chars,'words':page.get_text('words'),'raster_sha256':hashlib.sha256(pix.samples).hexdigest()})
            if label.startswith(('genuine-','spacing-whitespace','spacing-mixed','spacing-installed','spacing-adjacent'))and not label.endswith('reference'):pix.save(d/('page-'+str(page.number+1)+'.png'))
        text=run(['/usr/bin/pdftotext','-layout',pdf,'-'],True).stdout;(d/'poppler.txt').write_text(text)
        row={'input':str(path),'input_sha256':sha(path),'pdf':str(pdf),'pdf_sha256':sha(pdf),'pages':pages,'profile':str(profile),'home':str(home)};report['counts']['new_pdfs']+=1;return row
    rows={r['name']:r for r in report['cases']}
    for row in report['cases']:
        if not row.get('render')or 'docx'not in row:continue
        name=row['name'];out=render(Path(row['docx']),name+'-output');ref=render(Path(row['reference']),name+'-reference');row['consumer_output']=out;row['consumer_reference']=ref
        require(out['pages']==ref['pages'],'Strict independently edited spacing XML reference/output text/fonts/geometry/pixels differ: '+name)
        report['counts']['spacing_pdf_pairs']+=1;report['consumer']['comparisons'].append({'name':name,'oracle':'Raw source spacing overlay on frozen Normal-identity export plus independently constructed minimal settings OPC parts','full_text_fonts_spans_chars_geometry_raster_equal':True,'max_coordinate_difference_pt':0.0,'changed_pixels':0});flush()
    for name in ('genuine-simple','genuine-SampleDoc'):
        row=rows[name];source=render(Path(row['path']),name+'-source');row['consumer_source']=source;out=row['consumer_output'];base=render(Path(row['baseline_docx']),name+'-baseline');row['consumer_baseline']=base
        require(len(source['pages'])==len(out['pages'])==len(base['pages']),'Genuine page count remains')
        for a,b in zip(source['pages'],out['pages']):require(''.join(a['text'].split())==''.join(b['text'].split()),'Genuine full per-page text identity')
        a,b,old=source['pages'][0],out['pages'][0],base['pages'][0]
        if name=='genuine-simple':
            require(a['fonts']==b['fonts']==[('LiberationSerif',10.0)],'simple actual source/output font-size identity')
            require(a['width']!=b['width']and a['chars'][0]['origin'][0]!=b['chars'][0]['origin'][0],'Known simple page/margin difference retained')
            report['consumer']['known_limitations'].append({'name':name,'source_page':[a['width'],a['height']],'output_page':[b['width'],b['height']],'source_first_origin':a['chars'][0]['origin'],'output_first_origin':b['chars'][0]['origin'],'full_layout_equal':False,'note':'Source Letter/90pt margin versus A4/72pt remains outside guarantee'})
        else:
            require(a['fonts']==b['fonts']==old['fonts']==[('Carlito',11.0)],'SampleDoc actual page1 font-size remains regular11pt')
            def gaps(page):
                y=[span['origin'][1]for span in page['spans']];return [y[i+1]-y[i]for i in range(len(y)-1)]
            gs,go,gb=gaps(a),gaps(b),gaps(old);require(len(gs)==len(go)==len(gb)>0,'SampleDoc baseline gap count')
            prior=max(abs(x-y)for x,y in zip(gs,gb));now=max(abs(x-y)for x,y in zip(gs,go))
            require(now<prior,'Encoded SampleDoc spacing must improve measured page1 gaps')
            require(source['pages'][1]['fonts']==[('NotoSans-Regular',16.0)]and out['pages'][1]['fonts']==[('NotoSerif-Regular',16.0)],'Known Arial Black substitution remains explicit')
            report['consumer']['comparisons'].append({'name':name,'kind':'real-source-page1','source_gaps_pt':gs,'baseline_gaps_pt':gb,'output_gaps_pt':go,'prior_max_gap_error_pt':prior,'current_max_gap_error_pt':now,'source_first_origin':a['chars'][0]['origin'],'output_first_origin':b['chars'][0]['origin'],'page1_chars_exact':a['chars']==b['chars'],'page1_raster_exact':a['raster_sha256']==b['raster_sha256']})
            report['consumer']['known_limitations'].append({'name':name,'source_fonts':[p['fonts']for p in source['pages']],'output_fonts':[p['fonts']for p in out['pages']],'source_page2_first_origin':source['pages'][1]['chars'][0]['origin'],'output_page2_first_origin':out['pages'][1]['chars'][0]['origin'],'full_layout_equal':False,'note':'Bare break-only paragraph cannot carry source spacing/baseline font; empty baseline fonts, document grid/compatibility, page geometry and missing Arial Black substitution remain outside proof'})
        flush()
    # Independently authored installed-font DOC inputs test physical line and
    # adjacent margin values. Record source differences without equating an
    # encoded-value proof with general DOC/OOXML layout equivalence.
    for name in ('spacing-installed-multiline-240','spacing-installed-multiline-360','spacing-installed-multiline-480','spacing-installed-margins'):
        row=rows[name];source=render(Path(row['path']),name+'-source');row['consumer_source']=source;out=row['consumer_output']
        a,b=source['pages'][0],out['pages'][0];require(a['fonts']==b['fonts']==[('LiberationSerif',10.0)],'Installed-font consumer identity')
        def distinct_y(page):return sorted({s['origin'][1]for s in page['spans']})
        ya,yb=distinct_y(a),distinct_y(b);ga=[y-ya[0]for y in ya];gb=[y-yb[0]for y in yb]
        require(len(ga)==len(gb),'Installed spacing source/output physical line count')
        delta=max((abs(x-y)for x,y in zip(ga,gb)),default=0)
        report['consumer']['comparisons'].append({'name':name,'kind':'independent-binary-installed-font-source','source_relative_baselines_pt':ga,'output_relative_baselines_pt':gb,'max_relative_baseline_difference_pt':delta,'full_layout_claimed':False})
        if delta:report['consumer']['known_limitations'].append({'name':name,'source_output_relative_geometry_equal':False,'difference_pt':delta,'note':'Encoded-value import is bounded; physical binary/OOXML consumer geometry differs'})
        flush()
    # settings.xml is semantic: this deliberate no-settings counterpart must
    # separate summed vs max adjacent margins in the independent consumer.
    row=rows['spacing-adjacent-margin-max'];unconfigured=dest/'spacing-reference-without-settings.docx'
    with zipfile.ZipFile(row['reference'])as zi,zipfile.ZipFile(unconfigured,'x',zipfile.ZIP_DEFLATED)as zo:
        for item in zi.infolist():
            if item.filename=='word/settings.xml':continue
            data=zi.read(item.filename)
            if item.filename=='word/_rels/document.xml.rels':
                data,count=re.subn(rb'<Relationship\b[^>]*Target="settings.xml"[^>]*/>',b'',data);require(count==1,'Remove only settings relationship')
            elif item.filename=='[Content_Types].xml':
                data,count=re.subn(rb'<Override\b[^>]*PartName="/word/settings.xml"[^>]*/>',b'',data);require(count==1,'Remove only settings content type')
            zo.writestr(item,data)
    absent=render(unconfigured,'spacing-without-settings');row['consumer_without_settings']=absent
    good=row['consumer_output']['pages'][0];bad=absent['pages'][0]
    yg=[s['origin'][1]for s in good['spans']];yb=[s['origin'][1]for s in bad['spans']]
    require(len(yg)==len(yb)==3 and abs((yb[1]-yb[0])-(yg[1]-yg[0])-12.0)<0.001,'Minimal settings must independently distinguish12pt max vs24pt sum')
    report['consumer']['comparisons'].append({'name':'settings-semantics','with_settings_baselines':yg,'without_settings_baselines':yb,'adjacent_margin_extra_without_settings_pt':(yb[1]-yb[0])-(yg[1]-yg[0])})
    report['consumer']['known_limitations'].append({'scope':'all results','note':'No app browser, Windows WebView2, native print, Microsoft Word or WPS validation; grid/compatibility and complete section/page semantics are not proven'})
    require(sha(binary)==pin,'Independent consumer binary changed');flush()


def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,default=ROOT/'tmp/legacy-doc-normal-spacing-verify');p.add_argument('--prepare',action='store_true');p.add_argument('--manifest',type=Path);p.add_argument('--cli',type=Path);p.add_argument('--expected-cli-sha256');p.add_argument('--render',action='store_true');args=p.parse_args()
    if args.prepare:prepare(args);return
    require(args.manifest and args.cli and args.expected_cli_sha256,'Provide --prepare or pinned candidate and manifest');run_checks(args)

if __name__=='__main__':main()
