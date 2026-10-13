import {test, expect} from '@playwright/test';
import {getSchema, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, TextSelection} from '@tiptap/pm/state';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {splitBlock, joinBackward} from '@tiptap/pm/commands';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {wrapInList} from '@tiptap/pm/schema-list';
import {mkdirSync, writeFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const {ParagraphSpacing, omitDefaultParagraphSpacing} = await loadTsModule('./ui/word-paragraph-spacing.ts');
const {snapshotDocument} = await loadTsModule('./ui/word-style-profile.ts');
const {createWordBlockTypeTransaction} = await loadTsModule('./ui/word-block-type.ts');
const {WordParagraphIndentControls} = await loadTsModule('./ui/word-paragraph-indent-controls.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const {FontSize} = await loadTsModule('./ui/word-font-size.ts');
const {FontFamily} = await loadTsModule('./ui/word-font-family.ts');
const WordDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+', addAttributes: () => ({styleProfile: {default: null}})});
const schema = getSchema([StarterKit.configure({document: false}), WordDoc, TableKit.configure({table: false}), tables.WordTable,
  ParagraphSpacing, FontSize, FontFamily, page.WordPageGeometry, indent.ParagraphIndent]);
const geometry = {...page.presetPageGeometry('A4', 'portrait'), marginRightTwips: 1800};
const bodyWidth = geometry.widthTwips - geometry.marginLeftTwips - geometry.marginRightTwips;
const p = (text = 'body', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const h = (text = 'heading', attrs = {}) => schema.node('heading', {level: 2, ...attrs}, schema.text(text));
const doc = (children, pageGeometry = geometry) => schema.node('doc', {styleProfile: 'standard-v2', pageGeometry}, children);
const table = paragraph => schema.node('table', {columnWidthsTwips: [4000]}, [schema.node('tableRow', null, [schema.node('tableCell', null, [paragraph])])]);
const snapshot = document => indent.omitDefaultParagraphIndent(tables.omitDefaultTableWidths(omitDefaultParagraphSpacing(snapshotDocument({getJSON: () => document.toJSON()}))));
const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), indent.paragraphIndentGuard(message => errors.push(message)), page.pageGeometryGuard(message => errors.push(message)), tables.tableWidthGuard(message => errors.push(message))]});
const apply = (state, transaction) => state.apply(closeHistory(transaction));
const applyIndent = (state, patch) => apply(state, indent.createParagraphIndentTransaction(state, patch));
const raw = (attrs, pageGeometry = geometry, type = 'paragraph') => ({type: 'doc', ...(pageGeometry === undefined ? {} : {attrs: {pageGeometry}}),
  content: [{type, attrs: {...(type === 'heading' ? {level: 2} : {}), ...attrs}, content: [{type: 'text', text: 'bounded'}]}]});
const check = (attrs, pageGeometry = geometry, type = 'paragraph') => indent.validateParagraphIndentDocument(raw(attrs, pageGeometry, type));

test('all three indent fields parse signed exact twips without coercion or rounding', () => {
  for (const field of indent.paragraphIndentFields) {
    for (const [text, expected] of [['-1584', -31680], ['-.05', -1], [' -18.05 ', -361], ['-0', 0], ['1584', 31680]]) expect(indent.parseParagraphIndentPoints(field, text)).toBe(expected);
    for (const [text, expected] of [['-12px', -180], ['-.05pt', -1], ['-0', 0], ['1584pt', 31680]]) expect(indent.parseParagraphIndentCss(field, text)).toBe(expected);
    for (const value of ['-1584.05', '-.001', '--1', '+1', '1e2', 'NaN', '-Infinity']) expect(indent.parseParagraphIndentPoints(field, value)).toBeNull();
    for (const value of ['-0.01px', '-2em', 'calc(-1pt)', '-1%', 'auto']) expect(indent.parseParagraphIndentCss(field, value)).toBeNull();
    for (const value of [-31680, -1, 0, 31680]) expect(indent.validParagraphIndent(field, value)).toBe(true);
    for (const value of [-31681, 31681, null, undefined, true, '-20', 1.5, NaN, Infinity]) expect(indent.validParagraphIndent(field, value)).toBe(false);
  }
});

test('top-level body and heading line boxes fit paper edges and retain strictly positive widths', () => {
  for (const type of ['paragraph', 'heading']) {
    for (const attrs of [{indentLeftTwips: -1440}, {indentRightTwips: -1800}, {firstLineIndentTwips: -1440},
      {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: 0},
      {indentLeftTwips: -1000, firstLineIndentTwips: -440},
      // A negative right indent can place the remaining line box past the body edge.
      {indentLeftTwips: bodyWidth + 1799, indentRightTwips: -1800},
      {indentLeftTwips: bodyWidth + 1799, indentRightTwips: -1800, firstLineIndentTwips: -1440},
      {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: geometry.widthTwips - 1}]) expect(() => check(attrs, geometry, type)).not.toThrow();
    for (const attrs of [{indentLeftTwips: -1441}, {indentRightTwips: -1801}, {firstLineIndentTwips: -1441},
      {indentLeftTwips: -1000, firstLineIndentTwips: -441},
      {indentLeftTwips: -1441, firstLineIndentTwips: 1},
      {indentLeftTwips: bodyWidth + 1800, indentRightTwips: -1800, firstLineIndentTwips: -1},
      {indentLeftTwips: bodyWidth + 1799, indentRightTwips: -1800, firstLineIndentTwips: 1},
      {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: geometry.widthTwips}]) expect(() => check(attrs, geometry, type)).toThrow('段落缩进');
  }
});

test('overhang requires complete valid explicit geometry and preserves legacy no-page and zero-margin rejection', () => {
  const invalidPages = [null, {}, [], 'A4', {...geometry, extra: 1}, {...geometry, heightTwips: 1}, {...geometry, widthTwips: 12000},
    {...geometry, marginTopTwips: -1}, {...geometry, marginBottomTwips: 4321}, {...geometry, marginLeftTwips: 1.5},
    {...geometry, marginRightTwips: '1800'}, {widthTwips: geometry.widthTwips, marginLeftTwips: 1440, marginRightTwips: 1800},
    ...page.pageGeometryFields.map(field => {const value = {...geometry}; delete value[field]; return value;})];
  for (const value of invalidPages) {
    expect(page.isPageGeometry(value)).toBe(false);
    expect(() => check({indentLeftTwips: -1}, value)).toThrow();
  }
  const explicitUndefined = raw({indentLeftTwips: -1}); explicitUndefined.attrs.pageGeometry = undefined;
  expect(() => indent.validateParagraphIndentDocument(explicitUndefined)).toThrow();
  for (const attrs of [{indentLeftTwips: -1}, {indentRightTwips: -1}, {firstLineIndentTwips: -1}, {indentLeftTwips: 20, firstLineIndentTwips: -21}]) {
    const legacy = raw(attrs); delete legacy.attrs;
    expect(() => indent.validateParagraphIndentDocument(legacy)).toThrow();
    expect(() => check(attrs, {...geometry, marginLeftTwips: 0, marginRightTwips: 0})).toThrow();
  }
  const legacy = raw({indentLeftTwips: 9025}); delete legacy.attrs;
  expect(() => indent.validateParagraphIndentDocument(legacy)).not.toThrow(); legacy.content[0].attrs.indentLeftTwips++;
  expect(() => indent.validateParagraphIndentDocument(legacy)).toThrow();
});

test('page margins never widen cell or nested-container indentation permission', () => {
  for (const rootPage of [null, geometry]) for (const attrs of [{indentLeftTwips: -1}, {indentRightTwips: -1}, {firstLineIndentTwips: -1}, {indentLeftTwips: 20, firstLineIndentTwips: -21}]) {
    expect(() => indent.validateParagraphIndentDocument(snapshot(doc([table(p('cell', attrs))], rootPage)))).toThrow();
  }
  for (const wrap of [node => schema.node('blockquote', null, [node]), node => schema.node('bulletList', null, [schema.node('listItem', null, [node])]), table]) {
    expect(() => indent.validateParagraphIndentDocument(snapshot(doc([wrap(p('overhang', {indentLeftTwips: -1}))])))).toThrow();
  }
  expect(() => indent.validateParagraphIndentDocument(snapshot(doc([table(p('cell', {indentLeftTwips: 20, firstLineIndentTwips: -20}))])))).not.toThrow();
});

test('signed multi-paragraph edit preserves marks selection and draft values with one undo and redo', () => {
  const original = doc([p('first', {spaceBefore: 20}), h('second', {spaceAfter: 0})]);
  let state = stateFor(original, new AllSelection(original)); state = state.apply(state.tr.setStoredMarks([schema.marks.bold.create()]));
  const selected = state.selection.toJSON(), marks = state.storedMarks;
  state = applyIndent(state, {indentLeftTwips: -720, indentRightTwips: -360, firstLineIndentTwips: -720});
  const edited = state.doc;
  expect(state.selection.toJSON()).toEqual(selected); expect(state.storedMarks).toEqual(marks);
  expect(edited.firstChild.content.eq(original.firstChild.content)).toBe(true);
  expect(edited.firstChild.attrs.spaceBefore).toBe(20); expect(edited.lastChild.attrs.spaceAfter).toBe(0);
  expect(schema.nodeFromJSON(snapshot(edited)).eq(edited)).toBe(true);
  expect(indent.selectedParagraphIndent(state).fields.indentLeftTwips).toEqual({value: -720, mixed: false, explicit: true});
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true); expect(undo(state)).toBe(false);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
});

test('unsafe side edits and multi-selection body plus cell reject before shared transaction or history mutation', () => {
  const initial = doc([p('body'), table(p('cell'))]), state = stateFor(initial, new AllSelection(initial));
  const shared = state.tr, chainable = Object.create(state); Object.defineProperty(chainable, 'tr', {get: () => shared});
  for (const patch of [{indentLeftTwips: -1}, {indentRightTwips: -1}, {firstLineIndentTwips: -1}]) expect(() => indent.createParagraphIndentTransaction(chainable, patch)).toThrow();
  expect(shared.steps).toEqual([]); expect(state.doc).toBe(initial); expect(undo(state)).toBe(false);
  for (const pageGeometry of [null, geometry]) {
    const single = stateFor(doc([p()], pageGeometry));
    expect(() => indent.createParagraphIndentTransaction(single, {indentLeftTwips: -1441})).toThrow(); expect(undo(single)).toBe(false);
  }
});

test('reducing either protecting margin or resetting the page rejects helper and direct transactions atomically', () => {
  const initial = doc([p('edge', {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: 0})]);
  const errors = [], state = stateFor(initial, TextSelection.create(initial, 2), errors), selection = state.selection;
  const shared = state.tr, chainable = Object.create(state); Object.defineProperty(chainable, 'tr', {get: () => shared});
  for (const candidate of [{...geometry, marginLeftTwips: 1439}, {...geometry, marginRightTwips: 1799}, null]) {
    expect(() => page.createPageGeometryTransaction(chainable, candidate)).toThrow('段落缩进');
    const rejected = state.applyTransaction(state.tr.setDocAttribute('pageGeometry', candidate));
    expect(rejected.transactions).toEqual([]); expect(rejected.state).toBe(state);
    expect(state.doc).toBe(initial); expect(state.selection).toBe(selection); expect(undo(state)).toBe(false);
  }
  expect(shared.steps).toEqual([]); expect(errors).toHaveLength(3);
  let changed = apply(state, page.createPageGeometryTransaction(state, {...geometry, marginLeftTwips: 1500, marginRightTwips: 1900}));
  expect(changed.doc.firstChild.eq(initial.firstChild)).toBe(true);
  expect(undo(changed, tr => {changed = changed.apply(tr);})).toBe(true); expect(changed.doc.eq(initial)).toBe(true);
  expect(redo(changed, tr => {changed = changed.apply(tr);})).toBe(true); expect(changed.doc.attrs.pageGeometry.marginLeftTwips).toBe(1500);
});

test('reset indentation then reset page succeeds without automatic geometry creation or field clamping', () => {
  const initial = doc([p('edge', {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: 0})]); let state = stateFor(initial);
  state = applyIndent(state, {indentLeftTwips: null, indentRightTwips: null, firstLineIndentTwips: null});
  state = apply(state, page.createPageGeometryTransaction(state, null));
  expect(snapshot(state.doc).attrs).toEqual({styleProfile: 'standard-v2'}); expect(snapshot(state.doc).content[0]).not.toHaveProperty('attrs');
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.attrs.pageGeometry).toEqual(geometry);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(initial)).toBe(true);
  const legacy = stateFor(doc([p()], null)); expect(() => indent.createParagraphIndentTransaction(legacy, {indentLeftTwips: -1})).toThrow();
  expect(legacy.doc.attrs.pageGeometry).toBeNull();
});

test('app-owned heading conversions preserve signed overhang, spacing and separate undo steps', () => {
  const initial = doc([p('one', {indentLeftTwips: -1440, indentRightTwips: 0, firstLineIndentTwips: 0, spaceBefore: 40}),
    h('two', {indentLeftTwips: 0, indentRightTwips: -1800, firstLineIndentTwips: -1440})]);
  let state = stateFor(initial, new AllSelection(initial));
  for (const level of [1, 3, 0]) {
    const previous = state.doc;
    state = apply(state, createWordBlockTypeTransaction(state, level));
    state.doc.forEach((node, _, index) => {
      expect(node.type.name).toBe(level ? 'heading' : 'paragraph');
      for (const field of [...indent.paragraphIndentFields, 'spaceBefore', 'spaceAfter', 'lineSpacing']) expect(node.attrs[field]).toBe(previous.child(index).attrs[field]);
      expect(node.content.eq(previous.child(index).content)).toBe(true);
    });
  }
  for (let index = 0; index < 3; index++) expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.eq(initial)).toBe(true); expect(undo(state)).toBe(false);
});

test('real list wrapping and table movement of negative body paragraphs reject without dropping attributes', () => {
  for (const attrs of [{indentLeftTwips: -1}, {indentRightTwips: -1}, {firstLineIndentTwips: -1}]) {
    const original = doc([p('move', attrs)]), state = stateFor(original, TextSelection.create(original, 2)); let wrapped;
    expect(wrapInList(schema.nodes.bulletList)(state, tr => {wrapped = tr;})).toBe(true);
    for (const candidate of [wrapped, state.tr.replaceWith(0, original.content.size, table(original.firstChild))]) {
      const result = state.applyTransaction(candidate); expect(result.transactions).toEqual([]); expect(result.state).toBe(state);
    }
    expect(state.doc).toBe(original); expect(undo(state)).toBe(false);
  }
});

test('Enter and Backspace keep bounded overhang while tab input still rejects', () => {
  const initial = doc([p('firstsecond', {indentLeftTwips: -720, indentRightTwips: -360, firstLineIndentTwips: -720})]);
  let state = stateFor(initial, TextSelection.create(initial, 6));
  expect(splitBlock(state, tr => {state = apply(state, tr);})).toBe(true); expect(state.doc.childCount).toBe(2);
  for (let index = 0; index < 2; index++) for (const field of indent.paragraphIndentFields) expect(state.doc.child(index).attrs[field]).toBe(initial.firstChild.attrs[field]);
  expect(joinBackward(state, tr => {state = apply(state, tr);})).toBe(true); expect(state.doc.eq(initial)).toBe(true);
  const result = state.applyTransaction(state.tr.insertText('\t', 2)); expect(result.transactions).toEqual([]); expect(result.state).toBe(state);
});

// The shim exercises real ProseMirror parsing/serialization; it does not claim
// browser selection, native clipboard, CSS line layout or printed-page proof.
class ParseElement {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml';
  constructor(tag = 'div', attrs = {}) {
    this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = [];
    const propertyValue = property => (this.attrs.style ?? '').split(';').map(part => part.split(':')).find(([name]) => name.trim() === property)?.[1]?.trim() ?? '';
    this.style = new Proxy({getPropertyValue: propertyValue}, {get: (object, key) => {
      if (key === 'length') return this.attrs.style ? 1 : 0;
      if (key in object) return object[key];
      const property = String(key).replace(/[A-Z]/g, ch => `-${ch.toLowerCase()}`);
      return propertyValue(property);
    }, set: (object, key, value) => {
      if (key === 'cssText') this.attrs.style = String(value); else object[key] = value;
      return true;
    }});
  }
  get firstChild() {return this.childNodes[0] ?? null;}
  get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}
  getAttribute(name) {return this.attrs[name] ?? null;}
  hasAttribute(name) {return name in this.attrs;}
  setAttribute(name, value) {this.attrs[name] = String(value);}
  appendChild(child) {child.parentNode = this; this.childNodes.push(child); return child;}
  matches(selector) {const match = /^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(selector); return !!match && (!match[1] || this.tagName.toLowerCase() === match[1]) && (!match[2] || this.hasAttribute(match[2]));}
}
const parseDocument = {createElement: tag => new ParseElement(tag), createElementNS: (_, tag) => new ParseElement(tag), createDocumentFragment: () => new ParseElement('fragment'),
  createTextNode: value => ({nodeType: 3, nodeValue: value, get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}})};
const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: parseDocument});
const parse = element => {const root = new ParseElement(); root.appendChild(element); return ProseMirrorDOMParser.fromSchema(schema).parse(root);};

test('signed metadata and physical CSS survive real paste parsing but require target page bounds', () => {
  for (const source of [p('signed body', {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: 0}),
    h('signed heading', {indentLeftTwips: 0, indentRightTwips: -20, firstLineIndentTwips: -1440})]) {
    const pasted = parse(serialize(source)); expect(pasted.firstChild.eq(source)).toBe(true);
    for (const targetGeometry of [null, geometry]) {
      const initial = doc([p('keep')], targetGeometry), state = stateFor(initial);
      const result = state.applyTransaction(state.tr.replaceWith(0, initial.content.size, pasted.content));
      expect(result.transactions).toHaveLength(targetGeometry ? 1 : 0);
      expect(targetGeometry ? result.state.doc.firstChild.eq(source) : result.state === state).toBe(true);
    }
  }
  const element = new ParseElement('p', {style: 'margin-left:-18.05pt;margin-right:-12px;text-indent:-.05pt'}); element.appendChild(parseDocument.createTextNode('CSS paste'));
  const pasted = parse(element); expect(pasted.firstChild.attrs).toMatchObject({indentLeftTwips: -361, indentRightTwips: -180, firstLineIndentTwips: -1});
  const state = stateFor(doc([table(p('keep'))]));
  expect(state.applyTransaction(state.tr.replaceWith(3, state.doc.content.size - 3, pasted.content)).transactions).toEqual([]);
  for (const value of ['-1441', '-31681', 'false']) {
    const bad = new ParseElement('p', {'data-indent-left-twips': value}); bad.appendChild(parseDocument.createTextNode('bad paste'));
    const initial = stateFor(doc([p('keep')])); expect(initial.applyTransaction(initial.tr.replaceWith(0, initial.doc.content.size, parse(bad).content)).transactions).toEqual([]);
  }
});

class Element {
  dataset = {}; attrs = {}; children = []; value = ''; placeholder = ''; disabled = false; textContent = ''; validity = {badInput: false}; listeners = new Map();
  addEventListener(type, listener) {this.listeners.set(type, [...this.listeners.get(type) ?? [], listener]);}
  emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
  contains(other) {return other?.id?.startsWith('edit-paragraph-indent-');}
  replaceChildren(...children) {this.children = children;}
  removeAttribute(name) {delete this.attrs[name];}
  querySelectorAll() {return [];}
  cloneNode() {const clone = new Element(); clone.attrs = {...this.attrs}; clone.dataset = {...this.dataset}; clone.children = this.children.map(child => child.cloneNode()); return clone;}
}

test('actual property controls apply negative sides once and preserve unsafe-reset rejection and undo', () => {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) {const value = new Element(); value.id = id; elements.set(id, value);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  const initial = doc([p()]); let state = stateFor(initial), writes = 0, controls;
  const editor = {get state() {return state;}, view: {dispatch(tr) {const result = state.applyTransaction(tr); state = result.state; writes += Number(result.transactions.some(item => item.docChanged)); controls.update();}, focus() {}}};
  try {
    controls = new WordParagraphIndentControls({editor: () => editor, locked: () => false, notice: message => notices.push(message), syncSelection() {}}); controls.update();
    for (const [id, value] of [['left', '-72'], ['right', '-18'], ['first', '0']]) {const input = element(`edit-paragraph-indent-${id}`); input.emit('focus'); input.value = value; input.emit('input');}
    element('edit-paragraph-indent-apply').emit('click'); expect(writes).toBe(1); expect(state.doc.firstChild.attrs).toMatchObject({indentLeftTwips: -1440, indentRightTwips: -360, firstLineIndentTwips: 0});
    expect(element('edit-paragraph-indent-left').value).toBe('-72');
    element('edit-paragraph-indent-apply').emit('click'); expect(writes).toBe(1);
    const first = element('edit-paragraph-indent-first'); first.emit('focus'); first.value = '-.05'; first.emit('input');
    element('edit-paragraph-indent-apply').emit('click'); expect(writes).toBe(1); expect(notices.at(-1)).toContain('纸张');
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(initial)).toBe(true);
    const wide = doc([p('right margin supplies remaining width', {indentLeftTwips: bodyWidth + 359, indentRightTwips: -360})]);
    state = stateFor(wide); controls.update(); element('edit-paragraph-indent-right-reset').emit('click');
    expect(state.doc).toBe(wide); expect(writes).toBe(1); expect(undo(state)).toBe(false); expect(notices.at(-1)).toContain('正文宽度');
  } finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
});

test('immutable print snapshot preserves signed margins and physical page CSS through delayed font loading', async () => {
  const previous = globalThis.document, host = new Element(), source = new Element(), paragraph = new Element();
  paragraph.attrs = schema.nodes.paragraph.spec.toDOM(p('print', {indentLeftTwips: -1440, indentRightTwips: -1800, firstLineIndentTwips: 0}))[1]; source.children = [paragraph];
  const originalAttrs = {...paragraph.attrs}, mutablePage = {...geometry};
  let ready; const fonts = new Promise(resolve => {ready = resolve;});
  globalThis.document = {getElementById: () => host, createElement: () => new Element(), fonts: {ready: fonts}};
  try {
    const pending = preparePrintSnapshot(source, mutablePage); paragraph.attrs.style = 'margin-left: 99pt'; mutablePage.marginLeftTwips = 0; ready(); await pending;
    expect(host.children[0].textContent).toContain('margin: 72pt 90pt 72pt 72pt');
    expect(JSON.parse(host.children[1].dataset.pageGeometry)).toEqual(geometry);
    expect(host.children[1].children[0].attrs).toEqual(originalAttrs);
    expect(originalAttrs.style).toContain('margin-left: -72pt'); expect(originalAttrs.style).toContain('margin-right: -90pt');
  } finally {globalThis.document = previous;}
});

test('emit actual edit heading history and parsed paste snapshots for independent DOCX checking', () => {
  const text = Array.from({length: 80}, (_, index) => `marker${String(index).padStart(2, '0')} overhang`).join(' ');
  const marked = () => schema.node('paragraph', {spaceBefore: 0, spaceAfter: 0, lineSpacing: 240},
    schema.text(text, [schema.marks.fontSize.create({halfPoints: 24}), schema.marks.fontFamily.create({name: 'DejaVu Sans'})]));
  const initial = doc([marked(), marked()]); let state = stateFor(initial, new AllSelection(initial));
  const artifacts = {'initial': snapshot(initial)};
  state = applyIndent(state, {indentLeftTwips: -720, indentRightTwips: -360, firstLineIndentTwips: -720});
  const edited = state.doc; artifacts.edited = snapshot(edited);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); artifacts.undo = snapshot(state.doc); expect(state.doc.eq(initial)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); artifacts.redo = snapshot(state.doc); expect(state.doc.eq(edited)).toBe(true);
  state = apply(state, createWordBlockTypeTransaction(state, 2)); artifacts.headings = snapshot(state.doc);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); artifacts['heading-undo'] = snapshot(state.doc); expect(state.doc.eq(edited)).toBe(true);
  const pasted = parse(serialize(edited.firstChild)); expect(pasted.firstChild.toJSON()).toEqual(edited.firstChild.toJSON());
  const pastedResult = state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, pasted.content)); expect(pastedResult.transactions).toHaveLength(1); artifacts.pasted = snapshot(pastedResult.state.doc);
  const directory = 'tmp/paragraph-overhang-editor-models'; mkdirSync(directory, {recursive: true});
  for (const [name, document] of Object.entries(artifacts)) {
    indent.validateParagraphIndentDocument(document); page.validatePageGeometryDocument(document);
    writeFileSync(`${directory}/${name}.json`, JSON.stringify({version: 2, document}, null, 2) + '\n');
  }
  writeFileSync(`${directory}/manifest.json`, JSON.stringify({source: 'Real frontend createParagraphIndentTransaction/createWordBlockTypeTransaction, ProseMirror history and DOMParser/DOMSerializer; no browser layout',
    files: Object.keys(artifacts).map(name => `${name}.json`), assertions: {undoEqualsInitial: JSON.stringify(artifacts.undo) === JSON.stringify(artifacts.initial), redoEqualsEdited: JSON.stringify(artifacts.redo) === JSON.stringify(artifacts.edited), headingUndoEqualsEdited: JSON.stringify(artifacts['heading-undo']) === JSON.stringify(artifacts.edited)}}, null, 2) + '\n');
});
