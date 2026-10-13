import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const color = rgb => ({ type: 'textColor', attrs: { rgb } });
const rgbOf = node => node.marks?.find(mark => mark.type === 'textColor')?.attrs.rgb;
const model = { type: 'doc', content: [{ type: 'paragraph', content: [
  { type: 'text', text: '甲乙', marks: [{ type: 'bold' }, { type: 'fontSize', attrs: { halfPoints: 27 } }, color('CC0000'), { type: 'link', attrs: { href: 'https://example.com/' } }] },
  { type: 'text', text: '丙丁', marks: [{ type: 'italic' }, color('0000FF')] },
] }] };

async function selectAndApply(page, from, to, value) {
  await page.evaluate(({ from, to, value }) => {
    const root = document.querySelector('.tiptap p'), walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const positions = []; let offset = 0, node;
    while ((node = walker.nextNode())) {
      positions.push({ node, start: offset, end: offset + node.textContent.length }); offset += node.textContent.length;
    }
    const anchor = positions.find(item => item.end >= from), head = positions.find(item => item.end >= to);
    window.getSelection().setBaseAndExtent(anchor.node, from - anchor.start, head.node, to - head.start);
    document.getElementById('edit-text-color').value = value;
    document.getElementById('edit-text-color-apply').click();
  }, { from, to, value });
}

test('partial RGB changes preserve links and size, undo together and export one OOXML color per run', async ({ page }, info) => {
  let draft, imported, independent;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return model;
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const file = info.outputPath('colors.docx'), json = info.outputPath('colors.json');
      mkdirSync(path.dirname(file), { recursive: true }); writeFileSync(json, JSON.stringify(args.doc));
      const cli = path.resolve('target/debug/rust-wps.exe');
      const report = JSON.parse(execFileSync(cli, ['create-docx', json, file], { encoding: 'utf8', windowsHide: true }));
      imported = JSON.parse(execFileSync(cli, ['import-docx', file], { encoding: 'utf8', windowsHide: true })).document;
      const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
      independent = JSON.parse(execFileSync(python, ['-c', [
        'import sys,json', 'from docx import Document', 'from docx.oxml.ns import qn',
        'runs=Document(sys.argv[1]).paragraphs[0]._p.xpath(".//w:r")',
        'assert all(len(r.xpath("./w:rPr/w:color"))==1 for r in runs)',
        'print(json.dumps([["".join(t.text or "" for t in r.xpath("./w:t")),r.xpath("./w:rPr/w:color")[0].get(qn("w:val"))] for r in runs],ensure_ascii=True))',
      ].join('\n'), file], { encoding: 'utf8', windowsHide: true }));
      return { path: file, report };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await expect(page.locator('#edit-text-color')).toHaveAttribute('placeholder', '混合');
  await selectAndApply(page, 1, 3, '#12ab34');
  await expect.poll(() => draft?.content[0]?.content.map(node => [node.text, rgbOf(node)])).toEqual([['甲', 'CC0000'], ['乙', '12AB34'], ['丙', '12AB34'], ['丁', '0000FF']]);
  await expect(page.locator('.tiptap a [data-text-color="12AB34"]')).toHaveText('乙');
  await expect(page.locator('.tiptap a [data-text-color="12AB34"]')).toHaveCSS('color', 'rgb(18, 171, 52)');
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('甲乙');
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-text-color="12AB34"]')).toHaveCount(0);
  await page.locator('#edit-redo').click(); await expect(page.locator('.tiptap [data-text-color="12AB34"]')).toHaveCount(2);
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(independent).toEqual([['甲', 'CC0000'], ['乙', '12AB34'], ['丙', '12AB34'], ['丁', '0000FF']]);
  expect(imported.content[0].content.map(node => [node.text, rgbOf(node)])).toEqual(independent);
  expect(imported.content[0].content[1].marks).toEqual(expect.arrayContaining([{ type: 'fontSize', attrs: { halfPoints: 27 } }, { type: 'link', attrs: { href: 'https://example.com/' } }]));
});

test('caret colors affect future typing, reset removes color and clear keeps the link with one undo', async ({ page }) => {
  let draft;
  const original = { type: 'doc', content: [{ type: 'paragraph', content: [model.content[0].content[0]] }] };
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 2, 2, '1a6b3c'); await page.keyboard.insertText('新');
  await expect(page.locator('.tiptap [data-text-color="1A6B3C"]')).toHaveText('新');
  await page.locator('#edit-text-color-reset').click(); await page.keyboard.insertText('默认');
  await expect.poll(() => draft?.content[0]?.content.map(node => [node.text, rgbOf(node)])).toEqual([['甲乙', 'CC0000'], ['新', '1A6B3C'], ['默认', undefined]]);
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-clear-marks').click();
  await expect(page.locator('.tiptap [data-text-color]')).toHaveCount(0);
  await expect(page.locator('.tiptap [data-font-size]')).toHaveCount(0);
  await expect(page.locator('.tiptap strong')).toHaveCount(0);
  await expect(page.locator('.tiptap a')).toHaveAttribute('href', 'https://example.com/');
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-text-color="1A6B3C"]')).toHaveText('新');
  await expect(page.locator('.tiptap')).toHaveText('甲乙新默认');
});

test('HTML paste accepts opaque RGB and code colors but ignores alpha and variables and leaves breaks unmarked', async ({ page }, info) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const json = info.outputPath('paste.json'), file = info.outputPath('paste.docx');
      mkdirSync(path.dirname(file), { recursive: true }); writeFileSync(json, JSON.stringify(args.doc));
      const report = JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'), ['create-docx', json, file], { encoding: 'utf8', windowsHide: true }));
      return { path: file, report };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<p><span style="color:#123456">甲<br>换</span><code style="color:#abc">代</code><span style="color:rgba(1,2,3,1)">实</span><span style="color:rgba(1,2,3,.5)">半</span><span style="color:var(--unknown)">变</span></p><table><tbody><tr><td><p><span style="color:#987654">表格</span></p></td></tr></tbody></table>');
    data.setData('text/plain', '甲\n换代实半变');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData: data }));
  });
  await expect.poll(() => draft?.content[0]?.content.filter(node => node.type === 'text').map(node => [node.text, rgbOf(node)])).toEqual([['甲', '123456'], ['换', '123456'], ['代', 'AABBCC'], ['实', '010203'], ['半变', undefined]]);
  expect(draft.content[0].content[1]).toEqual({ type: 'hardBreak' });
  await expect(page.locator('.tiptap [data-text-color="AABBCC"] code')).toHaveText('代');
  await expect(page.locator('.tiptap td [data-text-color="987654"]')).toHaveText('表格');
  await page.locator('.tiptap p').first().click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-text-color').fill('234567'); await page.locator('#edit-text-color').press('Enter');
  await expect(page.locator('.tiptap code')).toHaveCSS('color', 'rgb(35, 69, 103)');
  await expect(page.locator('.tiptap td [data-text-color="234567"]')).toHaveText('表格');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
});

test('mixed, invalid and locked colors preserve history; focused application and explicit clearing work', async ({ page }) => {
  let writes = 0, finishSave;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return model;
    if (command === 'save_draft') { writes++; return; }
    if (command === 'save_new_document') return new Promise(resolve => { finishSave = resolve; });
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-text-color-apply').click(); await expect(page.locator('#edit-undo')).toBeDisabled();
  for (const value of ['red', '#123', '12345678', 'rgba(1,2,3,.5)', '#12345g']) {
    await selectAndApply(page, 0, 4, value); await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.waitForTimeout(800); expect(writes).toBe(0);
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  await expect(page.locator('#edit-text-color')).toBeDisabled();
  await expect(page.locator('#edit-text-color-picker')).toBeDisabled();
  await expect(page.locator('#edit-text-color-black')).toBeDisabled();
  await page.evaluate(() => {
    const input = document.getElementById('edit-text-color'); input.disabled = false; input.value = '#000000';
    const button = document.getElementById('edit-text-color-apply'); button.disabled = false; button.click();
    const picker = document.getElementById('edit-text-color-picker'); picker.disabled = false; picker.value = '#000000'; picker.dispatchEvent(new Event('change'));
    const black = document.getElementById('edit-text-color-black'); black.disabled = false; black.click();
  });
  await expect(page.locator('.tiptap [data-text-color="000000"]')).toHaveCount(0); expect(writes).toBe(0);
  finishSave(null); await expect(page.locator('#edit-text-color')).toBeEnabled();
  await page.locator('#edit-text-color').fill('#010203'); await page.locator('#edit-text-color-apply').click();
  await expect(page.locator('.tiptap [data-text-color="010203"]')).toHaveCount(2);
  await page.locator('#edit-undo').click(); await expect(page.locator('#edit-text-color')).toHaveAttribute('placeholder', '混合');
  await expect(page.locator('#edit-text-color-picker')).toHaveValue('#000000');
  await page.locator('#edit-text-color-picker').fill('#3579bd');
  await expect(page.locator('.tiptap [data-text-color="3579BD"]')).toHaveCount(2);
  await page.locator('#edit-undo').click();
  await expect(page.locator('#edit-text-color')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-text-color-black').focus(); await page.keyboard.press('Enter');
  await expect(page.locator('.tiptap [data-text-color="000000"]')).toHaveCount(2);
  await page.locator('#edit-undo').click();
  await expect(page.locator('#edit-text-color')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-text-color').fill('010203'); await page.locator('#edit-text-color').fill('');
  await page.locator('#edit-text-color-apply').click(); await expect(page.locator('.tiptap [data-text-color]')).toHaveCount(0);
});

test('color splitting beyond the structure budget leaves text, draft and undo unchanged', async ({ page }) => {
  let writes = 0;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return { type: 'doc', content: Array.from({ length: 4999 }, () => ({ type: 'paragraph', content: [{ type: 'text', text: 'abc' }] })) };
    if (command === 'save_draft') { writes++; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 1, 2, '#123456');
  await expect(page.locator('#notice')).toContainText('上限');
  await expect(page.locator('.tiptap [data-text-color]')).toHaveCount(0);
  await expect(page.locator('#edit-undo')).toBeDisabled(); await page.waitForTimeout(800); expect(writes).toBe(0);
});
