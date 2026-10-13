import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

const editor = '#editor-content .tiptap';
const table = `${editor} table`;
const makeDocument = (widths = [1500, 2400, 3600]) => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [
  {type: 'table', ...(widths ? {attrs: {columnWidthsTwips: widths}} : {}), content: Array.from({length: 3}, (_, row) => ({
    type: 'tableRow', content: Array.from({length: widths?.length ?? 3}, (_, column) => ({type: row ? 'tableCell' : 'tableHeader',
      content: [{type: 'paragraph', content: [{type: 'text', text: `R${row + 1}C${column + 1}`}]}]})),
  }))}, {type: 'paragraph', content: [{type: 'text', text: 'After table'}]},
]});
const findTable = document => document?.content?.find(node => node.type === 'table');
const cellParagraph = (page, row, column) => page.locator(`${table} tr`).nth(row).locator('th,td').nth(column).locator('p');
async function applyWidth(page, value) {
  await page.locator('#edit-column-width').fill(value);
  await page.locator('#edit-column-width-apply').click();
}

test('unequal fixed widths edit across all rows, persist to draft and real DOCX, reset and undo without stale columns', async ({page}, info) => {
  let draft, exported, reopened;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return makeDocument();
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    if (command === 'save_new_document') {
      const model = info.outputPath('widths.json'), output = info.outputPath('widths.docx');
      mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
      const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
      execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
      reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
      exported = structuredClone(args.doc); return {path: output};
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await cellParagraph(page, 1, 1).click();
  await expect(page.locator('#edit-column-width')).toHaveValue('120');
  await expect(page.locator(`${table} col`).nth(1)).toHaveCSS('width', '160px');
  await applyWidth(page, '100.55');
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2011,3600');
  await expect.poll(() => findTable(draft)?.attrs?.columnWidthsTwips).toEqual([1500, 2011, 3600]);
  for (const row of findTable(draft).content) for (const cell of row.content) expect(cell.attrs ?? {}).not.toHaveProperty('colwidth');
  const boxes = await page.locator(`${table} tr`).evaluateAll(rows => rows.map(row => [...row.children].map(cell => cell.getBoundingClientRect().width)));
  for (const row of boxes.slice(1)) for (let column = 0; column < 3; column++) expect(Math.abs(row[column] - boxes[0][column])).toBeLessThan(.1);
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(findTable(exported).attrs.columnWidthsTwips).toEqual([1500, 2011, 3600]);
  expect(findTable(reopened.document ?? reopened).attrs.columnWidthsTwips).toEqual([1500, 2011, 3600]);
  await page.locator('#edit-column-width-reset').click();
  await expect(page.locator(table)).not.toHaveAttribute('data-column-widths-twips');
  expect(await page.locator(`${table} col`).evaluateAll(cols => cols.map(col => col.style.width))).toEqual(['', '', '']);
  await expect.poll(() => findTable(draft)?.attrs?.columnWidthsTwips).toBeUndefined();
  await page.locator('#edit-undo').click();
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2011,3600');
  await page.locator('#edit-redo').click();
  expect(await page.locator(`${table} col`).evaluateAll(cols => cols.map(col => col.style.width))).toEqual(['', '', '']);
  await page.locator('#edit-undo').click();
  const restoredWidths = await page.locator(`${table} col`).evaluateAll(cols => cols.map(col => parseFloat(col.style.width)));
  for (const [index, twips] of [1500, 2011, 3600].entries()) expect(restoredWidths[index]).toBeCloseTo(twips / 15, 3);
});

test('structural toolbar operations retain first, middle and last unequal widths and undo once', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return makeDocument();
    if (command === 'save_draft') {draft = args.doc; return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  for (let column = 0; column < 3; column++) {
    await cellParagraph(page, 2, column).click(); await page.locator('#edit-delete-column').click();
    const expected = [1500, 2400, 3600]; expected.splice(column, 1);
    await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', expected.join(','));
    await expect(page.locator(`${table} col`)).toHaveCount(2);
    await expect(page.locator('#edit-column-width')).toBeEnabled();
    await expect.poll(() => findTable(draft)?.attrs?.columnWidthsTwips).toEqual(expected);
    await page.locator('#edit-undo').click();
    await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2400,3600');
  }
  await cellParagraph(page, 1, 1).click(); await page.locator('#edit-column').click();
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2400,2400,3600');
  await page.locator('#edit-undo').click();
  await cellParagraph(page, 2, 2).click(); await page.locator('#edit-delete-row').click();
  await expect(page.locator(`${table} tr`)).toHaveCount(2);
  await expect(page.locator('#edit-column-width')).toBeEnabled();
  await page.locator('#edit-undo').click();
  await expect(page.locator(`${table} tr`)).toHaveCount(3);
  await page.locator('#edit-delete-table').click(); await expect(page.locator(table)).toHaveCount(0);
  await page.locator('#edit-undo').click(); await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2400,3600');
});

test('range and sum errors, save lock and Escape leave draft and history untouched', async ({page}) => {
  let writes = 0, finishSave;
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return makeDocument([15000, 16000]);
    if (command === 'save_draft') {writes++; return;}
    if (command === 'save_new_document') return new Promise(resolve => {finishSave = resolve;});
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click(); await cellParagraph(page, 0, 0).click();
  for (const invalid of ['0', '-1', '.001', '1584.05', '800']) {
    await applyWidth(page, invalid); await expect(page.locator('#notice')).toHaveClass(/error/);
    await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '15000,16000');
    await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.locator('#edit-column-width').press('Escape'); await expect(page.locator('#edit-column-width')).toHaveValue('750');
  await page.locator('#edit-column').click(); await expect(page.locator('#notice')).toContainText('整表总宽');
  await expect(page.locator(`${table} col`)).toHaveCount(2);
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  await expect(page.locator('#edit-column-width')).toBeDisabled(); await expect(page.locator('#edit-column')).toBeDisabled();
  await page.locator('#edit-column-width-reset').evaluate(button => {button.disabled = false; button.click();});
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '15000,16000');
  finishSave(null); await expect(page.locator('#edit-column-width')).toBeEnabled();
  await page.waitForTimeout(800); expect(writes).toBe(0);
});

test('Tab adds a final row through fixed-safe commands and typing remains a separate undo step', async ({page}) => {
  await nativeBridge(page, async command => {if (command === 'load_draft') return makeDocument(); if (command === 'save_draft') return; throw new Error(command);});
  await page.goto('/'); await page.locator('#restore-draft').click(); await cellParagraph(page, 2, 2).click();
  await page.keyboard.press('Tab'); await expect(page.locator(`${table} tr`)).toHaveCount(4);
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2400,3600');
  await page.locator('#edit-undo').click(); await expect(page.locator(`${table} tr`)).toHaveCount(3);
  await cellParagraph(page, 1, 1).click(); await applyWidth(page, '100');
  await page.keyboard.type('Later'); await page.locator('#edit-undo').click();
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2000,3600');
  await expect(page.locator(table)).not.toContainText('Later');
});

test('HTML copy metadata, immutable print clone and auto reset retain one canonical geometry source', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {if (command === 'save_draft') {draft = args.doc; return;} throw new Error(command);});
  await page.addInitScript(() => {window.print = () => {window.printCalls = (window.printCalls ?? 0) + 1;};});
  await page.goto('/'); await page.locator('#new-document').click(); await page.locator(editor).focus();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<table data-column-widths-twips="1500,2011"><colgroup><col style="width:100px"><col style="width:134.06666666666666px"></colgroup><tbody><tr><td><p>Left</p></td><td><p>Right</p></td></tr></tbody></table>');
    data.setData('text/plain', 'Left\tRight');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  });
  await expect(page.locator(table)).toHaveAttribute('data-column-widths-twips', '1500,2011');
  await expect.poll(() => findTable(draft)?.attrs?.columnWidthsTwips).toEqual([1500, 2011]);
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  const print = '#print-document table';
  const printedStyle = await page.locator(print).getAttribute('style');
  expect(printedStyle).toBe(await page.locator(table).getAttribute('style'));
  expect(await page.locator(`${print} col`).evaluateAll(cols => cols.map(col => col.style.width))).toEqual(await page.locator(`${table} col`).evaluateAll(cols => cols.map(col => col.style.width)));
  await cellParagraph(page, 0, 0).click(); await page.locator('#edit-column-width-reset').click();
  await expect(page.locator(print)).toHaveAttribute('data-column-widths-twips', '1500,2011');
  await expect(page.locator(print)).toHaveAttribute('style', printedStyle);
  await page.locator('#edit-print').click();
  await expect(page.locator(print)).not.toHaveAttribute('data-column-widths-twips');
  expect(await page.locator(`${print} col`).evaluateAll(cols => cols.map(col => col.style.width))).toEqual(['', '']);
});

test('auto initialization explains defaults and pending DOM selection chooses the current column', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {if (command === 'load_draft') return makeDocument(null); if (command === 'save_draft') {draft = args.doc; return;} throw new Error(command);});
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.evaluate(() => {
    const target = document.querySelectorAll('.tiptap table tr')[1].children[2].querySelector('p').firstChild;
    window.getSelection().setBaseAndExtent(target, 1, target, 1);
    document.getElementById('edit-column-width').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await expect(page.locator('#edit-column-width-state')).toContainText('第 3 列');
  await applyWidth(page, '100.05');
  await expect(page.locator('#notice')).toContainText('450 pt');
  await expect.poll(() => findTable(draft)?.attrs?.columnWidthsTwips).toEqual([3000, 3000, 2001]);
  await page.locator('#edit-undo').click(); await expect(page.locator(table)).not.toHaveAttribute('data-column-widths-twips');
});
