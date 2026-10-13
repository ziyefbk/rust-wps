import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';

test('close flushes the latest text before destroying the window, in serial order', async ({ page }) => {
  const writes = []; const order = []; let resolveFirst;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') {
      order.push('write-start'); writes.push(JSON.stringify(args.doc));
      if (writes.length === 1) await new Promise(resolve => { resolveFirst = resolve; });
      order.push('write-end'); return;
    }
    if (command === 'plugin:window|destroy') { order.push('destroy'); return; }
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('先写入的草稿');
  await expect.poll(() => writes.length).toBe(1);
  await page.keyboard.insertText('最后一次修改');
  page.on('dialog', dialog => dialog.accept());
  const close = page.evaluate(() => window.requestNativeClose());
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'false');
  expect(order).toEqual(['write-start']);
  resolveFirst(); await close;
  expect(writes).toHaveLength(2); expect(writes[1]).toContain('最后一次修改');
  expect(order).toEqual(['write-start', 'write-end', 'write-start', 'write-end', 'destroy']);
});

test('close is cancelled on disk failure and typing remains available', async ({ page }) => {
  let destroyed = false;
  await nativeBridge(page, async command => {
    if (command === 'save_draft') throw new Error('磁盘写入失败');
    if (command === 'plugin:window|destroy') destroyed = true;
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('不可丢失');
  page.on('dialog', dialog => dialog.accept());
  await page.evaluate(() => window.requestNativeClose());
  expect(destroyed).toBe(false);
  await expect(page.getByRole('status')).toContainText('已保留当前文档');
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'true');
  await page.locator('.tiptap').click(); await page.keyboard.insertText('继续编辑');
  await expect(page.locator('.tiptap')).toContainText('继续编辑');
});

test('cancel close leaves content intact and never destroys the window', async ({ page }) => {
  const calls = [];
  await nativeBridge(page, async command => { calls.push(command); });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('取消退出');
  page.on('dialog', dialog => dialog.dismiss());
  await page.evaluate(() => window.requestNativeClose());
  expect(calls).not.toContain('plugin:window|destroy');
  await expect(page.locator('.tiptap')).toContainText('取消退出');
});

test('failed final draft write blocks switching to a new document', async ({ page }) => {
  await nativeBridge(page, async command => { if (command === 'save_draft') throw new Error('磁盘不可用'); });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('原文必须保留');
  page.on('dialog', dialog => dialog.accept());
  await page.locator('#new-document').click();
  await expect(page.getByRole('status')).toContainText('已保留当前文档');
  await expect(page.locator('.tiptap')).toContainText('原文必须保留');
});

test('pending export blocks close until the export has finished', async ({ page }) => {
  let finishExport, destroyed = false;
  await nativeBridge(page, async command => {
    if (command === 'save_new_document') return new Promise(resolve => { finishExport = resolve; });
    if (command === 'plugin:window|destroy') destroyed = true;
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('导出中的文档');
  await page.locator('#edit-save').click();
  await expect.poll(() => !!finishExport).toBe(true);
  await page.evaluate(() => window.requestNativeClose());
  expect(destroyed).toBe(false); await expect(page.getByRole('status')).toContainText('请等待导出完成');
  finishExport({ path: 'new.docx' });
  await expect(page.locator('#draft-state')).toHaveText('DOCX 已导出');
  await page.evaluate(() => window.requestNativeClose()); expect(destroyed).toBe(true);
});

test('native file operation pauses editing and cancellation retains the document', async ({ page }) => {
  let finishOpen, destroyed = false;
  await nativeBridge(page, async command => {
    if (command === 'open_document') return new Promise(resolve => { finishOpen = resolve; });
    if (command === 'plugin:window|destroy') destroyed = true;
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator('.tiptap').click(); await page.keyboard.insertText('打开前的修改');
  page.on('dialog', dialog => dialog.accept());
  await page.locator('#open').click();
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'false');
  await page.evaluate(() => window.requestNativeClose());
  expect(destroyed).toBe(false);
  finishOpen(null);
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('.tiptap')).toContainText('打开前的修改');
});
