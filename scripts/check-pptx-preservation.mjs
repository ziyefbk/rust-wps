import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,mkdtempSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const cli=path.resolve('target/debug/rust-wps.exe'),desktop=path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
mkdirSync('tmp',{recursive:true});const directory=mkdtempSync(path.resolve('tmp/pptx-preservation-'));
const model=JSON.parse(readFileSync('docs/examples/presentation-v1.json','utf8'));
model.size_emu=[9144000,6858000];model.slides[0].hidden=true;model.slides[0].notes='简文演示\n备注保留 <&>\n';
const input=path.join(directory,'source.pptx'),output=path.join(directory,'edited.pptx'),json=path.join(directory,'model.json');
writeFileSync(json,JSON.stringify(model));execFileSync(cli,['create-pptx',json,input],options);const original=readFileSync(input);
const worker=request=>JSON.parse(execFileSync(desktop,['--document-worker'],{...options,input:JSON.stringify(request)}));
const saved=worker({operation:'replace_presentation',path:input,output,find:'简文演示',replacement:'替换正文'});
assert.equal(saved.result,'presentation_saved');assert.equal(saved.saved.report.replacements,1);
const reopened=worker({operation:'open',path:output});assert.equal(reopened.result,'opened');
assert.equal(reopened.document.presentation_model,undefined);
assert.match(reopened.document.inspection.warnings.join('\n'),/Editable presentation unavailable/);
const report=reopened.document.presentation;assert.deepEqual(report.size_emu,model.size_emu);
assert.equal(report.slides[0].hidden,true);assert.equal(report.slides[0].notes,model.slides[0].notes);
assert.match(report.slides[0].text,/替换正文/);assert.doesNotMatch(report.slides[0].text,/备注保留/);
const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
const independent=JSON.parse(execFileSync(python,['-c',`import json,sys
from pptx import Presentation
p=Presentation(sys.argv[1]);s=p.slides[0]
print(json.dumps({'size':[p.slide_width,p.slide_height],'show':s._element.get('show'),'notes':s.notes_slide.notes_text_frame.text,'text':[x.text for x in s.shapes if x.has_text_frame]},ensure_ascii=True))`,output],options));
assert.deepEqual(independent.size,model.size_emu);assert.equal(independent.show,'0');assert.equal(independent.notes,model.slides[0].notes);assert.match(independent.text.join('\n'),/替换正文/);
assert.deepEqual(readFileSync(input),original);
console.log('Mixed-feature PPTX replacement preserves notes, hidden state, dimensions and source; worker refuses stale editable model; independent python-pptx confirms output.');
