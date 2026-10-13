import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {readFileSync} from 'node:fs';
const shape=(text,index=0)=>({id:index+1,name:text,kind:'shape',bounds_emu:[index*914400,914400,3657600,914400],text,runs:[{text,font_size_hundredths_pt:2400}],children:[]});
const report=()=>({size_emu:[10972800,6172200],unreadable_parts:[],slides:['首页','隐藏','末页'].map((text,i)=>({id:i+256,name:text,hidden:i===1,text,parse_error:null,shapes:[shape(text,i)]}))});
test('slideshow skips hidden slides, navigates bounded pages, contains shortcuts and restores focus',async({page})=>{
  let opened=0;await nativeBridge(page,async(command)=>{if(command==='open_document'){opened++;return {name:'show.pptx',path:'show.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:report()};}throw new Error(command);});
  await page.goto('/');await page.locator('#open').click();await page.locator('#pptx-play').click();
  await expect(page.getByRole('dialog',{name:'幻灯片放映'})).toBeVisible();await expect(page.locator('#pptx-show-status')).toContainText('1 / 2 · 原第 1 页');await expect(page.locator('#pptx-show-prev')).toBeDisabled();
  await page.keyboard.press('ArrowRight');await expect(page.locator('#pptx-show-status')).toContainText('2 / 2 · 原第 3 页');await expect(page.locator('#pptx-show-canvas')).toContainText('末页');await expect(page.locator('#pptx-show-next')).toBeDisabled();
  const ids=await page.locator('clipPath').evaluateAll(elements=>elements.map(e=>e.id));expect(new Set(ids).size).toBe(ids.length);
  await page.keyboard.press('Home');await expect(page.locator('#pptx-show-status')).toContainText('原第 1 页');await page.keyboard.press('End');await page.keyboard.press('Control+o');expect(opened).toBe(1);
  await page.keyboard.press('PageUp');await page.locator('#pptx-show-stage').click();await expect(page.locator('#pptx-show-status')).toContainText('原第 3 页');
  await page.keyboard.press('Escape');await expect(page.locator('#presentation-show')).toHaveCount(0);await expect(page.locator('#pptx-play')).toBeFocused();await expect(page.locator('#slide-picker')).toHaveValue('0');
  await page.locator('#slide-picker').selectOption('2');await page.locator('#pptx-play').click();await expect(page.locator('#pptx-show-status')).toContainText('原第 3 页');
  await page.evaluate(()=>document.getElementById('open').click());await expect.poll(()=>opened).toBe(2);await expect(page.locator('#presentation-show')).toHaveCount(0);
  await page.locator('#pptx-play').click();await expect(page.locator('#presentation-show')).toHaveCount(1);await page.locator('#pptx-show-exit').click();
});
test('slideshow uses latest authored text and pictures without changing draft or undo state',async({page},info)=>{
  const model=JSON.parse(readFileSync('docs/examples/presentation-v1.json','utf8'));
  model.slides[0].images=[{bounds_emu:[0,0,1828800,914400],data_url:'data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64'),alt:'图片'}];
  let writes=0;await nativeBridge(page,async(command)=>{if(command==='load_presentation_draft')return model;if(command==='save_presentation_draft'){writes++;return;}throw new Error(command);});
  await page.goto('/');await page.locator('#restore-presentation').click();await page.locator('#pptx-box-text').fill('刚刚修改的文字');await expect(page.locator('#presentation-edit-state')).toContainText('本地草稿已保存');const before=writes;
  await page.locator('#pptx-play').click();await expect(page.locator('#pptx-show-canvas')).toContainText('刚刚修改的文字');await expect(page.locator('#pptx-show-canvas image')).toHaveCount(1);
  await expect(page.locator('#pptx-show-canvas g rect').first()).toHaveAttribute('stroke','none');
  const box=await page.locator('#pptx-show-canvas').boundingBox();expect(box.width/box.height).toBeCloseTo(model.size_emu[0]/model.size_emu[1],2);expect(box.height).toBeLessThan(820);
  await page.screenshot({path:info.outputPath('slideshow.png')});await page.locator('#pptx-show-exit').click();expect(writes).toBe(before);
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).toHaveValue(model.slides[0].text_boxes[0].text);
});
test('slideshow reports render failure, falls back to text, and handles fullscreen rejection',async({page})=>{
  const broken=report();broken.slides[0].parse_error='missing slide';broken.slides[0].text='<script>这是安全文本</script>';
  await nativeBridge(page,async()=>({name:'broken.pptx',path:'broken.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:broken}));
  await page.goto('/');await page.locator('#open').click();await page.evaluate(()=>Element.prototype.requestFullscreen=()=>Promise.reject(new Error('unavailable')));await page.locator('#pptx-play').click();
  await expect(page.locator('#pptx-show-info')).toContainText('读取失败');await expect(page.locator('#pptx-show-stage pre')).toHaveText('<script>这是安全文本</script>');await expect(page.locator('#pptx-show-stage script')).toHaveCount(0);
  await page.locator('#pptx-show-fullscreen').click();await expect(page.locator('#pptx-show-feedback')).toContainText('无法进入系统全屏');await page.locator('#pptx-show-exit').click();await page.locator('#new-document').click();await expect(page.locator('#presentation-show')).toHaveCount(0);
});
test('all-hidden presentation disables playback',async({page})=>{
  const hidden=report();hidden.slides.forEach(s=>s.hidden=true);await nativeBridge(page,async()=>({name:'hidden.pptx',path:'hidden.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:hidden}));
  await page.goto('/');await page.locator('#open').click();await expect(page.locator('#pptx-play')).toBeDisabled();
});
test('real browser fullscreen can enter and exit without losing the playing slide',async({page})=>{
  await nativeBridge(page,async()=>({name:'full.pptx',path:'full.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:report()}));
  await page.goto('/');await page.locator('#open').click();await page.locator('#slide-picker').selectOption('2');await page.locator('#pptx-play').click();
  await page.locator('#pptx-show-fullscreen').click();await expect.poll(()=>page.evaluate(()=>document.fullscreenElement?.id)).toBe('presentation-show-surface');
  await page.locator('#pptx-show-fullscreen').click();await expect.poll(()=>page.evaluate(()=>document.fullscreenElement===null)).toBe(true);await expect(page.locator('#pptx-show-status')).toContainText('原第 3 页');await page.keyboard.press('Escape');await expect(page.locator('#presentation-show')).toHaveCount(0);
});
