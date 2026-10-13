import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { nativeBridge } from './bridge.mjs';
import path from 'node:path';

for (const format of ['docx', 'doc']) {
test(`external ${format} import reviews risks, cancels safely and opens editable copy`, async ({ page }) => {
  const name = `SampleDoc.${format}`;
  const source = path.resolve(`tmp/corpus/${name}`);
  const cli = path.resolve('target/debug/rust-wps.exe');
  const inspection = JSON.parse(execFileSync(cli, ['inspect', source], { encoding: 'utf8' }));
  const report = JSON.parse(execFileSync(cli, [`import-${format}`, source], { encoding: 'utf8' }));
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return { name, path: source, inspection, authored: null };
    if (command === 'import_current') return report;
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.locator('#open').click();
  await page.locator('#import-document').click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.locator('#import-risks')).toContainText(format === 'docx' ? '页面布局、字体和分页需要人工核对' : '旧版 DOC 转换仍属实验功能');
  if (format === 'doc') await expect(page.locator('#import-risks')).toContainText('已保留受支持的正文手动分页');
  else await expect(page.locator('#import-risks')).not.toContainText('转换副本不保留原始分页');
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.locator('#text-content')).toContainText('I am a test document');
  await expect(page.locator('.tiptap')).toHaveCount(0);
  await page.locator('#import-document').click();
  await page.getByRole('button', { name: '接受变化并编辑副本' }).click();
  await expect(page.locator('.tiptap')).toContainText('This is page two');
  await expect(page.locator('#editor-content [data-page-break]')).toHaveCount(1);
  await expect(page.locator('#title')).toHaveText(`${name} · 编辑副本`);
  await expect.poll(() => draft).toBeTruthy();
  await page.locator('.tiptap p').first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 中文修改');
  await expect(page.locator('.tiptap')).toContainText('中文修改');
  await expect(page.locator('#import-document')).toBeDisabled();
});
}

test('failed import retains original reading view', async ({ page }) => {
  await nativeBridge(page, async command => {
    if (command === 'open_document') return { name: 'complex.docx', path: 'complex.docx', inspection: { format: 'docx', text: '原文件内容', bytes: 100, warnings: [] } };
    if (command === 'import_current') throw new Error('Merged table cells require a richer editor model');
  });
  await page.goto('/'); await page.locator('#open').click(); await page.locator('#import-document').click();
  await expect(page.getByRole('status')).toContainText('Merged table cells');
  await expect(page.locator('#text-content')).toHaveText('原文件内容');
  await expect(page.locator('#import-document')).toBeEnabled();
});
