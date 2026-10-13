import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';

const warning = 'Contextual same-style spacing resolved to fixed paragraph values; named-style and reorder dynamics are not retained';
const paragraph = (text, before, after) => ({type: 'paragraph', attrs: {spaceBefore: before, spaceAfter: after, lineSpacing: 240}, content: [{type: 'text', text}]});
const normalized = {type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [paragraph('First source item', 120, 0), paragraph('Second source item', 0, 180)]};

// Requires a real browser. Discovery does not establish dialog/render behavior.
test('contextual normalization warning precedes editing, cancellation is safe, and fixed values remain editable', async ({page}) => {
  let draft, writes = 0;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return {name: 'stock-list.docx', path: 'stock-list.docx', inspection: {format: 'docx', text: 'Original source preview', bytes: 100, warnings: []}, authored: null};
    if (command === 'import_current') return {document: normalized, compatibility: {risks: [warning]}};
    if (command === 'save_draft') {draft = structuredClone(args.doc); writes++; return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#open').click(); await page.locator('#import-document').click();
  await expect(page.locator('#import-review')).toBeVisible();
  await expect(page.locator('#import-risks')).toContainText('按源段落顺序转换为固定段前/段后值');
  await expect(page.locator('#import-risks')).toContainText('命名样式及重排后的动态间距不保留');
  await expect(page.locator('.tiptap')).toHaveCount(0); expect(writes).toBe(0);
  await page.locator('#import-review button[value="cancel"]').click();
  await expect(page.locator('#text-content')).toContainText('Original source preview'); expect(writes).toBe(0);
  await page.locator('#import-document').click(); await page.locator('#import-review button[value="import"]').click();
  await expect(page.locator('.tiptap p')).toHaveCount(2);
  await expect.poll(() => draft?.content?.map(p => [p.attrs.spaceBefore, p.attrs.spaceAfter])).toEqual([[120, 0], [0, 180]]);
  await page.locator('.tiptap p').first().click();
  await page.locator('#edit-space-after').fill('7'); await page.locator('#edit-paragraph-spacing-apply').click();
  await expect.poll(() => draft?.content?.[0]?.attrs.spaceAfter).toBe(140);
  await page.locator('#edit-undo').click(); await expect.poll(() => draft?.content?.[0]?.attrs.spaceAfter).toBe(0);
});

test('unproved contextual import retains read-only source and explains consumer ambiguity', async ({page}) => {
  await nativeBridge(page, async command => {
    if (command === 'open_document') return {name: 'asymmetric.docx', path: 'asymmetric.docx', inspection: {format: 'docx', text: 'Original asymmetric source', bytes: 100, warnings: []}, authored: null};
    if (command === 'import_current') throw new Error('Unsupported paragraph spacing cannot be preserved in an editable copy');
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#open').click(); await page.locator('#import-document').click();
  await expect(page.getByRole('status')).toContainText('标准与已验证消费者结果不同');
  await expect(page.locator('#text-content')).toContainText('Original asymmetric source');
  await expect(page.locator('.tiptap')).toHaveCount(0);
  await expect(page.locator('#import-document')).toBeEnabled();
});
