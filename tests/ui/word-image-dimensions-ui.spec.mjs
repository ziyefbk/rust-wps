import {test, expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

// These are browser acceptance scenarios, not covered by the browser-free
// model/controller suite. Discover only where browser sockets are unavailable.
const src = `data:image/png;base64,${readFileSync('tests/fixtures/slide-image.png').toString('base64')}`;
const image = attrs => ({type: 'image', attrs: {src, alt: 'original pixels', title: 'keep metadata', ...attrs}});
const paragraph = text => ({type: 'paragraph', content: text ? [{type: 'text', text}] : []});
const documentWith = (...content) => ({type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [...content, paragraph('After images')]});
const root = '#editor-content .tiptap', pictures = `${root} img`;
const pair = (displayWidthEmu = 952500, displayHeightEmu = 476250) => ({displayWidthEmu, displayHeightEmu});
const imageAttrs = draft => draft?.content?.filter(node => node.type === 'image').map(node => node.attrs);
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
const select = async (page, index = 0) => {await page.locator(pictures).nth(index).click(); await expect(page.locator(pictures).nth(index)).toHaveClass(/ProseMirror-selectednode/);};
const apply = async (page, width, height) => {
  await page.locator('#edit-image-keep-ratio').uncheck();
  await page.locator('#edit-image-width').fill(width); await page.locator('#edit-image-height').fill(height);
  await page.locator('#edit-image-dimensions-apply').click();
};

test('selected image exact dimensions persist in draft and real DOCX, preserving source bytes and one-step undo', async ({page}, info) => {
  let exported, reopened;
  const draft = await open(page, documentWith(image(), image()), (command, args) => {
    if (command !== 'save_new_document') throw new Error(command);
    const model = info.outputPath('image-dimensions.json'), output = info.outputPath('image-dimensions.docx');
    mkdirSync(path.dirname(model), {recursive: true}); writeFileSync(model, JSON.stringify(args.doc));
    const cli = path.resolve(`target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
    execFileSync(cli, ['create-docx', model, output], {encoding: 'utf8', windowsHide: true});
    reopened = JSON.parse(execFileSync(cli, ['read-docx-model', output], {encoding: 'utf8', windowsHide: true}));
    exported = structuredClone(args.doc); return {path: output};
  });
  await select(page); await apply(page, '1234567/12700', '75.01');
  await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '1234567');
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayHeightEmu).toBe(952627);
  expect(imageAttrs(draft())[0].src).toBe(src); expect(imageAttrs(draft())[1]).not.toHaveProperty('displayWidthEmu');
  await page.locator('#edit-undo').click(); await expect(page.locator(pictures).first()).not.toHaveAttribute('data-display-width-emu');
  await page.locator('#edit-redo').click(); await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '1234567');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(imageAttrs(reopened.document ?? reopened)).toEqual(imageAttrs(exported));
  expect(imageAttrs(exported)[0].src).toBe(src);
  await page.locator('#edit-image-dimensions-reset').click(); await expect(page.locator(pictures).first()).not.toHaveAttribute('data-display-width-emu');
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayWidthEmu).toBeUndefined();
  await page.locator('#edit-undo').click(); await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '1234567');
});

test('keep aspect ratio edits either axis and restore natural ratio stays within both bounds', async ({page}) => {
  const draft = await open(page, documentWith(image(pair(952500, 952500))));
  await select(page); await expect(page.locator('#edit-image-natural-ratio')).toBeEnabled();
  await page.locator('#edit-image-width').fill('150'); await expect(page.locator('#edit-image-height')).toHaveValue('150');
  await page.locator('#edit-image-dimensions-apply').click();
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayWidthEmu).toBe(1905000);
  await page.locator('#edit-image-height').fill('75'); await expect(page.locator('#edit-image-width')).toHaveValue('75');
  await page.locator('#edit-image-dimensions-apply').click();
  await page.locator('#edit-image-natural-ratio').click();
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayWidthEmu).toBe(952500);
  const natural = await page.locator(pictures).evaluate(img => ({width: img.naturalWidth, height: img.naturalHeight}));
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayHeightEmu).toBe(Math.round(952500 * natural.height / natural.width));
});

test('invalid values and unsupported list/table selection leave original geometry and history intact', async ({page}) => {
  const nested = {type: 'bulletList', content: [{type: 'listItem', content: [paragraph('item'), image()]}]};
  await open(page, documentWith(image(pair()), nested));
  await select(page); await apply(page, '.0001', '75'); await expect(page.locator('#notice')).toContainText('整数 EMU');
  await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '952500');
  await expect(page.locator('#edit-undo')).toBeDisabled();
  await select(page, 1);
  for (const id of ['edit-image-width', 'edit-image-height', 'edit-image-keep-ratio', 'edit-image-dimensions-apply', 'edit-image-dimensions-reset', 'edit-image-natural-ratio']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await expect(page.locator('#edit-image-dimension-state')).toContainText('列表内图片');
});

test('changing selected image or pressing Escape discards pending dimensions and prevents cross-image application', async ({page}) => {
  await open(page, documentWith(image(pair()), image(pair(1905000, 952500))));
  await select(page); await page.locator('#edit-image-width').fill('200');
  await select(page, 1); await expect(page.locator('#edit-image-width')).toHaveValue('150');
  await page.locator('#edit-image-width').fill('75'); await page.locator('#edit-image-width').press('Escape');
  await expect(page.locator('#edit-image-width')).toHaveValue('150');
  await page.locator('#edit-image-dimensions-apply').click();
  await expect(page.locator(pictures).first()).toHaveAttribute('data-display-width-emu', '952500');
  await expect(page.locator(pictures).nth(1)).toHaveAttribute('data-display-width-emu', '1905000');
  await expect(page.locator('#edit-undo')).toBeDisabled();
});

test('maximum and distorted explicit dimensions survive responsive editor CSS and immutable print snapshot', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};});
  await open(page, documentWith(image(pair(5715000, 7620000))));
  await expect(page.locator(pictures)).toHaveCSS('width', '600px'); await expect(page.locator(pictures)).toHaveCSS('height', '800px');
  await page.setViewportSize({width: 900, height: 900});
  await expect(page.locator(pictures)).toHaveCSS('width', '600px'); await expect(page.locator(pictures)).toHaveCSS('object-fit', 'fill');
  await page.locator('#edit-print').click(); await expect(page.locator('#notice')).toContainText('已准备 A4 打印副本');
  await select(page); await apply(page, '75', '75');
  await expect(page.locator('#print-document img')).toHaveAttribute('data-display-width-emu', '5715000');
  await page.emulateMedia({media: 'print'});
  await expect(page.locator('#print-document img')).toHaveCSS('width', '600px'); await expect(page.locator('#print-document img')).toHaveCSS('height', '800px');
  await expect(page.locator('#print-document img')).toHaveCSS('max-height', 'none'); await expect(page.locator('#print-document img')).toHaveCSS('object-fit', 'fill');
});

test('save locks prevent synthetic image-size changes', async ({page}) => {
  let finishSave, exported;
  await open(page, documentWith(image(pair())), (command, args) => {
    if (command === 'save_new_document') {exported = args.doc; return new Promise(resolve => {finishSave = resolve;});}
    throw new Error(command);
  });
  await select(page); await page.locator('#edit-save').click(); await expect.poll(() => !!finishSave).toBe(true);
  for (const id of ['edit-image-width', 'edit-image-height', 'edit-image-dimensions-apply', 'edit-image-natural-ratio', 'edit-image-dimensions-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.evaluate(() => {
    document.getElementById('edit-image-width').value = '450'; document.getElementById('edit-image-height').value = '600';
    for (const id of ['edit-image-dimensions-apply', 'edit-image-dimensions-reset', 'edit-image-natural-ratio']) document.getElementById(id).dispatchEvent(new MouseEvent('click'));
  });
  await expect(page.locator(pictures)).toHaveAttribute('data-display-width-emu', '952500'); expect(imageAttrs(exported)[0]).toMatchObject(pair());
  finishSave({path: 'image.docx'}); await expect(page.locator('#edit-image-width')).toBeEnabled();
});

for (const attrs of [{displayWidthEmu: 1}, {displayWidthEmu: null, displayHeightEmu: null}, pair('100', 200)]) test(`malformed incoming image dimensions reject before replacing current editor: ${JSON.stringify(attrs)}`, async ({page}) => {
  await open(page, documentWith(image(attrs)));
  await expect(page.locator('#notice')).toContainText('图片显示尺寸'); await expect(page.locator('#authoring-sheet')).toBeHidden();
});

test('print decode preparation locks geometry until the immutable snapshot is ready', async ({page}) => {
  await page.addInitScript(() => {window.print = () => {};});
  await open(page, documentWith(image(pair()))); await select(page);
  await expect(page.locator('#edit-image-natural-ratio')).toBeEnabled();
  await page.evaluate(() => {
    const decode = HTMLImageElement.prototype.decode;
    HTMLImageElement.prototype.decode = function () {
      return decode.call(this).then(() => new Promise(resolve => {window.finishImagePrintDecode = resolve;}));
    };
  });
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => !!window.finishImagePrintDecode)).toBe(true);
  for (const id of ['edit-image-width', 'edit-image-height', 'edit-image-dimensions-apply', 'edit-image-natural-ratio', 'edit-image-dimensions-reset']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await page.evaluate(() => {
    document.getElementById('edit-image-dimensions-reset').dispatchEvent(new MouseEvent('click'));
    window.finishImagePrintDecode();
  });
  await expect(page.locator('#notice')).toContainText('已准备 A4 打印副本');
  await expect(page.locator('#print-document img')).toHaveAttribute('data-display-width-emu', '952500');
  await expect(page.locator(pictures)).toHaveAttribute('data-display-width-emu', '952500');
});

test('HTML paste uses paired owned metadata, ignores pixel sizes and rejects malformed pairs atomically', async ({page}) => {
  const draft = await open(page, documentWith(paragraph('Paste here')));
  const paste = async html => {
    await page.locator(`${root} p`).last().click();
    await page.evaluate(html => {
      const data = new DataTransfer(); data.setData('text/html', html);
      document.querySelector('#editor-content .tiptap').dispatchEvent(new ClipboardEvent('paste', {bubbles: true, cancelable: true, clipboardData: data}));
    }, html);
  };
  await paste(`<img src="${src}" width="123" height="45" style="width:900px;height:900px" data-display-width-emu="952500" data-display-height-emu="476250">`);
  await expect(page.locator(pictures)).toHaveCSS('width', '100px'); await expect(page.locator(pictures)).toHaveCSS('height', '50px');
  await expect.poll(() => imageAttrs(draft())?.[0]?.displayWidthEmu).toBe(952500);
  expect(imageAttrs(draft())[0].width).toBeNull(); expect(imageAttrs(draft())[0].height).toBeNull();
  await paste(`<img src="${src}" data-display-width-emu="952500">`);
  await expect(page.locator('#notice')).toContainText('图片显示尺寸'); await expect(page.locator(pictures)).toHaveCount(1);
});
