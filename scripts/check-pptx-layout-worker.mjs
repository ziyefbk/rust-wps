import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
for(const level of ['layout','master'])for(const suffix of ['','-ambiguous']){
 const file=path.resolve(`tests/fixtures/${level}-placeholders${suffix}.pptx`),before=readFileSync(file);
 const expected=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',file],options));
 const worker=JSON.parse(execFileSync(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),['--document-worker'],{...options,input:JSON.stringify({operation:'open',path:file})}));
 assert.equal(worker.result,'opened');assert.deepEqual(worker.document.presentation,expected);
 assert.equal(expected.slides[0].shapes[0].bounds_emu,null);
 assert.equal(Boolean(expected.slides[0].shapes[0][`${level}_bounds_emu`]),!suffix);
 assert.equal(worker.document.presentation_model,undefined);assert.deepEqual(readFileSync(file),before);
}
console.log('Layout placeholder positions survive worker open, ambiguity stays unresolved, sources unchanged.');
