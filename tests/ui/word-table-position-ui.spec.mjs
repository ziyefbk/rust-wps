import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// Browser acceptance scenarios. In restricted cloud environments these are
// discovery-only; the model suite does not substitute for focus/layout checks.
const editor = '#editor-content .tiptap', tables = `${editor} table`;
const makeTable = (attrs = {}) => ({type: 'table', attrs: {columnWidthsTwips: [1000, 2000, 3000], ...attrs}, content:
  Array.from({length: 2}, (_, row) => ({type: 'tableRow', content: Array.from({length: 3}, (_, col) => ({type: row ? 'tableCell' : 'tableHeader',
    content: [{type: 'paragraph', content: [{type: 'text', text: `R${row + 1}C${col + 1}`}]}]}))}))});
const documentWith = (...content) => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [...content,
  {type: 'paragraph', content: [{type: 'text', text: 'After table'}]}]});
const tableAttrs = draft => draft?.content?.filter(node => node.type === 'table').map(node => node.attrs ?? {});
const cell = (page, table = 0) => page.locator(tables).nth(table).locator('tr').nth(1).locator('td').nth(1).locator('p');
const applyIndent = async (page, value) => {await page.locator('#edit-table-indent').fill(value); await page.locator('#edit-table-indent-apply').click();};

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

test('whole-table alignment and exact indent persist in draft and real DOCX with separate reset and undo', async ({page}, info) => {
  let exported, reopened;
  const draft = await open(page, documentWith(makeTable()), (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('position.json'), output = info.outputPath('position.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
    exported = structuredClone(args.doc); return {path: output};
  });
  await cell(page).click();
  await expect(page.locator('#edit-table-align')).toHaveValue(''); await expect(page.locator('#edit-table-indent')).toHaveValue('');
  await page.locator('#edit-table-align').selectOption('left'); await applyIndent(page, '36.05');
  await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '721');
  expect(await page.locator(tables).evaluate(table => parseFloat(getComputedStyle(table).marginLeft))).toBeCloseTo((721 - 108) / 15, 3);
  await expect.poll(() => tableAttrs(draft())?.[0]).toEqual({columnWidthsTwips: [1000, 2000, 3000], tableAlign: 'left', tableIndentTwips: 721});
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(tableAttrs(exported)).toEqual(tableAttrs(reopened.document ?? reopened));
  await page.locator('#edit-table-align').selectOption('');
  await expect(page.locator(tables)).not.toHaveAttribute('data-table-align'); await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '721');
  await applyIndent(page, '0'); await expect(page.locator(tables)).toHaveCSS('margin-left', '-7.2px');
  await applyIndent(page, ''); await expect(page.locator(tables)).not.toHaveAttribute('data-table-position');
  await page.locator('#edit-undo').click(); await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '0');
  await page.locator('#edit-redo').click(); await expect(page.locator(tables)).not.toHaveAttribute('data-table-indent-twips');
  await expect(page.locator(tables)).toHaveAttribute('data-column-widths-twips', '1000,2000,3000');
});

test('center/right visibly align fixed tables, clear positive indent atomically and keep explicit zero', async ({page}) => {
  const draft = await open(page, documentWith(makeTable({tableIndentTwips: 720})));
  await cell(page).click();
  await page.locator('#edit-table-align').selectOption('center');
  await expect(page.locator(tables)).not.toHaveAttribute('data-table-indent-twips');
  await expect.poll(() => tableAttrs(draft())?.[0].tableAlign).toBe('center');
  const measure = () => page.locator(tables).evaluate(table => {
    const a = table.getBoundingClientRect(), b = table.parentElement.getBoundingClientRect();
    return {left: a.left - b.left, right: b.right - a.right};
  });
  const centered = await measure(); expect(Math.abs(centered.left - centered.right)).toBeLessThan(1);
  await page.locator('#edit-undo').click();
  await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '720'); await expect(page.locator('#edit-table-align')).toHaveValue('');
  await page.locator('#edit-table-align').selectOption('right'); expect(Math.abs((await measure()).right)).toBeLessThan(1);
  await applyIndent(page, '0'); await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '0');
  await applyIndent(page, '1'); await expect(page.locator('#notice')).toContainText('正缩进');
  await expect(page.locator('#edit-table-align')).toHaveValue('right'); await expect(page.locator(tables)).toHaveAttribute('data-table-indent-twips', '0');
  await page.locator('#edit-table-position-reset').click();
  await expect(page.locator(tables)).not.toHaveAttribute('data-table-position'); await expect(page.locator(tables)).toHaveAttribute('data-column-widths-twips', '1000,2000,3000');
});

test('mixed multi-table state and pending native DOM selection edit only explicitly chosen fields', async ({page}) => {
  const draft = await open(page, documentWith(makeTable({tableIndentTwips: 0}), makeTable({tableAlign: 'left'})));
  await page.evaluate(() => {
    const tables = document.querySelectorAll('.tiptap table');
    const first = tables[0].querySelector('p').firstChild, last = tables[1].querySelector('p').firstChild;
    window.getSelection().setBaseAndExtent(first, 1, last, 2);
    document.getElementById('edit-table-align').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await expect(page.locator('#edit-table-align')).toHaveValue('mixed'); await expect(page.locator('#edit-table-indent')).toHaveAttribute('placeholder', '混合');
  await expect(page.locator('#edit-table-position-state')).toHaveText('2 张表格');
  await page.locator('#edit-table-align').selectOption('center');
  await expect.poll(() => tableAttrs(draft())?.map(attrs => attrs.tableAlign)).toEqual(['center', 'center']);
  expect(tableAttrs(draft()).map(attrs => attrs.tableIndentTwips)).toEqual([0, undefined]);
  await page.locator('#edit-undo').click();
  await expect(page.locator(tables).nth(0)).toHaveAttribute('data-table-indent-twips', '0');
  await expect(page.locator(tables).nth(1)).toHaveAttribute('data-table-align', 'left');
});

test('invalid input, combined budget, Escape and save lock leave document and undo history intact', async ({page}) => {
  let finishSave;
  const draft = await open(page, documentWith(makeTable({columnWidthsTwips: [10000, 10000, 10000]})), command => {
    if (command === 'save_new_document') return new Promise(resolve => {finishSave = resolve;});
    throw new Error(command);
  });
  await cell(page).click();
  for (const value of ['-1', '.001', '1584.05', '84.05']) {
    await applyIndent(page, value); await expect(page.locator('#notice')).toHaveClass(/error/);
    await expect(page.locator(tables)).not.toHaveAttribute('data-table-position'); await expect(page.locator('#edit-undo')).toBeDisabled();
  }
  await page.locator('#edit-table-indent').press('Escape'); await expect(page.locator('#edit-table-indent')).toHaveValue('');
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  for (const id of ['edit-table-align', 'edit-table-indent', 'edit-table-indent-apply', 'edit-table-position-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.locator('#edit-table-align').evaluate(select => {select.disabled = false; select.value = 'center'; select.dispatchEvent(new Event('change'));});
  await expect(page.locator(tables)).not.toHaveAttribute('data-table-position');
  finishSave(null); await expect(page.locator('#edit-table-align')).toBeEnabled();
  await page.waitForTimeout(800); expect(draft()).toBeUndefined();
});

test('auto-positioned table normalizes base width and print snapshot retains then resets independent geometry', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};});
  const auto = makeTable(); delete auto.attrs.columnWidthsTwips;
  const draft = await open(page, documentWith(auto)); await cell(page).click();
  await page.locator('#edit-table-align').selectOption('left'); await applyIndent(page, '36');
  await expect(page.locator(tables)).toHaveCSS('table-layout', 'auto');
  expect(await page.locator(tables).evaluate(table => table.style.width)).toBe('600px');
  await page.locator('#edit-print').click();
  const printed = '#print-document table';
  await expect(page.locator(printed)).toHaveAttribute('data-table-indent-twips', '720');
  expect(await page.locator(printed).getAttribute('style')).toBe(await page.locator(tables).getAttribute('style'));
  await page.locator('#edit-table-position-reset').click();
  await expect(page.locator(tables)).not.toHaveAttribute('data-table-position');
  expect(await page.locator(tables).evaluate(table => [table.style.width, table.style.marginLeft, table.style.marginRight, table.style.tableLayout])).toEqual(['', '', '', '']);
  await expect(page.locator(printed)).toHaveAttribute('data-table-indent-twips', '720');
  await page.locator('#edit-print').click(); await expect(page.locator(printed)).not.toHaveAttribute('data-table-position');
  await expect.poll(() => tableAttrs(draft())?.[0]).toEqual({});
});

test('pasted canonical positioning metadata survives while foreign CSS is not promoted', async ({page}) => {
  const draft = await open(page, documentWith(makeTable()));
  await page.locator(`${editor} > p`).last().click();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData('text/html', '<table data-column-widths-twips="1500,2000" data-table-align="left" data-table-indent-twips="1"><tbody><tr><td><p>A</p></td><td><p>B</p></td></tr></tbody></table><p>gap</p><table style="margin-left:36pt;text-align:right"><tbody><tr><td><p>C</p></td></tr></tbody></table>');
    data.setData('text/plain', 'A\tB\ngap\nC');
    document.querySelector('.tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  });
  await expect(page.locator(tables)).toHaveCount(3);
  await expect(page.locator(tables).nth(1)).toHaveAttribute('data-table-indent-twips', '1');
  await expect(page.locator(tables).nth(2)).not.toHaveAttribute('data-table-position');
  await expect.poll(() => tableAttrs(draft())?.[1]).toEqual({columnWidthsTwips: [1500, 2000], tableAlign: 'left', tableIndentTwips: 1});
});
