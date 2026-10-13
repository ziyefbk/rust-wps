import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// Actual-browser acceptance. Discovery/parse only in the restricted current
// environment; model shims do not prove browser layout, focus or native print.
const editor = '#editor-content .tiptap', tables = `${editor} table`;
const p = text => ({type: 'paragraph', content: [{type: 'text', text}]});
const table = (attrs = {}, columns = attrs.columnWidthsTwips?.length ?? 3) => ({type: 'table', attrs, content:
  Array.from({length: 2}, (_, row) => ({type: 'tableRow', content: Array.from({length: columns}, (_, column) => ({type: row ? 'tableCell' : 'tableHeader', content: [p(`${row}:${column}`)]}))}))});
const doc = (content, attrs = {}) => ({type: 'doc', attrs: {styleProfile: 'standard-v2', ...attrs}, content: [...content, p('After table')]});
const attrs = draft => draft?.content?.filter(node => node.type === 'table').map(node => node.attrs ?? {});
const cell = (page, index = 0) => page.locator(tables).nth(index).locator('tr').nth(1).locator('td').first().locator('p');
async function open(page, content, extra = () => {}) {
  let draft, writes = 0;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return content;
    if (command === 'save_draft') {draft = structuredClone(args.doc); writes++; return;}
    return extra(command, args);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  return {draft: () => draft, writes: () => writes};
}
async function apply(page, left, right) {
  await page.locator('#edit-table-cell-margin-left').fill(left); await page.locator('#edit-table-cell-margin-right').fill(right);
  await page.locator('#edit-table-cell-margins-apply').click();
}

test('current table pair controls retain exact asymmetric and zero values through draft DOCX reset undo and redo', async ({page}, info) => {
  let exported, reopened;
  const saved = await open(page, doc([table({columnWidthsTwips: [1000, 2000, 3000]}), table()]), (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('cell-margins.json'), output = info.outputPath('cell-margins.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true})); exported = structuredClone(args.doc); return {path: output};
  });
  await cell(page).click(); await apply(page, '18.05', '9');
  await expect(page.locator(tables).first()).toHaveAttribute('data-table-cell-margins-twips', '361,180');
  await expect(page.locator(tables).last()).not.toHaveAttribute('data-table-cell-margins-twips');
  await expect.poll(() => attrs(saved.draft())?.[0].tableCellMarginsTwips).toEqual({left: 361, right: 180});
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(attrs(reopened.document ?? reopened)).toEqual(attrs(exported));
  await apply(page, '0', '0'); await expect(page.locator(tables).first()).toHaveAttribute('data-table-cell-margins-twips', '0,0');
  await page.locator('#edit-table-cell-margins-reset').click(); await expect(page.locator(tables).first()).not.toHaveAttribute('data-table-cell-margins-twips');
  await page.locator('#edit-undo').click(); await expect(page.locator(tables).first()).toHaveAttribute('data-table-cell-margins-twips', '0,0');
  await page.locator('#edit-redo').click(); await expect(page.locator(tables).first()).not.toHaveAttribute('data-table-cell-margins-twips');
});

test('direct-cell computed padding and left-border origins distinguish omitted legacy and explicit indent', async ({page}) => {
  await open(page, doc([table({columnWidthsTwips: [3000, 3000, 3000]}), table({columnWidthsTwips: [3000, 3000, 3000], tableIndentTwips: 720})]));
  const measure = index => page.locator(tables).nth(index).evaluate(table => ({left: table.getBoundingClientRect().left,
    padding: getComputedStyle(table.querySelector('td')).paddingLeft, borderOffset: getComputedStyle(table).marginLeft}));
  const first = await measure(0); await cell(page).click(); await apply(page, '18', '9');
  const after = await measure(0); expect(after.left).toBeCloseTo(first.left, 3); expect(after.padding).toBe('24px');
  await cell(page, 1).click(); await apply(page, '18', '9'); const indented = await measure(1);
  expect(parseFloat(indented.borderOffset)).toBeCloseTo((720 - 360) / 15, 3);
  await expect(page.locator('#edit-table-cell-margins-state')).toContainText('移动左边框');
  await page.locator('#edit-table-cell-margins-reset').click(); expect(parseFloat((await measure(1)).borderOffset)).toBeCloseTo((720 - 108) / 15, 3);
});

test('pair apply restores caret and stored bold mark while outside and spanning table selections disable editing', async ({page}) => {
  await open(page, doc([table(), p('middle'), table()])); await cell(page).click();
  await page.evaluate(() => {
    const text = document.querySelector('#editor-content .tiptap table td p').firstChild;
    window.getSelection().setBaseAndExtent(text, 1, text, 1);
    document.getElementById('edit-table-cell-margin-left').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await page.locator('#edit-bold').click(); await apply(page, '1', '2'); await page.keyboard.type('X');
  await expect(cell(page)).toContainText('1X:0'); await expect(cell(page).locator('strong')).toHaveText('X');
  await page.evaluate(() => {
    const tables = document.querySelectorAll('#editor-content .tiptap table');
    const first = tables[0].querySelector('p').firstChild, last = tables[1].querySelector('p').firstChild;
    window.getSelection().setBaseAndExtent(first, 0, last, 1);
    document.getElementById('edit-table-cell-margin-left').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  });
  await expect(page.locator('#edit-table-cell-margins-apply')).toBeDisabled();
  await page.locator(`${editor} > p`).last().click(); await expect(page.locator('#edit-table-cell-margin-left')).toBeDisabled();
});

test('partial malformed and unsafe geometry input no-ops locks and automatic column expansion are atomic', async ({page}) => {
  let finishSave;
  const saved = await open(page, doc([table()]), command => {
    if (command === 'save_new_document') return new Promise(resolve => {finishSave = resolve;}); throw new Error(command);
  });
  await cell(page).click(); await apply(page, '', ''); await expect(page.locator('#edit-undo')).toBeDisabled();
  for (const [left, right] of [['1', ''], ['.001', '0'], ['150', '0']]) {
    await apply(page, left, right); await expect(page.locator('#notice')).toHaveClass(/error/); await expect(page.locator(tables)).not.toHaveAttribute('data-table-cell-margins-twips');
  }
  await page.locator('#edit-table-cell-margin-left').press('Escape'); await page.waitForTimeout(800); expect(saved.writes()).toBe(0);
  await apply(page, '70', '70'); await page.locator('#edit-column').click(); await expect(page.locator('#notice')).toHaveClass(/error/); await expect(page.locator(tables).locator('tr').first().locator('th')).toHaveCount(3);
  await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  for (const id of ['edit-table-cell-margin-left', 'edit-table-cell-margin-right', 'edit-table-cell-margins-apply', 'edit-table-cell-margins-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  finishSave(null); await expect(page.locator('#edit-table-cell-margins-apply')).toBeEnabled();
});

test('canonical clipboard metadata survives and malformed or misplaced owned attributes reject the whole paste', async ({page}) => {
  await open(page, doc([table()])); await page.locator(`${editor} > p`).last().click();
  const paste = html => page.evaluate(html => {
    const data = new DataTransfer(); data.setData('text/html', html); data.setData('text/plain', 'pasted');
    document.querySelector('#editor-content .tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  }, html);
  await paste('<table data-table-cell-margins-twips="0,108"><tbody><tr><td><p>good</p></td></tr></tbody></table>');
  await expect(page.locator(tables)).toHaveCount(2); await expect(page.locator(tables).last()).toHaveAttribute('data-table-cell-margins-twips', '0,108');
  for (const html of ['<table data-table-cell-margins-twips="0,"><tr><td>bad</td></tr></table>', '<p DATA-TABLE-CELL-MARGINS-TWIPS="0,0">bad</p>']) {
    await paste(html); await expect(page.locator('#notice')).toHaveClass(/error/); await expect(page.locator(tables)).toHaveCount(2); await expect(page.locator(editor)).not.toContainText('bad');
  }
});

test('immutable print snapshot retains pair variables and padding across editor resets and next snapshot clears them', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};});
  await open(page, doc([table({columnWidthsTwips: [3000, 3000, 3000], tableIndentTwips: 720})]));
  await cell(page).click(); await apply(page, '18', '9'); await page.locator('#edit-print').click();
  const printed = page.locator('#print-document table'); await expect(printed).toHaveAttribute('data-table-cell-margins-twips', '360,180');
  expect(await printed.evaluate(table => [table.style.getPropertyValue('--word-cell-margin-left'), table.style.marginLeft])).toEqual(['18pt', '18pt']);
  await page.locator('#edit-table-cell-margins-reset').click(); await expect(page.locator(tables)).not.toHaveAttribute('data-table-cell-margins-twips');
  await expect(printed).toHaveAttribute('data-table-cell-margins-twips', '360,180');
  await page.locator('#edit-print').click(); await expect(printed).not.toHaveAttribute('data-table-cell-margins-twips');
  expect(await printed.evaluate(table => table.style.getPropertyValue('--word-cell-margin-left'))).toBe('');
});
