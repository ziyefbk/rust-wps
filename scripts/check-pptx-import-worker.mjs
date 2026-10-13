import {execFileSync} from 'node:child_process';
import {readFileSync,mkdirSync,mkdtempSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
const cli=path.resolve('target/debug/rust-wps.exe'),desktop=path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
mkdirSync('tmp',{recursive:true});const dir=mkdtempSync(path.resolve('tmp/pptx-import-worker-')),simple=path.join(dir,'simple.pptx');
execFileSync(cli,['create-pptx',path.resolve('docs/examples/presentation-v1.json'),simple],options);
for(const [file,accepted] of [[simple,true],[path.resolve('tests/fixtures/basic-slides.pptx'),false]]){
 const before=readFileSync(file),expected=JSON.parse(execFileSync(cli,['import-pptx',file],options));
 const actual=JSON.parse(execFileSync(desktop,['--document-worker'],{...options,input:JSON.stringify({operation:'import_presentation',path:file})}));
 assert.equal(actual.result,'presentation_imported');assert.deepEqual(actual.report,expected);
 assert.equal(Boolean(actual.report.model),accepted);assert.equal(actual.report.blockers.length===0,accepted);
 assert.deepEqual(readFileSync(file),before);
}
console.log('PPTX import worker matches CLI for accepted and blocked sources; files unchanged.');
