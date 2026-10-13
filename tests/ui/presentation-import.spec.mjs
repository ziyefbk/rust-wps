import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import path from 'node:path';
const options={encoding:'utf8',windowsHide:true},exe=path.resolve('target/debug/rust-wps.exe');
for(const kind of ['authored','poi'])test(`PPTX ${kind} import reviews actual audit, cancels, then edits and exports a new copy`,async({page},info)=>{
 const source=kind==='poi'?path.resolve('tests/fixtures/apache-poi/sample.pptx'):info.outputPath('external.pptx'),original=info.outputPath('original.pptx');mkdirSync(path.dirname(original),{recursive:true});
 if(kind==='authored'){
 execFileSync(exe,['create-pptx',path.resolve('docs/examples/presentation-v1.json'),original],options);
 const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
 execFileSync(python,['-c',`import zipfile,sys
with zipfile.ZipFile(sys.argv[1]) as a,zipfile.ZipFile(sys.argv[2],'w') as b:
 for n in a.namelist():
  if n.startswith('rust-wps/'):continue
  data=a.read(n)
  if n=='[Content_Types].xml':data=data.replace(b'<Override PartName="/rust-wps/presentation-v1.json" ContentType="application/json"/>',b'')
  b.writestr(n,data)`,original,source],options);
 }
 const before=readFileSync(source),presentation=JSON.parse(execFileSync(exe,['inspect-pptx',source],options)),audit=JSON.parse(execFileSync(exe,['import-pptx',source],options));let draft,saved;
 expect(audit.blockers).toEqual([]);
 await nativeBridge(page,async(command,args)=>{
  if(command==='open_document')return {name:'external.pptx',path:source,inspection:{format:'pptx',text:'',bytes:before.length,warnings:[]},presentation};
  if(command==='import_presentation_current')return audit;
  if(command==='save_presentation_draft'){draft=args.model;return;}
  if(command==='save_new_presentation'){const json=info.outputPath('edited.json'),output=info.outputPath('edited.pptx');writeFileSync(json,JSON.stringify(args.model));execFileSync(exe,['create-pptx',json,output],options);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));return {path:output};}
  throw new Error(command);
 });
 await page.goto('/');await page.locator('#open').click();await page.locator('#pptx-import').click();
 if(kind==='poi'){await expect(page.locator('#import-risks')).toContainText('4 个对象');await expect(page.locator('#import-risks')).toContainText('段落边界与软换行');}
 await expect(page.locator('#import-risks')).toContainText('叠放顺序');await page.locator('#import-review button[value="cancel"]').click();await expect(page.locator('#presentation-editor')).toHaveCount(0);
 await page.locator('#pptx-import').click();await page.locator('#import-review button[value="import"]').click();
 await expect(page.locator('#pptx-box-text')).toHaveValue(audit.model.slides[0].text_boxes[0].text);await expect(page.locator('#import-document')).toBeDisabled();
 await page.locator('#pptx-box-text').fill('导入后编辑');await expect.poll(()=>draft?.slides[0].text_boxes[0].text).toBe('导入后编辑');
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');const expected=structuredClone(audit.model);expected.slides[0].text_boxes[0].text='导入后编辑';expect(saved).toEqual(expected);expect(readFileSync(source)).toEqual(before);
});
for(const failure of ['blocked','error'])test(`PPTX import ${failure} preserves reading view`,async({page})=>{
 const source=path.resolve('tests/fixtures/basic-slides.pptx'),presentation=JSON.parse(execFileSync(exe,['inspect-pptx',source],options));
 const audit=JSON.parse(execFileSync(exe,['import-pptx',source],options));
 await nativeBridge(page,async command=>{if(command==='open_document')return {name:'source.pptx',path:source,inspection:{format:'pptx',text:'',bytes:100,warnings:[]},presentation};if(command==='import_presentation_current'){if(failure==='error')throw new Error('审计失败');return audit;}throw new Error(command);});
 await page.goto('/');await page.locator('#open').click();await page.locator('#slide-picker').selectOption('1');await page.locator('#pptx-import').click();
 if(failure==='blocked'){await expect(page.locator('#import-review button[value="import"]')).toBeDisabled();await expect(page.locator('#import-risks')).toContainText('不能转换');await page.locator('#import-review button[value="cancel"]').click();}else await expect(page.getByRole('status')).toContainText('审计失败');
 await expect(page.locator('#slide-picker')).toHaveValue('1');await expect(page.locator('#presentation-editor')).toHaveCount(0);await expect(page.locator('#import-document')).toBeEnabled();
});
