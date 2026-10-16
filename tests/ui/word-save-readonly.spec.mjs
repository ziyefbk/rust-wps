// Real Chromium/TipTap frontend regression with a simulated host only.
// Direct module calls below are limited to stale-session/other-lock unit probes.
import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';

async function pendingSave(page) {
  let resolveSave, rejectSave, calls = 0, destroyed = false;
  await nativeBridge(page, async command => {
    if (command === 'save_new_document') {
      calls++;
      return new Promise((resolve, reject) => { resolveSave = resolve; rejectSave = reject; });
    }
    if (command === 'plugin:window|destroy') { destroyed = true; return; }
    if (command === 'save_draft') return;
    throw new Error(`Unexpected host command ${command}`);
  });
  page.on('dialog', async dialog => { await dialog.dismiss(); throw new Error(`Unexpected dialog: ${dialog.message()}`); });
  await page.goto('/'); await page.locator('#new-document').click();
  const editor = page.locator('.tiptap');
  await editor.click(); await page.keyboard.insertText('必须保留的中文');
  await page.locator('#edit-save').click();
  await expect.poll(() => !!resolveSave).toBe(true);
  return { editor, resolve: value => resolveSave(value), reject: error => rejectSave(error), calls: () => calls, destroyed: () => destroyed };
}

for (const outcome of ['success', 'cancel', 'error']) test(`Save ${outcome} visibly locks editor and controls, then restores editing`, async ({ page }) => {
  const pending = await pendingSave(page);
  await expect(pending.editor).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator('#notice')).toContainText('编辑区暂时只读');
  for (const id of ['edit-save','edit-bold','edit-undo','edit-page-break','edit-style','edit-align']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await pending.editor.click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText('被拒绝的输入');
  await expect(pending.editor).toHaveText('必须保留的中文');
  await page.keyboard.press('Control+s'); await page.locator('#new-document').click();
  await expect(pending.editor).toHaveAttribute('contenteditable', 'false');
  await page.locator('#open').click();
  await expect(pending.editor).toHaveAttribute('contenteditable', 'false');
  await page.evaluate(() => window.requestNativeClose());
  expect(pending.calls()).toBe(1); expect(pending.destroyed()).toBe(false);
  await expect(page.locator('#notice')).toContainText('请等待导出完成');
  if (outcome === 'error') pending.reject(new Error('模拟磁盘失败'));
  else pending.resolve(outcome === 'cancel' ? null : { path: 'saved.docx' });
  await expect(pending.editor).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#edit-bold')).toBeEnabled();
  await expect(page.locator('#notice')).toContainText(outcome === 'success' ? '已导出 DOCX' : outcome === 'cancel' ? '已取消 DOCX 导出' : 'DOCX 导出失败');
  await pending.editor.click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText('恢复编辑');
  await expect(pending.editor).toHaveText('必须保留的中文恢复编辑');
});

test('A completed unrelated operation cannot unpause a pending Save', async ({ page }) => {
  const pending = await pendingSave(page);
  await page.evaluate(async () => {
    const authoring = await import('/authoring.ts');
    authoring.pauseAuthoring(true); authoring.pauseAuthoring(false);
  });
  await expect(pending.editor).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator('#edit-bold')).toBeDisabled();
  pending.resolve(null);
  await expect(pending.editor).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#notice')).toContainText('已取消 DOCX 导出');
});

test('Save completion retains a separate active pause lock', async ({ page }) => {
  const pending = await pendingSave(page);
  await page.evaluate(async () => { (await import('/authoring.ts')).pauseAuthoring(true); });
  pending.resolve({ path: 'saved.docx' });
  await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  await expect(pending.editor).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator('#edit-bold')).toBeDisabled();
  await page.evaluate(async () => { (await import('/authoring.ts')).pauseAuthoring(false); });
  await expect(pending.editor).toHaveAttribute('contenteditable', 'true');
});

for (const outcome of ['success','cancel','error']) test(`Stale Save ${outcome} cannot unlock or replace the notice of a newer paused session`, async ({ page }) => {
  const pending = await pendingSave(page);
  await page.evaluate(async () => {
    const authoring = await import('/authoring.ts');
    await authoring.openAuthored({type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'新会话内容'}]}]}, '新会话', 'new-session.docx');
    authoring.pauseAuthoring(true);
  });
  const notice = await page.locator('#notice').innerText();
  if (outcome === 'error') pending.reject(new Error('旧会话导出失败'));
  else pending.resolve(outcome === 'cancel' ? null : { path: 'old-session.docx' });
  // The next bridge turn ensures the pending invoke/finally has completed.
  await page.waitForTimeout(100);
  await expect(page.locator('.tiptap')).toHaveText('新会话内容');
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator('#notice')).toHaveText(notice);
  await expect(page.locator('#draft-state')).toHaveText('文档已打开');
  await page.evaluate(async () => { (await import('/authoring.ts')).pauseAuthoring(false); });
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'true');
});
