// Browser acceptance specifications using real UI with simulated IPC transport.
// They do not prove native storage, installed-font fallback or Windows printing.
import {test, expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
const root = '#editor-content .tiptap';
const attribute = 'data-font-family-metadata';
const entries = [{name: 'Arial Black', family: 'swiss'}, {name: 'Unused Serif', family: 'roman'}];
const warning = '尚未据此调整缺失字体替代';
const document = () => ({type: 'doc', attrs: {styleProfile: 'standard-v2', fontFamilyMetadata: structuredClone(entries)},
  content: [{type: 'paragraph', content: [{type: 'text', text: 'Original', marks: [{type: 'fontFamily', attrs: {name: 'Arial Black'}}]}]}]});
const file = authored => ({name: 'font-metadata.docx', path: 'font-metadata.docx', authored,
  inspection: {format: 'docx', text: 'Original', bytes: 100, warnings: []}});
async function pasteHTML(page, html) {
  await page.locator(root).focus();
  await page.evaluate(html => {
    const data = new DataTransfer(); data.setData('text/html', html);
    data.setData('text/plain', new DOMParser().parseFromString(html, 'text/html').body.textContent ?? '');
    document.querySelector('#editor-content .tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  }, html);
}

test('open edit font reset delete-all undo draft export and reopen retain even unused root metadata', async ({page}) => {
  let current = document(), draft, exported;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return file(current);
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    if (command === 'save_new_document') {exported = structuredClone(args.doc); current = exported; return {path: 'font-metadata.docx'};}
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator(root)).toHaveAttribute(attribute, JSON.stringify(entries));
  await expect(page.locator('#notice')).toContainText(warning);
  await page.locator(`${root} p`).click(); await page.keyboard.press('End'); await page.keyboard.insertText(' edited');
  await expect.poll(() => draft?.content[0].content[0].text).toBe('Original edited');
  await page.locator(root).focus(); await page.keyboard.press('ControlOrMeta+A');
  await page.locator('#edit-font-family-reset').click();
  await expect.poll(() => JSON.stringify(draft)).not.toContain('"type":"fontFamily"');
  expect(draft.attrs.fontFamilyMetadata).toEqual(entries);
  await page.locator(root).focus(); await page.keyboard.press('ControlOrMeta+A'); await page.keyboard.press('Backspace');
  await expect.poll(() => draft?.content).toEqual([{type: 'paragraph', attrs: {textAlign: null}}]);
  expect(draft.attrs.fontFamilyMetadata).toEqual(entries);
  await page.locator('#edit-undo').click(); await expect(page.locator(root)).toHaveText('Original edited');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(exported.attrs.fontFamilyMetadata).toEqual(entries);
  await page.locator('#open').click(); await expect(page.locator(root)).toHaveText('Original edited');
  await expect(page.locator(root)).toHaveAttribute(attribute, JSON.stringify(entries));
});

test('HTML foreign or malformed root metadata never replaces destination metadata or introduces it to a new document', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return document();
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept()); await page.goto('/');
  for (const present of [true, false]) {
    await page.locator(present ? '#restore-draft' : '#new-document').click();
    for (const metadata of ['[{"name":"Foreign","family":"roman"}]', 'null', '{bad']) {
      await page.locator(root).focus(); await page.keyboard.press('ControlOrMeta+A');
      await pasteHTML(page, `<div ${attribute}='${metadata}'><p><span style='font-family: "Foreign"'>Pasted ${metadata.length}</span></p></div>`);
      await expect.poll(() => draft?.content[0]?.content[0]?.text).toBe(`Pasted ${metadata.length}`);
      expect(draft.attrs?.fontFamilyMetadata).toEqual(present ? entries : undefined);
      if (present) await expect(page.locator(root)).toHaveAttribute(attribute, JSON.stringify(entries));
      else await expect(page.locator(root)).not.toHaveAttribute(attribute);
      expect(draft.content[0].content[0].marks).toContainEqual({type: 'fontFamily', attrs: {name: 'Foreign'}});
    }
  }
});

test('invalid raw root metadata and nested metadata refuse draft restore without replacing active content', async ({page}) => {
  let candidate, draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return candidate;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept()); await page.goto('/'); await page.locator('#new-document').click();
  await page.locator(root).click(); await page.keyboard.insertText('Keep current');
  await expect.poll(() => draft?.content[0]?.content[0]?.text).toBe('Keep current');
  const original = await page.locator(root).innerHTML();
  for (const value of [null, [], [{name: 'Bad', family: 'modern'}], [{name: 'Arial', family: 'swiss'}, {name: 'ARIAL', family: 'swiss'}],
    [{name: 'Arial', family: 'swiss', extra: true}], Array.from({length: 257}, (_, index) => ({name: `Font ${index}`, family: 'roman'}))]) {
    candidate = document(); candidate.attrs.fontFamilyMetadata = value;
    await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText('草稿恢复失败');
    expect(await page.locator(root).innerHTML()).toBe(original); await expect(page.locator(root)).toHaveAttribute('contenteditable', 'true');
  }
  candidate = document(); delete candidate.attrs.fontFamilyMetadata; candidate.content[0].attrs = {fontFamilyMetadata: entries};
  await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText('草稿恢复失败');
  expect(await page.locator(root).innerHTML()).toBe(original);
});

test('print clone freezes inert metadata and PDF export discloses unchanged fallback before cancellation', async ({page}) => {
  let releaseExport, exportedArgs;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return document();
    if (command === 'save_draft') return;
    if (command === 'export_current_pdf') {exportedArgs = args; await new Promise(resolve => {releaseExport = resolve;}); return null;}
    throw new Error(command);
  });
  await page.addInitScript(() => {window.print = () => {window.printCalls = (window.printCalls ?? 0) + 1;};});
  await page.goto('/'); await page.locator('#restore-draft').click(); await expect(page.locator('#notice')).toContainText(warning);
  const familyBefore = await page.locator(`${root} [data-font-family]`).evaluate(element => getComputedStyle(element).fontFamily);
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  await expect(page.locator('#print-document .tiptap')).toHaveAttribute(attribute, JSON.stringify(entries));
  await expect(page.locator('#notice')).toContainText(warning);
  const clone = await page.locator('#print-document').innerHTML();
  await page.locator(`${root} p`).click(); await page.keyboard.press('End'); await page.keyboard.insertText(' later');
  expect(await page.locator('#print-document').innerHTML()).toBe(clone);
  expect(await page.locator(`${root} [data-font-family]`).first().evaluate(element => getComputedStyle(element).fontFamily)).toBe(familyBefore);
  const current = await page.locator(root).innerHTML(); await page.locator('#edit-export-pdf').click();
  await expect.poll(() => !!releaseExport).toBe(true); await expect(page.locator('#notice')).toContainText(warning);
  expect(exportedArgs).toEqual({wordStyleProfile: 'standard-v2'});
  await expect(page.locator(root)).toHaveAttribute('contenteditable', 'false'); releaseExport();
  await expect(page.locator('#notice')).toContainText('已取消 PDF 导出');
  await expect(page.locator(root)).toHaveAttribute('contenteditable', 'true'); expect(await page.locator(root).innerHTML()).toBe(current);
  await expect(page.locator('#print-document')).toBeEmpty();
});

test('external editable-copy import and new-document transitions retain or omit metadata according to the document', async ({page}) => {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return {...file(null), name: 'external.doc', inspection: {format: 'doc', text: 'Original', bytes: 100, warnings: []}};
    if (command === 'import_current') return {document: document(), compatibility: {risks: ['Font family metadata preserved from source']}};
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept()); await page.goto('/'); await page.locator('#open').click();
  await page.locator('#import-document').click(); await page.getByRole('button', {name: '接受变化并编辑副本'}).click();
  await expect(page.locator(root)).toHaveAttribute(attribute, JSON.stringify(entries));
  await expect(page.locator('#notice')).toContainText(warning); await expect.poll(() => draft?.attrs?.fontFamilyMetadata).toEqual(entries);
  await page.locator('#new-document').click(); await expect(page.locator(root)).not.toHaveAttribute(attribute);
  await page.locator(root).click(); await page.keyboard.insertText('New');
  await expect.poll(() => draft?.content[0]?.content[0]?.text).toBe('New'); expect(draft.attrs).toEqual({styleProfile: 'standard-v2'});
});
