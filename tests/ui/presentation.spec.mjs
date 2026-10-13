import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
import {mkdirSync} from 'node:fs';
for (const outcome of ['success','cancel','error']) test(`native close waits for PPTX replacement ${outcome} and remains retryable`,async({page})=>{
  const source=path.resolve('tests/fixtures/basic-slides.pptx');
  const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',source],{encoding:'utf8',windowsHide:true}));
  let started=false, destroyed=false, finish;
  await nativeBridge(page,async(command)=>{
    if(command==='open_document')return {name:'slides.pptx',path:source,inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation};
    if(command==='replace_presentation_current'){
      started=true;await new Promise(resolve=>{finish=resolve;});
      if(outcome==='error')throw new Error('save failed');
      return outcome==='cancel'?null:{path:'saved.pptx',report:{replacements:1},presentation};
    }
    if(command==='plugin:window|destroy'){destroyed=true;return;}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await page.locator('#pptx-find').fill('简文');await page.locator('#pptx-replacement').fill('修改');await page.locator('#pptx-replace-save').click();
  await expect.poll(()=>started).toBe(true);
  await page.evaluate(()=>window.requestNativeClose());
  expect(destroyed).toBe(false);await expect(page.getByRole('status')).toContainText('当前操作尚未完成');
  await expect(page.locator('#pptx-replace-save')).toBeDisabled();await expect(page.locator('#slide-picker')).toBeDisabled();
  finish();await expect(page.locator('#pptx-replace-save')).toBeEnabled();
  await page.evaluate(()=>window.requestNativeClose());expect(destroyed).toBe(true);
});
test('PPTX replacement cancellation and failure retain page; success previews actual saved copy',async({page},info)=>{
  const source=path.resolve('tests/fixtures/basic-slides.pptx');
  const exe=path.resolve('target/debug/rust-wps.exe');
  const options={encoding:'utf8',windowsHide:true};
  const presentation=JSON.parse(execFileSync(exe,['inspect-pptx',source],options));
  let calls=0;
  await nativeBridge(page,async(command,args)=>{
    if(command==='open_document')return {name:'slides.pptx',path:source,inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation};
    if(command==='replace_presentation_current'){
      calls++;if(calls===1)return null;if(calls===2)throw new Error('output exists');
      const output=info.outputPath('edited.pptx');mkdirSync(path.dirname(output),{recursive:true});
      const report=JSON.parse(execFileSync(exe,['replace-pptx',source,output,args.find,args.replacement],options));
      return {path:output,report,presentation:JSON.parse(execFileSync(exe,['inspect-pptx',output],options))};
    }
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();await page.locator('#slide-picker').selectOption('1');
  await page.locator('#pptx-find').fill('Hidden slide');await page.locator('#pptx-replacement').fill('Saved slide');
  await page.locator('#pptx-replace-save').click();await expect(page.getByRole('status')).toContainText('已取消');
  await expect(page.locator('#slide-text')).toContainText('Hidden slide');
  await page.locator('#pptx-replace-save').click();await expect(page.getByRole('status')).toContainText('output exists');
  await expect(page.locator('#slide-text')).toContainText('Hidden slide');
  await page.locator('#pptx-replace-save').click();await expect(page.getByRole('status')).toContainText('当前显示已保存副本');
  await expect(page.locator('#slide-text')).toContainText('Saved slide');await expect(page.locator('#title')).toHaveText('edited.pptx');
  await expect(page.locator('#slide-picker')).toHaveValue('1');
});
test('PPTX navigates real ordered content, retains it after failure and clears on new document',async({page})=>{
  const source=path.resolve('tests/fixtures/basic-slides.pptx');
  const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',source],{encoding:'utf8',windowsHide:true}));
  let count=0;
  await nativeBridge(page,async(command)=>{
    if(command==='open_document'){
      count++;
      if(count===2) throw new Error('bad presentation');
      if(count===3)return {name:'plain.docx',path:'plain.docx',inspection:{format:'docx',text:'Other document',bytes:10,warnings:[]}};
      return {name:'slides.pptx',path:source,inspection:{format:'pptx',text:'',bytes:100,warnings:[]},presentation};
    }
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#format')).toHaveText('PPTX');
  await expect(page.locator('#slide-text')).toContainText('简文 PPTX <test>');
  await expect(page.locator('#slide-objects')).toContainText('group');
  await expect(page.locator('#slide-canvas')).toHaveAttribute('viewBox','0 0 1152 648');
  await expect(page.locator('#slide-canvas g').first().locator('rect')).toHaveAttribute('x','96');
  await expect(page.locator('#slide-canvas g').first().locator('rect')).toHaveAttribute('y','192');
  await expect(page.locator('#slide-canvas text').first()).toHaveText('简文 PPTX <test>');
  const run=page.locator('#slide-canvas text').first().locator('tspan tspan').first();
  await expect(run).toHaveAttribute('font-size','32');await expect(run).toHaveAttribute('font-weight','bold');
  await expect(run).toHaveAttribute('font-style','italic');await expect(run).toHaveAttribute('fill','rgb(160,32,64)');
  await expect(page.locator('#slide-preview-info')).toContainText('组合内容');
  await expect(page.locator('#slide-canvas image')).toHaveCount(1);
  await expect(page.locator('#slide-canvas image')).toHaveAttribute('x','672');
  await expect(page.locator('#slide-canvas image')).toHaveAttribute('width','288');
  await expect(page.locator('#slide-canvas image')).toHaveAttribute('href',/^data:image\/png;base64,/);
  expect(await page.locator('#slide-canvas image').evaluate(async node=>{
    const img=new Image();img.src=node.getAttribute('href');await img.decode();return img.naturalWidth;
  })).toBe(32);
  await expect(page.locator('#slide-canvas text').first()).toHaveAttribute('clip-path',/^url\(#slide-preview-\d+-clip-0\)$/);
  expect((await page.locator('#slide-canvas text').first().boundingBox()).height).toBeGreaterThan(8);
  await page.locator('#presentation').screenshot({path:'test-results/pptx-canvas.png'});
  await expect(page.locator('#presentation script')).toHaveCount(0);
  await page.locator('#slide-picker').selectOption('1');
  await expect(page.locator('#slide-text')).toContainText('Hidden slide');
  await expect(page.locator('#slide-canvas text').first()).toHaveText('Hidden slide');
  await expect(page.locator('#slide-status')).toContainText('隐藏页');
  await expect(page.locator('#pptx-import')).toBeEnabled();
  await page.locator('#open').click();
  await expect(page.locator('#slide-text')).toContainText('Hidden slide');
  await page.locator('#open').click();
  await expect(page.locator('#presentation')).toBeHidden();
  await expect(page.locator('#text-content')).toContainText('Other document');
});

test('picture preview refuses external and active data sources',async({page})=>{
  const seen=[];page.on('request',request=>{if(request.url().includes('example.invalid'))seen.push(request.url());});
  await nativeBridge(page,async()=>({name:'images.pptx',path:'images.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:{size_emu:[10972800,6172200],unreadable_parts:[],slides:[{id:256,name:'',hidden:false,text:'',parse_error:null,shapes:['https://example.invalid/image.png','data:image/svg+xml;base64,PHN2Zy8+'].map((image,i)=>({id:i,name:'unsupported',kind:'picture',bounds_emu:[0,0,914400,914400],children:[],image,image_notice:'Unsupported image'}))}]}}));
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#slide-canvas image')).toHaveCount(0);
  await expect(page.locator('#slide-objects')).toContainText('Unsupported image');
  expect(seen).toEqual([]);
});

test('position preview rejects unknown dimensions, skips invalid bounds and bounds visible shapes',async({page})=>{
  let count=0;
  const shape={id:1,name:'<img src=x onerror=alert(1)>',kind:'shape',text:'<script>unsafe</script>',bounds_emu:[0,0,914400,914400],children:[]};
  await nativeBridge(page,async(command)=>{
    if(command!=='open_document')throw new Error(command);
    count++;
    return {name:'test.pptx',path:'test.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:{size_emu:count===1?null:[10972800,6172200],unreadable_parts:[],slides:[{id:256,name:'',hidden:false,text:'source',parse_error:null,shapes:[{...shape,bounds_emu:[0,0,-1,1]},...Array.from({length:599},(_,i)=>({...shape,id:i+2}))]}]}};
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#slide-canvas')).toHaveCount(0);await expect(page.locator('#slide-preview-info')).toContainText('尺寸缺失或异常');
  await page.locator('#open').click();await expect(page.locator('#slide-canvas g')).toHaveCount(499);
  await expect(page.locator('#slide-preview-info')).toContainText('101 个形状未绘制');
  await expect(page.locator('#presentation img,#presentation script')).toHaveCount(0);
  await expect(page.locator('#slide-canvas text').first()).toHaveText('<script>unsafe</script>');
  await expect(page.locator('#slide-canvas text').first().locator('tspan tspan').first()).toHaveAttribute('font-size','24');
  await expect(page.locator('#slide-canvas text').first().locator('tspan tspan').first()).toHaveAttribute('font-weight','normal');
});
