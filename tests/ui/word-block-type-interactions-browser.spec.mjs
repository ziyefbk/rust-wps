// Browser specifications: discover only until an authorized browser is usable.
import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const editor = '#editor-content .tiptap';
const attrs = [{textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180},
  {textAlign: 'right', spaceBefore: 240, spaceAfter: 0, lineSpacing: 360, indentLeftTwips: 0, indentRightTwips: 100, firstLineIndentTwips: 120}];
const p = (text, value) => ({type: 'paragraph', attrs: value, ...(text ? {content: [{type: 'text', text}]} : {})});
async function open(page, original) {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return original;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-draft').click(); return () => draft;
}
for (const level of [1, 2, 3]) test(`keyboard Ctrl Alt ${level} preserves mixed paragraph values in saved draft`, async ({page}) => {
  const draft = await open(page, {type: 'doc', content: [p('First', attrs[0]), p('Second', attrs[1])]});
  await page.locator(editor).click(); await page.keyboard.press('Control+a'); await page.keyboard.press(`Control+Alt+${level}`);
  await expect(page.locator(`${editor} h${level}`)).toHaveCount(2);
  await expect.poll(() => draft()?.content.slice(0, 2).map(node => node.attrs)).toEqual(attrs.map(value => ({...value, level})));
  await page.locator('#edit-undo').click(); await expect(page.locator(`${editor} h${level}`)).toHaveCount(0);
});
test('keyboard heading toggle and Ctrl Alt zero both preserve paragraph formatting', async ({page}) => {
  const draft = await open(page, {type: 'doc', content: [p('First', attrs[0]), p('Second', attrs[1]), p('Outside', {})]});
  await page.locator(`${editor} p`).first().click(); await page.keyboard.press('Control+Alt+2'); await page.keyboard.press('Control+Alt+2');
  await expect.poll(() => draft()?.content[0].attrs).toEqual(attrs[0]);
  await page.keyboard.press('Control+Alt+3'); await page.keyboard.press('Control+Alt+0');
  await expect.poll(() => draft()?.content[0].attrs).toEqual(attrs[0]);
});
for (const level of [1, 2, 3]) test(`typed ${'#'.repeat(level)} marker keeps existing format and Backspace restores it`, async ({page}) => {
  const draft = await open(page, {type: 'doc', content: [p('', attrs[0]), p('Outside', attrs[1])]});
  await page.locator(`${editor} p`).first().click(); await page.keyboard.type(`${'#'.repeat(level)} `);
  await expect(page.locator(`${editor} h${level}`)).toHaveCount(1);
  await expect.poll(() => draft()?.content[0].attrs).toEqual({...attrs[0], level});
  await page.keyboard.press('Backspace'); await expect(page.locator(`${editor} p`).first()).toHaveText(`${'#'.repeat(level)} `);
  await expect.poll(() => draft()?.content[0].attrs).toEqual(attrs[0]);
});
test('Backspace undoes heading input rule and removes only its automatically appended paragraph', async ({page}) => {
  await open(page, {type: 'doc', content: [p('', attrs[0])]}); await page.locator(`${editor} p`).click(); await page.keyboard.type('## ');
  await expect(page.locator(`${editor} h2`)).toHaveCount(1); await expect(page.locator(`${editor} p`)).toHaveCount(1);
  await page.keyboard.press('Backspace'); await expect(page.locator(`${editor} h2`)).toHaveCount(0); await expect(page.locator(`${editor} p`)).toHaveCount(1);
  await expect(page.locator(`${editor} p`)).toHaveText('## ');
});
test('list-first keyboard and marker attempts preserve the list and normal typed text', async ({page}) => {
  await open(page, {type: 'doc', content: [{type: 'bulletList', content: [{type: 'listItem', content: [p('', {spaceAfter: 0})]}]}, p('Outside', {})]});
  await page.locator(`${editor} li p`).click(); await page.keyboard.press('Control+Alt+2'); await expect(page.locator('#notice')).toContainText('列表首段');
  await page.keyboard.type('## '); await expect(page.locator(`${editor} li p`)).toHaveText('## '); await expect(page.locator(`${editor} h2`)).toHaveCount(0);
});
