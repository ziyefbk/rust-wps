import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const size = halfPoints => ({ type: 'fontSize', attrs: { halfPoints } });
const fontSize = node => node.marks?.find(mark => mark.type === 'fontSize')?.attrs.halfPoints;
const model = { type: 'doc', content: [{ type: 'paragraph', content: [
  { type: 'text', text: '甲乙', marks: [{ type: 'bold' }, size(24)] },
  { type: 'text', text: '丙丁', marks: [{ type: 'italic' }, size(32)] },
] }] };

async function selectAndApply(page, from, to, value) {
  await page.evaluate(({ from, to, value }) => {
    const root = document.querySelector('.tiptap p');
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const positions = []; let offset = 0, node;
    while ((node = walker.nextNode())) {
      positions.push({ node, start: offset, end: offset + node.textContent.length });
      offset += node.textContent.length;
    }
    const anchor = positions.find(item => item.end >= from);
    const head = positions.find(item => item.end >= to);
    window.getSelection().setBaseAndExtent(anchor.node, from - anchor.start, head.node, to - head.start);
    document.getElementById('edit-font-size').value = value;
    document.getElementById('edit-font-size-apply').click();
  }, { from, to, value });
}

test('half-point size changes the pending partial selection, preserves styles and exports real DOCX', async ({ page }, info) => {
  let draft, imported, independent;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return model;
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const file = info.outputPath('sizes.docx'), json = info.outputPath('sizes.json');
      mkdirSync(path.dirname(file), { recursive: true }); writeFileSync(json, JSON.stringify(args.doc));
      const cli = path.resolve('target/debug/rust-wps.exe');
      const report = JSON.parse(execFileSync(cli, ['create-docx', json, file], { encoding: 'utf8', windowsHide: true }));
      imported = JSON.parse(execFileSync(cli, ['import-docx', file], { encoding: 'utf8', windowsHide: true })).document;
      const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
      independent = JSON.parse(execFileSync(python, ['-c', 'import sys,json;from docx import Document;from docx.oxml.ns import qn;print(json.dumps([[r.text,r.font.size.pt,r._r.rPr.find(qn("w:szCs")).get(qn("w:val"))] for r in Document(sys.argv[1]).paragraphs[0].runs],ensure_ascii=True))', file], { encoding: 'utf8', windowsHide: true }));
      return { path: file, report };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await expect(page.locator('#edit-font-size')).toHaveValue('');
  await expect(page.locator('#edit-font-size')).toHaveAttribute('placeholder', /混合/);
  await selectAndApply(page, 1, 3, '13.5');
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('乙丙');
  await expect(page.locator('.tiptap strong')).toHaveText(['甲', '乙']);
  await expect(page.locator('.tiptap em')).toHaveText(['丙', '丁']);
  await expect(page.locator('#edit-font-size')).toHaveValue('13.5');
  await expect.poll(() => draft?.content[0]?.content.map(n => [n.text, fontSize(n)])).toEqual([['甲', 24], ['乙', 27], ['丙', 27], ['丁', 32]]);
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveCount(0);
  await page.locator('#edit-redo').click(); await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('乙丙');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(independent).toEqual([['甲', 12, '24'], ['乙', 13.5, '27'], ['丙', 13.5, '27'], ['丁', 16, '32']]);
  expect(imported.content[0].content.map(n => [n.text, fontSize(n)])).toEqual([['甲', 24], ['乙', 27], ['丙', 27], ['丁', 32]]);
});

test('caret size affects future input, reset and clear preserve text and link with independent undo', async ({ page }) => {
  let draft;
  const original = { type: 'doc', content: [{ type: 'paragraph', content: [
    { type: 'text', text: '保留', marks: [{ type: 'bold' }, { type: 'link', attrs: { href: 'https://example.com/' } }, size(24)] },
  ] }] };
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 2, 2, '18.5'); await page.keyboard.insertText('新');
  await expect(page.locator('.tiptap [data-font-size="37"]')).toHaveText('新');
  await page.locator('#edit-font-size-reset').click(); await page.keyboard.insertText('默认');
  await expect.poll(() => draft?.content[0]?.content.map(n => [n.text, fontSize(n)])).toEqual([['保留', 24], ['新', 37], ['默认', undefined]]);
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-clear-marks').click();
  await expect(page.locator('.tiptap [data-font-size]')).toHaveCount(0);
  await expect(page.locator('.tiptap strong')).toHaveCount(0);
  await expect(page.locator('.tiptap a')).toHaveAttribute('href', 'https://example.com/');
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-font-size="37"]')).toHaveText('新');
  await expect(page.locator('.tiptap')).toHaveText('保留新默认');
});

test('invalid and locked size actions do not alter the draft or history', async ({ page }) => {
  let draft, writes = 0, finishSave;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return model;
    if (command === 'save_draft') { draft = args.doc; writes++; return; }
    if (command === 'save_new_document') return new Promise(resolve => { finishSave = resolve; });
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  for (const value of ['0', '1.1', '1638.5', '13.5000000000000001', 'NaN']) {
    await selectAndApply(page, 0, 4, value);
    await expect(page.locator('.tiptap [data-font-size="24"]')).toHaveText('甲乙');
    await expect(page.locator('.tiptap [data-font-size="32"]')).toHaveText('丙丁');
    await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.waitForTimeout(800); expect(writes).toBe(0); expect(draft).toBeUndefined();
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  await expect(page.locator('#edit-font-size')).toBeDisabled();
  await page.evaluate(() => {
    const input = document.getElementById('edit-font-size'); input.value = '20'; input.disabled = false;
    const button = document.getElementById('edit-font-size-apply'); button.disabled = false; button.click();
  });
  expect(writes).toBe(0); await expect(page.locator('.tiptap [data-font-size="40"]')).toHaveCount(0);
  finishSave(null); await expect(page.locator('#edit-font-size')).toBeEnabled();
});

test('HTML paste keeps supported half-point units and drops unsupported sizes without unsafe style text', async ({ page }) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<p><span style="font-size:13.5pt">甲<br>换</span><span style="font-size:20px">乙</span><span style="font-size:13px">丙</span><span style="font-size:99999pt">丁</span></p>');
    data.setData('text/plain', '甲\n换乙丙丁');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData: data }));
  });
  await expect.poll(() => draft?.content[0]?.content.filter(n => n.type === 'text').map(n => [n.text, fontSize(n)])).toEqual([['甲', 27], ['换', 27], ['乙', 30], ['丙丁', undefined]]);
  expect(draft.content[0].content[1]).toEqual({ type: 'hardBreak' });
  await expect(page.locator('.tiptap [data-font-size="27"]').first()).toHaveCSS('font-size', '18px');
  await expect(page.locator('.tiptap [data-font-size="30"]')).toHaveCSS('font-size', '20px');
});

test('size spans hard breaks and table text while keeping nontext nodes valid for Rust export', async ({ page }, info) => {
  let draft;
  const original = { type: 'doc', content: [
    { type: 'paragraph', content: [{ type: 'text', text: '前', marks: [{ type: 'superscript' }] }, { type: 'hardBreak' }, { type: 'text', text: '后' }] },
    { type: 'table', content: [{ type: 'tableRow', content: [{ type: 'tableCell', content: [{ type: 'paragraph', content: [{ type: 'text', text: '表格', marks: [{ type: 'code' }] }] }] }] }] },
  ] };
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const json = info.outputPath('breaks.json'), file = info.outputPath('breaks.docx');
      mkdirSync(path.dirname(json), { recursive: true }); writeFileSync(json, JSON.stringify(args.doc));
      const report = JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'), ['create-docx', json, file], { encoding: 'utf8', windowsHide: true }));
      return { path: file, report };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap p').first().click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-font-size').fill('15'); await page.locator('#edit-font-size').press('Enter');
  await expect.poll(() => draft?.content[0]?.content[1]).toEqual({ type: 'hardBreak' });
  expect(fontSize(draft.content[0].content[0])).toBe(30); expect(fontSize(draft.content[0].content[2])).toBe(30);
  expect(fontSize(draft.content[1].content[0].content[0].content[0].content[0])).toBe(30);
  await expect(page.locator('.tiptap [data-font-size="30"] sup')).toHaveText('前');
  expect(await page.locator('.tiptap sup').evaluate(node => parseFloat(getComputedStyle(node).fontSize))).toBeLessThan(20);
  await expect(page.locator('.tiptap [data-font-size="30"] code')).toHaveText('表格');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
});

test('mixed empty input is a no-op until edited and a focused value survives application', async ({ page }) => {
  let writes = 0;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return model;
    if (command === 'save_draft') { writes++; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await expect(page.locator('#edit-font-size')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-font-size-apply').click();
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.waitForTimeout(800); expect(writes).toBe(0);
  await page.locator('#edit-font-size').fill('13.5'); await page.locator('#edit-font-size-apply').click();
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('甲乙丙丁');
  await page.locator('#edit-undo').click(); await expect(page.locator('#edit-font-size')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-font-size').fill('14'); await page.locator('#edit-font-size').fill('');
  await page.locator('#edit-font-size-apply').click(); await expect(page.locator('.tiptap [data-font-size]')).toHaveCount(0);
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-font-size="24"]')).toHaveText('甲乙');
});

test('splitting a near-limit document refuses before changing body, draft or undo history', async ({ page }) => {
  let writes = 0;
  const original = { type: 'doc', content: Array.from({ length: 4999 }, () => ({ type: 'paragraph', content: [{ type: 'text', text: 'abc' }] })) };
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { writes++; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 1, 2, '13.5');
  await expect(page.locator('#notice')).toContainText('上限');
  await expect(page.locator('.tiptap [data-font-size]')).toHaveCount(0);
  await expect(page.locator('.tiptap p')).toHaveCount(4999);
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.waitForTimeout(800); expect(writes).toBe(0);
});
