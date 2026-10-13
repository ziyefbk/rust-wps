// Browser acceptance specifications. Discovery only until browser/native
// execution is authorized and available; model checks are not visual evidence.
import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const font = {family: 'Liberation Serif', halfPoints: 48, bold: true, italic: true};
const p = (value, text = '') => ({type: 'paragraph', ...(value ? {attrs: {emptyParagraphFont: value, spaceBefore: 0, spaceAfter: 0, lineSpacing: 240}} : {}),
  ...(text ? {content: [{type: 'text', text}]} : {})});
const doc = (...content) => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content});
const root = '#editor-content .tiptap', selected = `${root} [data-empty-paragraph-font]`;
async function open(page, content = doc(p(font), p(null, 'Following'))) {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return content;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click(); await page.locator(`${root} p`).first().click(); return () => draft;
}
async function paste(page, html, text = '') {
  await page.locator(root).evaluate((element, {html, text}) => {
    const transfer = new DataTransfer(); transfer.setData('text/html', html); transfer.setData('text/plain', text);
    element.dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: transfer}));
  }, {html, text});
}

test('imported empty font summary and explicit profile reset preserve draft spacing and atomic history', async ({page}) => {
  const draft = await open(page);
  await expect(page.locator('#edit-empty-paragraph-font-state')).toContainText('Liberation Serif · 24 pt · 粗体 · 斜体');
  await expect(page.locator('#edit-empty-paragraph-font-state')).toContainText('首次输入或粘贴内容后结束');
  await expect(page.locator(selected)).toHaveCSS('font-size', '32px');
  await expect(page.locator(selected)).toHaveCSS('font-weight', '700');
  await page.locator('#edit-empty-paragraph-font-reset').click(); await expect(page.locator(selected)).toHaveCount(0);
  await expect.poll(() => draft()?.content[0]?.attrs).toEqual({textAlign: null, spaceBefore: 0, spaceAfter: 0, lineSpacing: 240});
  await page.locator('#edit-undo').click(); await expect(page.locator(selected)).toHaveCount(1);
  await expect.poll(() => draft()?.content[0]?.attrs?.emptyParagraphFont).toEqual(font);
  await page.locator('#edit-redo').click(); await expect(page.locator(selected)).toHaveCount(0);
});

test('first browser typing uses typing controls and deletion never revives the imported empty override', async ({page}) => {
  const draft = await open(page);
  await page.locator('#edit-font-family').fill('Arial'); await page.locator('#edit-font-family-apply').click();
  await page.locator('#edit-font-size').fill('10.5'); await page.locator('#edit-font-size-apply').click();
  await expect(page.locator(selected)).toHaveCount(1);
  await page.keyboard.type('x'); await expect(page.locator(selected)).toHaveCount(0);
  await expect(page.locator(`${root} p`).first()).toHaveText('x');
  await expect.poll(() => draft()?.content[0]?.content?.[0]?.marks).toEqual([{type: 'fontSize', attrs: {halfPoints: 21}}, {type: 'fontFamily', attrs: {name: 'Arial'}}]);
  await page.keyboard.press('Backspace'); await expect(page.locator(`${root} p`).first()).toHaveText('');
  await expect(page.locator(selected)).toHaveCount(0);
});

test('empty heading keyboard conversion retains baseline until its first real text insertion', async ({page}) => {
  await open(page); await page.keyboard.press('Control+Alt+2');
  await expect(page.locator(`${root} h2[data-empty-paragraph-font]`)).toHaveCount(1);
  await expect(page.locator(`${root} h2`)).toHaveCSS('font-size', '32px');
  await page.keyboard.type('Heading'); await expect(page.locator(selected)).toHaveCount(0);
  await expect(page.locator(`${root} h2`)).toHaveCSS('font-size', '20px');
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} h2[data-empty-paragraph-font]`)).toHaveCount(1);
});

test('Enter split clears source and new empty blocks and undo restores only original imported empty paragraph', async ({page}) => {
  await open(page); await page.keyboard.press('Enter'); await expect(page.locator(selected)).toHaveCount(0);
  await expect(page.locator(`${root} p`)).toHaveCount(3);
  await page.locator('#edit-undo').click(); await expect(page.locator(selected)).toHaveCount(1); await expect(page.locator(`${root} p`)).toHaveCount(2);
  await page.keyboard.press('Delete'); await expect(page.locator(selected)).toHaveCount(0);
});

test('canonical empty HTML paste can retain metadata; malformed and nonempty metadata reject the complete paste', async ({page}) => {
  await open(page, doc(p(null), p(null, 'Following')));
  const canonical = JSON.stringify(font).replaceAll('"', '&quot;');
  await paste(page, `<p data-pm-slice="0 0 []" data-empty-paragraph-font="${canonical}"></p>`);
  await expect(page.locator(selected)).toHaveCount(1);
  for (const html of ['<p data-empty-paragraph-font="null">bad</p>', `<p data-empty-paragraph-font="${canonical}">bad</p>`,
    `<span data-empty-paragraph-font="${canonical}">bad</span>`]) {
    const before = await page.locator(root).innerHTML(); await paste(page, html, 'bad');
    await expect(page.locator('#notice')).toContainText('空段字体'); expect(await page.locator(root).innerHTML()).toBe(before);
  }
});

test('print clone freezes empty baseline without synthetic text or changing editor defaults', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};}); await open(page);
  await page.locator('#edit-print').click(); const print = page.locator('#print-document [data-empty-paragraph-font]');
  await expect(print).toHaveCount(1); await expect(print).toHaveText('');
  expect(await print.getAttribute('style')).toContain('font-size: 24pt');
  await page.locator('#edit-empty-paragraph-font-reset').click(); await expect(page.locator(selected)).toHaveCount(0);
  expect(await print.getAttribute('style')).toContain('font-size: 24pt');
  await page.emulateMedia({media: 'print'}); await expect(print).toHaveCSS('font-size', '32px');
});

test('malformed raw draft metadata rejects before replacing the currently open new document', async ({page}) => {
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return doc(p({...font, extra: true}));
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#new-document').click(); const before = await page.locator(root).innerHTML();
  await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText('空段字体');
  expect(await page.locator(root).innerHTML()).toBe(before);
});
