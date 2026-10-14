// Browser acceptance specifications. The IPC stub verifies actual UI payloads,
// not Rust file generation or a native WebView. Model checks are not UI passes.
import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const root = '#editor-content .tiptap';
const metadata = 'data-break-paragraph', splitMetadata = 'data-split-page-break-and-paragraph-mark';
const metrics = {font: {family: 'Calibri', halfPoints: 22}, spaceBefore: 0, spaceAfter: 200, lineSpacing: 276};
const p = text => ({type: 'paragraph', ...(text ? {content: [{type: 'text', text}]} : {})});
const document = () => ({type: 'doc', attrs: {styleProfile: 'standard-v2', splitPageBreakAndParagraphMark: true},
  content: [p('Before'), p(), {type: 'pageBreak', attrs: {breakParagraph: metrics}}, p(), p('After')]});
const file = authored => ({name: 'break-paragraph.docx', path: 'break-paragraph.docx', authored,
  inspection: {format: 'docx', text: 'Before After', bytes: 100, warnings: []}});

test('authoring load edit draft export payload and reopen keep source metrics and root while new bare breaks remain usable', async ({page}) => {
  let current = document(), draft, exported;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return file(current);
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    if (command === 'save_new_document') {exported = structuredClone(args.doc); current = exported; return {path: 'break-paragraph.docx'};}
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator(root)).toHaveAttribute(splitMetadata, 'true');
  await expect(page.locator(`${root} [${metadata}]`)).toHaveAttribute(metadata, JSON.stringify(metrics));
  await expect(page.locator('#notice')).toContainText('打印和 PDF 暂不复现');
  await page.locator(`${root} > p`).first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' edited');
  await expect.poll(() => draft?.content[0].content[0].text).toBe('Before edited');
  expect(draft.attrs.splitPageBreakAndParagraphMark).toBe(true);
  expect(draft.content[2]).toEqual({type: 'pageBreak', attrs: {breakParagraph: metrics}});
  expect(draft.content[1].content).toBeUndefined(); expect(draft.content[3].content).toBeUndefined();
  await page.locator(`${root} > p`).last().click(); await page.keyboard.press('End'); await page.locator('#edit-page-break').click();
  await expect(page.locator(`${root} [data-page-break]`)).toHaveCount(2);
  await expect.poll(() => draft?.content.filter(node => node.type === 'pageBreak').length).toBe(2);
  expect(draft.content.filter(node => node.type === 'pageBreak')[1]).toEqual({type: 'pageBreak'});
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} [data-page-break]`)).toHaveCount(1);
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(exported.attrs.splitPageBreakAndParagraphMark).toBe(true); expect(exported.content[2].attrs.breakParagraph).toEqual(metrics);
  await page.locator('#open').click(); await expect(page.locator(`${root} > p`).first()).toHaveText('Before edited');
  await expect(page.locator(`${root} [${metadata}]`)).toHaveAttribute(metadata, JSON.stringify(metrics));
});

test('real print clone freezes metadata and warns before PDF export while cancellation leaves authoring intact', async ({page}) => {
  let noticeAtExport, releaseExport;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return document();
    if (command === 'save_draft') return;
    if (command === 'export_current_pdf') {await new Promise(resolve => {releaseExport = resolve;}); return null;}
    throw new Error(command);
  });
  await page.addInitScript(() => {window.print = () => {window.printCalls = (window.printCalls ?? 0) + 1;};});
  await page.goto('/'); await page.locator('#restore-draft').click();
  await expect(page.locator('#notice')).toContainText('打印和 PDF 暂不复现');
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  await expect(page.locator('#print-document .tiptap')).toHaveAttribute(splitMetadata, 'true');
  const snapshot = await page.locator('#print-document').innerHTML();
  await page.locator(`${root} > p`).first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' later');
  expect(await page.locator('#print-document').innerHTML()).toBe(snapshot);
  const original = await page.locator(root).innerHTML(); await page.locator('#edit-export-pdf').click();
  await expect.poll(() => !!releaseExport).toBe(true); noticeAtExport = await page.locator('#notice').textContent();
  expect(noticeAtExport).toContain('打印和 PDF 暂不复现');
  await expect(page.locator(root)).toHaveAttribute('contenteditable', 'false'); releaseExport();
  await expect(page.locator('#notice')).toContainText('已取消 PDF 导出');
  await expect(page.locator(root)).toHaveAttribute('contenteditable', 'true');
  expect(await page.locator(root).innerHTML()).toBe(original); await expect(page.locator('#print-document')).toBeEmpty();
});

test('invalid raw root or partial break metrics refuse restore before replacing the current document', async ({page}) => {
  let candidate;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return candidate;
    if (command === 'save_draft') return;
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#new-document').click(); await page.locator(root).click(); await page.keyboard.insertText('Keep this');
  const original = await page.locator(root).innerHTML();
  for (const mutate of [value => {value.attrs.splitPageBreakAndParagraphMark = false;}, value => {value.attrs.splitPageBreakAndParagraphMark = null;},
    value => {delete value.content[2].attrs.breakParagraph.font;}, value => {value.content[2].attrs.breakParagraph = null;}]) {
    candidate = structuredClone(document()); mutate(candidate);
    await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText('草稿恢复失败');
    expect(await page.locator(root).innerHTML()).toBe(original); await expect(page.locator(root)).toHaveAttribute('contenteditable', 'true');
  }
});
