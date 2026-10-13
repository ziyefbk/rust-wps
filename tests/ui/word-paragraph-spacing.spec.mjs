import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

const editor = '#editor-content .tiptap';
const paragraph = (text, attrs) => ({type: 'paragraph', ...(attrs ? {attrs} : {}), content: [{type: 'text', text}]});
const blockAttrs = model => {
  const result = [];
  const visit = node => {
    if (['paragraph', 'heading'].includes(node.type)) result.push(node.attrs ?? {});
    for (const child of node.content ?? []) visit(child);
  };
  visit(model); return result;
};
const apply = '#edit-paragraph-spacing-apply';
const reset = '#edit-paragraph-spacing-reset';

async function fillSpacing(page, values) {
  for (const [id, value] of Object.entries(values)) await page.locator(`#edit-${id}`).fill(value);
  await page.locator(apply).click();
}

test('mixed multi-paragraph spacing covers headings, lists and cells, undoes once and exports real DOCX', async ({page}, info) => {
  let draft, exported, independent, imported;
  const original = {type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [
    paragraph('正文', {spaceBefore: 0, lineSpacing: 241}),
    {type: 'heading', attrs: {level: 2, textAlign: 'center', spaceBefore: 240}, content: [{type: 'text', text: '标题', marks: [{type: 'bold'}]}]},
    {type: 'bulletList', content: [{type: 'listItem', content: [paragraph('列表')]}]},
    {type: 'table', content: [{type: 'tableRow', content: [
      {type: 'tableHeader', content: [paragraph('表头')]}, {type: 'tableCell', content: [paragraph('表格一'), paragraph('表格二')]},
    ]}]},
  ]};
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = structuredClone(args.doc); return; }
    if (command === 'save_new_document') {
      const json = info.outputPath('spacing.json'), output = info.outputPath('spacing.docx');
      mkdirSync(path.dirname(json), {recursive: true}); writeFileSync(json, JSON.stringify(args.doc));
      const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
      const report = JSON.parse(execFileSync(cli, ['create-docx', json, output], {encoding: 'utf8', windowsHide: true}));
      imported = JSON.parse(execFileSync(cli, ['import-docx', output], {encoding: 'utf8', windowsHide: true})).document;
      const python = process.env.PYTHON ?? (process.platform === 'win32'
        ? path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe') : 'python3');
      independent = JSON.parse(execFileSync(python, ['-c', [
        'import json,sys', 'from docx import Document', 'from docx.oxml.ns import qn', 'result=[]',
        'for p in Document(sys.argv[1]).element.body.iter(qn("w:p")):',
        ' s=p.find(qn("w:pPr")).find(qn("w:spacing"))',
        ' result.append({k:s.get(qn("w:"+k)) for k in ["before","after","line","lineRule"]})',
        'print(json.dumps(result))',
      ].join('\n'), output], {encoding: 'utf8', windowsHide: true}));
      exported = structuredClone(args.doc); return {path: output, report};
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(editor).click(); await page.keyboard.press('Control+a');
  await expect(page.locator('#edit-space-before')).toHaveAttribute('placeholder', '混合');
  await expect(page.locator('#edit-line-spacing')).toHaveAttribute('placeholder', '混合');
  await fillSpacing(page, {'space-before': '3.05', 'space-after': '7', 'line-spacing': '1.15'});
  await expect(page.locator(`${editor} [data-line-spacing="276"]`)).toHaveCount(6);
  await expect(page.locator(`${editor} h2`)).toHaveCSS('text-align', 'center');
  await expect(page.locator(`${editor} h2 strong`)).toHaveText('标题');
  await expect.poll(() => blockAttrs(draft ?? {}).map(({spaceBefore, spaceAfter, lineSpacing}) => [spaceBefore, spaceAfter, lineSpacing])).toEqual(Array(6).fill([61, 140, 276]));
  await page.locator('#edit-undo').click();
  await expect(page.locator(`${editor} [data-line-spacing="276"]`)).toHaveCount(0);
  await expect(page.locator(`${editor} [data-line-spacing="241"]`)).toHaveCount(1);
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.locator('#edit-redo').click();
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(independent).toEqual(Array(6).fill({before: '61', after: '140', line: '276', lineRule: 'auto'}));
  expect(blockAttrs(imported).map(({spaceBefore, spaceAfter, lineSpacing}) => [spaceBefore, spaceAfter, lineSpacing])).toEqual(Array(6).fill([61, 140, 276]));
  expect(exported.attrs.styleProfile).toBe('standard-v2');
  await page.locator('#edit-clear-marks').click();
  await expect(page.locator(`${editor} [data-line-spacing="276"]`)).toHaveCount(6);
  await page.locator(reset).click();
  await expect(page.locator(`${editor} [data-space-before], ${editor} [data-space-after], ${editor} [data-line-spacing]`)).toHaveCount(0);
  await expect.poll(() => blockAttrs(draft ?? {}).some(attrs => ['spaceBefore', 'spaceAfter', 'lineSpacing'].some(key => key in attrs))).toBe(false);
});

test('caret spacing preserves exact repeating line units, untouched apply is clean, and reset omits nulls', async ({page}) => {
  let draft, writes = 0;
  const original = {type: 'doc', content: [paragraph('Exact', {spaceBefore: 0, lineSpacing: 241}), paragraph('Other')]};
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = structuredClone(args.doc); writes++; return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(`${editor} p`).first().click();
  await expect(page.locator('#edit-line-spacing')).toHaveValue('241/240');
  await page.locator(apply).click(); await page.waitForTimeout(800);
  expect(writes).toBe(0); await expect(page.locator('#edit-undo')).toBeDisabled();
  await fillSpacing(page, {'space-after': '0.05'});
  await expect.poll(() => draft?.content[0]?.attrs).toMatchObject({spaceBefore: 0, spaceAfter: 1, lineSpacing: 241});
  expect(draft.content[1].attrs ?? {}).not.toHaveProperty('spaceAfter');
  await page.locator(reset).click();
  await expect.poll(() => draft?.content[0]?.attrs?.lineSpacing).toBeUndefined();
  expect(draft).not.toHaveProperty('attrs');
  expect(JSON.stringify(draft)).not.toMatch(/"(?:spaceBefore|spaceAfter|lineSpacing)":null/);
  await page.locator('#edit-undo').click();
  await expect(page.locator(`${editor} p`).first()).toHaveAttribute('data-line-spacing', '241');
  await expect(page.locator(`${editor} p`).first()).toHaveAttribute('data-space-after', '1');
});

test('pending DOM selection applies to selected blocks and mixed untouched fields remain unchanged', async ({page}) => {
  let draft;
  const original = {type: 'doc', content: [paragraph('First', {spaceBefore: 20}), paragraph('Second', {spaceBefore: 40}), paragraph('Third')]};
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { draft = structuredClone(args.doc); return; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.evaluate(() => {
    const blocks = document.querySelectorAll('.tiptap p');
    window.getSelection().setBaseAndExtent(blocks[0].firstChild, 2, blocks[1].firstChild, 2);
    document.getElementById('edit-space-after').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await expect(page.locator('#edit-space-before')).toHaveAttribute('placeholder', '混合');
  await fillSpacing(page, {'space-after': '5'});
  await expect.poll(() => blockAttrs(draft ?? {}).map(attrs => attrs.spaceBefore)).toEqual([20, 40, undefined]);
  expect(blockAttrs(draft).map(attrs => attrs.spaceAfter)).toEqual([100, 100, undefined]);
  // Explicitly edit a mixed blank back to blank to reset just that field.
  await page.locator('#edit-space-before').fill('1'); await page.locator('#edit-space-before').fill('');
  await page.locator(apply).click();
  await expect.poll(() => blockAttrs(draft ?? {}).map(attrs => attrs.spaceBefore)).toEqual([undefined, undefined, undefined]);
  expect(blockAttrs(draft).map(attrs => attrs.spaceAfter)).toEqual([100, 100, undefined]);
});

test('invalid group values and save-locked actions leave text, draft and undo history untouched', async ({page}) => {
  let writes = 0, finishSave;
  const original = {type: 'doc', content: [paragraph('Keep', {lineSpacing: 360})]};
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') { writes++; return; }
    if (command === 'save_new_document') return new Promise(resolve => { finishSave = resolve; });
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(`${editor} p`).click();
  for (const invalid of ['0.4', '10.1', '1.0042', '119/240', 'NaN']) {
    await fillSpacing(page, {'space-before': '12', 'line-spacing': invalid});
    await expect(page.locator('#notice')).toContainText('0.5–10 行');
    await expect(page.locator(`${editor} p`)).not.toHaveAttribute('data-space-before');
    await expect(page.locator(`${editor} p`)).toHaveAttribute('data-line-spacing', '360');
    await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.locator('#edit-line-spacing').press('Escape');
  await page.waitForTimeout(800); expect(writes).toBe(0);
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  await expect(page.locator('#edit-space-before')).toBeDisabled();
  await expect(page.locator('#edit-line-spacing')).toBeDisabled();
  await page.locator(reset).evaluate(button => { button.disabled = false; button.click(); });
  await expect(page.locator(`${editor} p`)).toHaveAttribute('data-line-spacing', '360');
  finishSave(null); await expect(page.locator('#edit-line-spacing')).toBeEnabled();
  expect(writes).toBe(0);
});

test('inline spacing survives immutable print snapshots and HTML paste keeps supported canonical values', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => { if (command === 'save_draft') { draft = args.doc; return; } throw new Error(command); });
  await page.addInitScript(() => { window.print = () => { window.printCalls = (window.printCalls ?? 0) + 1; }; });
  await page.goto('/'); await page.locator('#new-document').click(); await page.locator(editor).focus();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<p data-line-spacing="241" style="margin-top:3.05pt;margin-bottom:9px;line-height:1.0041666666666667">ExactPrint</p><h2 style="margin-top:4pt;line-height:150%">HeadingPrint</h2>');
    data.setData('text/plain', 'ExactPrint\nHeadingPrint');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  });
  await expect(page.locator(`${editor} p`)).toHaveAttribute('data-line-spacing', '241');
  await expect(page.locator(`${editor} h2`)).toHaveAttribute('data-line-spacing', '360');
  await expect.poll(() => draft?.content[0]?.attrs).toMatchObject({spaceBefore: 61, spaceAfter: 135, lineSpacing: 241});
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  const printStyle = await page.locator('#print-document p').getAttribute('style');
  expect(printStyle).toBe(await page.locator(`${editor} p`).getAttribute('style'));
  await page.locator(`${editor} p`).click(); await fillSpacing(page, {'line-spacing': '2'});
  await expect(page.locator('#print-document p')).toHaveAttribute('data-line-spacing', '241');
  await expect(page.locator('#print-document p')).toHaveAttribute('style', printStyle);
  await page.emulateMedia({media: 'print'});
  expect(await page.locator('#print-document p').evaluate(element => parseFloat(getComputedStyle(element).marginTop))).toBeCloseTo(61 / 15, 4);
});

test('invalid incoming spacing is rejected before replacing an active editor', async ({page}) => {
  let restore;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return restore;
    if (command === 'save_draft') return;
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator(editor).click(); await page.keyboard.insertText('Keep active');
  for (const attrs of [{spaceBefore: null}, {spaceAfter: 31681}, {lineSpacing: 119}, {lineSpacing: '360'}]) {
    restore = {type: 'doc', content: [paragraph('Must not replace', attrs)]};
    await page.locator('#restore-draft').click();
    await expect(page.locator('#notice')).toContainText('草稿恢复失败');
    await expect(page.locator(editor)).toHaveText('Keep active');
    await expect(page.locator(editor)).toHaveAttribute('data-style-profile', 'standard-v2');
    await expect(page.locator('#edit-line-spacing')).toBeEnabled();
  }
});
