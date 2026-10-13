import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// Discovery-only while browser/native execution is blocked. These are future
// acceptance scenarios, not evidence for CSS layout, actual focus or printing.
const geometry = extra => ({widthTwips: 11906, heightTwips: 16838, marginTopTwips: 1440, marginRightTwips: 1440, marginBottomTwips: 1440, marginLeftTwips: 1440, ...extra});
const paragraph = text => ({type: 'paragraph', content: text ? [{type: 'text', text}] : []});
const document = (pageGeometry, content = [paragraph('Page geometry probe')]) => ({type: 'doc', attrs: {styleProfile: 'standard-v2', ...(pageGeometry === undefined ? {} : {pageGeometry})}, content});
const root = '#editor-content .tiptap';
async function open(page, content, extra = () => {}) {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return content;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    return extra(command, args);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  return () => draft;
}
async function apply(page, size = 'Letter', orientation = 'landscape') {
  await page.locator('#edit-page-size').selectOption(size);
  await page.locator('#edit-page-orientation').selectOption(orientation);
  for (const [side, value] of [['top', '18.05'], ['right', '36'], ['bottom', '54'], ['left', '72.05']]) await page.locator(`#edit-page-margin-${side}`).fill(value);
  await page.locator('#edit-page-apply').click();
}

test('paper direction and asymmetric margins survive draft DOCX trusted reopen and grouped undo redo', async ({page}, info) => {
  let exported, reopened;
  const draft = await open(page, document(undefined), (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('page.json'), output = info.outputPath('page.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
    exported = structuredClone(args.doc); return {path: output};
  });
  await apply(page);
  const expected = geometry({widthTwips: 15840, heightTwips: 12240, marginTopTwips: 361, marginRightTwips: 720, marginBottomTwips: 1080, marginLeftTwips: 1441});
  await expect.poll(() => draft()?.attrs?.pageGeometry).toEqual(expected);
  await expect(page.locator('#authoring-sheet')).toHaveCSS('width', '1056px');
  await page.locator('#edit-undo').click(); await expect(page.locator('#edit-page-reset')).toBeDisabled();
  await page.locator('#edit-redo').click(); await expect(page.locator('#edit-page-orientation')).toHaveValue('landscape');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(reopened.document ?? reopened).toEqual(exported);
});

test('untouched source tolerance twips remain exact during margin edit and orientation reversal', async ({page}) => {
  const draft = await open(page, document(geometry({widthTwips: 11907, heightTwips: 16837})));
  await page.locator('#edit-page-margin-left').fill('80.05'); await page.locator('#edit-page-apply').click();
  await expect.poll(() => draft()?.attrs?.pageGeometry?.widthTwips).toBe(11907);
  await page.locator('#edit-page-orientation').selectOption('landscape'); await page.locator('#edit-page-apply').click();
  await expect.poll(() => draft()?.attrs?.pageGeometry).toEqual(geometry({widthTwips: 16837, heightTwips: 11907, marginLeftTwips: 1601}));
});

test('A5 rejects positioned automatic table and explains fixed-column prerequisite', async ({page}) => {
  const table = {type: 'table', attrs: {tableAlign: 'left'}, content: [{type: 'tableRow', content: [{type: 'tableCell', content: [paragraph('wide automatic table')]}]}]};
  await open(page, document(undefined, [table]));
  await page.locator('#edit-page-size').selectOption('A5'); await page.locator('#edit-page-apply').click();
  await expect(page.locator('#notice')).toContainText('定位自动表格按 450 pt'); await expect(page.locator('#edit-undo')).toBeDisabled();
  await expect(page.locator(`${root} table`)).toContainText('wide automatic table'); await expect(page.locator('#edit-page-reset')).toBeDisabled();
});

test('page-aware implicit image uses exporter frame in editor and immutable snapshot without dimension metadata mutation', async ({page}) => {
  const src = `data:image/png;base64,${readFileSync('tests/fixtures/slide-image.png').toString('base64')}`;
  const content = document(geometry(), [{type: 'image', attrs: {src}}, paragraph('after')]);
  await page.addInitScript(() => {window.print = () => {};}); await open(page, content);
  await expect(page.locator(`${root} img`)).toHaveAttribute('data-page-natural-image', '');
  const before = await page.locator(`${root} img`).boundingBox();
  await page.locator('#edit-print').click(); await expect(page.locator('#notice')).toContainText('已准备所选页面');
  expect(await page.locator('#print-document style').textContent()).toContain('size: 595.3pt 841.9pt');
  await apply(page); expect(await page.locator('#print-document style').textContent()).toContain('size: 595.3pt 841.9pt');
  await page.emulateMedia({media: 'print'}); const after = await page.locator('#print-document img').boundingBox();
  expect(after.width).toBeCloseTo(before.width, 4); expect(after.height).toBeCloseTo(before.height, 4);
});

test('native IPC snapshot sends exact per-edge geometry and locks every page action until completion', async ({page}) => {
  let finish, args;
  const source = geometry({widthTwips: 15840, heightTwips: 12240, marginTopTwips: 1, marginRightTwips: 20, marginBottomTwips: 321, marginLeftTwips: 1441});
  await open(page, document(source), (command, value) => {
    if (command !== 'export_current_pdf') throw new Error(command);
    args = structuredClone(value); return new Promise(resolve => {finish = resolve;});
  });
  await page.locator('#edit-export-pdf').click(); await expect.poll(() => !!finish).toBe(true);
  expect(args).toEqual({wordStyleProfile: 'standard-v2', wordPageGeometry: source});
  for (const id of ['edit-page-size', 'edit-page-orientation', 'edit-page-margin-top', 'edit-page-margin-right', 'edit-page-margin-bottom', 'edit-page-margin-left', 'edit-page-apply', 'edit-page-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.evaluate(() => {document.getElementById('edit-page-margin-top').value = '99'; document.getElementById('edit-page-reset').dispatchEvent(new MouseEvent('click')); document.getElementById('edit-page-apply').dispatchEvent(new MouseEvent('click'));});
  expect(JSON.parse(await page.locator(root).getAttribute('data-page-geometry'))).toEqual(source);
  finish({path: 'output.pdf', bytes: 1}); await expect(page.locator('#notice')).toContainText('已另存 PDF');
});

test('default-page reset restores old profile printing and removes explicit-page snapshot style', async ({page}) => {
  let args;
  await open(page, document(geometry()), (command, value) => {if (command === 'export_current_pdf') {args = value; return null;}});
  await page.locator('#edit-page-reset').click(); await expect(page.locator('#authoring-sheet')).not.toHaveAttribute('data-page-geometry');
  await page.locator('#edit-export-pdf').click(); await expect(page.locator('#notice')).toContainText('已取消 PDF');
  expect(args).toEqual({wordStyleProfile: 'standard-v2'});
});

for (const value of [null, geometry({marginLeftTwips: -1}), {...geometry(), extra: 0}, geometry({widthTwips: 10000})]) test(`invalid incoming page never replaces editor: ${JSON.stringify(value)}`, async ({page}) => {
  await open(page, document(value)); await expect(page.locator('#notice')).toContainText('页面'); await expect(page.locator('#authoring-sheet')).toBeHidden();
});


test('Letter and A5 automatic table reflow stays automatic through print snapshot and page reset', async ({page}) => {
  const table = {type: 'table', content: [{type: 'tableRow', content: [{type: 'tableCell', content: [paragraph('automatic table')]}]}]};
  await page.addInitScript(() => {window.print = () => {};});
  const draft = await open(page, document(undefined, [table]));
  await page.locator('#edit-page-size').selectOption('Letter'); await page.locator('#edit-page-margin-left').fill('90'); await page.locator('#edit-page-margin-right').fill('90'); await page.locator('#edit-page-apply').click();
  await expect(page.locator(`${root} table`)).toHaveCSS('width', '576px');
  await page.locator('#edit-page-size').selectOption('A5'); await page.locator('#edit-page-margin-left').fill('72'); await page.locator('#edit-page-margin-right').fill('72'); await page.locator('#edit-page-apply').click();
  await expect.poll(() => draft()?.attrs?.pageGeometry?.widthTwips).toBe(8391);
  expect(draft().content[0].attrs?.columnWidthsTwips).toBeUndefined();
  await page.locator('#edit-print').click(); await expect(page.locator('#notice')).toContainText('已准备所选页面');
  expect(await page.locator('#print-document [data-page-auto-table]').getAttribute('style')).toContain('275.55pt');
  await page.locator('#edit-page-reset').click(); await expect(page.locator(`${root} [data-page-auto-table]`)).toHaveCount(0);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} [data-page-auto-table]`)).toHaveCount(1);
});
