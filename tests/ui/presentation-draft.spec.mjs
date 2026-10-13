import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
test('presentation drafts serialize writes, flush latest content before close and restore',async({page})=>{
  const writes=[],order=[];let finish,saved;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_presentation_draft'){
      writes.push(structuredClone(args.model));order.push('start');
      if(writes.length===1)await new Promise(resolve=>{finish=resolve;});
      saved=structuredClone(args.model);order.push('end');return;
    }
    if(command==='load_presentation_draft')return saved;
    if(command==='plugin:window|destroy'){order.push('destroy');return;}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('第一版');await page.locator('#pptx-box-italic').check();
  await expect.poll(()=>writes.length).toBe(1);await page.locator('#pptx-box-text').fill('最新内容');
  page.on('dialog',dialog=>dialog.accept());const close=page.evaluate(()=>window.requestNativeClose());
  await expect(page.locator('#pptx-box-text')).toBeDisabled();expect(order).toEqual(['start']);
  finish();await close;expect(order).toEqual(['start','end','start','end','destroy']);
  expect(writes[1].slides[0].text_boxes[0].text).toBe('最新内容');
  await page.reload();await page.locator('#restore-presentation').click();
  await expect(page.locator('#pptx-box-text')).toHaveValue('最新内容');await expect(page.locator('#pptx-box-italic')).toBeChecked();
});
test('final presentation draft failure blocks leaving and permits retry without losing edits',async({page})=>{
  let failing=true,destroyed=false;
  await nativeBridge(page,async(command)=>{
    if(command==='save_presentation_draft'){if(failing)throw new Error('disk full');return;}
    if(command==='plugin:window|destroy'){destroyed=true;return;}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('必须保留');page.on('dialog',dialog=>dialog.accept());
  await page.locator('#new-document').click();await expect(page.getByRole('status')).toContainText('已保留当前文稿');
  await expect(page.locator('#pptx-box-text')).toHaveValue('必须保留');await expect(page.locator('#pptx-box-text')).toBeEnabled();expect(destroyed).toBe(false);
  failing=false;await page.evaluate(()=>window.requestNativeClose());expect(destroyed).toBe(true);
});
