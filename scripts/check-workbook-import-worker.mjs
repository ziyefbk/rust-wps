import {execFileSync} from 'node:child_process';
import {readFileSync,mkdtempSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const source=path.resolve('tests/fixtures/external-basic.xlsx');
const options={encoding:'utf8',windowsHide:true,timeout:35000,maxBuffer:32*1024*1024};
const directory=mkdtempSync(path.resolve('tmp/spreadsheets/import-worker-'));
const dated=path.join(directory,'1904.xlsx');
const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
execFileSync(python,['-c','import openpyxl,sys; from datetime import datetime; b=openpyxl.load_workbook(sys.argv[1]); b.epoch=openpyxl.utils.datetime.CALENDAR_MAC_1904; b.active["E1"]=datetime(2024,1,1); b.save(sys.argv[2])',source,dated],options);
for(const file of [source,dated]) {
  const before=readFileSync(file);
  const cli=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['import-xlsx',file],options));
  const worker=JSON.parse(execFileSync(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),['--document-worker'],{...options,input:JSON.stringify({operation:'import_workbook',path:file})}));
  assert.equal(worker.result,'workbook_imported');
  assert.deepEqual(worker.report,cli);
  assert.ok(worker.report.model);
  assert.equal(worker.report.blockers.length,0);
  assert.equal(worker.report.model.date1904??false,file===dated);
  assert.deepEqual(readFileSync(file),before);
}
console.log(JSON.stringify({realWorkbookImportWorker:true,matchesCli:true,sourceUnchanged:true,bothDateSystems:true}));
