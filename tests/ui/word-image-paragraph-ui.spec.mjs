import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// Browser acceptance scenarios. Discover only while browser/native execution is
// blocked; model/DOM declaration checks are not CSS, focus or print evidence.
const src = `data:image/png;base64,${readFileSync('tests/fixtures/slide-image.png').toString('base64')}`;
const font = {family: 'Arial', halfPoints: 22, bold: false, italic: false};
const layout = extra => ({spaceBefore: 0, spaceAfter: 120, lineSpacing: 360, textAlign: 'left', paragraphFont: font, runFont: {...font, halfPoints: 18}, ...extra});
const image = (value, extra = {}) => ({type: 'image', attrs: {src, displayWidthEmu: 95250, displayHeightEmu: 95250,
  ...(value === undefined ? {} : {imageParagraph: value}), ...extra}});
const paragraph = text => ({type: 'paragraph', content: text ? [{type: 'text', text}] : []});
const doc = (...content) => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [...content, paragraph('Following text')]});
const root = '#editor-content .tiptap', wrappers = `${root} [data-image-paragraph]`, pictures = `${root} img`;
async function open(page, content, extra = () => {}) {
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return content;
    if (command === 'save_draft') {draft = structuredClone(args.doc); return;}
    return extra(command, args);
  });
  await page.goto('/'); await page.locator('#restore-draft').click();
  return () => draft;
}
const imageAttrs = value => value?.content?.filter(node => node.type === 'image').map(node => node.attrs);
async function select(page, index = 0) {
  await page.locator(pictures).nth(index).click();
  await expect(page.locator(`${root} .ProseMirror-selectednode`)).toHaveCount(1);
}
async function apply(page, before = '4.05', after = '0', line = '241/240', align = 'center') {
  await page.locator('#edit-image-space-before').fill(before); await page.locator('#edit-image-space-after').fill(after);
  await page.locator('#edit-image-line-spacing').fill(line); await page.locator('#edit-image-alignment').selectOption(align);
  await page.locator('#edit-image-paragraph-apply').click();
}

test('selected image paragraph preserves fonts, dimensions and blank neighbors through draft/DOCX/undo/reset', async ({page}, info) => {
  let exported, reopened;
  const draft = await open(page, doc(paragraph(), image(layout()), paragraph(), image(layout({textAlign: 'right'}))), (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('image-paragraph.json'), output = info.outputPath('image-paragraph.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
    exported = structuredClone(args.doc); return {path: output};
  });
  await select(page); await apply(page);
  await expect.poll(() => imageAttrs(draft())?.[0]?.imageParagraph).toEqual(layout({spaceBefore: 81, spaceAfter: 0, lineSpacing: 241, textAlign: 'center'}));
  expect(imageAttrs(draft())[0].src).toBe(src); expect(imageAttrs(draft())[0].displayWidthEmu).toBe(95250);
  expect(imageAttrs(draft())[1].imageParagraph.textAlign).toBe('right');
  await page.locator('#edit-undo').click(); await expect(page.locator('#edit-image-line-spacing')).toHaveValue('1.5');
  await page.locator('#edit-redo').click(); await expect(page.locator('#edit-image-line-spacing')).toHaveValue('241/240');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(reopened.document ?? reopened).toEqual(exported);
  await page.locator('#edit-image-paragraph-reset').click(); await expect(page.locator(wrappers)).toHaveCount(1);
  await expect(page.locator('#notice')).toContainText('基线字体'); await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '95250');
  await page.locator('#edit-undo').click(); await expect(page.locator(wrappers)).toHaveCount(2);
});

test('real line-box wrapper aligns inline images and shares exact font/spacing declarations with immutable print clone', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};});
  await open(page, doc(image(layout({spaceBefore: 80, spaceAfter: 240, textAlign: 'center'}))));
  const wrapper = page.locator(wrappers);
  await expect(wrapper).toHaveCSS('display', 'block'); await expect(wrapper).toHaveCSS('text-align', 'center');
  await expect(wrapper).toHaveCSS('margin-top', '5.33333px'); await expect(wrapper).toHaveCSS('margin-bottom', '16px');
  await expect(page.locator(pictures)).toHaveCSS('display', 'inline'); await expect(page.locator(pictures)).toHaveCSS('vertical-align', 'baseline');
  await expect(page.locator(pictures)).toHaveCSS('width', '10px');
  await page.locator('#edit-print').click(); await expect(page.locator('#notice')).toContainText('已准备 A4 打印副本');
  const before = await page.locator('#print-document [data-image-paragraph]').getAttribute('data-image-paragraph');
  await select(page); await apply(page, '0', '0', '2', 'right');
  expect(await page.locator('#print-document [data-image-paragraph]').getAttribute('data-image-paragraph')).toBe(before);
  await page.emulateMedia({media: 'print'});
  await expect(page.locator('#print-document [data-image-paragraph]')).toHaveCSS('text-align', 'center');
  await expect(page.locator('#print-document img')).toHaveCSS('vertical-align', 'baseline');
  await expect(page.locator('#print-document img')).toHaveCSS('width', '10px');
});

test('pending paragraph fields never migrate to another image and Escape preserves imported fonts and history', async ({page}) => {
  await open(page, doc(image(layout()), image(layout({spaceBefore: 240, textAlign: 'right'}))));
  await select(page); await page.locator('#edit-image-space-before').fill('99'); await select(page, 1);
  await expect(page.locator('#edit-image-space-before')).toHaveValue('12');
  await page.locator('#edit-image-space-before').fill('1'); await page.locator('#edit-image-space-before').press('Escape');
  await expect(page.locator('#edit-image-space-before')).toHaveValue('12');
  await page.locator('#edit-image-paragraph-apply').click(); await expect(page.locator('#edit-undo')).toBeDisabled();
});

test('invalid numeric/HTML metadata and nested image paragraph selection cannot silently rewrite image layout', async ({page}) => {
  const nested = {type: 'bulletList', content: [{type: 'listItem', content: [paragraph('item'), image(undefined, {displayWidthEmu: undefined, displayHeightEmu: undefined})]}]};
  await open(page, doc(image(layout()), nested)); await select(page); await apply(page, '0.01');
  await expect(page.locator('#notice')).toContainText('0.05 pt'); await expect(page.locator('#edit-undo')).toBeDisabled();
  await select(page, 1); await expect(page.locator('#edit-image-paragraph-apply')).toBeDisabled();
  await page.locator(`${root} p`).last().click();
  await page.evaluate(src => {
    const data = new DataTransfer(); data.setData('text/html', `<div data-image-paragraph="broken"><span><img src="${src}"></span>extra text</div>`);
    document.querySelector('#editor-content .tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
  }, src);
  await expect(page.locator(pictures)).toHaveCount(2); await expect(page.locator('#notice')).toContainText('图片段落');
});

test('save lock disables every paragraph action and synthetic apply/reset cannot alter export snapshot', async ({page}) => {
  let finishSave, exported;
  await open(page, doc(image(layout())), (command, args) => {
    if (command === 'save_new_document') {exported = args.doc; return new Promise(resolve => {finishSave = resolve;});}
    throw new Error(command);
  });
  await select(page); await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  for (const id of ['edit-image-space-before', 'edit-image-space-after', 'edit-image-line-spacing', 'edit-image-alignment', 'edit-image-paragraph-apply', 'edit-image-paragraph-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.evaluate(() => {
    document.getElementById('edit-image-line-spacing').value = '10';
    for (const id of ['edit-image-paragraph-apply', 'edit-image-paragraph-reset']) document.getElementById(id).dispatchEvent(new MouseEvent('click'));
  });
  expect(imageAttrs(exported)[0].imageParagraph).toEqual(layout());
  expect(JSON.parse(await page.locator(wrappers).getAttribute('data-image-paragraph'))).toEqual(layout());
  finishSave({path: 'image.docx'}); await expect(page.locator('#edit-image-paragraph-apply')).toBeEnabled();
});

for (const value of [null, {spaceBefore: 0}, layout({paragraphFont: null}), {...layout(), unknown: true}]) test(`raw malformed paragraph metadata rejects before replacing editor: ${JSON.stringify(value)}`, async ({page}) => {
  await open(page, doc(image(value))); await expect(page.locator('#notice')).toContainText('图片段落');
  await expect(page.locator('#authoring-sheet')).toBeHidden();
});
