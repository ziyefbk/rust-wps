import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const source=path.resolve('tests/fixtures/apache-poi/basic_test_ppt_file.ppt');
const before=readFileSync(source);
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
const expected=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-ppt',source],options));
const response=JSON.parse(execFileSync(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),['--document-worker'],{...options,input:JSON.stringify({operation:'open',path:source})}));
assert.equal(response.result,'opened');assert.equal(response.document.inspection.format,'ppt');
assert.deepEqual(response.document.presentation,expected);
assert.equal(response.document.presentation_model,undefined);
assert.equal(response.document.authored,null);
assert.deepEqual(readFileSync(source),before);
console.log('Legacy PPT worker matches real CLI report; source unchanged; no editable model.');
for(const name of ['WithMaster.ppt','with_textbox.ppt','Password_Protected-hello.ppt']){
  const file=path.resolve('tests/fixtures/apache-poi',name),original=readFileSync(file);
  const result=JSON.parse(execFileSync(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),['--document-worker'],{...options,input:JSON.stringify({operation:'open',path:file})}));
  if(name.startsWith('Password')){assert.equal(result.result,'error');assert.match(JSON.stringify(result),/Encrypted/);}
  else{assert.equal(result.result,'opened');assert.deepEqual(result.document.presentation,JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-ppt',file],options)));}
  assert.deepEqual(readFileSync(file),original);
}
console.log('Real textbox, master and encrypted PPT worker cases passed.');
