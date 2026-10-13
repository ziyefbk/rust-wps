// Real model/transaction/plugin callbacks and standards-shaped DOM only.
// No browser CSS layout, physical keyboard, clipboard or native IME is exercised.
import {test, expect} from '@playwright/test';
import {Editor, getSchema, getExtensionField} from '@tiptap/core';
import {EditorState, TextSelection, AllSelection, Plugin} from '@tiptap/pm/state';
import {DOMParser as ModelDOMParser, DOMSerializer, Fragment, Slice} from '@tiptap/pm/model';
import {closeHistory, history, undo, redo, undoDepth} from '@tiptap/pm/history';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
import {interactionExtensions, snapshot, schema as oldSchema} from './word-block-type-harness.mjs';
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const {WordEmptyParagraphFontControls} = await loadTsModule('./ui/word-empty-paragraph-font-controls.ts');
const {createWordBlockTypeTransaction, validateWordBlockTypeDocument} = await loadTsModule('./ui/word-block-type.ts');
const {createTableStructureTransaction} = await loadTsModule('./ui/word-table-widths.ts');
const {createFontFamilyTransaction} = await loadTsModule('./ui/word-font-family.ts');
const {createFontSizeTransaction} = await loadTsModule('./ui/word-font-size.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const extensions = () => [...interactionExtensions(), empty.EmptyParagraphFont];
const schema = getSchema(extensions());
const font = extra => ({family: 'Liberation Serif', halfPoints: 48, bold: true, italic: false, ...extra});
const p = (value = undefined, text = '', attrs = {}) => schema.node('paragraph', {...attrs, ...(value === undefined ? {} : {emptyParagraphFont: value})}, text ? schema.text(text) : null);
const h = (value = font(), level = 2) => schema.node('heading', {level, emptyParagraphFont: value});
const doc = (...content) => schema.node('doc', {styleProfile: 'standard-v2'}, content);
const stateFor = document => EditorState.create({doc: document, selection: TextSelection.atStart(document), plugins: [history(), empty.emptyParagraphFontGuard()]});
const apply = (state, transaction) => state.applyTransaction(empty.normalizeEmptyParagraphFontTransaction(transaction)).state;
const raw = (value, type = 'paragraph', content) => ({type: 'doc', content: [{type, attrs: {emptyParagraphFont: value}, ...(content === undefined ? {} : {content})}]});
const invalid = [null, false, 1, 'Arial', [], {}, {family: 'Arial'}, {...font(), extra: true}, {...font(), bold: 'false'}, {...font(), italic: null},
  ...[1, 3277, 3.5, '24', NaN, Infinity].map(halfPoints => font({halfPoints})),
  ...['', ' a', 'a ', 'x'.repeat(32), 'bad\nfont', '\ud800', '😀'.repeat(16)].map(family => font({family}))];

for (const type of ['paragraph', 'heading']) test(`${type} accepts complete exact empty font boundaries and rejects all malformed raw values`, () => {
  for (const value of [font(), font({halfPoints: 2}), font({halfPoints: 3276, family: '字'.repeat(31), italic: true, bold: false})]) {
    expect(empty.isEmptyParagraphFont(value)).toBe(true);
    expect(() => empty.validateEmptyParagraphFontDocument(raw(value, type))).not.toThrow();
    expect(() => empty.validateEmptyParagraphFontDocument(raw(value, type, []))).not.toThrow();
  }
  for (const value of invalid) {
    expect(empty.isEmptyParagraphFont(value)).toBe(false);
    expect(() => empty.validateEmptyParagraphFontDocument(raw(value, type))).toThrow('空段字体');
  }
  for (const content of [[{type: 'text', text: ' '}], [{type: 'text', text: '\t'}], [{type: 'hardBreak'}], [{type: 'text', text: 'x'}], null, '']) {
    expect(() => empty.validateEmptyParagraphFontDocument(raw(font(), type, content))).toThrow('空段字体');
  }
});

test('misplaced raw node/mark metadata rejects before schema normalization can erase it', () => {
  for (const type of ['doc', 'text', 'image', 'hardBreak', 'table', 'tableRow', 'tableCell', 'bulletList', 'listItem']) {
    expect(() => empty.validateEmptyParagraphFontDocument(raw(font(), type))).toThrow('空段字体');
  }
  expect(() => empty.validateEmptyParagraphFontDocument({type: 'paragraph', marks: [{type: 'bold', attrs: {emptyParagraphFont: font()}}]})).toThrow('空段字体');
  const source = readFileSync('ui/authoring.ts', 'utf8');
  expect(source.indexOf('validateEmptyParagraphFontDocument(initial)')).toBeLessThan(source.indexOf('closeAuthoring();', source.indexOf('async function create(')));
});

test('absent attribute retains old JSON exactly, null is internal-only and drafts are lossless', () => {
  const legacy = oldSchema.node('doc', null, [oldSchema.node('paragraph'), oldSchema.node('heading', {level: 3}, oldSchema.text('same'))]);
  const current = schema.nodeFromJSON(legacy.toJSON());
  expect(snapshot(current)).toEqual(snapshot(legacy));
  expect(empty.omitDefaultEmptyParagraphFont(p().toJSON()).attrs).not.toHaveProperty('emptyParagraphFont');
  expect(p().toJSON().attrs.emptyParagraphFont).toBeNull();
  const original = doc(p(font()), h(), p(undefined, 'after'));
  expect(schema.nodeFromJSON(snapshot(original)).eq(original)).toBe(true);
  expect(() => snapshot(doc(p(font(), 'invalid')))).toThrow('空段字体');
});

for (const kind of ['typing', 'paste', 'composition', 'hardBreak', 'whitespace']) test(`first ${kind} clears override atomically, preserves marks and restores exact original on undo`, () => {
  let state = stateFor(doc(p(font()), p(undefined, 'after'))), original = state.doc;
  const marks = [schema.marks.italic.create(), schema.marks.fontFamily.create({name: 'Arial'}), schema.marks.fontSize.create({halfPoints: 21})];
  state = state.apply(state.tr.setStoredMarks(marks));
  let tr = kind === 'hardBreak' ? state.tr.replaceSelectionWith(schema.node('hardBreak')) : state.tr.insertText(kind === 'whitespace' ? ' ' : '输入');
  if (kind === 'paste') tr.setMeta('paste', true).setMeta('uiEvent', 'paste');
  if (kind === 'composition') tr.setMeta('composition', 3);
  const result = state.applyTransaction(empty.normalizeEmptyParagraphFontTransaction(closeHistory(tr)));
  expect(result.transactions).toHaveLength(1); state = result.state;
  expect(state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  if (kind !== 'hardBreak') expect(state.doc.firstChild.firstChild.marks).toEqual(marks);
  expect(state.doc.lastChild.eq(original.lastChild)).toBe(true); expect(undoDepth(state)).toBe(1);
  const changed = state.doc;
  expect(undo(state, tr => {state = apply(state, tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = apply(state, tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
});

test('typing controls affect stored marks without changing the imported empty baseline', () => {
  let state = stateFor(doc(p(font())));
  state = apply(state, createFontFamilyTransaction(state, 'Arial')); state = apply(state, createFontSizeTransaction(state, 25));
  expect(state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font()); expect(undoDepth(state)).toBe(0);
  state = apply(state, state.tr.insertText('x'));
  expect(state.doc.firstChild.firstChild.marks.map(mark => mark.toJSON())).toEqual([{type: 'fontSize', attrs: {halfPoints: 25}}, {type: 'fontFamily', attrs: {name: 'Arial'}}]);
  expect(state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
});

test('split clears source and new blocks; joins never transfer either side metadata', () => {
  let state = stateFor(doc(p(font()), p(font({halfPoints: 32})), p(undefined, 'outside'))), original = state.doc;
  state = apply(state, state.tr.split(1));
  expect(state.doc.child(0).attrs.emptyParagraphFont).toBeNull(); expect(state.doc.child(1).attrs.emptyParagraphFont).toBeNull();
  expect(state.doc.child(2).attrs.emptyParagraphFont).toEqual(font({halfPoints: 32}));
  undo(state, tr => {state = apply(state, tr);}); expect(state.doc.eq(original)).toBe(true);
  for (const nodes of [[p(font()), p(font({halfPoints: 32}))], [p(font()), p(undefined, 'text')], [p(undefined, 'text'), p(font())]]) {
    const before = stateFor(doc(...nodes)), joined = apply(before, before.tr.join(nodes[0].nodeSize));
    expect(joined.doc.firstChild.attrs.emptyParagraphFont).toBeNull(); expect(joined.doc.childCount).toBe(1);
  }
});

test('delete-to-empty and insertion/deletion in one root never revive source metadata', () => {
  let state = stateFor(doc(p(font())));
  state = apply(state, state.tr.insertText('x')); state = apply(state, state.tr.delete(1, 2));
  expect(state.doc.firstChild.content.size).toBe(0); expect(state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  const source = stateFor(doc(p(font())));
  const result = apply(source, source.tr.insertText('x').delete(1, 2));
  expect(result.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
});

test('unrelated boundary insertions retain source override and valid empty pasted blocks retain their own canonical values', () => {
  let state = stateFor(doc(p(font()), p(undefined, 'after')));
  state = apply(state, state.tr.insert(0, p(undefined, 'before')));
  expect(state.doc.child(1).attrs.emptyParagraphFont).toEqual(font());
  state = apply(state, state.tr.insert(state.doc.firstChild.nodeSize + 2, p(font({halfPoints: 30}))).setMeta('paste', true));
  expect(state.doc.child(1).attrs.emptyParagraphFont).toEqual(font());
  expect(state.doc.child(2).attrs.emptyParagraphFont).toEqual(font({halfPoints: 30}));
});

test('empty heading conversions preserve font geometry and reset returns to that profile with atomic history', () => {
  let state = stateFor(doc(p(font(), '', {spaceBefore: 0, lineSpacing: 241, indentLeftTwips: 120}))), original = state.doc;
  for (const level of [1, 2, 3, 0]) {
    state = apply(state, closeHistory(createWordBlockTypeTransaction(state, level)));
    expect(state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font());
  }
  expect(state.doc.eq(original)).toBe(true);
  state = apply(state, closeHistory(createWordBlockTypeTransaction(state, 2))); const heading = state.doc;
  state = apply(state, closeHistory(empty.createEmptyParagraphFontReset(state))); const reset = state.doc;
  expect(reset.firstChild.attrs).toMatchObject({emptyParagraphFont: null, level: 2, spaceBefore: 0, lineSpacing: 241, indentLeftTwips: 120});
  undo(state, tr => {state = apply(state, tr);}); expect(state.doc.eq(heading)).toBe(true);
  redo(state, tr => {state = apply(state, tr);}); expect(state.doc.eq(reset)).toBe(true);
  expect(schema.nodeFromJSON(snapshot(reset)).eq(reset)).toBe(true);
});

test('reset preserves selection/stored marks; stale document/selection and nonempty targets are no-ops', () => {
  let state = stateFor(doc(p(font()), p(font({halfPoints: 32})))); state = state.apply(state.tr.setStoredMarks([]));
  const token = empty.captureEmptyParagraphFont(state), tr = empty.createEmptyParagraphFontReset(state, token);
  expect(tr.selection.eq(state.selection)).toBe(true); expect(tr.storedMarks).toEqual([]);
  expect(empty.createEmptyParagraphFontReset(state.apply(state.tr.setSelection(TextSelection.create(state.doc, 3))), token)).toBeNull();
  expect(empty.createEmptyParagraphFontReset(apply(state, tr), token)).toBeNull();
  expect(empty.createEmptyParagraphFontReset(EditorState.create({doc: state.doc, selection: new AllSelection(state.doc)}))).toBeNull();
  expect(empty.createEmptyParagraphFontReset(stateFor(doc(p(undefined, 'x'))))).toBeNull();
});

test('guards and block/input conversion never normalize malformed metadata or hide illegal nonempty source', () => {
  const state = stateFor(doc(p(font()), p(undefined, 'outside')));
  for (const value of invalid.filter(value => value !== null)) {
    const tr = state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, emptyParagraphFont: value});
    expect(() => empty.normalizeEmptyParagraphFontTransaction(tr)).toThrow();
    expect(state.applyTransaction(tr).transactions).toEqual([]); expect(undoDepth(state)).toBe(0);
  }
  for (const node of [p(font(), 'x'), p({family: 'Arial'})]) {
    expect(() => empty.normalizeEmptyParagraphFontTransaction(state.tr.replaceWith(0, 2, node).setMeta('paste', true))).toThrow('空段字体');
    expect(() => validateWordBlockTypeDocument(doc(node))).toThrow('空段字体');
    expect(() => createWordBlockTypeTransaction(stateFor(doc(node)), 2)).toThrow('空段字体');
  }
});

test('prospective text/node/depth/draft budgets reject without history changes', () => {
  let nested = p(); for (let i = 0; i < 33; i++) nested = schema.node('bulletList', null, schema.node('listItem', null, [p(), nested]));
  for (const document of [doc(p(font()), ...Array.from({length: 5000}, () => p(undefined, 'x'))),
    doc(p(font()), p(undefined, '中'.repeat(700000))), doc(p(font()), nested),
    doc(p(font()), schema.node('image', {src: 'data:image/png;base64,', alt: 'a'.repeat(32 * 1024 * 1024)}))]) {
    const state = stateFor(document); expect(() => empty.createEmptyParagraphFontReset(state)).toThrow(/上限/); expect(undoDepth(state)).toBe(0);
  }
});

const editors = [];
function editorFor(document, other = [], options = {}) {
  const errors = [], editor = new Editor({element: null, extensions: [...interactionExtensions({onError: message => errors.push(message), ...options}), empty.EmptyParagraphFont.configure({onError: message => errors.push(message)})], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  for (const plugin of other) editor.registerPlugin(plugin);
  // element:null uses Tiptap's minimal view, so compose precisely the extension
  // dispatcher that createView installs for a real editor view.
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); editors.push(editor); return {editor, errors};
}
const select = (editor, from, to = from) => editor.commands.setTextSelection({from, to});
const input = (editor, text) => {
  const {from, to} = editor.state.selection;
  const handled = editor.state.plugins.some(plugin => plugin.props.handleTextInput?.(editor.view, from, to, text));
  if (!handled) editor.view.dispatch(editor.state.tr.insertText(text, from, to));
};
const field = (editor, method) => {const extension = editor.extensionManager.extensions.find(value => value.name === 'emptyParagraphFont'); return getExtensionField(extension, method, {name: extension.name, options: extension.options, storage: extension.storage, editor});};
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

test('real full plugin typing observes only final normalized document and heading input rules never restore source font', () => {
  const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), observations = [];
  editor.on('update', () => observations.push(snapshot(editor.state.doc)));
  input(editor, '#'); input(editor, '#'); input(editor, ' ');
  expect(editor.state.doc.firstChild.type.name).toBe('heading'); expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe('## ');
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  expect(errors).toEqual([]); for (const value of observations) expect(() => empty.validateEmptyParagraphFontDocument(value)).not.toThrow();
});

test('real composing insertion and compositionend callbacks keep source font out of text and history', async () => {
  const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
  editor.view.composing = true; input(editor, '汉字'); editor.view.composing = false;
  for (const plugin of editor.state.plugins) plugin.props.handleDOMEvents?.compositionend?.(editor.view, {});
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull(); expect(editor.state.doc.firstChild.firstChild.marks).toEqual([]);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
});

test('real split command does not propagate metadata, and rejected edits preserve source and history', () => {
  const {editor} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
  expect(editor.commands.splitBlock()).toBe(true);
  expect(editor.state.doc.child(0).attrs.emptyParagraphFont).toBeNull(); expect(editor.state.doc.child(1).attrs.emptyParagraphFont).toBeNull();
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true);
  const reject = new Plugin({filterTransaction: tr => !tr.docChanged}), blocked = editorFor(doc(p(font())), [reject]).editor;
  input(blocked, 'x'); expect(blocked.state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font()); expect(undoDepth(blocked.state)).toBe(0);
});

class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; dataset = {}; listeners = new Map(); hidden = false; disabled = false; textContent = '';
  constructor(tag = 'div', attrs = {}) {
    this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = []; this.classList = {remove() {}};
    this.style = new Proxy({length: 0}, {get: (target, property) => property === 'cssText' ? this.attrs.style ?? '' : target[property] ?? '',
      set: (target, property, value) => {if (property === 'cssText') this.attrs.style = value; else target[property] = value; return true;}});
  }
  get firstChild() {return this.childNodes[0] ?? null;}
  get firstElementChild() {return this.childNodes.find(node => node.nodeType === 1) ?? null;}
  get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}
  getAttribute(name) {return this.attrs[name] ?? null;}
  hasAttribute(name) {return name in this.attrs;}
  setAttribute(name, value) {this.attrs[name] = String(value);}
  removeAttribute(name) {delete this.attrs[name];}
  appendChild(child) {child.parentNode = this; this.childNodes.push(child); return child;}
  replaceChildren(...nodes) {this.childNodes = []; nodes.forEach(node => this.appendChild(node));}
  matches(selector) {
    if (selector.includes(',')) return selector.split(',').some(part => this.matches(part));
    if (selector.startsWith('.')) return (this.attrs.class ?? '').split(' ').includes(selector.slice(1));
    const match = /^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(selector);
    return !!match && (!match[1] || this.tagName.toLowerCase() === match[1]) && (!match[2] || this.hasAttribute(match[2]));
  }
  querySelectorAll(selector) {return this.childNodes.flatMap(child => child.nodeType === 1 ? [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)] : []);}
  cloneNode() {const node = new Element(this.tagName, this.attrs); node.dataset = {...this.dataset}; for (const child of this.childNodes) node.appendChild(child.cloneNode(true)); return node;}
  addEventListener(type, handler) {this.listeners.set(type, [...this.listeners.get(type) ?? [], handler]);}
  emit(type, data = {}) {for (const handler of this.listeners.get(type) ?? []) handler({preventDefault() {}, ...data});}
}
const fakeDocument = {createElement: tag => new Element(tag), createElementNS: (_, tag) => new Element(tag), createDocumentFragment: () => new Element('fragment'),
  createTextNode: text => ({nodeType: 3, nodeValue: text, get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}, cloneNode() {return fakeDocument.createTextNode(text);}})};
const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: fakeDocument});
const parse = element => {const root = new Element(); root.appendChild(element); return ModelDOMParser.fromSchema(schema).parse(root);};

test('paragraph and heading DOM use only validated literal CSS, contain no fake text, and clipboard roundtrip is exact', () => {
  for (const node of [p(font()), h(font({italic: true, bold: false}), 3)]) {
    const element = serialize(node);
    expect(element.childNodes).toHaveLength(0);
    expect(JSON.parse(element.getAttribute(empty.EMPTY_PARAGRAPH_FONT_METADATA))).toEqual(node.attrs.emptyParagraphFont);
    expect(element.getAttribute('style')).toContain(empty.emptyParagraphFontStyle(node.attrs.emptyParagraphFont));
    expect(parse(element).firstChild.eq(node)).toBe(true);
  }
  const ordinary = serialize(p()); expect(ordinary.hasAttribute(empty.EMPTY_PARAGRAPH_FONT_METADATA)).toBe(false);
  const invalidNonempty = serialize(p(font(), 'text')); expect(invalidNonempty.hasAttribute(empty.EMPTY_PARAGRAPH_FONT_METADATA)).toBe(false);
  expect(invalidNonempty.getAttribute('style') ?? '').not.toContain('font-family');
});

test('quoted family escapes bypass unsafe upstream style merging while preserving other block declarations', () => {
  const value = font({family: 'a";color:red;/*\\b'}), node = p(value, '', {spaceBefore: 80, lineSpacing: 241, indentLeftTwips: 120});
  const style = serialize(node).getAttribute('style');
  expect(style).toContain('margin-top: 4pt'); expect(style).toContain('margin-left: 6pt'); expect(style).toContain(`line-height: ${241 / 240}`);
  expect(style.endsWith(empty.emptyParagraphFontStyle(value))).toBe(true);
  expect(style).toContain('font-family: "a\\";color:red;/*\\\\b"');
  expect(parse(serialize(node)).firstChild.eq(node)).toBe(true);
});

test('HTML parsing preserves malformed metadata as a rejecting sentinel instead of discarding it', () => {
  for (const value of [...invalid.map(value => JSON.stringify(value)), '{bad', ' '.repeat(4097)]) {
    const element = new Element('p', {[empty.EMPTY_PARAGRAPH_FONT_METADATA]: value}), parsed = parse(element);
    expect(parsed.firstChild.attrs.emptyParagraphFont).toBe('invalid-empty-paragraph-font');
    const state = stateFor(doc(p(font()))), tr = state.tr.replaceWith(0, 2, parsed.content).setMeta('paste', true);
    expect(() => empty.normalizeEmptyParagraphFontTransaction(tr)).toThrow('空段字体'); expect(undoDepth(state)).toBe(0);
  }
  for (const child of [new Element('br'), fakeDocument.createTextNode(' '), fakeDocument.createTextNode('text'), new Element('span')]) {
    const element = serialize(p(font())); element.appendChild(child);
    expect(parse(element).firstChild.attrs.emptyParagraphFont).toBe('invalid-empty-paragraph-font');
  }
});

test('raw HTML preflight rejects misplaced metadata on unsupported tags before the schema loses it', () => {
  const {editor} = editorFor(doc(p())), transform = field(editor, 'transformPastedHTML'), saved = globalThis.DOMParser;
  let element;
  globalThis.DOMParser = class {parseFromString() {return {querySelectorAll() {return [element];}};}};
  try {
    element = serialize(p(font())); expect(transform('<p>unchanged</p>')).toBe('<p>unchanged</p>');
    for (const tag of ['div', 'img', 'span', 'table', 'h4']) {
      element = new Element(tag, {[empty.EMPTY_PARAGRAPH_FONT_METADATA]: JSON.stringify(font())});
      expect(transform('raw malicious metadata')).toContain('data-empty-paragraph-font="invalid"');
    }
    element = serialize(p(font())); element.appendChild(new Element('br')); expect(transform('raw br')).toContain('="invalid"');
  } finally {globalThis.DOMParser = saved;}
});

test('immutable print snapshot shares baseline declarations and retains real empty line break without fake text', async () => {
  const saved = globalThis.document, host = new Element(), source = new Element('div', {contenteditable: 'true', 'aria-label': 'document'});
  const paragraph = serialize(p(font())); paragraph.appendChild(new Element('br', {class: 'ProseMirror-trailingBreak'})); source.appendChild(paragraph);
  globalThis.document = {...fakeDocument, getElementById: () => host, fonts: {ready: Promise.resolve()}};
  try {
    await preparePrintSnapshot(source);
    const cloned = host.firstChild.firstChild;
    expect(cloned.getAttribute('style')).toBe(paragraph.getAttribute('style'));
    expect(cloned.childNodes).toHaveLength(1); expect(cloned.firstChild.tagName).toBe('BR');
    paragraph.setAttribute('style', 'font-size: 1pt'); expect(cloned.getAttribute('style')).toContain('font-size: 24pt');
    expect(host.firstChild.hasAttribute('contenteditable')).toBe(false);
  } finally {globalThis.document = saved;}
});

test('simple table/list empty paragraphs keep metadata without turning it into text or container attributes', () => {
  const table = schema.node('table', null, schema.node('tableRow', null, schema.node('tableCell', null, p(font()))));
  const list = schema.node('bulletList', null, schema.node('listItem', null, p(font())));
  for (const node of [table, list]) {
    const document = doc(node); expect(() => empty.validateEmptyParagraphFontDocument(snapshot(document))).not.toThrow();
    expect(schema.nodeFromJSON(snapshot(document)).eq(document)).toBe(true);
    const blocks = []; document.descendants((child, pos) => {if (child.isTextblock) blocks.push(pos);});
    let state = stateFor(document); state = state.apply(state.tr.setSelection(TextSelection.create(document, blocks[0] + 1)));
    state = apply(state, state.tr.insertText('x')); expect(snapshot(state.doc)).not.toEqual(snapshot(document));
    expect(state.doc.textContent).toBe('x'); expect(JSON.stringify(snapshot(state.doc))).not.toContain('emptyParagraphFont');
  }
});

function controlsHarness(run, reject = false) {
  const saved = globalThis.document, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id);};
  globalThis.document = {getElementById: element};
  let state = stateFor(doc(p(font()), p(font({halfPoints: 32})))), locked = false, dispatches = 0, controls;
  let editor = {isEditable: true, get state() {return state;}, view: {composing: false, focus() {}, dispatch(tr) {
    dispatches++; if (!(reject && tr.docChanged)) state = apply(state, tr); controls.update();
  }}};
  let synchronize = () => {};
  controls = new WordEmptyParagraphFontControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message), syncSelection: () => synchronize()}); controls.update();
  try {run({element, controls, notices, get state() {return state;}, get dispatches() {return dispatches;}, get editor() {return editor;},
    lock(value) {locked = value; controls.update();}, move(notify = true) {state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 3))); if (notify) controls.update();},
    synchronize(fn) {synchronize = fn;}, replaceOwner() {editor = {...editor}; controls.update();},
    undo() {undo(state, tr => {state = apply(state, tr);}); controls.update();}});
  } finally {globalThis.document = saved;}
}

test('visible imported-empty summary separates typing controls and reset is one undoable document change', () => controlsHarness(harness => {
  const reset = harness.element('edit-empty-paragraph-font-reset'), before = harness.state.doc;
  expect(harness.element('empty-paragraph-font-controls').hidden).toBe(false);
  expect(harness.element('edit-empty-paragraph-font-state').textContent).toContain('Liberation Serif · 24 pt · 粗体');
  expect(harness.element('edit-empty-paragraph-font-state').textContent).toContain('首次输入或粘贴内容后结束');
  expect(harness.element('edit-empty-paragraph-font-state').textContent).toContain('后续文字');
  reset.emit('click'); expect(harness.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  expect(harness.state.doc.child(1).eq(before.child(1))).toBe(true); expect(undoDepth(harness.state)).toBe(1); expect(harness.notices).toHaveLength(1);
  harness.undo(); expect(harness.state.doc.eq(before)).toBe(true);
}));

for (const reason of ['lock', 'readonly', 'composition', 'staleSelection', 'owner', 'pointerCancel']) test(`reset ${reason} cannot apply to a stale or locked paragraph`, () => controlsHarness(harness => {
  const button = harness.element('edit-empty-paragraph-font-reset'), before = harness.state.doc;
  button.emit('pointerdown');
  if (reason === 'lock') harness.lock(true);
  if (reason === 'readonly') harness.editor.isEditable = false;
  if (reason === 'composition') harness.editor.view.composing = true;
  if (reason === 'staleSelection') harness.move();
  if (reason === 'owner') harness.replaceOwner();
  if (reason === 'pointerCancel') {button.emit('pointercancel'); harness.move(false);}
  button.emit('click'); expect(harness.state.doc.eq(before)).toBe(true); expect(harness.notices).toEqual([]); expect(undoDepth(harness.state)).toBe(0);
}));

test('reset pointerdown synchronizes pending DOM selection and blocked candidate gives no false success notice', () => {
  controlsHarness(harness => {
    harness.synchronize(() => harness.move()); const before = harness.state.doc;
    harness.element('edit-empty-paragraph-font-reset').emit('pointerdown'); harness.element('edit-empty-paragraph-font-reset').emit('click');
    expect(harness.state.doc.firstChild.eq(before.firstChild)).toBe(true); expect(harness.state.doc.child(1).attrs.emptyParagraphFont).toBeNull();
  });
  controlsHarness(harness => {
    const before = harness.state.doc; harness.element('edit-empty-paragraph-font-reset').emit('click');
    expect(harness.state.doc.eq(before)).toBe(true); expect(harness.notices).toEqual([]); expect(undoDepth(harness.state)).toBe(0);
  }, true);
});

for (const marker of ['#', '##']) for (const mode of ['separate-hash-space', 'single-trigger', 'applyInputRules', 'compositionend']) test(`full input-rule chain ${marker} ${mode} clears imported baseline without stale transaction or resurrection`, async () => {
  const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
  const observed = []; editor.on('update', () => observed.push(snapshot(editor.state.doc)));
  if (mode === 'separate-hash-space') {input(editor, marker); input(editor, ' ');}
  if (mode === 'single-trigger') input(editor, `${marker} `);
  if (mode === 'applyInputRules') {
    editor.view.dispatch(editor.state.tr.insertText(`${marker} `).setMeta('applyInputRules', {from: 1, text: `${marker} `}));
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  if (mode === 'compositionend') {
    editor.view.composing = true; input(editor, `${marker} `); editor.view.composing = false;
    for (const plugin of editor.state.plugins) plugin.props.handleDOMEvents?.compositionend?.(editor.view, {});
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  expect(editor.state.doc.firstChild.type.name).toBe('heading');
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull(); expect(editor.state.doc.firstChild.content.size).toBe(0);
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
  expect(editor.state.doc.firstChild.textContent).toBe(`${marker} `); expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  expect(errors).toEqual([]); for (const value of observed) expect(() => empty.validateEmptyParagraphFontDocument(value)).not.toThrow();
  // Normal history can deliberately restore the original document; no ordinary
  // delete-to-empty or input-rule undo is allowed to recover its source font.
  for (let count = 0; count < 10 && editor.commands.undo(); count++) {}
  expect(editor.state.doc.eq(original)).toBe(true);
});

for (const mode of ['code', 'locked', 'readonly', 'paste']) test(`first-insertion adapter preserves existing ${mode} input-rule boundary`, () => {
  const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside')), [], {locked: () => mode === 'locked'}), original = editor.state.doc;
  if (mode === 'code') editor.view.dispatch(editor.state.tr.setStoredMarks([editor.schema.marks.code.create()]));
  if (mode === 'readonly') {
    editor.setEditable(false);
    expect(editor.state.plugins.some(plugin => plugin.props.handleTextInput?.(editor.view, 1, 1, '## '))).toBe(false);
    expect(editor.state.doc.eq(original)).toBe(true); expect(undoDepth(editor.state)).toBe(0); return;
  }
  if (mode === 'paste') editor.view.dispatch(editor.state.tr.insertText('## ').setMeta('paste', true).setMeta('uiEvent', 'paste'));
  else input(editor, '## ');
  expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.firstChild.textContent).toBe('## ');
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull(); expect(editor.commands.undoInputRule()).toBe(false);
  if (mode === 'code') expect(editor.state.doc.firstChild.firstChild.marks.map(mark => mark.type.name)).toEqual(['code']);
  expect(errors).toEqual([]);
});

test('one-event trigger Backspace restores literal text and later typing has separate history', () => {
  for (const backspace of [false, true]) {
    const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
    input(editor, '## '); const heading = editor.state.doc;
    if (backspace) {
      const event = {key: 'Backspace', keyCode: 8, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false};
      expect(editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event))).toBe(true);
      expect(editor.state.doc.firstChild.textContent).toBe('## '); expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
    } else {
      input(editor, 'later'); expect(editor.commands.undoInputRule()).toBe(false);
      expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(heading)).toBe(true);
      expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe('## ');
      expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
    }
    for (let count = 0; count < 10 && editor.commands.undo(); count++) {}
    expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
  }
});

test('first-insertion adapter preserves stored marks for ordinary text and never duplicates rejected input', () => {
  const {editor} = editorFor(doc(p(font()), p(undefined, 'outside')));
  editor.view.dispatch(editor.state.tr.setStoredMarks([editor.schema.marks.italic.create(), editor.schema.marks.fontSize.create({halfPoints: 21})]));
  input(editor, 'ordinary'); expect(editor.state.doc.firstChild.textContent).toBe('ordinary');
  expect(editor.state.doc.firstChild.firstChild.marks.map(mark => mark.toJSON())).toEqual([{type: 'fontSize', attrs: {halfPoints: 21}}, {type: 'italic'}]);
  const blocked = editorFor(doc(p(font(), '', {indentLeftTwips: 120}), p(undefined, 'outside'))).editor, original = blocked.state.doc;
  input(blocked, '\t'); expect(blocked.state.doc.eq(original)).toBe(true); expect(undoDepth(blocked.state)).toBe(0);
  const denied = editorFor(doc(p(font())), [new Plugin({filterTransaction: tr => !tr.docChanged})]).editor;
  input(denied, '## '); expect(denied.state.doc.firstChild.content.size).toBe(0); expect(denied.state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font());
  expect(undoDepth(denied.state)).toBe(0); expect(denied.commands.undoInputRule()).toBe(false);
});

test('raw handlePaste rejects open empty malformed slices before clipboard fitting can discard evidence', () => {
  const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
  for (const value of [{family: 'Arial'}, 'invalid-empty-paragraph-font']) {
    const slice = new Slice(Fragment.from(editor.schema.nodeFromJSON(p(value).toJSON())), 1, 1);
    expect(editor.state.plugins.some(plugin => plugin.props.handlePaste?.(editor.view, {}, slice))).toBe(true);
    expect(editor.state.doc.eq(original)).toBe(true); expect(undoDepth(editor.state)).toBe(0);
  }
  const canonical = new Slice(Fragment.from(editor.schema.nodeFromJSON(p(font({halfPoints: 30})).toJSON())), 0, 0);
  expect(editor.state.plugins.some(plugin => plugin.props.handlePaste?.(editor.view, {}, canonical))).toBe(false);
  editor.view.dispatch(editor.state.tr.replaceSelection(canonical).setMeta('paste', true));
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font({halfPoints: 30}));
  expect(errors).toHaveLength(2);
});

test('automatic/fixed table structural edits preserve surviving empty fonts at boundary and middle cells with exact undo/redo', () => {
  const actions = ['addColumnBefore', 'addColumnAfter', 'deleteColumn', 'addRowBefore', 'addRowAfter', 'deleteRow'];
  const values = document => {const result = []; document.descendants(node => {if (node.isTextblock && node.attrs.emptyParagraphFont) result.push(node.attrs.emptyParagraphFont.halfPoints);}); return result;};
  for (const columnWidthsTwips of [null, [1200, 1200, 1200]]) for (const index of [0, 4, 8]) for (const action of actions) {
    const table = schema.node('table', {columnWidthsTwips}, [0, 1, 2].map(row => schema.node('tableRow', null,
      [0, 1, 2].map(column => schema.node('tableCell', null, p(font({halfPoints: 22 + row * 3 + column})))))));
    const {editor, errors} = editorFor(doc(table, p(undefined, 'outside'))), positions = [];
    editor.state.doc.descendants((node, pos) => {if (node.isTextblock) positions.push(pos);}); select(editor, positions[index] + 1);
    const before = editor.state.doc, transaction = createTableStructureTransaction(editor.state, action), candidate = transaction.doc;
    const expected = Array.from({length: 9}, (_, cell) => cell).filter(cell => action === 'deleteRow' ? Math.floor(cell / 3) !== Math.floor(index / 3)
      : action === 'deleteColumn' ? cell % 3 !== index % 3 : true).map(cell => 22 + cell);
    // Additions introduce default paragraphs, never neighboring font copies.
    expect(values(candidate)).toEqual(expected);
    editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));
    expect(editor.state.doc.eq(candidate)).toBe(true); expect(values(editor.state.doc)).toEqual(expected);
    expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
    expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(candidate)).toBe(true); expect(errors).toEqual([]);
  }
});

test('real bullet/ordered list wrapping and lifting preserve an untouched imported body empty font', () => {
  for (const command of ['toggleBulletList', 'toggleOrderedList']) {
    const {editor, errors} = editorFor(doc(p(font()), p(undefined, 'outside'))), original = editor.state.doc;
    expect(editor.commands[command]()).toBe(true);
    const block = editor.state.doc.firstChild.firstChild.firstChild;
    expect(block.type.name).toBe('paragraph'); expect(block.content.size).toBe(0); expect(block.attrs.emptyParagraphFont).toEqual(font());
    expect(editor.commands[command]()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
  }
});
