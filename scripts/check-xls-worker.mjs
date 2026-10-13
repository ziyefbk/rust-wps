import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
const cases=JSON.parse(execFileSync(python,['scripts/check-xls.py'],options));
for(const {source,date1904} of cases.systems) {
  const before=readFileSync(source);
  const expected=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-xls',source],options));
  const response=JSON.parse(execFileSync(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),['--document-worker'],{...options,input:JSON.stringify({operation:'open',path:source})}));
  assert.equal(response.result,'opened');assert.equal(response.document.inspection.format,'xls');
  assert.deepEqual(response.document.workbook,expected);assert.equal(response.document.workbook.date1904,date1904);
  assert.equal(response.document.workbook_model,undefined);
  assert.deepEqual(readFileSync(source),before);
}
console.log(JSON.stringify({realXlsWorker:true,matchesCli:true,bothDateSystems:true,readOnly:true,sourceUnchanged:true}));
