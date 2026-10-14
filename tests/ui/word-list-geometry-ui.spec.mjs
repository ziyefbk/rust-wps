import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';

// Browser-required specifications only. Discovery does not establish actual
// keyboard, clipboard, CSS geometry, WebView2 or native-print acceptance.
const root='#editor-content .tiptap';
const font={fontFamily:'Calibri',fontSize:22};
const frame={leftTwips:1134,rightTwips:1134,hangingTwips:567};
const pageGeometry={widthTwips:12240,heightTwips:15840,marginTopTwips:1134,marginBottomTwips:1134,marginLeftTwips:1701,marginRightTwips:850};
const original=()=>({type:'doc',attrs:{styleProfile:'standard-v2',pageGeometry},content:[{type:'orderedList',attrs:{start:9,markerFont:font},content:['First numbered paragraph with enough text to wrap on a narrow line. '.repeat(6),'Second numbered paragraph'].map(text=>({type:'listItem',content:[{type:'paragraph',attrs:{listGeometry:frame,spaceBefore:0,spaceAfter:160,lineSpacing:259},content:[{type:'text',text}]}]}))},{type:'paragraph',content:[{type:'text',text:'Outside'}]}]});
async function open(page,model=original()){
  let draft,writes=0;await nativeBridge(page,(command,args)=>{if(command==='load_draft')return model;if(command==='save_draft'){draft=structuredClone(args.doc);writes++;return;}throw new Error(command);});
  await page.goto('/');await page.locator('#restore-draft').click();return{draft:()=>draft,writes:()=>writes};
}
async function startItem(page,index=0){await page.locator(root).focus();await page.locator(`${root} li p`).nth(index).evaluate(node=>{window.getSelection().setBaseAndExtent(node.firstChild,0,node.firstChild,0);});}

test('preserved list marker font and first wrapped line frames use physical scoped geometry',async({page})=>{
  await open(page);const paragraph=page.locator(`${root} li p`).first();
  const measured=await paragraph.evaluate(node=>{const p=getComputedStyle(node),marker=getComputedStyle(node,'::before'),range=document.createRange();range.selectNodeContents(node);const lines=[...range.getClientRects()];return{left:p.marginLeft,right:p.marginRight,markerLeft:marker.left,markerWidth:marker.width,markerFont:marker.fontFamily,markerSize:marker.fontSize,markerColor:marker.color,lineLefts:lines.map(r=>r.left),listPadding:getComputedStyle(node.closest('ol')).paddingLeft};});
  expect(parseFloat(measured.left)).toBeCloseTo(75.6,3);expect(parseFloat(measured.right)).toBeCloseTo(75.6,3);expect(parseFloat(measured.markerLeft)).toBeCloseTo(-37.8,3);
  expect(measured.markerWidth).toBe('0px');expect(measured.markerFont).toContain('Calibri');expect(parseFloat(measured.markerSize)).toBeCloseTo(44/3,3);expect(measured.markerColor).toBe('rgb(0, 0, 0)');expect(measured.listPadding).toBe('0px');
  expect(measured.lineLefts.length).toBeGreaterThan(1);for(const left of measured.lineLefts)expect(left).toBeCloseTo(measured.lineLefts[0],3);
});

test('physical Enter inherits geometry and draft with exact undo redo',async({page})=>{
  const saved=await open(page);await startItem(page);await page.keyboard.press('ArrowRight');await page.keyboard.press('Enter');await expect(page.locator(`${root} ol > li`)).toHaveCount(3);
  await expect.poll(()=>saved.draft()?.content[0]?.content.length).toBe(3);for(const item of saved.draft().content[0].content)expect(item.content[0].attrs.listGeometry).toEqual(frame);
  expect(saved.draft().content[0].attrs.markerFont).toEqual(font);await page.locator('#edit-undo').click();await expect(page.locator(`${root} ol > li`)).toHaveCount(2);
  await page.locator('#edit-redo').click();await expect(page.locator(`${root} ol > li`)).toHaveCount(3);
});

test('physical Backspace Tab and list toggles refuse structure without draft history',async({page})=>{
  const saved=await open(page);await startItem(page,1);const before=await page.locator(root).innerHTML();
  for(const key of ['Backspace','Tab']){await page.keyboard.press(key);await expect(page.locator('#notice')).toContainText('编号');expect(await page.locator(root).innerHTML()).toBe(before);}
  await page.locator('#edit-orderedList').click();await expect(page.locator('#notice')).toContainText('编号');await expect(page.locator(`${root} ol > li`)).toHaveCount(2);await expect(page.locator('#edit-undo')).toBeDisabled();await page.waitForTimeout(800);expect(saved.writes()).toBe(0);
});

test('page reset and shrink refuse invalid preserved frames before draft write',async({page})=>{
  const model=original();model.content[0].content.forEach(item=>item.content[0].attrs.listGeometry={leftTwips:8000,rightTwips:180,hangingTwips:360});
  const saved=await open(page,model);await page.locator('#edit-page-reset').click();await expect(page.locator('#notice')).toContainText('编号');
  await page.locator('#edit-page-size').selectOption('A5');await page.locator('#edit-page-apply').click();await expect(page.locator('#notice')).toContainText('编号');await expect(page.locator('#edit-undo')).toBeDisabled();await page.waitForTimeout(800);expect(saved.writes()).toBe(0);
});

test('malformed and wrong-container clipboard geometry is refused before text changes',async({page})=>{
  const saved=await open(page);await startItem(page);const before=await page.locator(root).innerHTML();
  for(const html of ['<p data-word-list-geometry="null">BAD</p>','<span data-word-list-marker-font="{}">BAD</span>']){
    await page.locator(root).evaluate((node,html)=>{const data=new DataTransfer();data.setData('text/html',html);data.setData('text/plain','BAD');node.dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));},html);
    await expect(page.locator('#notice')).toContainText('编号');expect(await page.locator(root).innerHTML()).toBe(before);
  }
  await expect(page.locator('#edit-undo')).toBeDisabled();await page.waitForTimeout(800);expect(saved.writes()).toBe(0);
});

test('printing captures counter marker font and frame before later text edit',async({page})=>{
  await page.addInitScript(()=>{window.print=()=>{};});await open(page);await page.locator('#edit-print').click();const printed=page.locator('#print-document ol');
  await expect(printed).toHaveAttribute('data-word-list-marker-font',JSON.stringify(font));const before=await printed.innerHTML();
  await startItem(page);await page.keyboard.type('Changed ');expect(await printed.innerHTML()).toBe(before);
  await page.emulateMedia({media:'print'});expect(await printed.locator('p').first().evaluate(node=>parseFloat(getComputedStyle(node,'::before').left))).toBeCloseTo(-37.8,3);
});
