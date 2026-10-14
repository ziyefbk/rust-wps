import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
// Discovery-only in the restricted cloud environment; actual browser/native
// keyboard, selection synchronization and disk drafts remain unverified.
const root = '#editor-content .tiptap';
const pageGeometry = {widthTwips: 12240, heightTwips: 15840, marginTopTwips: 1134, marginBottomTwips: 1134, marginLeftTwips: 1701, marginRightTwips: 850};
const original = () => ({type: 'doc', attrs: {styleProfile: 'standard-v2', pageGeometry}, content: [
  {type: 'orderedList', attrs: {start: 7, markerFont: {fontFamily: 'Calibri', fontSize: 22}}, content: ['First', 'Middle', 'Last'].map((text, i) => ({type: 'listItem', content: [{type: 'paragraph', attrs: {listGeometry: {leftTwips: 720 + i * 240, rightTwips: i * 80, hangingTwips: 360}, spaceAfter: i * 40}, content: [{type: 'text', text}]}]}))},
  {type: 'paragraph', content: [{type: 'text', text: 'Outside'}]},
]});
async function open(page) {let draft, writes = 0; await nativeBridge(page, (command, args) => {if (command === 'load_draft') return original(); if (command === 'save_draft') {draft = structuredClone(args.doc); writes++; return;} throw new Error(command);}); await page.goto('/'); await page.locator('#restore-draft').click(); return {draft: () => draft, writes: () => writes};}
async function select(page, fromItem, toItem, partial = false) {await page.locator(root).focus(); await page.locator(root).evaluate((editor, {fromItem, toItem, partial}) => {const paragraphs = [...editor.querySelectorAll('ol > li > p'), editor.querySelector(':scope > p')]; window.getSelection().setBaseAndExtent(paragraphs[fromItem].firstChild, partial ? 1 : 0, paragraphs[toItem].firstChild, 0);}, {fromItem, toItem, partial});}
for (const [label, from, to, key] of [['first', 0, 1, 'Delete'], ['middle', 1, 2, 'Backspace'], ['last', 2, 3, 'Delete']]) test(`physical ${key} removes complete ${label} item and preserves draft numbering geometry`, async ({page}) => {
  const saved = await open(page); await select(page, from, to); await page.keyboard.press(key);
  await expect(page.locator(`${root} ol > li`)).toHaveCount(2); await expect.poll(() => saved.draft()?.content[0]?.content.length).toBe(2);
  const expected = original().content[0]; expected.content.splice(from, 1);
  expect(saved.draft().content[0].attrs).toEqual(expected.attrs);
  for (let i = 0; i < 2; i++) {expect(saved.draft().content[0].content[i].content[0].attrs.listGeometry).toEqual(expected.content[i].content[0].attrs.listGeometry); expect(saved.draft().content[0].content[i].content[0].content).toEqual(expected.content[i].content[0].content);}
  await page.locator('#edit-undo').click(); await expect(page.locator(`${root} ol > li`)).toHaveCount(3); await page.locator('#edit-redo').click(); await expect(page.locator(`${root} ol > li`)).toHaveCount(2);
});
test('physical whole-list boundary deletion keeps outside paragraph and undo restores the numbered list', async ({page}) => {const saved = await open(page); await select(page, 0, 3); await page.keyboard.press('Delete'); await expect(page.locator(`${root} ol`)).toHaveCount(0); await expect(page.locator(root)).toHaveText('Outside'); await expect.poll(() => saved.draft()?.attrs.pageGeometry).toEqual(pageGeometry); await page.locator('#edit-undo').click(); await expect(page.locator(`${root} ol > li`)).toHaveCount(3);});
test('physical Ctrl+A Delete preserves page attrs and clears all list formatting', async ({page}) => {const saved = await open(page); await page.locator(root).focus(); await page.keyboard.press('Control+a'); await page.keyboard.press('Delete'); await expect(page.locator(`${root} ol`)).toHaveCount(0); await expect(page.locator(`${root} > p`)).toHaveCount(1); await expect.poll(() => saved.draft()?.attrs.pageGeometry).toEqual(pageGeometry); expect(JSON.stringify(saved.draft())).not.toContain('markerFont'); expect(JSON.stringify(saved.draft())).not.toContain('listGeometry'); await page.locator('#edit-undo').click(); await expect(page.locator(`${root} ol > li`)).toHaveCount(3);});
test('physical partial cross-item deletion refuses before document history or draft changes', async ({page}) => {const saved = await open(page); const before = await page.locator(root).innerHTML(); await select(page, 0, 1, true); await page.keyboard.press('Delete'); await expect(page.locator('#notice')).toContainText('编号'); expect(await page.locator(root).innerHTML()).toBe(before); await expect(page.locator('#edit-undo')).toBeDisabled(); await page.waitForTimeout(800); expect(saved.writes()).toBe(0);});
