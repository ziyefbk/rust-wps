// Discovery-only browser acceptance specs. No browser, WebView, actual print
// pagination, system clipboard, or IME result is claimed by model tests.
import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const root = '#editor-content .tiptap';
const p = (text, attrs = {}) => ({type: 'paragraph', attrs, ...(text ? {content: [{type: 'text', text}]} : {})});
const doc = content => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content});
const flags = {keepNext: false, keepLines: true, pageBreakBefore: true};
async function open(page, content = doc([p('Firstsecond', flags), p('Following')])) {
  let draft;
  await nativeBridge(page, async (command, args) => {if (command === 'load_draft') return content; if (command === 'save_draft') {draft = structuredClone(args.doc); return;} throw new Error(command);});
  await page.goto('/'); await page.locator('#restore-draft').click(); await page.locator(`${root} > :first-child`).click(); return () => draft;
}
async function paste(page, html) {
  await page.locator(root).evaluate((element, html) => {const data = new DataTransfer(); data.setData('text/html', html); data.setData('text/plain', 'Paste');
    element.dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));}, html);
}
test('tri-state pagination controls preserve explicit false draft, field reset and atomic undo redo', async ({page}) => {
  const draft = await open(page); await expect(page.locator('#edit-keep-next')).toHaveValue('false');
  await page.locator('#edit-keep-lines').selectOption('false'); await expect(page.locator(`${root} > p`).first()).toHaveAttribute('data-word-keep-lines', 'false');
  await expect.poll(() => draft()?.content[0]?.attrs?.keepLines).toBe(false);
  await page.locator('#edit-undo').click(); await expect(page.locator('#edit-keep-lines')).toHaveValue('true');
  await page.locator('#edit-redo').click(); await expect(page.locator('#edit-keep-lines')).toHaveValue('false');
  await page.locator('#edit-keep-next').selectOption('default'); await expect.poll(() => draft()?.content[0]?.attrs?.keepNext).toBeUndefined();
  await page.locator('#edit-pagination-reset').click(); await expect(page.locator(`${root} [data-word-page-break-before]`)).toHaveCount(0);
});
test('mixed selection controls respect pending DOM range and reject list or cell movement', async ({page}) => {
  const draft = await open(page, doc([p('First', flags), p('Second', {keepNext: true})]));
  await page.evaluate(() => {const blocks = document.querySelectorAll('.tiptap p'); window.getSelection().setBaseAndExtent(blocks[0].firstChild, 1, blocks[1].firstChild, 2);
    document.getElementById('edit-keep-next').dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));});
  await expect(page.locator('#edit-keep-next')).toHaveValue('mixed'); await page.locator('#edit-keep-next').selectOption('false');
  await expect.poll(() => draft()?.content.slice(0, 2).map(node => node.attrs?.keepNext)).toEqual([false, false]);
  const before = await page.locator(root).innerHTML(); await page.locator('#edit-bulletList').click(); await expect(page.locator('#notice')).toContainText('段落分页'); expect(await page.locator(root).innerHTML()).toBe(before);
});
test('real Enter keeps break-before on leading paragraph and Backspace join is refused until reset', async ({page}) => {
  const draft = await open(page); await page.keyboard.press('Home'); for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Enter'); await expect(page.locator(`${root} > p`)).toHaveCount(3);
  await expect(page.locator(`${root} > p`).nth(0)).toHaveAttribute('data-word-page-break-before', 'true'); await expect(page.locator(`${root} > p`).nth(1)).not.toHaveAttribute('data-word-page-break-before');
  await expect.poll(() => draft()?.content[1]?.attrs?.keepNext).toBe(false); await page.keyboard.press('Backspace'); await expect(page.locator('#notice')).toContainText('段落分页'); await expect(page.locator(`${root} > p`)).toHaveCount(3);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} > p`)).toHaveCount(2);
});
test('body heading shortcut conversion preserves false and unsupported selection disables controls', async ({page}) => {
  await open(page, doc([p('Heading', flags), {type: 'bulletList', content: [{type: 'listItem', content: [p('list')]}]}]));
  await page.keyboard.press('Control+Alt+2'); await expect(page.locator(`${root} > h2`)).toHaveAttribute('data-word-keep-next', 'false');
  await page.locator(`${root} li p`).click(); for (const id of ['edit-keep-next', 'edit-keep-lines', 'edit-page-break-before']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.locator(root).click(); await page.keyboard.press('Control+a'); await expect(page.locator('#edit-keep-next')).toBeDisabled();
});
test('closed paragraph and open inline paste retain destination flags; nested and malformed metadata reject', async ({page}) => {
  await open(page, doc([p('')])); await page.keyboard.press('Home');
  await paste(page, '<p data-pm-slice="0 0 []" data-word-keep-next="false" data-word-keep-lines="true">Paste</p>');
  await expect(page.locator(`${root} [data-word-keep-next="false"]`)).toHaveCount(1);
  await page.keyboard.press('Home');
  await paste(page, '<p data-pm-slice="1 1 []" data-word-keep-next="true">Inline</p>');
  await expect(page.locator(`${root} [data-word-keep-next="false"]`)).toHaveCount(1);
  await expect(page.locator(`${root} [data-word-keep-next="true"]`)).toHaveCount(0);
  for (const html of ['<p data-pm-slice="1 1 []" data-word-keep-next="true">Paste</p><p>Next</p>', '<p data-word-keep-next="0">Paste</p>', '<ul><li><p data-word-keep-next="false">Paste</p></li></ul>']) {
    const before = await page.locator(root).innerHTML(); await paste(page, html); await expect(page.locator('#notice')).toContainText('段落分页'); expect(await page.locator(root).innerHTML()).toBe(before);
  }
});
test('immutable print clone false overrides heading default and subsequent edits do not alter it', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};}); await open(page, doc([{...p('Heading', {...flags, level: 2, pageBreakBefore: false}), type: 'heading'}, p('Following')]));
  await page.locator('#edit-print').click(); const printed = page.locator('#print-document h2');
  await expect(printed).toHaveAttribute('data-word-keep-next', 'false'); await page.locator('#edit-keep-next').selectOption('true'); await expect(printed).toHaveAttribute('data-word-keep-next', 'false');
  await page.emulateMedia({media: 'print'}); await expect(printed).toHaveCSS('break-after', 'auto'); await expect(printed).toHaveCSS('break-inside', 'avoid'); await expect(printed).toHaveCSS('break-before', 'auto');
});

test('ordinary imported all-false paragraphs merge with keyboard and one undo without losing marks', async ({page}) => {
  const allFalse = {keepNext: false, keepLines: false, pageBreakBefore: false};
  const draft = await open(page, doc([p('First', allFalse), p('Second', allFalse)]));
  await page.locator(`${root} p`).nth(1).click(); await page.keyboard.press('Home'); await page.keyboard.press('Backspace');
  await expect(page.locator(`${root} p`)).toHaveCount(1); await expect(page.locator(`${root} p`)).toHaveText('FirstSecond');
  await expect.poll(() => draft()?.content[0]?.attrs?.keepNext).toBe(false);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} p`)).toHaveCount(2);
});
