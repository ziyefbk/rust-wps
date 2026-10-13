import { test, expect } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

for (const [name, sequence, expected] of [
  ['leading', ['break', 'Second page'], ['', 'Second page']],
  ['consecutive', ['First page', 'break', 'break', 'Third page'], ['First page', '', 'Third page']],
  ['trailing', ['First page', 'break'], ['First page', '']],
]) {
  test(`manual ${name} page breaks preserve blank PDF pages`, async ({ page }) => {
    const doc = { type: 'doc', content: sequence.map(text => text === 'break' ? { type: 'pageBreak' } : { type: 'paragraph', content: [{ type: 'text', text }] }) };
    await nativeBridge(page, async command => {
      if (command === 'open_document') return { name: 'pages.docx', path: 'test.docx', authored: doc, inspection: { format: 'docx', text: 'pages', bytes: 100, warnings: [] } };
      if (command === 'save_draft') return;
    });
    await page.addInitScript(() => { window.print = () => {}; });
    await page.goto('/'); await page.locator('#open').click(); await page.locator('#edit-print').click();
    await expect(page.getByRole('status')).toContainText('已准备 A4 打印副本');
    await page.emulateMedia({ media: 'print' });
    mkdirSync('tmp/pdfs', { recursive: true });
    const output = path.resolve(`tmp/pdfs/manual-${name}.pdf`);
    await page.pdf({ path: output, preferCSSPageSize: true });
    const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
    const pages = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from pypdf import PdfReader; print(json.dumps([p.extract_text().strip() for p in PdfReader(sys.argv[1]).pages]))', output], { encoding: 'utf8' }));
    expect(pages).toEqual(expected);
  });
}

test('inserted manual page break puts short chapters on separate PDF pages', async ({ page }) => {
  await nativeBridge(page, async command => {
    if (command === 'save_draft') return;
  });
  await page.addInitScript(() => { window.print = () => { window.printCalls = (window.printCalls ?? 0) + 1; }; });
  await page.goto('/'); await page.locator('#new-document').click();
  const editor = page.locator('#editor-content .tiptap');
  await editor.click(); await page.keyboard.insertText('First chapter');
  await page.locator('#edit-page-break').click();
  await page.keyboard.insertText('Second chapter');
  await expect(editor.locator('[data-page-break]')).toHaveCount(1);
  await expect(editor).toContainText('Second chapter');
  await page.locator('#edit-print').click();
  await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  await page.emulateMedia({ media: 'print' });
  mkdirSync('tmp/pdfs', { recursive: true });
  const output = path.resolve('tmp/pdfs/manual-page-break.pdf');
  await page.pdf({ path: output, preferCSSPageSize: true, printBackground: true });
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const pages = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from pypdf import PdfReader; print(json.dumps([p.extract_text() for p in PdfReader(sys.argv[1]).pages]))', output], { encoding: 'utf8' }));
  expect(pages).toHaveLength(2);
  expect(pages[0]).toContain('First chapter'); expect(pages[0]).not.toContain('Second chapter');
  expect(pages[1]).toContain('Second chapter'); expect(pages.join('')).not.toContain('手动分页符');
});

test('print snapshot produces multipage A4 PDF with Chinese text and tables', async ({ page }) => {
  const doc = JSON.parse(readFileSync('tests/fixtures/authoring.json', 'utf8'));
  for (let i = 0; i < 70; i++) doc.content.push({ type: 'paragraph', content: [{ type: 'text', text: `打印段落 ${i + 1} 中文内容与分页验证。` }] });
  await nativeBridge(page, async command => {
    if (command === 'open_document') return { name: '打印验证.docx', path: 'test.docx', authored: doc, inspection: { format: 'docx', text: '打印正文', bytes: 1000, warnings: [] } };
    if (command === 'save_draft') return;
  });
  await page.addInitScript(() => { window.print = () => { window.printCalls = (window.printCalls ?? 0) + 1; }; });
  await page.goto('/'); await page.locator('#open').click();
  await page.locator('#edit-print').click();
  await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  await expect(page.getByRole('status')).toContainText('已准备 A4 打印副本');
  // Snapshot remains stable when editing resumes, and printing is not a save.
  await page.locator('.tiptap').first().locator('h1').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 后续修改');
  await expect(page.locator('#print-document')).not.toContainText('后续修改');
  await expect(page.locator('#draft-state')).toContainText('未导出');
  await page.emulateMedia({ media: 'print' });
  await expect(page.locator('.app')).toBeHidden();
  await expect(page.locator('#print-document')).toBeVisible();
  mkdirSync('tmp/pdfs', { recursive: true });
  const output = path.resolve('tmp/pdfs/authoring-print.pdf');
  await page.pdf({ path: output, preferCSSPageSize: true, printBackground: true, displayHeaderFooter: false });
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const report = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from pypdf import PdfReader; r=PdfReader(sys.argv[1]); print(json.dumps({"pages":len(r.pages),"sizes":[[float(p.mediabox.width),float(p.mediabox.height)] for p in r.pages],"text":"\\n".join(p.extract_text() for p in r.pages)},ensure_ascii=True))', output], { encoding: 'utf8' }));
  expect(report.pages).toBeGreaterThan(1);
  for (const [width, height] of report.sizes) { expect(width).toBeCloseTo(595.28, 0); expect(height).toBeCloseTo(841.89, 0); }
  expect(report.text).toContain('Rust 办公文档'); expect(report.text).toContain('打印段落 70');
  expect(report.text).not.toContain('恢复本地草稿'); expect(report.text).not.toContain('后续修改');
  await page.emulateMedia({ media: 'screen' });
  await expect(page.locator('#print-document')).toBeHidden();
});

test('broken embedded image prevents printing and retains an editable document', async ({ page }) => {
  const doc = { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: '正文保留' }] }, { type: 'image', attrs: { src: 'data:image/png;base64,bm90IGEgcG5n', alt: '损坏图片' } }] };
  await nativeBridge(page, async command => {
    if (command === 'open_document') return { name: 'image.docx', path: 'test.docx', authored: doc, inspection: { format: 'docx', text: '正文保留', bytes: 100, warnings: [] } };
    if (command === 'save_draft') return;
  });
  await page.addInitScript(() => { window.print = () => { window.printCalls = (window.printCalls ?? 0) + 1; }; });
  await page.goto('/'); await page.locator('#open').click(); await page.locator('#edit-print').click();
  await expect(page.getByRole('status')).toContainText('打印准备失败');
  expect(await page.evaluate(() => window.printCalls ?? 0)).toBe(0);
  await expect(page.locator('#print-document')).toBeEmpty();
  await page.locator('#editor-content .tiptap p').first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 可以继续编辑');
  await expect(page.locator('#editor-content')).toContainText('可以继续编辑');
});
