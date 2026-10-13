import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {loadTsModule} from './load-ts.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';

const family = name => ({type: 'fontFamily', attrs: {name}});
const familyOf = node => node.marks?.find(mark => mark.type === 'fontFamily')?.attrs.name;
const original = {type: 'doc', content: [{type: 'paragraph', content: [
  {type: 'text', text: '甲乙', marks: [{type: 'bold'}, {type: 'fontSize', attrs: {halfPoints: 27}}, {type: 'textColor', attrs: {rgb: '1A6B3C'}}, {type: 'link', attrs: {href: 'https://example.com/'}}]},
  {type: 'text', text: '丙丁', marks: [{type: 'italic'}, {type: 'code'}]},
]}]};

async function selectAndApply(page, from, to, value) {
  await page.evaluate(({from, to, value}) => {
    const root = document.querySelector('.tiptap p');
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const positions = []; let offset = 0, node;
    while ((node = walker.nextNode())) {
      positions.push({node, start: offset, end: offset + node.textContent.length}); offset += node.textContent.length;
    }
    const anchor = positions.find(item => item.end >= from), head = positions.find(item => item.end >= to);
    window.getSelection().setBaseAndExtent(anchor.node, from - anchor.start, head.node, to - head.start);
    const input = document.getElementById('edit-font-family');
    input.value = value; input.dispatchEvent(new Event('input', {bubbles: true}));
    document.getElementById('edit-font-family-apply').click();
  }, {from, to, value});
}

test('literal font names validate and paste CSS decodes names without accepting declarations', async () => {
  const {isFontFamilyName, fontFamilyCss, parseFontFamilyCss} = await loadTsModule('./ui/word-font-family.ts');
  for (const name of ['Courier New', '中文字体', 'A "quoted", Font\\Name; &', '😀'.repeat(15) + 'A', '字'.repeat(31)]) {
    expect(isFontFamilyName(name)).toBe(true);
    expect(parseFontFamilyCss(fontFamilyCss(name))).toBe(name);
  }
  for (const name of ['', ' Arial', 'Arial ', '字'.repeat(32), '\uD800', '\uDC00', 'A\u0085B', 'A\uFEFFB']) {
    if (name === 'A\uFEFFB') expect(isFontFamilyName(name)).toBe(true);
    else expect(isFontFamilyName(name)).toBe(false);
  }
  expect(parseFontFamilyCss('sans-serif, "Courier New"')).toBe('Courier New');
  expect(parseFontFamilyCss('"A\\22 B"')).toBe('A"B');
  for (const css of ['serif', 'url(javascript:evil)', 'Arial; color:red', '"unclosed', 'Arial,', '"A\\0 B"']) {
    expect(parseFontFamilyCss(css)).toBeNull();
  }
});

test('partial selection keeps marks and links, updates code font, and exports four exact DOCX slots', async ({page}, info) => {
  let draft, imported, independent;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const file = info.outputPath('family.docx'), json = info.outputPath('family.json');
      mkdirSync(path.dirname(file), {recursive: true}); writeFileSync(json, JSON.stringify(args.doc));
      const cli = path.resolve('target/debug/rust-wps.exe');
      const report = JSON.parse(execFileSync(cli, ['create-docx', json, file], {encoding: 'utf8', windowsHide: true}));
      imported = JSON.parse(execFileSync(cli, ['import-docx', file], {encoding: 'utf8', windowsHide: true})).document;
      const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
      independent = JSON.parse(execFileSync(python, ['-c', [
        'import json,sys', 'from docx import Document', 'from docx.oxml.ns import qn',
        'result=[]',
        'for r in Document(sys.argv[1]).paragraphs[0]._p.xpath(".//w:r"):',
        ' f=r.rPr.find(qn("w:rFonts")) if r.rPr is not None else None',
        ' result.append(["".join(t.text or "" for t in r.xpath("./w:t")), [f.get(qn("w:"+x)) for x in ("ascii","hAnsi","eastAsia","cs")] if f is not None else None])',
        'print(json.dumps(result,ensure_ascii=True))',
      ].join('\n'), file], {encoding: 'utf8', windowsHide: true}));
      return {path: file, report};
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 1, 3, 'Courier New');
  await expect.poll(() => draft?.content[0]?.content.map(node => [node.text, familyOf(node)])).toEqual([
    ['甲', undefined], ['乙', 'Courier New'], ['丙', 'Courier New'], ['丁', undefined],
  ]);
  await expect(page.locator('.tiptap a [data-font-family="Courier New"]')).toHaveText('乙');
  await expect(page.locator('.tiptap code [data-font-family="Courier New"]')).toHaveCSS('font-family', /Courier New/);
  expect(draft.content[0].content[1].marks).toEqual(expect.arrayContaining([
    {type: 'bold'}, {type: 'fontSize', attrs: {halfPoints: 27}}, {type: 'textColor', attrs: {rgb: '1A6B3C'}},
    {type: 'link', attrs: expect.objectContaining({href: 'https://example.com/'})}, family('Courier New'),
  ]));
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-font-family]')).toHaveCount(0);
  await page.locator('#edit-redo').click(); await expect(page.locator('.tiptap [data-font-family]')).toHaveCount(2);
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(independent.filter(row => ['乙', '丙'].includes(row[0]))).toEqual([
    ['乙', Array(4).fill('Courier New')], ['丙', Array(4).fill('Courier New')],
  ]);
  expect(imported.content[0].content.filter(node => ['乙', '丙'].includes(node.text)).map(familyOf)).toEqual(['Courier New', 'Courier New']);
});

test('caret font affects future typing; default, clear and undo preserve content and links', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return {type: 'doc', content: [{type: 'paragraph', content: [original.content[0].content[0]]}]};
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 2, 2, 'Consolas'); await page.keyboard.insertText('新');
  await expect(page.locator('.tiptap [data-font-family="Consolas"]')).toHaveText('新');
  await page.locator('#edit-font-family-reset').click(); await page.keyboard.insertText('默认');
  await expect.poll(() => draft?.content[0]?.content.map(node => [node.text, familyOf(node)])).toEqual([
    ['甲乙', undefined], ['新', 'Consolas'], ['默认', undefined],
  ]);
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-clear-marks').click();
  await expect(page.locator('.tiptap [data-font-family]')).toHaveCount(0);
  await expect(page.locator('.tiptap a')).toHaveAttribute('href', 'https://example.com/');
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-font-family="Consolas"]')).toHaveText('新');
  await expect(page.locator('.tiptap')).toHaveText('甲乙新默认');
});

test('mixed, invalid and save-locked input leaves draft and undo unchanged', async ({page}) => {
  let writes = 0, finishSave;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return {type: 'doc', content: [{type: 'paragraph', content: [
      {type: 'text', text: '甲', marks: [family('Arial')]}, {type: 'text', text: '乙', marks: [family('Consolas')]},
    ]}]};
    if (command === 'save_draft') { writes++; return; }
    if (command === 'save_new_document') return new Promise(resolve => { finishSave = resolve; });
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await expect(page.locator('#edit-font-family')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-font-family-apply').click(); await expect(page.locator('#edit-undo')).toBeDisabled();
  for (const value of [' Bad', 'Bad ', 'A'.repeat(32), 'A\u0085B']) {
    await selectAndApply(page, 0, 2, value);
    await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.waitForTimeout(800); expect(writes).toBe(0);
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  await expect(page.locator('#edit-font-family')).toBeDisabled();
  await page.evaluate(() => {
    const input = document.getElementById('edit-font-family'); input.disabled = false; input.value = 'Courier New';
    const button = document.getElementById('edit-font-family-apply'); button.disabled = false; button.click();
  });
  await expect(page.locator('.tiptap [data-font-family="Courier New"]')).toHaveCount(0);
  expect(writes).toBe(0); finishSave(null); await expect(page.locator('#edit-font-family')).toBeEnabled();
});

test('HTML paste retains literal font and text-only mark across break and table', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<p><span style="font-family:&quot;Courier New&quot;">甲<br>乙</span><code style="font-family:Consolas">代</code><span style="font-family:serif">无</span><span style="font-family:Arial;color:red">安</span></p><table><tbody><tr><td><p><span style="font-family:SimSun">表</span></p></td></tr></tbody></table>');
    data.setData('text/plain', '甲\n乙代无安表');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  });
  await expect.poll(() => draft?.content[0]?.content.filter(node => node.type === 'text').map(node => [node.text, familyOf(node)])).toEqual([
    ['甲', 'Courier New'], ['乙', 'Courier New'], ['代', 'Consolas'], ['无', undefined], ['安', 'Arial'],
  ]);
  expect(draft.content[0].content[1]).toEqual({type: 'hardBreak'});
  await expect(page.locator('.tiptap code')).toHaveCSS('font-family', /Consolas/);
  await expect(page.locator('.tiptap td [data-font-family="SimSun"]')).toHaveText('表');
});

test('font splitting beyond structure budget refuses without changing body or draft', async ({page}) => {
  let writes = 0;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return {type: 'doc', content: Array.from({length: 4999}, () => ({type: 'paragraph', content: [{type: 'text', text: 'abc'}]}))};
    if (command === 'save_draft') { writes++; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await selectAndApply(page, 1, 2, 'Courier New');
  await expect(page.locator('#notice')).toContainText('上限');
  await expect(page.locator('.tiptap [data-font-family]')).toHaveCount(0);
  await expect(page.locator('.tiptap p')).toHaveCount(4999);
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.waitForTimeout(800); expect(writes).toBe(0);
});
