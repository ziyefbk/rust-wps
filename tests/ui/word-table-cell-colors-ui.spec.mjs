import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';

// Browser-required specifications. Discovery is not execution: real native
// picker/clipboard, WebView2, and printed color/contrast remain unverified.
const editor = '#editor-content .tiptap';
const p = (text, marks) => ({type: 'paragraph', content: [{type: 'text', text, ...(marks ? {marks} : {})}]});
const documentWith = () => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [
  {type: 'table', content: [0, 1].map(row => ({type: 'tableRow', content: [0, 1].map(column => ({
    type: row ? 'tableCell' : 'tableHeader', content: [p(`${row}:${column} ordinary`)]}))}))}, p('After table')]});
async function open(page, document = documentWith()) {
  let draft;
  await nativeBridge(page, (command, args) => {if (command === 'load_draft') return document; if (command === 'save_draft') {draft = structuredClone(args.doc); return;} throw new Error(command);});
  await page.goto('/'); await page.locator('#restore-draft').click(); return () => draft;
}
const cell = page => page.locator(`${editor} tr`).nth(1).locator('td').first();
async function fill(page, value) {await page.locator('#edit-cell-fill').fill(value); await page.locator('#edit-cell-fill-apply').click();}

test('hex fill and automatic mode persist independently with defaults undo draft and outside disablement', async ({page}) => {
  const draft = await open(page); await cell(page).locator('p').click(); await fill(page, '#ab12ef');
  await page.locator('#edit-cell-text-color-mode').selectOption('auto');
  await expect(cell(page)).toHaveAttribute('data-cell-fill-rgb', 'AB12EF'); await expect(cell(page)).toHaveAttribute('data-cell-text-color-mode', 'auto');
  await expect.poll(() => draft()?.content[0].content[1].content[0].attrs?.cellFillRgb).toBe('AB12EF');
  await expect.poll(() => draft()?.content[0].content[1].content[0].attrs?.cellTextColorMode).toBe('auto');
  await page.locator('#edit-cell-fill-reset').click(); await expect(cell(page)).not.toHaveAttribute('data-cell-fill-rgb'); await expect(cell(page)).toHaveAttribute('data-cell-text-color-mode', 'auto');
  await page.locator('#edit-undo').click(); await expect(cell(page)).toHaveAttribute('data-cell-fill-rgb', 'AB12EF');
  await page.locator('#edit-redo').click(); await expect(cell(page)).not.toHaveAttribute('data-cell-fill-rgb');
  await fill(page, '000000'); await page.locator('#edit-cell-text-color-mode-reset').click(); await expect(cell(page)).not.toHaveAttribute('data-cell-text-color-mode'); await expect(cell(page)).toHaveAttribute('data-cell-fill-rgb', '000000');
  await page.locator(`${editor} > p`).last().click(); await expect(page.locator('#edit-cell-fill')).toBeDisabled(); await expect(page.locator('#edit-cell-text-color-mode')).toBeDisabled();
});

test('bounded preview contrasts dark and light fills while explicit text RGB and links retain their own colors', async ({page}) => {
  const document = documentWith(); document.content[0].content[1].content[0].content.push(
    p('explicit red', [{type: 'textColor', attrs: {rgb: 'FF0000'}}]),
    p('link', [{type: 'link', attrs: {href: 'https://example.com/'}}]),
    {type: 'heading', attrs: {level: 2}, content: [{type: 'text', text: 'heading'}]});
  await open(page, document); await cell(page).locator('p').first().click(); await fill(page, '000000'); await page.locator('#edit-cell-text-color-mode').selectOption('auto');
  await expect(cell(page)).toHaveCSS('background-color', 'rgb(0, 0, 0)'); await expect(cell(page).locator('p').first()).toHaveCSS('color', 'rgb(255, 255, 255)');
  await expect(cell(page).locator('h2')).toHaveCSS('color', 'rgb(255, 255, 255)'); await expect(cell(page).locator('[data-text-color]')).toHaveCSS('color', 'rgb(255, 0, 0)');
  await expect(cell(page).locator('a')).toHaveCSS('color', 'rgb(5, 99, 193)'); await expect(page.locator('#edit-cell-color-state')).toContainText('不代表 Word / LibreOffice');
  await fill(page, 'FFFFFF'); await expect(cell(page).locator('p').first()).toHaveCSS('color', 'rgb(0, 0, 0)');
  await page.locator('#edit-cell-fill-reset').click(); await expect(cell(page).locator('p').first()).toHaveCSS('color', 'rgb(0, 0, 0)');
});

test('mixed text-range cell selection applies exact cells once and untouched mixed Apply is non-destructive', async ({page}) => {
  const document = documentWith(); document.content[0].content[1].content[0].attrs = {cellFillRgb: '000000', cellTextColorMode: 'auto'};
  await open(page, document);
  await page.locator(`${editor} tr`).nth(1).evaluate(row => {
    const start = row.children[0].querySelector('p').firstChild, end = row.children[1].querySelector('p').firstChild;
    window.getSelection().setBaseAndExtent(start, 0, end, end.textContent.length);
  });
  await page.locator('#edit-cell-fill').click(); await expect(page.locator('#edit-cell-fill')).toHaveAttribute('placeholder', '混合');
  await page.locator('#edit-cell-fill-apply').click(); await expect(cell(page)).toHaveAttribute('data-cell-fill-rgb', '000000');
  await fill(page, 'ABCDEF'); await expect(page.locator(`${editor} [data-cell-fill-rgb="ABCDEF"]`)).toHaveCount(2);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${editor} [data-cell-fill-rgb]`)).toHaveCount(1);
});

test('adding a row or column does not copy neighboring authored cell fill and auto mode', async ({page}) => {
  await open(page); await cell(page).locator('p').click(); await fill(page, '000000'); await page.locator('#edit-cell-text-color-mode').selectOption('auto');
  await page.locator('#edit-row').click(); await expect(page.locator(`${editor} tr`)).toHaveCount(3);
  await expect(page.locator(`${editor} [data-cell-fill-rgb]`)).toHaveCount(1); await expect(page.locator(`${editor} [data-cell-text-color-mode]`)).toHaveCount(1);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${editor} tr`)).toHaveCount(2);
  await cell(page).locator('p').click(); await page.locator('#edit-column').click(); await expect(page.locator(`${editor} tr`).nth(1).locator('td')).toHaveCount(3);
  await expect(page.locator(`${editor} [data-cell-fill-rgb]`)).toHaveCount(1);
});

test('print snapshot retains fill and derived preview across reset until a new print captures defaults', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};}); await open(page); await cell(page).locator('p').click();
  await fill(page, '000000'); await page.locator('#edit-cell-text-color-mode').selectOption('auto'); await page.locator('#edit-print').click();
  const printed = page.locator('#print-document tr').nth(1).locator('td').first();
  await expect(printed).toHaveAttribute('data-cell-fill-rgb', '000000'); await expect(printed.locator('p')).toHaveCSS('color', 'rgb(255, 255, 255)');
  await page.locator('#edit-cell-fill-reset').click(); await page.locator('#edit-cell-text-color-mode-reset').click();
  await expect(cell(page)).not.toHaveAttribute('data-cell-fill-rgb'); await expect(printed).toHaveAttribute('data-cell-fill-rgb', '000000');
  await page.locator('#edit-print').click(); await expect(printed).not.toHaveAttribute('data-cell-fill-rgb'); await expect(printed).not.toHaveAttribute('data-cell-text-color-mode');
});

test('invalid fill and abandoned keyboard editing do not alter a cell', async ({page}) => {
  await open(page); await cell(page).locator('p').click(); await fill(page, '#GG0000'); await expect(cell(page)).not.toHaveAttribute('data-cell-fill-rgb');
  await page.locator('#edit-cell-fill').fill('123456'); await page.locator('#edit-cell-fill').press('Escape'); await expect(page.locator('#edit-cell-fill')).toHaveValue('');
  await page.locator('#edit-cell-fill').fill('123456'); await page.locator(`${editor} > p`).last().click(); await expect(page.locator('#edit-cell-fill')).toHaveValue('');
  await cell(page).locator('p').click(); await expect(cell(page)).not.toHaveAttribute('data-cell-fill-rgb');
});

test('no-fill is transparent on a header distinct from white default and print with draft undo', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};}); const draft = await open(page);
  const header = page.locator(`${editor} th`).first(); await header.locator('p').click();
  await expect(header).toHaveCSS('background-color', 'rgb(232, 238, 247)');
  await page.locator('#edit-cell-fill-none').click(); await expect(header).toHaveAttribute('data-cell-fill-mode', 'none');
  await expect(header).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)'); await expect(page.locator('#edit-cell-fill')).toHaveAttribute('placeholder', '无底色');
  await expect.poll(() => draft()?.content[0].content[0].content[0].attrs?.cellFillMode).toBe('none');
  await page.locator('#edit-print').click(); const printed = page.locator('#print-document th').first();
  await expect(printed).toHaveAttribute('data-cell-fill-mode', 'none'); await expect(printed).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await fill(page, 'FFFFFF'); await expect(header).not.toHaveAttribute('data-cell-fill-mode'); await expect(header).toHaveCSS('background-color', 'rgb(255, 255, 255)');
  await expect(printed).toHaveAttribute('data-cell-fill-mode', 'none');
  await page.locator('#edit-undo').click(); await expect(header).toHaveAttribute('data-cell-fill-mode', 'none');
  await page.locator('#edit-cell-fill-reset').click(); await expect(header).not.toHaveAttribute('data-cell-fill-mode'); await expect(header).toHaveCSS('background-color', 'rgb(232, 238, 247)');
});
