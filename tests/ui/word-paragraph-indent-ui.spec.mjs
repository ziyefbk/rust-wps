import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// Discovery-only in this environment. These scenarios have NOT established
// real browser selection, CSS layout, clipboard or native printing behavior.
const root = '#editor-content .tiptap', apply = '#edit-paragraph-indent-apply';
const paragraph = (text, attrs) => ({type: 'paragraph', ...(attrs ? {attrs} : {}), content: text ? [{type: 'text', text}] : []});
const document = (content, attrs = {styleProfile: 'standard-v2'}) => ({type: 'doc', attrs, content});
const table = (paragraphs, attrs) => ({type: 'table', ...(attrs ? {attrs} : {}), content: [{type: 'tableRow', content: paragraphs.map(value => ({type: 'tableCell', content: [value]}))}]});
const geometry = extra => ({widthTwips: 11906, heightTwips: 16838, marginTopTwips: 1440, marginRightTwips: 1440, marginBottomTwips: 1440, marginLeftTwips: 1440, ...extra});
async function open(page, original, extra = command => {throw new Error(command);}) {
  let draft, writes = 0;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') {draft = structuredClone(args.doc); writes++; return;}
    return extra(command, args);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  return {draft: () => draft, writes: () => writes};
}
async function fill(page, patch) {
  for (const [field, value] of Object.entries(patch)) await page.locator(`#edit-paragraph-indent-${field}`).fill(value);
  await page.locator(apply).click();
}

test('body heading and cell signed indents survive real export trusted reopen draft and one undo redo', async ({page}, info) => {
  let exported, reopened, independent;
  const original = document([paragraph('body'), {type: 'heading', attrs: {level: 2}, content: [{type: 'text', text: 'heading', marks: [{type: 'bold'}]}]}, table([paragraph('cell')])]);
  const saved = await open(page, original, (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('indent.json'), output = info.outputPath('indent.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
    const python = process.env.PYTHON ?? (process.platform === 'win32' ? path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe') : 'python3');
    independent = JSON.parse(execFileSync(python, ['-c', 'import json,sys\nfrom docx import Document\nfrom docx.oxml.ns import qn\nprint(json.dumps([{k:p.find(qn("w:pPr")).find(qn("w:ind")).get(qn("w:"+k)) for k in ["left","right","hanging","firstLine"]} for p in Document(sys.argv[1]).element.body.iter(qn("w:p"))]))', output], {encoding: 'utf8', windowsHide: true}));
    exported = structuredClone(args.doc); return {path: output};
  });
  await page.locator(root).click(); await page.keyboard.press('Control+a');
  await fill(page, {left: '18.05', right: '0', first: '-18'});
  await expect(page.locator(`${root} [data-first-line-indent-twips="-360"]`)).toHaveCount(3);
  await expect(page.locator(`${root} h2 strong`)).toHaveText('heading');
  await expect.poll(() => saved.draft()?.content[0]?.attrs).toMatchObject({indentLeftTwips: 361, indentRightTwips: 0, firstLineIndentTwips: -360});
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} [data-first-line-indent-twips]`)).toHaveCount(0); await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.locator('#edit-redo').click(); await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(reopened.document ?? reopened).toEqual(exported); expect(independent).toEqual(Array(3).fill({left: '361', right: '0', hanging: '360', firstLine: null}));
});

test('pending DOM selection mixed values and field reset affect only requested paragraphs', async ({page}) => {
  const saved = await open(page, document([paragraph('First', {indentLeftTwips: 20}), paragraph('Second', {indentLeftTwips: 40}), paragraph('Third')]));
  await page.evaluate(() => {
    const blocks = document.querySelectorAll('.tiptap p'); window.getSelection().setBaseAndExtent(blocks[0].firstChild, 2, blocks[1].firstChild, 2);
    document.getElementById('edit-paragraph-indent-right').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await expect(page.locator('#edit-paragraph-indent-left')).toHaveAttribute('placeholder', '混合');
  await fill(page, {right: '1.05'});
  await expect.poll(() => saved.draft()?.content.map(node => node.attrs?.indentLeftTwips)).toEqual([20, 40, undefined]);
  expect(saved.draft().content.map(node => node.attrs?.indentRightTwips)).toEqual([21, 21, undefined]);
  await page.locator('#edit-paragraph-indent-left-reset').click();
  await expect.poll(() => saved.draft()?.content.map(node => node.attrs?.indentLeftTwips)).toEqual([undefined, undefined, undefined]);
  expect(saved.draft().content.map(node => node.attrs?.indentRightTwips)).toEqual([21, 21, undefined]);
});

test('invalid atomic group and unsafe left-only reset leave content draft history unchanged', async ({page}) => {
  const saved = await open(page, document([paragraph('Hanging', {indentLeftTwips: 360, firstLineIndentTwips: -360})]));
  await page.locator(`${root} p`).click(); await page.locator('#edit-paragraph-indent-left-reset').click();
  await expect(page.locator('#notice')).toContainText('悬挂'); await expect(page.locator('#edit-undo')).toBeDisabled();
  for (const patch of [{right: '450'}, {left: '1', first: '-2'}, {left: '.001', first: '0'}]) {
    await fill(page, patch); await expect(page.locator(`${root} p`)).toHaveAttribute('data-indent-left-twips', '360'); await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.waitForTimeout(800); expect(saved.writes()).toBe(0);
  await page.locator('#edit-paragraph-indent-reset').click(); await expect(page.locator(`${root} p`)).not.toHaveAttribute('data-first-line-indent-twips');
  await expect.poll(() => saved.draft()?.content[0]?.attrs?.indentLeftTwips).toBeUndefined();
});

test('caret marks Enter split and Backspace merge retain signed indentation while unrelated blocks remain default', async ({page}) => {
  const saved = await open(page, document([paragraph('firstsecond', {indentLeftTwips: 360, firstLineIndentTwips: -180}), paragraph('untouched')]));
  await page.locator(`${root} p`).first().click(); await page.keyboard.press('Home'); for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Enter'); await expect(page.locator(`${root} p[data-first-line-indent-twips="-180"]`)).toHaveCount(2);
  await page.keyboard.press('Backspace'); await expect(page.locator(`${root} p[data-first-line-indent-twips="-180"]`)).toHaveCount(1);
  await expect.poll(() => saved.draft()?.content[0]?.content?.[0]?.text).toBe('firstsecond');
  expect(saved.draft().content[1].attrs?.firstLineIndentTwips).toBeUndefined();
});

test('narrowing page or automatic table column insertion refuses overflow before draft and undo', async ({page}) => {
  const saved = await open(page, document([table([paragraph('wide cell', {indentLeftTwips: 6000})])]));
  await page.locator(`${root} td p`).click(); await page.locator('#edit-column').click();
  await expect(page.locator('#notice')).toContainText('段落缩进'); await expect(page.locator(`${root} td`)).toHaveCount(1);
  await page.locator('#edit-page-size').selectOption('A5'); await page.locator('#edit-page-apply').click();
  await expect(page.locator('#notice')).toContainText('段落缩进'); await expect(page.locator('#edit-page-reset')).toBeDisabled(); await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.waitForTimeout(800); expect(saved.writes()).toBe(0);
});

test('lists disable indentation and conversion of already-indented body to a list is rejected', async ({page}) => {
  const original = document([paragraph('body', {indentLeftTwips: 20}), {type: 'bulletList', content: [{type: 'listItem', content: [paragraph('list')]}]}]);
  await open(page, original); await page.locator(`${root} li p`).click(); await expect(page.locator(apply)).toBeDisabled();
  await page.locator(`${root} > p`).click(); await page.locator('#edit-bulletList').click();
  await expect(page.locator('#notice')).toContainText('段落缩进'); await expect(page.locator(`${root} > p`)).toHaveText('body'); await expect(page.locator('#edit-undo')).toBeDisabled();
});

test('paste exact HTML signed CSS and immutable printing retain margins even after later edits', async ({page}) => {
  await open(page, document([paragraph('')])); await page.addInitScript(() => {window.print = () => {};});
  await page.evaluate(() => {window.print = () => {};}); await page.locator(root).focus();
  await page.evaluate(() => {
    const data = new DataTransfer(); data.setData('text/html', '<p data-indent-left-twips="361" style="margin-left:18.05pt;margin-right:0.05pt;text-indent:-18pt">Hanging paste</p>'); data.setData('text/plain', 'Hanging paste');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  });
  await expect(page.locator(`${root} p`)).toHaveAttribute('data-first-line-indent-twips', '-360');
  await page.locator('#edit-print').click(); const printed = await page.locator('#print-document p').getAttribute('style');
  expect(printed).toContain('text-indent: -18pt'); await page.locator(`${root} p`).click(); await fill(page, {first: '0'});
  await expect(page.locator('#print-document p')).toHaveAttribute('style', printed);
  await page.emulateMedia({media: 'print'}); expect(await page.locator('#print-document p').evaluate(node => parseFloat(getComputedStyle(node).textIndent))).toBeCloseTo(-24, 4);
});

test('new indentation cell side padding follows 108 twips and reset removes new table decoration', async ({page}) => {
  await open(page, document([table([paragraph('cell')], {columnWidthsTwips: [2000]})]));
  await page.locator(`${root} td p`).click(); await fill(page, {left: '0'});
  await expect(page.locator(`${root} [data-paragraph-indent-table]`)).toHaveCount(1); await expect(page.locator(`${root} td`)).toHaveCSS('padding-left', '7.2px');
  await page.locator('#edit-paragraph-indent-reset').click(); await expect(page.locator(`${root} [data-paragraph-indent-table]`)).toHaveCount(0);
});

test('invalid imported indents cannot replace active document and save locks block controls', async ({page}) => {
  let restore, finishSave;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return restore; if (command === 'save_draft') return;
    if (command === 'save_new_document') return new Promise(resolve => {finishSave = resolve;}); throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept()); await page.goto('/'); await page.locator('#new-document').click();
  await page.locator(root).click(); await page.keyboard.insertText('Keep active');
  for (const attrs of [{indentLeftTwips: null}, {indentRightTwips: 9026}, {firstLineIndentTwips: -1}]) {
    restore = document([paragraph('Do not replace', attrs)]); await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText('草稿恢复失败'); await expect(page.locator(root)).toHaveText('Keep active');
  }
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true); await expect(page.locator(apply)).toBeDisabled();
  await page.locator('#edit-paragraph-indent-reset').evaluate(button => {button.disabled = false; button.click();}); await expect(page.locator(root)).toHaveText('Keep active');
  finishSave(null); await expect(page.locator(apply)).toBeEnabled();
});
