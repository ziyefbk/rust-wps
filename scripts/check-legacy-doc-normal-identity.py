"""Independent Normal-identity increment checks; entrypoint is --normal-identity.

The expected model and OOXML come from the pinned iteration-29 CLI, never the
candidate parser. Independent binary identity checks permit exactly one change:
source-proven Normal paragraphs lose a guessed heading type and its level.
The historical font-only checker/report and its known bold-heading failure stay
unchanged and are explicitly superseded only within this proof subset.
"""
import copy, hashlib, importlib.util, json, os, shutil, struct, subprocess, tempfile
import xml.etree.ElementTree as ET
from pathlib import Path
import zipfile

ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('font_controls',ROOT/'scripts/check-legacy-doc-fonts.py')
c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
require,sha,save=c.require,c.sha,c.save
BASELINE=ROOT/'tmp/legacy-doc-font-final-cli'
BASELINE_SHA='1977766cd91f48bb0cec0a4dc8d964ca3c339ac1cfcd548a8d6b4bba90a9c5bc'
PREVIOUS=ROOT/'tmp/legacy-doc-font-validation-20261014/run-iemy7_8j/report.json'
PREVIOUS_SHA='b6466a56fe8ad3a56ab0633abbd22353e323eb54bc2a539d2e8bed4500ff69e1'
OLD_NOTICE='Legacy DOC inferred heading type and weight may differ from the source Normal paragraph'
NEW_NOTICE='Legacy DOC encoded Normal paragraph identity is preserved for audited body paragraphs'
W,NS=c.W,c.NS

def pap_properties(data):
    # PAPX may have one explicit zero pad byte. STD UPX may be unpadded.
    p=0;result=[]
    while p<len(data):
        if p+1==len(data) and data[p]==0:break
        require(p+2<=len(data),'PAPX opcode truncated')
        op=c.u16(data,p);p+=2;n=[1,1,2,4,2,2,None,3][op>>13]
        if n is None:require(p<len(data),'PAPX length missing');n=data[p];p+=1
        require(p+n<=len(data),'PAPX operand truncated');result.append((op,data[p:p+n]));p+=n
    return result

def normal_identity_oracle(path):
    """Read every physical PAPX and built-in identity independently of Rust."""
    o=c.source_oracle(path);ss=c.streams(path);w=ss['WordDocument'];t=ss['1Table' if c.u16(w,10)&512 else '0Table']
    st=t[c.u32(w,0xa2):c.u32(w,0xa2)+c.u32(w,0xa6)];base=c.u16(st,4);q=2+c.u16(st,0);n=c.u16(st,q);record=st[q+2:q+2+n]
    normal=o['styles'][0]
    require(normal['sti']==0 and normal['kind']==1 and normal['base']==4095,'Only built-in unbased Normal is proven')
    require(base!=18 or c.u16(record,10)&0x1fff==0,'Linked/revision Normal is unproved')
    require(len(normal['upx'])==2 and bytes.fromhex(normal['upx'][0])[:2]==b'\0\0','Normal UPX identity')
    style_ops=pap_properties(bytes.fromhex(normal['upx'][0])[2:]);require(all(op in (0x6412,0xa413,0xa414) for op,_ in style_ops),'Normal has structural/outline/unknown modifier')
    off,n=c.u32(w,0x102),c.u32(w,0x106);plc=t[off:off+n];require(n>=12 and (n-4)%8==0,'PAPX index framing');k=(n-4)//8
    clx=t[c.u32(w,0x1a2):c.u32(w,0x1a2)+c.u32(w,0x1a6)];fc=(c.u32(clx,15)&0x3fffffff)//2
    positions=[];cursor=fc
    for i in range(k):
        pn=c.u32(plc,4*(k+1)+4*i)&0x3fffff;pg=w[pn*512:(pn+1)*512];count=pg[511]
        require(len(pg)==512 and count>0,'PAPX page valid')
        for j in range(count):
            a,b=c.u32(pg,j*4),c.u32(pg,(j+1)*4);require(a==cursor and b>a,'Exact ordered paragraph coverage');cursor=b
            at=pg[4*(count+1)+13*j]*2
            if at:
                reread=pg[at]==0;p=at+int(reread);end=p+pg[p]*2+int(reread);require(end<=511,'PAPX payload boundary');istd=c.u16(pg,p+1);props=pg[p+3:end]
            else:istd=0;props=b''
            ops=pap_properties(props);require(istd==0 and all(op in (0x6412,0xa413,0xa414) for op,_ in ops),'Active paragraph is not proven Normal')
            text=w[a:b].decode('cp1252');require(text.endswith('\r') and '\r' not in text[:-1],'PAPX exactly matches one source paragraph')
            positions.append({'start':a-fc,'end':b-fc,'istd':istd,'type':'pageBreak' if text=='\f\r' else 'paragraph','text':text[:-1],'papx_sprms':[(hex(op),v.hex()) for op,v in ops]})
    require(cursor==fc+len(o['text']) and ''.join(p['text']+'\r' for p in positions)==o['text'],'PAPX exact full source text')
    return {'source_sha256':sha(path),'identity':'built-in unbased unlinked unrevised Normal','normal_style':normal,'paragraphs':positions}

def with_papx(parts,settings):
    """Independently encode one authored PAPX page, settings=(istd, grpprl)."""
    w,t,p=parts;raw=w[p['fc']:p['fc']+p['text_len']];ends=[i+1 for i,b in enumerate(raw) if b==13]
    require(len(ends)==len(settings),'PAPX author recipe count');pg=bytearray(512);pg[511]=len(ends);c.set32(pg,0,p['fc']);end=510
    for i,(stop,(istd,props)) in enumerate(zip(ends,settings)):
        if len(props)%2:props+=b'\0'
        payload=b'\0'+bytes([(2+len(props))//2])+c.p16(istd)+props;end=(end-len(payload))&~1
        require(end>4*(len(ends)+1)+13*len(ends),'PAPX author capacity');pg[end:end+len(payload)]=payload;c.set32(pg,4*(i+1),p['fc']+stop);pg[4*(len(ends)+1)+13*i]=end//2
    w[p['papg']*512:(p['papg']+1)*512]=pg;return w,t,p

def prepare(args):
    require(sha(BASELINE)==BASELINE_SHA,'Frozen iteration-29 CLI identity');require(sha(PREVIOUS)==PREVIOUS_SHA,'Frozen iteration-29 consumer report identity')
    prior=json.loads(PREVIOUS.read_text());old_manifest=Path(prior['manifest']);require(sha(old_manifest)==prior['manifest_sha256'],'Historical manifest identity')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='normal-fixtures-',dir=args.root));cases=copy.deepcopy(json.loads(old_manifest.read_text())['cases'])
    def add(name,parts,outcome='preserved',required_heading=False):
        path=dest/(name+'.doc');save(path,c.compound(parts[0],parts[1]));case={'name':name,'path':str(path),'sha256':sha(path),'kind':'independently-authored-normal-identity-control','outcome':outcome,'requires_baseline_heading':required_heading}
        if outcome=='preserved':case['expected']=c.expanded_source(c.source_oracle(path));case['normal_identity_oracle']=normal_identity_oracle(path)
        cases.append(case)
    add('normal-first-short',c.authored(text='Quarterly status'),required_heading=True)
    add('normal-allcaps-later',c.authored(text='Intro.\rSECOND NORMAL LINE\rOrdinary body.'),required_heading=True)
    add('normal-no-heading',c.authored(text='A complete ordinary sentence with no heading heuristic.'),required_heading=False)
    add('normal-blank-breaks',c.authored(text='NORMAL TITLE\r\r\f\rSECOND PAGE\rLast.'),required_heading=True)
    marks=c.spr(0x6870,bytes.fromhex('20509000'))+c.spr(0x4a43,27)+c.spr(0x4a4f,2)+c.spr(0x4a51,2)
    add('normal-inline-marks',c.authored(text='Marked Normal title',default=1,runs=[(6,marks),(20,c.spr(0x4a43,27))]),required_heading=True)
    # The display name is deliberately misleading; built-in numeric identity wins.
    p=c.authored(text='Numeric Normal identity');w,t,pos=p;start=pos['style0']+2+10+2;t[start:start+12]='HdgOne'.encode('utf-16le')
    add('normal-misleading-display-name',p,required_heading=True)
    text='True title\rOrdinary body.'
    heading=c.std('Heading 1',sti=1,style_index=1,props=c.spr(0x486d,0x409))
    add('actual-heading-style',with_papx(c.authored(text=text,extras={1:heading}),[(1,b''),(0,b'')]),'unsupported',True)
    deceptive=c.std('Normal',sti=1,style_index=1,props=c.spr(0x486d,0x409))
    add('actual-heading-misleading-normal-name',with_papx(c.authored(text=text,extras={1:deceptive}),[(1,b''),(0,b'')]),'unsupported',True)
    for label,value in [('outline-heading',0),('outline-body-explicit',9)]:
        add(label,with_papx(c.authored(text=text),[(0,c.spr(0x2640,value)),(0,b'')]),'unsupported',True)
    # Rebuild the Normal style with a paragraph outline while keeping all tables self-consistent.
    p=c.authored(text='Style outline title');w,t,pos=p;st0=c.u32(w,0xa2);stn=c.u32(w,0xa6);st=bytes(t[st0:st0+stn]);oldn=c.u16(st,20);replacement=c.std('Normal',paps=c.spr(0x2640,0),props=c.spr(0x486d,0x409));newst=st[:20]+replacement+st[22+oldn:];delta=len(newst)-len(st);t[st0:st0+stn]=newst;c.set32(w,0xa6,len(newst))
    for field in (0x112,0x1a2,0xfa,0x102,0xca):c.set32(w,field,c.u32(w,field)+delta)
    add('normal-style-outline-active',(w,t,pos),'unsupported',True)
    add('normal-unknown-pap-modifier',with_papx(c.authored(text='Unproved Normal title'),[(0,c.spr(0x2405,1))]),'unsupported',True)
    add('normal-unknown-char-modifier',c.authored(text='Unproved character title',runs=[(25,c.spr(0x2aee,1))]),'unsupported',True)
    explicit_marks=c.spr(0x0835,1)+c.spr(0x0836,1)+c.spr(0x2a3e,1)+c.spr(0x0837,1)
    add('normal-unsupported-explicit-marks',c.authored(text='Explicit styled title',runs=[(22,explicit_marks)]),'unsupported',True)
    add('normal-first-then-invalid-font',c.authored(text='Normal title\rInvalid later.',runs=[(13,b''),(28,c.spr(0x4a4f,99)+c.spr(0x4a51,99))]),'refused')
    p=c.authored(text='Normal title');w,t,pos=p;c.set16(w,pos['papg']*512+508,99);add('normal-invalid-active-style',p,'refused')
    for case in cases:
        if case['outcome']=='preserved':case['normal_identity_oracle']=normal_identity_oracle(Path(case['path']))
    manifest={'classification':'Normal identity: genuine files and independent author controls','baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_report':str(PREVIOUS),'historical_report_sha256':sha(PREVIOUS),'historical_manifest':str(old_manifest),'historical_manifest_sha256':sha(old_manifest),'checker_sha256':sha(__file__),'cases':cases}
    save(dest/'manifest.json',manifest);print(dest/'manifest.json');return dest/'manifest.json'

def normalized_inspect(value):
    if isinstance(value,dict):return {k:normalized_inspect(v) for k,v in value.items() if k!='elapsed_ms'}
    if isinstance(value,list):return [normalized_inspect(v) for v in value]
    return value

def expected_import(old,oracle):
    new=copy.deepcopy(old);blocks=new['document']['content'];require(len(blocks)==len(oracle['paragraphs']),'Exact Normal/model paragraph count');changed=[]
    for i,(block,proof) in enumerate(zip(blocks,oracle['paragraphs'])):
        if proof['type']=='pageBreak':require(block=={'type':'pageBreak'},'Exact page-break block');continue
        require(block['type'] in ('paragraph','heading'),'Normal model block kind')
        require(''.join(n['text'] for n in block.get('content',[]))==proof['text'],'Normal model full text identity')
        if block['type']=='heading':
            changed.append(i);block['type']='paragraph';block['attrs'].pop('level',None)
            if not block['attrs']:block.pop('attrs')
    risks=new['compatibility']['risks'];risks[:]=sorted(set(r for r in risks if r!=OLD_NOTICE)|{NEW_NOTICE})
    return new,changed

def xml_tree(node):return (node.tag,tuple(sorted(node.attrib.items())),node.text or '',tuple(xml_tree(child) for child in node))

def identity_reference(old,out,changed):
    with zipfile.ZipFile(old) as zi,zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED) as zo:
        for item in zi.infolist():
            data=zi.read(item.filename)
            if item.filename=='word/document.xml':
                root=ET.fromstring(data);ps=root.findall('w:body/w:p',NS)
                for i in changed:
                    pp=ps[i].find('w:pPr',NS);style=pp.find('w:pStyle',NS);require(style is not None and style.get('{'+W+'}val') in ('Heading1','Heading2','Heading3'),'Frozen heading style exists');pp.remove(style)
                data=ET.tostring(root,encoding='utf-8',xml_declaration=True)
            zo.writestr(item,data)

def assert_xml_reference(output,reference,baseline):
    with zipfile.ZipFile(output) as a,zipfile.ZipFile(reference) as b,zipfile.ZipFile(baseline) as old:
        require(set(a.namelist())==set(old.namelist()),'Output package parts unchanged')
        for name in a.namelist():
            if name=='word/document.xml':require(xml_tree(ET.fromstring(a.read(name)))==xml_tree(ET.fromstring(b.read(name))),'Exact paragraph identity-only XML difference')
            elif name!='rust-wps/model-v2.json':
                require(a.read(name)==old.read(name),'Non-model package part changed: '+name)


def run_checks(args):
    cli=args.cli.resolve();require(sha(cli)==args.expected_cli_sha256,'Candidate CLI identity');require(sha(BASELINE)==BASELINE_SHA,'Frozen baseline identity')
    manifest=json.loads(args.manifest.read_text());require(sha(__file__)==manifest['checker_sha256'],'Manifest/checker source identity');require(sha(PREVIOUS)==PREVIOUS_SHA==manifest['historical_report_sha256'],'Historical report unchanged')
    args.root.mkdir(parents=True,exist_ok=True);dest=Path(tempfile.mkdtemp(prefix='normal-run-',dir=args.root))
    report={'status':'running','checker_sha256':sha(__file__),'dispatcher_sha256':sha(ROOT/'scripts/check-legacy-doc-fonts.py'),'manifest':str(args.manifest),'manifest_sha256':sha(args.manifest),'cli':str(cli),'cli_sha256':sha(cli),'baseline':str(BASELINE),'baseline_sha256':BASELINE_SHA,'historical_report':str(PREVIOUS),'historical_report_sha256':sha(PREVIOUS),'commands':[],'cases':[],'failures':[],'counts':{'preserved':0,'changed_paragraphs':0,'unchanged_positive_models':0,'unsupported_exact_responses':0,'refused':0,'inspect_exact':0,'trusted_reopen':0,'no_overwrite':0,'trusted_tamper_refusals':0,'new_pdfs':0,'identity_pdf_pairs':0}}
    previous=json.loads(PREVIOUS.read_text());historical={v['name']:v for v in previous['cases']}
    protected={Path(v['path']) for v in manifest['cases']}|{cli,BASELINE,PREVIOUS,args.manifest,Path(manifest['historical_manifest']),Path(__file__),ROOT/'scripts/check-legacy-doc-fonts.py',ROOT/'src/legacy_typography.rs',ROOT/'src/importing.rs',ROOT/'src/lib.rs'}
    protected.update((ROOT/'tmp/legacy-doc-font-validation-20261014').rglob('*.json'))
    for p in PREVIOUS.parent.rglob('*'):
        if p.is_file() and 'profile' not in p.parts and 'home' not in p.parts:protected.add(p)
    before={str(p):sha(p) for p in sorted(protected)};report['protected_before']=before
    def run(argv,ok=None):
        require(shutil.disk_usage(dest).free>=600*1024*1024,'600 MiB free-space floor before CLI check')
        pr=subprocess.run(list(map(str,argv)),capture_output=True,text=True,timeout=120);report['commands'].append({'argv':list(map(str,argv)),'code':pr.returncode,'stdout':pr.stdout,'stderr':pr.stderr})
        if ok is not None:require((pr.returncode==0)==ok,'Unexpected command result: '+str(argv)+' '+pr.stderr)
        return pr
    def flush():(dest/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    for case in manifest['cases']:
        row={k:v for k,v in case.items() if k not in ('expected','raw_oracle')};report['cases'].append(row)
        try:
            source=Path(case['path']);require(sha(source)==case['sha256'],'Source pin')
            old=run([BASELINE,'import-doc',source]);new=run([cli,'import-doc',source]);row['returncode']=new.returncode;row['stderr']=new.stderr
            require(old.returncode==new.returncode,'Import outcome changed outside intended Normal semantics')
            for tool,label in ((BASELINE,'old'),(cli,'new')):
                inspection=run([tool,'inspect',source],True);row[label+'_inspect']=json.loads(inspection.stdout)
            require(normalized_inspect(row['old_inspect'])==normalized_inspect(row['new_inspect']),'Read-only inspection changed');report['counts']['inspect_exact']+=1
            if case['outcome']=='refused':
                require(new.returncode!=0 and old.stderr==new.stderr,'Malformed refusal reason changed');require(new.stdout==old.stdout=='','No partial editable result on refusal');report['counts']['refused']+=1
            else:
                require(new.returncode==0,'Valid fixture rejected: '+new.stderr);oldj=json.loads(old.stdout);newj=json.loads(new.stdout);row['risks']=newj['compatibility']['risks'];save(dest/(case['name']+'.baseline-import.json'),oldj);save(dest/(case['name']+'.import.json'),newj)
                if case.get('requires_baseline_heading'):require(any(b['type']=='heading' for b in oldj['document']['content']),'Author control must exercise existing heading path')
                if case['outcome']=='unsupported':
                    require(oldj==newj and OLD_NOTICE not in newj['compatibility']['risks'] and NEW_NOTICE not in newj['compatibility']['risks'],'Unsupported full report/model changed or falsely claimed identity');report['counts']['unsupported_exact_responses']+=1
                else:
                    oracle=normal_identity_oracle(source);require(oracle==case['normal_identity_oracle'],'Independent identity oracle changed');expected,changed=expected_import(oldj,oracle);require(newj==expected,'Only proven heading type/level and exact diagnostics may change')
                    require(c.strip_type(c.expanded_model(newj['document']))==case['expected'],'Existing independent typography oracle changed')
                    for suffix,model,tool in [('baseline',oldj['document'],BASELINE),('output',newj['document'],cli),('expected',expected['document'],BASELINE)]:
                        mp=dest/(case['name']+'.'+suffix+'.json');save(mp,model);out=dest/(case['name']+'.'+suffix+'.docx');run([tool,'create-docx',mp,out],True)
                        require(json.loads(run([cli,'read-docx-model',out],True).stdout)==model,'Trusted reopen complete model');report['counts']['trusted_reopen']+=1
                        for existing in (source,out):
                            pin=sha(existing);run([cli,'create-docx',mp,existing],False);require(sha(existing)==pin,'No-overwrite source/target changed');report['counts']['no_overwrite']+=1
                    baseline=dest/(case['name']+'.baseline.docx');output=dest/(case['name']+'.output.docx');reference=dest/(case['name']+'.identity-reference.docx');identity_reference(baseline,reference,changed);assert_xml_reference(output,reference,baseline)
                    require(output.read_bytes()==(dest/(case['name']+'.expected.docx')).read_bytes(),'Independent old-CLI expected model export differs')
                    require(c.xml_chars(output)==case['expected'],'Four literal font slots and halfpoints preserved')
                    if case['name'] in historical:require(baseline.read_bytes()==Path(historical[case['name']]['docx']).read_bytes(),'Frozen iteration29 export drift')
                    if not changed:require(output.read_bytes()==baseline.read_bytes(),'Non-heading positive export changed');report['counts']['unchanged_positive_models']+=1
                    report['counts']['changed_paragraphs']+=len(changed);report['counts']['preserved']+=1
                    row.update(changed_paragraphs=changed,model=str(dest/(case['name']+'.output.json')),docx=str(output),docx_sha256=sha(output),baseline_docx=str(baseline),reference=str(reference),reference_sha256=sha(reference))
            row['status']='passed'
        except Exception as e:row['status']='failed';row['error']=str(e);report['failures'].append({'name':case['name'],'error':str(e)})
        flush()
    if not report['failures']:
        sample=next(r for r in report['cases'] if r['name']=='genuine-SampleDoc');source=Path(sample['docx'])
        for label in ('reinsert-heading','direct-outline','change-color','change-size','change-font'):
            out=dest/('tamper-'+label+'.docx')
            with zipfile.ZipFile(source) as zi,zipfile.ZipFile(out,'x',zipfile.ZIP_DEFLATED) as zo:
                for item in zi.infolist():
                    data=zi.read(item.filename)
                    if item.filename=='word/document.xml':
                        root=ET.fromstring(data);p=root.find('w:body/w:p',NS);pp=p.find('w:pPr',NS)
                        if label in ('reinsert-heading','direct-outline'):ET.SubElement(pp,'{'+W+'}'+('pStyle' if label=='reinsert-heading' else 'outlineLvl'),{'{'+W+'}val':'Heading1' if label=='reinsert-heading' else '0'})
                        elif label=='change-color':root.find('.//w:color',NS).set('{'+W+'}val','205090')
                        elif label=='change-size':root.find('.//w:sz',NS).set('{'+W+'}val','23')
                        else:root.find('.//w:rFonts',NS).set('{'+W+'}ascii','Arial')
                        data=ET.tostring(root,encoding='utf-8',xml_declaration=True)
                    zo.writestr(item,data)
            run([cli,'read-docx-model',out],False);report['counts']['trusted_tamper_refusals']+=1
        flush()
    if args.render and not report['failures']:
        try:consumer_checks(dest,report,run,flush,previous)
        except Exception as e:report['failures'].append({'stage':'consumer','error':str(e)});flush()
    report['protected_after']={str(p):sha(p) for p in sorted(protected)};require(report['protected_after']==before,'Protected evidence/source/CLI changed');report['status']='PASS-with-explicit-source-spacing-page-font-substitution-limits' if not report['failures'] else 'FAILED';flush()
    print(json.dumps({'report':str(dest/'report.json'),'sha256':sha(dest/'report.json'),'counts':report['counts'],'failures':report['failures']},indent=2))
    if report['failures']:raise SystemExit(1)


def consumer_checks(dest,report,run,flush,previous):
    import fitz
    lo=Path('/opt/codex/runtimes/codex-primary-runtime/dependencies/bin/override/soffice')
    binaries=list(Path('/opt').glob('**/program/soffice.bin'));require(len(binaries)==1,'Unique fixed consumer executable');binary=binaries[0];pin=sha(binary)
    report['consumer']={'binary':str(binary),'sha256':pin,'version':run([lo,'--version'],True).stdout,'pymupdf':fitz.VersionBind,'poppler_sha256':sha('/usr/bin/pdftotext'),'runs':[],'comparisons':[],'known_limitations':[]}
    def render(path,label):
        require(shutil.disk_usage(dest).free>=650*1024*1024,'650 MiB free-space floor before independent consumer')
        d=dest/label;d.mkdir();profile=d/'profile';home=d/'home';home.mkdir();env=dict(os.environ,HOME=str(home),XDG_CACHE_HOME=str(d/'cache'),XDG_CONFIG_HOME=str(d/'config'))
        argv=[str(lo),'-env:UserInstallation='+profile.as_uri(),'--headless','--convert-to','pdf','--outdir',str(d),str(path)]
        pr=subprocess.run(argv,capture_output=True,text=True,timeout=120,env=env);report['consumer']['runs'].append({'argv':argv,'HOME':str(home),'XDG_CACHE_HOME':str(d/'cache'),'returncode':pr.returncode,'stdout':pr.stdout,'stderr':pr.stderr})
        pdf=d/(path.stem+'.pdf');require(pr.returncode==0 and pdf.exists(),'Fresh consumer PDF missing: '+pr.stderr);doc=fitz.open(pdf);pages=[]
        for page in doc:
            spans=[sp for block in page.get_text('dict')['blocks'] if 'lines' in block for line in block['lines'] for sp in line['spans']];chars=[]
            for block in page.get_text('rawdict')['blocks']:
                if 'lines' not in block:continue
                for line in block['lines']:
                    for sp in line['spans']:chars.extend({'c':c['c'],'font':sp['font'],'size':sp['size'],'origin':c['origin'],'bbox':c['bbox']} for c in sp['chars'])
            pix=page.get_pixmap(matrix=fitz.Matrix(1,1));pages.append({'text':page.get_text(),'width':page.rect.width,'height':page.rect.height,'fonts':sorted({(sp['font'],sp['size']) for sp in spans}),'spans':spans,'chars':chars,'words':page.get_text('words'),'raster_sha256':hashlib.sha256(pix.samples).hexdigest()})
            if label.startswith(('genuine-','normal-inline-marks-','normal-blank-breaks-')) and not label.endswith('reference'):pix.save(d/('page-'+str(page.number+1)+'.png'))
        text=run(['/usr/bin/pdftotext','-layout',pdf,'-'],True).stdout;(d/'poppler.txt').write_text(text)
        row={'input':str(path),'input_sha256':sha(path),'pdf':str(pdf),'pdf_sha256':sha(pdf),'pages':pages,'profile':str(profile),'home':str(home)};report['counts']['new_pdfs']+=1;return row
    rows={r['name']:r for r in report['cases']}
    for row in report['cases']:
        if row['outcome']!='preserved':continue
        name=row['name'];out=render(Path(row['docx']),name+'-output');ref=render(Path(row['reference']),name+'-reference');row['consumer_output']=out;row['consumer_reference']=ref
        require(out['pages']==ref['pages'],'Strict identity-only reference/output full text/fonts/geometry/pixels differ: '+name)
        report['counts']['identity_pdf_pairs']+=1;report['consumer']['comparisons'].append({'name':name,'oracle':'independent Heading style removal on frozen iteration29 export','full_text_spans_fonts_coordinates_raster_equal':True,'max_coordinate_difference_pt':0.0,'changed_pixels':0});flush()
    for name in ('genuine-simple','genuine-SampleDoc'):
        row=rows[name];source=render(Path(row['path']),name+'-source');row['consumer_source']=source;out=row['consumer_output'];require(len(source['pages'])==len(out['pages']),'Genuine page count identity')
        for a,b in zip(source['pages'],out['pages']):require(''.join(a['text'].split())==''.join(b['text'].split()),'Genuine full per-page text identity')
        if name=='genuine-simple':
            a,b=source['pages'][0],out['pages'][0];require(a['fonts']==b['fonts']==[('LiberationSerif',10.0)],'simple source/output face and size');require(a['width']!=b['width'] and a['chars'][0]['origin'][0]!=b['chars'][0]['origin'][0],'Known simple page/margin differences remain')
            report['consumer']['known_limitations'].append({'name':name,'source_page':[a['width'],a['height']],'output_page':[b['width'],b['height']],'source_first_origin':a['chars'][0]['origin'],'output_first_origin':b['chars'][0]['origin'],'full_layout_equal':False,'note':'Source Letter/90pt margin versus output A4/72pt remains outside guarantee'})
        else:
            baseline=render(Path(row['baseline_docx']),name+'-baseline');row['consumer_baseline']=baseline;a,b,old=source['pages'][0],out['pages'][0],baseline['pages'][0]
            require(a['fonts']==b['fonts']==[('Carlito',11.0)],'SampleDoc proven Normal source/output regular Carlito 11pt')
            require(old['fonts']==[('Carlito',11.0),('Carlito-Bold',11.0)],'Frozen baseline retains known false bold heading')
            first_source,first_output,first_old=a['spans'][0],b['spans'][0],old['spans'][0]
            require(first_source['text']==first_output['text']==first_old['text']=='I am a test document','Real first line identity')
            require(first_source['font']==first_output['font']=='Carlito' and first_old['font']=='Carlito-Bold','Real first line weight repaired')
            require(first_source['size']==first_output['size']==first_old['size']==11.0,'Physical 11pt remains')
            require(first_source['origin']==first_output['origin'],'Source first-line position exactly restored')
            require(first_old['origin'][1]-first_output['origin'][1]==12.0,'Only inherited Heading1 12pt top gap removed')
            require(source['pages'][1]['fonts']==[('NotoSans-Regular',16.0)] and out['pages'][1]['fonts']==[('NotoSerif-Regular',16.0)],'Known Arial Black substitution discrepancy retained')
            require(out['pages'][1]==baseline['pages'][1],'Unrelated second-page layout/colors/text/pixels changed')
            source_gaps=[a['spans'][i+1]['origin'][1]-a['spans'][i]['origin'][1] for i in range(len(a['spans'])-1)]
            output_gaps=[b['spans'][i+1]['origin'][1]-b['spans'][i]['origin'][1] for i in range(len(b['spans'])-1)]
            require(source_gaps!=output_gaps,'Remaining paragraph spacing mismatch must stay explicit')
            report['consumer']['comparisons'].append({'name':name,'kind':'real-source-firstline','source_font':first_source['font'],'output_font':first_output['font'],'old_font':first_old['font'],'size_pt':11.0,'source_origin':first_source['origin'],'output_origin':first_output['origin'],'old_origin':first_old['origin'],'removed_false_heading_gap_pt':12.0,'second_page_exactly_unchanged':True})
            report['consumer']['known_limitations'].append({'name':name,'source_fonts':[p['fonts'] for p in source['pages']],'output_fonts':[p['fonts'] for p in out['pages']],'source_line_gaps_pt':source_gaps,'output_line_gaps_pt':output_gaps,'heading_weight_equal':True,'arial_black_substitute_equal':False,'full_layout_equal':False,'source_paragraph_spacing_preserved':False})
        flush()
    # Compare new consumer runs to the protected historical baseline measurements;
    # these historic PDFs are reused evidence, not counted as new renders.
    oldsample=next(r for r in previous['cases'] if r['name']=='genuine-SampleDoc')['consumer_output']
    require(json.loads(json.dumps(rows['genuine-SampleDoc']['consumer_baseline']['pages']))==oldsample['pages'],'Fixed baseline consumer changed from iteration29')
    require(sha(binary)==pin,'Consumer executable changed')


def main(args):
    if args.root==c.DEFAULT:args.root=ROOT/'tmp/legacy-doc-normal-independent'
    if args.prepare:prepare(args);return
    require(args.manifest and args.cli and args.expected_cli_sha256,'Normal identity requires --prepare or manifest and pinned CLI')
    run_checks(args)
