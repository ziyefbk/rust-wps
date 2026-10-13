import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';

// Discovery only in this environment. These require a real browser and are
// not replaced by the browser-free transaction/DOM-contract suite.
const editor = '#editor-content .tiptap';
const p = text => ({type: 'paragraph', content: [{type: 'text', text}]});
const documentWith = (text = 'short') => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [
  {type: 'table', attrs: {columnWidthsTwips: [3000, 3000]}, content: [0, 1].map(row => ({type: 'tableRow', content: [0, 1].map(column => ({
    type: row ? 'tableCell' : 'tableHeader', content: [p(`${row}:${column} ${text}`)]}))}))}, p('After table')]});
async function open(page, content = documentWith()) {
  let draft;
  await nativeBridge(page, (command, args) => {if (command === 'load_draft') return content; if (command === 'save_draft') {draft = structuredClone(args.doc); return;} throw new Error(command);});
  await page.goto('/'); await page.locator('#restore-draft').click(); return () => draft;
}
const row = page => page.locator(`${editor} tr`).nth(1);
const cell = page => row(page).locator('td').first();
async function height(page, value) {await page.locator('#edit-row-min-height').fill(value); await page.locator('#edit-row-min-height-apply').click();}

test('row minimum and cell alignment controls retain draft explicit zero top independent resets undo and outside disablement', async ({page}) => {
  const draft = await open(page); await cell(page).locator('p').click(); await height(page, '36.05');
  await page.locator('#edit-cell-vertical-align').selectOption('center');
  await expect(row(page)).toHaveAttribute('data-row-min-height-twips', '721'); await expect(cell(page)).toHaveAttribute('data-cell-vertical-align', 'center');
  await expect.poll(() => draft()?.content[0].content[1].attrs?.rowMinHeightTwips).toBe(721);
  await expect.poll(() => draft()?.content[0].content[1].content[0].attrs?.cellVerticalAlign).toBe('center');
  await height(page, '0'); await page.locator('#edit-cell-vertical-align').selectOption('top');
  await page.locator('#edit-row-min-height-reset').click(); await expect(row(page)).not.toHaveAttribute('data-row-min-height-twips');
  await expect(cell(page)).toHaveAttribute('data-cell-vertical-align', 'top');
  await page.locator('#edit-undo').click(); await expect(row(page)).toHaveAttribute('data-row-min-height-twips', '0');
  await page.locator('#edit-redo').click(); await expect(row(page)).not.toHaveAttribute('data-row-min-height-twips');
  await page.locator(`${editor} > p`).last().click(); await expect(page.locator('#edit-row-min-height')).toBeDisabled(); await expect(page.locator('#edit-cell-vertical-align')).toBeDisabled();
});

test('real CSS table minimum expands naturally for long content and vertical alignment moves short-cell text', async ({page}) => {
  const document = documentWith(); document.content[0].content[1].content[1].content = [p('Long cell text '.repeat(100))];
  await open(page, document); await cell(page).locator('p').click(); await height(page, '18');
  const measure = () => cell(page).evaluate(cell => ({cell: cell.getBoundingClientRect().height, text: cell.querySelector('p').getBoundingClientRect().top - cell.getBoundingClientRect().top,
    overflow: getComputedStyle(cell).overflow, display: getComputedStyle(cell).display}));
  const top = await measure(); expect(top.cell).toBeGreaterThan(24); expect(top.display).toBe('table-cell'); expect(top.overflow).not.toBe('hidden');
  await page.locator('#edit-cell-vertical-align').selectOption('center'); const middle = await measure(); expect(middle.text).toBeGreaterThan(top.text + 10);
  await page.locator('#edit-cell-vertical-align').selectOption('bottom'); const bottom = await measure(); expect(bottom.text).toBeGreaterThan(middle.text + 10);
  expect(bottom.cell).toBeCloseTo(top.cell, 2);
});

test('printing captures independent immutable row-height and vertical-alignment declarations', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};}); await open(page); await cell(page).locator('p').click();
  await height(page, '72'); await page.locator('#edit-cell-vertical-align').selectOption('bottom'); await page.locator('#edit-print').click();
  const printed = page.locator('#print-document tr').nth(1); await expect(printed).toHaveAttribute('data-row-min-height-twips', '1440');
  await expect(printed.locator('td').first()).toHaveCSS('vertical-align', 'bottom');
  await page.locator('#edit-row-min-height-reset').click(); await page.locator('#edit-cell-vertical-align-reset').click();
  await expect(row(page)).not.toHaveAttribute('data-row-min-height-twips'); await expect(printed).toHaveAttribute('data-row-min-height-twips', '1440');
  await page.locator('#edit-print').click(); await expect(printed).not.toHaveAttribute('data-row-min-height-twips');
  await expect(printed.locator('td').first()).not.toHaveAttribute('data-cell-vertical-align');
});
