import {test, expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
import {AllSelection, TextSelection} from '@tiptap/pm/state';
import {undoDepth, undo} from '@tiptap/pm/history';
import {spacing, minimum, controls, schema, p, h, doc, snapshot, stateFor} from './word-minimum-line-height-harness.mjs';
import {Element, dom, serialize, parse, wrapped, parserResult} from './word-minimum-line-height-dom.mjs';
import {loadTsModule} from './load-ts.mjs';
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const attr = {minimumLineHeightTwips: 276};
const data = 'data-word-minimum-line-height-twips';
function controlHarness(document, run) {
  const previous = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const el = id => {if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id);};
  let state = stateFor(document), locked = false, composing = false;
  const owner = {get state() {return state;}, isEditable: true, view: {get composing() {return composing;}, focus() {}, dispatch: tr => {state = state.applyTransaction(tr).state;}}};
  let current = owner;
  globalThis.document = {getElementById: el}; globalThis.HTMLElement = Element;
  const control = new controls.WordParagraphSpacingControls({editor: () => current, locked: () => locked, notice: (...args) => notices.push(args), syncSelection() {}}); control.update();
  try {run({el, control, notices, owner, state: () => state, select: selection => {state = state.apply(state.tr.setSelection(selection)); control.update();},
    locked: value => {locked = value; control.update();}, composing: value => {composing = value;}, replace: () => {current = {...owner}; control.update();},
    type: (id, value) => {el(id).emit('focus'); el(id).value = value; el(id).emit('input');},
    mode: value => {el('edit-line-mode').emit('focus'); el('edit-line-mode').value = value; el('edit-line-mode').emit('change');},
    click: id => el(id).emit('click')});} finally {globalThis.document = previous; globalThis.HTMLElement = previousElement;}
}

test('minimum real DOM schema transport preserves exact model for empty large-font multiline and headings', () => {
  for (const node of [p('', attr), schema.node('paragraph', {...attr, spaceBefore: 20}, [schema.text('line1'), schema.node('hardBreak'), schema.text('line2')]), h('heading', attr), schema.node('paragraph', attr, [schema.text('large', [schema.mark('fontSize', {halfPoints: 144})]), schema.node('hardBreak'), schema.text('next')])]) {
    const element = serialize(node), parsed = parse(element); expect(parsed.firstChild.toJSON()).toEqual(node.toJSON());
    expect(element.getAttribute(data)).toBe('276'); expect(element.style.lineHeight).toBe('');
    expect(snapshot(parsed).content[0].attrs.minimumLineHeightTwips).toBe(276);
  }
});
test('minimum raw HTML preflight rejects nested wrong-node conflict and malformed metadata before schema loss', () => {
  const cases = [new Element('span', {[data]: '276'}), new Element('p', {[data]: '0'}), new Element('h4', {[data]: '276'}),
    new Element('p', {[data]: '276', 'data-line-spacing': '240'}), new Element('p', {[data]: '276', style: 'line-height:13.8pt'})];
  const listed = new Element('li'); listed.appendChild(new Element('p', {[data]: '276'})); cases.push(listed);
  const image = new Element('p', {[data]: '276'}); image.appendChild(new Element('img')); cases.push(image);
  for (const element of cases) parserResult(wrapped(element), () => expect(minimum.preflightMinimumLineHeightHTML(`<p ${data}="276"></p>`)).toContain(`${data}="invalid"`));
  const valid = `<p ${data}="276"></p>`;
  parserResult(wrapped(new Element('p', {[data]: '276'})), () => expect(minimum.preflightMinimumLineHeightHTML(valid)).toBe(valid));
});
test('minimum arbitrary CSS point line-height never becomes authoritative minimum metadata', () => {
  const element = new Element('p', {style: 'line-height:13.8pt'}); element.appendChild(dom.createTextNode('x'));
  const parsed = parse(element); expect(parsed.firstChild.attrs.minimumLineHeightTwips).toBeNull(); expect(parsed.firstChild.attrs.lineSpacing).toBeNull();
  parserResult(wrapped(element), () => expect(minimum.preflightMinimumLineHeightHTML('original')).toBe('original'));
});
test('minimum malformed DOM metadata survives parsing as rejected sentinel without draft/history mutation', () => {
  for (const raw of ['', '0', '31681', '276.0', ' 276', 'null']) {
    const element = serialize(p('invalid', attr)); element.setAttribute(data, raw); const parsed = parse(element);
    expect(parsed.firstChild.attrs.minimumLineHeightTwips).toBe('invalid-minimum-line-height');
    const state = stateFor(doc([p('safe')])); expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]);
    expect(undoDepth(state)).toBe(0);
  }
});
test('minimum actual controls switch modes apply exact pt and preserve gaps with single undo', () => {
  const original = doc([p('x', {spaceBefore: 20, spaceAfter: 40, lineSpacing: 241})]);
  controlHarness(original, app => {
    expect(app.el('edit-line-mode').value).toBe('multiple'); expect(app.el('edit-line-spacing').value).toBe('241/240');
    app.mode('minimum'); app.type('edit-minimum-line-height', '13.8'); app.click('edit-paragraph-spacing-apply');
    expect(snapshot(app.state().doc).content[0].attrs).toEqual({textAlign: null, spaceBefore: 20, spaceAfter: 40, minimumLineHeightTwips: 276}); expect(undoDepth(app.state())).toBe(1);
    expect(app.el('edit-line-spacing-state').textContent).toContain('13.8 pt'); expect(app.el('edit-line-spacing-state').textContent).toContain('不模拟');
    app.type('edit-space-before', '3'); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.firstChild.attrs.minimumLineHeightTwips).toBe(276);
    app.click('edit-line-spacing-reset'); expect(snapshot(app.state().doc).content[0].attrs).toEqual({textAlign: null, spaceBefore: 60, spaceAfter: 40});
    app.mode('multiple'); app.type('edit-line-spacing', '1.5'); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.firstChild.attrs.lineSpacing).toBe(360);
  });
});
test('minimum actual controls keep untouched mixed line modes on gap-only apply and reset all explicitly', () => {
  const original = doc([p('a', attr), h('b', {lineSpacing: 360})]);
  controlHarness(original, app => {
    app.select(new AllSelection(app.state().doc)); expect(app.el('edit-line-mode').value).toBe('mixed');
    app.type('edit-space-after', '2'); app.click('edit-paragraph-spacing-apply');
    expect(app.state().doc.child(0).attrs.minimumLineHeightTwips).toBe(276); expect(app.state().doc.child(1).attrs.lineSpacing).toBe(360);
    app.click('edit-paragraph-spacing-reset');
    for (const node of snapshot(app.state().doc).content) {expect(node.attrs?.minimumLineHeightTwips).toBeUndefined(); expect(node.attrs?.lineSpacing).toBeUndefined(); expect(node.attrs?.spaceAfter).toBeUndefined();}
  });
});
test('minimum actual controls reject invalid value stale selection and replaced editor without applying pending state', () => {
  const original = doc([p('a', attr), p('b')]);
  controlHarness(original, app => {
    app.type('edit-minimum-line-height', '0'); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.eq(original)).toBe(true); expect(app.notices).toHaveLength(1);
    app.control.cancelEditing(); app.type('edit-minimum-line-height', '14'); app.select(TextSelection.create(app.state().doc, 4)); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.eq(original)).toBe(true);
    app.type('edit-line-spacing', '2'); app.replace(); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.eq(original)).toBe(true); expect(undoDepth(app.state())).toBe(0);
  });
});
test('minimum actual controls obey locked composing Escape and default-mode events', () => {
  const original = doc([p('x', attr)]);
  controlHarness(original, app => {
    app.type('edit-minimum-line-height', '14'); app.locked(true); expect(app.el('edit-minimum-line-height').disabled).toBe(true); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.eq(original)).toBe(true);
    app.locked(false); app.type('edit-minimum-line-height', '15'); app.composing(true); app.click('edit-paragraph-spacing-apply'); expect(app.state().doc.eq(original)).toBe(true); app.composing(false);
    app.type('edit-minimum-line-height', '16'); app.el('edit-minimum-line-height').emit('keydown', {key: 'Escape'}); expect(app.el('edit-minimum-line-height').value).toBe('13.8');
    app.mode('default'); app.click('edit-paragraph-spacing-apply'); expect(snapshot(app.state().doc).content[0].attrs).toEqual({textAlign: null});
  });
});
test('minimum print snapshot retains exact metadata without fixed line boxes and is immutable', async () => {
  const source = serialize(p('large\nnext', attr)), host = new Element(), previous = globalThis.document;
  globalThis.document = {...dom, getElementById: () => host, fonts: {ready: Promise.resolve()}};
  try {
    await preparePrintSnapshot(source); source.setAttribute(data, '999');
    expect(host.firstChild.getAttribute(data)).toBe('276'); expect(host.firstChild.style.lineHeight).toBe('');
  } finally {globalThis.document = previous;}
});
test('minimum production wiring validates raw model and metadata before normalization and image removal', () => {
  const source = readFileSync('ui/authoring.ts', 'utf8'), html = readFileSync('ui/index.html', 'utf8');
  expect(source.indexOf('validateParagraphSpacingDocument(initial)')).toBeLessThan(source.indexOf('editor = new Editor'));
  expect(source).toContain('MinimumLineHeight.configure'); expect(source).toContain('preflightTableRowPaginationHTML(preflightMinimumLineHeightHTML(html))');
  expect(source).toContain('paragraphSpacingControls?.reset()'); expect(source).toContain('paragraphSpacingControls?.update()');
  for (const id of ['edit-line-mode', 'edit-minimum-line-height', 'edit-line-spacing-reset', 'edit-line-spacing-state']) expect(html).toContain(`id="${id}"`);
  expect(html).toContain('aria-label="最小行高（pt）"');
});

 test('minimum raw duplicate and mixed-case declarations reject before DOM parser normalization', () => {
  const single = wrapped(new Element('p', {[data]: '276'}));
  for (const duplicate of [data, data.toUpperCase()]) parserResult(single, () => {
    expect(minimum.preflightMinimumLineHeightHTML(`<p ${data}="276" ${duplicate}="400">x</p>`)).toContain(`${data}="invalid"`);
  });
  for (const text of [`<!-- <p ${data}="276"> -->`, `<p title='${data}="276"'>ordinary</p>`, `<script>const x='${data}'</script>`]) {
    parserResult(wrapped(new Element('p')), () => expect(minimum.preflightMinimumLineHeightHTML(text)).toBe(text));
  }
});

test('minimum literal newlines remain in source and serialize exactly while legacy default HTML parsing normalizes equally', () => {
  for (const attrs of [{}, attr]) {
    const node = p('line1\nline2', attrs), element = serialize(node), parsed = parse(element);
    expect(node.textContent).toBe('line1\nline2'); expect(element.firstChild.nodeValue).toBe('line1\nline2');
    // Existing ProseMirror default HTML parsing collapses whitespace. This is
    // a no-minimum control, not a promise to preserve literal newlines on paste.
    expect(parsed.firstChild.textContent).toBe('line1 line2');
    expect(parsed.firstChild.attrs.minimumLineHeightTwips).toBe(attrs.minimumLineHeightTwips ?? null);
  }
});

test('minimum DOM double mirrors serializer cssText assignment and preserves existing large-font mark without minimum', () => {
  const node = schema.node('paragraph', null, schema.text('large', [schema.mark('fontSize', {halfPoints: 144})]));
  const element = serialize(node), span = element.querySelector('span');
  expect(span.style.fontSize).toBe('72pt'); expect(span.style.length).toBe(1); expect(span.style.getPropertyValue('font-size')).toBe('72pt');
  expect(parse(element).firstChild.toJSON()).toEqual(node.toJSON());
});
