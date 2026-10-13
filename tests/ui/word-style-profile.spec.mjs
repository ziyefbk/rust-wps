import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

const modern = 'standard-v2';
const editor = '#editor-content .tiptap';
const simple = text => ({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] });
const copy = value => structuredClone(value);
const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
const runPython = (script, ...args) => JSON.parse(execFileSync(python, ['-c', script, ...args], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));

async function pasteHtml(page, html) {
  await page.locator(editor).focus();
  await page.evaluate(html => {
    const data = new DataTransfer(); data.setData('text/html', html);
    data.setData('text/plain', new DOMParser().parseFromString(html, 'text/html').body.textContent);
    document.querySelector('#editor-content .tiptap').dispatchEvent(new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData: data }));
  }, html);
}

async function computed(page, selector) {
  return page.locator(selector).first().evaluate(element => {
    const style = getComputedStyle(element);
    return { family: style.fontFamily, size: parseFloat(style.fontSize), weight: style.fontWeight,
      color: style.color, line: parseFloat(style.lineHeight), before: parseFloat(style.marginTop), after: parseFloat(style.marginBottom) };
  });
}

function expectBody(style) {
  expect(style.family).toContain('Arial'); expect(style.family).toContain('Microsoft YaHei');
  expect(style.size).toBeCloseTo(11 * 4 / 3, 2); expect(style.weight).toBe('400');
  expect(style.color).toBe('rgb(0, 0, 0)'); expect(style.line).toBeCloseTo(22, 2);
  expect(style.before).toBe(0); expect(style.after).toBe(8);
}

test('new standard document uses inherited defaults, exports V2 styles and reopens without downgrading', async ({ page }, info) => {
  test.setTimeout(60000);
  let draft, savedModel, independentlyRead, savedPath;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = copy(args.doc); return; }
    if (command === 'save_new_document') {
      const json = info.outputPath('defaults.json'), output = info.outputPath('defaults.docx');
      mkdirSync(path.dirname(json), { recursive: true }); writeFileSync(json, JSON.stringify(args.doc));
      const report = JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'), ['create-docx', json, output], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
      independentlyRead = runPython([
        'import sys,json,zipfile', 'from docx import Document', 'from docx.oxml.ns import qn',
        'doc=Document(sys.argv[1]); styles=doc.styles.element',
        'r=styles.find(qn("w:docDefaults")).find(qn("w:rPrDefault")).find(qn("w:rPr"))',
        'p=styles.find(qn("w:docDefaults")).find(qn("w:pPrDefault")).find(qn("w:pPr")).find(qn("w:spacing"))',
        'fonts=r.find(qn("w:rFonts"))',
        'headings=[]',
        'for level in range(1,4):',
        '    s=doc.styles["Heading "+str(level)]; rp=s.element.find(qn("w:rPr")); sp=s.element.find(qn("w:pPr")).find(qn("w:spacing"))',
        '    headings.append({"points":s.font.size.pt,"complex_half_points":int(rp.find(qn("w:szCs")).get(qn("w:val"))),"bold":s.font.bold,"before":int(sp.get(qn("w:before"))),"after":int(sp.get(qn("w:after")))})',
        'with zipfile.ZipFile(sys.argv[1]) as z:',
        '    names=z.namelist(); embedded=json.loads(z.read("rust-wps/model-v2.json"))',
        'print(json.dumps({"model":embedded,"has_old_model":"rust-wps/model-v1.json" in names,"fonts":{k:fonts.get(qn("w:"+k)) for k in ["ascii","hAnsi","eastAsia","cs"]},"size":int(r.find(qn("w:sz")).get(qn("w:val"))),"complex_size":int(r.find(qn("w:szCs")).get(qn("w:val"))),"color":r.find(qn("w:color")).get(qn("w:val")),"body_spacing":{k:p.get(qn("w:"+k)) for k in ["before","after","line","lineRule"]},"headings":headings,"margins":[getattr(doc.sections[0],k+"_margin").mm for k in ["left","right","top","bottom"]]},ensure_ascii=True))',
      ].join('\n'), output);
      savedModel = independentlyRead.model.document; savedPath = output;
      return { path: output, report };
    }
    if (command === 'open_document') return { name: 'defaults.docx', path: savedPath, authored: savedModel, inspection: { format: 'docx', text: 'BodyProbe 中文', bytes: 1000, warnings: [] } };
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', modern);
  await pasteHtml(page, '<p>BodyProbe 中文</p><h1>HeadingOne</h1><h2>HeadingTwo</h2><h3>HeadingThree</h3><p><code>CodeProbe</code></p><table><tbody><tr><th><p>HeaderProbe <strong>BoldHeader</strong></p></th><td><p>CellProbe</p></td></tr></tbody></table>');
  expectBody(await computed(page, `${editor} > p`));
  for (const [level, points] of [[1, 18], [2, 15], [3, 13]]) {
    const style = await computed(page, `${editor} h${level}`);
    expect(style.size).toBeCloseTo(points * 4 / 3, 2); expect(style.weight).toBe('700');
    expect(style.family).toContain('Arial'); expect(style.color).toBe('rgb(0, 0, 0)');
    expect(style.line).toBeCloseTo(points * 2, 2); expect(style.before).toBe(16); expect(style.after).toBe(8);
  }
  const code = await computed(page, `${editor} code`); expect(code.family).toContain('Consolas'); expect(code.size).toBeCloseTo(11 * 4 / 3, 2);
  await expect(page.locator(`${editor} th`)).toHaveCSS('font-weight', '400');
  await expect(page.locator(`${editor} th strong`)).toHaveCSS('font-weight', '700');
  expectBody(await computed(page, `${editor} td p`));
  await expect.poll(() => draft?.attrs?.styleProfile).toBe(modern);
  expect(JSON.stringify(draft)).not.toContain('fontSize'); expect(JSON.stringify(draft)).not.toContain('textColor');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(independentlyRead.model.version).toBe(2); expect(independentlyRead.has_old_model).toBe(false);
  expect(savedModel.attrs.styleProfile).toBe(modern);
  expect(independentlyRead.fonts).toEqual({ ascii: 'Arial', hAnsi: 'Arial', eastAsia: 'Microsoft YaHei', cs: 'Arial' });
  expect(independentlyRead.size).toBe(22); expect(independentlyRead.complex_size).toBe(22); expect(independentlyRead.color).toBe('000000');
  expect(independentlyRead.body_spacing).toEqual({ before: '0', after: '120', line: '360', lineRule: 'auto' });
  expect(independentlyRead.headings).toEqual([18, 15, 13].map(points => ({ points, complex_half_points: points * 2, bold: true, before: 240, after: 120 })));
  for (const margin of independentlyRead.margins) expect(margin).toBeCloseTo(25.4, 3);
  await page.locator('#open').click();
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', modern);
  expectBody(await computed(page, `${editor} > p`));
  await expect(page.locator(`${editor} h1`)).toHaveText('HeadingOne');
  await page.locator(`${editor} > p`).first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' reopened');
  await expect.poll(() => JSON.stringify(draft)).toContain('reopened'); expect(draft.attrs.styleProfile).toBe(modern);
});

test('old pending drafts finish before modern drafts and PDF cancellation preserves each profile', async ({ page }) => {
  const legacy = simple('LegacyProbe');
  const requests = [], completed = [], pdfArgs = [];
  let releaseFirst;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return legacy;
    if (command === 'save_draft') {
      const snapshot = copy(args.doc); requests.push(snapshot);
      if (requests.length === 1) await new Promise(resolve => { releaseFirst = resolve; });
      completed.push(snapshot); return;
    }
    if (command === 'export_current_pdf') { pdfArgs.push(copy(args)); return null; }
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#restore-draft').click();
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', 'legacy-v1');
  await expect(page.locator(editor)).toHaveCSS('font-size', '16px');
  const oldHtml = await page.locator(editor).innerHTML();
  await page.locator('#edit-export-pdf').click(); await expect(page.locator('#notice')).toContainText('已取消 PDF 导出');
  expect(pdfArgs).toEqual([{}]); expect(await page.locator(editor).innerHTML()).toBe(oldHtml); expect(requests).toHaveLength(0);
  await page.locator(`${editor} p`).click(); await page.keyboard.press('End'); await page.keyboard.insertText(' old edit');
  await expect.poll(() => !!releaseFirst).toBe(true);
  await page.locator('#new-document').click();
  await expect(page.locator(editor)).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', 'legacy-v1');
  expect(requests).toHaveLength(1); expect(completed).toHaveLength(0);
  releaseFirst();
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', modern);
  await expect.poll(() => completed.length).toBe(2);
  for (const snapshot of completed) expect(snapshot).not.toHaveProperty('attrs');
  await page.locator(editor).click(); await page.keyboard.insertText('ModernProbe');
  await expect.poll(() => completed.length).toBe(3);
  expect(completed[2].attrs.styleProfile).toBe(modern); expect(JSON.stringify(completed[2])).toContain('ModernProbe');
  expect((await computed(page, `${editor} p`)).size).toBeCloseTo(11 * 4 / 3, 2);
  const modernHtml = await page.locator(editor).innerHTML(), beforeCancel = copy(completed);
  await page.locator('#edit-export-pdf').click(); await expect(page.locator('#notice')).toContainText('已取消 PDF 导出');
  expect(pdfArgs).toEqual([{}, { wordStyleProfile: modern }]);
  expect(await page.locator(editor).innerHTML()).toBe(modernHtml); expect(completed).toEqual(beforeCancel);
  await expect(page.locator('#print-document')).toBeEmpty();
});

test('standard print snapshot retains defaults and produces an 11pt PDF at one-inch margins', async ({ page }, info) => {
  test.setTimeout(60000);
  await nativeBridge(page, async command => { if (command === 'save_draft') return; throw new Error(command); });
  await page.addInitScript(() => { window.print = () => { window.printCalls = (window.printCalls ?? 0) + 1; }; });
  await page.goto('/'); await page.locator('#new-document').click();
  await pasteHtml(page, '<p>BodyProbe 中文</p><h1>PrintHeading</h1><p>SecondProbe</p>');
  const screenBody = await computed(page, `${editor} > p`), screenHeading = await computed(page, `${editor} h1`);
  await page.locator('#edit-print').click(); await expect.poll(() => page.evaluate(() => window.printCalls)).toBe(1);
  await expect(page.locator('#print-document .tiptap')).toHaveAttribute('data-style-profile', modern);
  const snapshotHtml = await page.locator('#print-document').innerHTML();
  await page.locator(`${editor} > p`).first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' later edit');
  await expect(page.locator(editor)).toContainText('later edit');
  expect(await page.locator('#print-document').innerHTML()).toBe(snapshotHtml);
  await page.emulateMedia({ media: 'print' });
  expect(await computed(page, '#print-document .tiptap > p')).toEqual(screenBody);
  expect(await computed(page, '#print-document h1')).toEqual(screenHeading);
  await expect(page.locator('#print-document .tiptap')).toHaveCSS('page', 'word-standard-v2');
  const pdf = info.outputPath('standard-print.pdf'); mkdirSync(path.dirname(pdf), { recursive: true });
  await page.pdf({ path: pdf, preferCSSPageSize: true, printBackground: true, displayHeaderFooter: false });
  const report = runPython([
    'import sys,json,math', 'from pypdf import PdfReader',
    'reader=PdfReader(sys.argv[1]); samples=[]',
    'def visit(text,cm,tm,font,size):',
    '    if "BodyProbe" in text:',
    '        scale=math.hypot(tm[2]*cm[0]+tm[3]*cm[2],tm[2]*cm[1]+tm[3]*cm[3])',
    '        samples.append({"points":size*scale,"x":tm[4]*cm[0]+tm[5]*cm[2]+cm[4],"font":str(font.get("/BaseFont")) if font else None})',
    'text="\\n".join(page.extract_text(visitor_text=visit) for page in reader.pages)',
    'print(json.dumps({"pages":len(reader.pages),"sizes":[[float(p.mediabox.width),float(p.mediabox.height)] for p in reader.pages],"text":text,"samples":samples},ensure_ascii=True))',
  ].join('\n'), pdf);
  expect(report.pages).toBe(1); expect(report.text).toContain('BodyProbe'); expect(report.text).toContain('PrintHeading'); expect(report.text).not.toContain('later edit');
  expect(report.samples.length).toBeGreaterThan(0);
  for (const sample of report.samples) { expect(Math.abs(sample.points - 11)).toBeLessThan(0.15); expect(Math.abs(sample.x - 72)).toBeLessThan(1); expect(sample.font).toMatch(/Arial/i); }
  expect(Math.abs(report.sizes[0][0] - 595.28)).toBeLessThan(1); expect(Math.abs(report.sizes[0][1] - 841.89)).toBeLessThan(1);
  writeFileSync(info.outputPath('standard-print-report.json'), JSON.stringify(report, null, 2));
});

test('unknown and null draft profiles fail without replacing the active document, while old fixtures stay legacy', async ({ page }) => {
  let toRestore, draft, saved, restoreCalls = 0;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') { restoreCalls++; return toRestore; }
    if (command === 'save_draft') { draft = copy(args.doc); return; }
    if (command === 'save_new_document') { saved = copy(args.doc); return { path: 'legacy.docx' }; }
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#new-document').click();
  await page.locator(editor).click(); await page.keyboard.insertText('KeepModernProbe');
  await expect.poll(() => draft?.attrs?.styleProfile).toBe(modern);
  const original = copy(draft);
  for (const profile of [null, 'future-v99']) {
    toRestore = { ...simple('MustNotReplace'), attrs: { styleProfile: profile } };
    const expectedCalls = restoreCalls + 1;
    await page.locator('#restore-draft').click(); await expect.poll(() => restoreCalls).toBe(expectedCalls);
    await expect(page.locator('#edit-font-size')).toBeEnabled(); await expect(page.locator('#notice')).toContainText('草稿恢复失败');
    await expect(page.locator(editor)).toHaveAttribute('data-style-profile', modern);
    await expect(page.locator(editor)).toHaveText('KeepModernProbe');
    expect(draft).toEqual(original);
  }
  toRestore = JSON.parse(readFileSync('tests/fixtures/authoring.json', 'utf8'));
  await page.locator('#restore-draft').click();
  await expect(page.locator(editor)).toHaveAttribute('data-style-profile', 'legacy-v1');
  await expect(page.locator(editor)).toHaveCSS('font-size', '16px');
  await page.locator('#edit-save').click(); await expect(page.locator('#notice')).toContainText('已导出 DOCX');
  expect(saved).not.toHaveProperty('attrs'); expect(saved.type).toBe('doc');
});

test('external DOC and DOCX imports adopt the standard profile after accepting normalization', async ({ page }) => {
  let format = 'doc', draft;
  const imported = simple('ImportedProbe');
  imported.content[0].content[0].marks = [{ type: 'fontSize', attrs: { halfPoints: 27 } }, { type: 'textColor', attrs: { rgb: '123456' } }];
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return { name: `external.${format}`, path: `external.${format}`, authored: null, inspection: { format, text: 'ImportedProbe', bytes: 1000, warnings: [] } };
    if (command === 'import_current') return { document: imported, compatibility: { risks: ['Default document style normalized in editable copy'] } };
    if (command === 'save_draft') { draft = copy(args.doc); return; }
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept()); await page.goto('/');
  for (format of ['doc', 'docx']) {
    await page.locator('#open').click(); await page.locator('#import-document').click();
    await expect(page.locator('#import-risks')).toContainText('编辑副本使用统一的正文和标题默认样式');
    await page.getByRole('button', { name: '取消', exact: true }).click();
    await expect(page.locator(editor)).toHaveCount(0);
    await page.locator('#import-document').click();
    draft = undefined;
    await page.getByRole('button', { name: '接受变化并编辑副本' }).click();
    await expect(page.locator(editor)).toHaveAttribute('data-style-profile', modern);
    await expect.poll(() => draft?.attrs?.styleProfile).toBe(modern);
    await expect(page.locator(`${editor} [data-font-size="27"]`)).toHaveCSS('font-size', '18px');
    await expect(page.locator(`${editor} [data-text-color="123456"]`)).toHaveCSS('color', 'rgb(18, 52, 86)');
    expect(draft.content[0].content[0].marks).toEqual(expect.arrayContaining(imported.content[0].content[0].marks));
  }
});
