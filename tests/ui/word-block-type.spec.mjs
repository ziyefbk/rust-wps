import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const editor = '#editor-content .tiptap';
const attrs = [{textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180},
  {textAlign: 'right', spaceBefore: 240, spaceAfter: 0, lineSpacing: 360, indentLeftTwips: 0, indentRightTwips: 100, firstLineIndentTwips: 120}];
const p = (text, attributes) => ({type: 'paragraph', attrs: attributes, content: [{type: 'text', text, marks: [{type: 'bold'}]}]});

test('style toolbar preserves each selected block formatting draft and isolated undo redo', async ({page}) => {
  const original = {type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [p('First', attrs[0]), p('Second', attrs[1])]};
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(editor).click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-style').selectOption('2');
  await expect(page.locator(`${editor} h2`)).toHaveCount(2);
  await expect.poll(() => draft?.content.map(node => node.attrs)).toEqual(attrs.map(value => ({...value, level: 2})));
  await expect(page.locator(`${editor} h2`).first()).toHaveCSS('text-align', 'center');
  await expect(page.locator(`${editor} h2 strong`)).toHaveCount(2);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${editor} p`)).toHaveCount(2);
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await page.locator('#edit-redo').click(); await expect(page.locator(`${editor} h2`)).toHaveCount(2);
  await page.locator('#edit-style').selectOption('0');
  await expect.poll(() => draft?.content.map(node => node.attrs)).toEqual(attrs);
  await page.locator('#edit-undo').click(); await expect(page.locator(`${editor} h2`)).toHaveCount(2);
});

test('pending DOM selection converts only its blocks before selectionchange notification', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return {type: 'doc', content: [p('First', attrs[0]), p('Second', attrs[1]), p('Unselected', {})]};
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(`${editor} p`).last().click();
  await page.evaluate(() => {
    const blocks = document.querySelectorAll('.tiptap p strong');
    window.getSelection().setBaseAndExtent(blocks[1].firstChild, 3, blocks[0].firstChild, 1);
    const select = document.getElementById('edit-style'); select.value = '3'; select.dispatchEvent(new Event('change', {bubbles: true}));
  });
  await expect(page.locator(`${editor} h3`)).toHaveCount(2); await expect(page.locator(`${editor} p`)).toHaveText('Unselected');
  await expect.poll(() => draft?.content.slice(0, 2).map(node => node.attrs)).toEqual(attrs.map(value => ({...value, level: 3})));
});

test('mixed selection containing list first paragraph fails atomically and keeps draft and history clean', async ({page}) => {
  let writes = 0;
  const original = {type: 'doc', content: [p('Before', attrs[0]), {type: 'bulletList', content: [{type: 'listItem', content: [p('List first', {spaceBefore: 0, lineSpacing: 241})]}]}]};
  await nativeBridge(page, async command => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') {writes++; return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  await page.locator(editor).click(); await page.keyboard.press('Control+a'); await page.locator('#edit-style').selectOption('2');
  await expect(page.locator('#notice')).toContainText('列表首段须保留为正文');
  await expect(page.locator(`${editor} h2`)).toHaveCount(0); await expect(page.locator(`${editor} li`)).toHaveText('List first');
  await expect(page.locator('#edit-undo')).toBeDisabled(); await page.waitForTimeout(800); expect(writes).toBe(0);
});
