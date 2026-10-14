// Actual ProseMirror/Tiptap schema, transactions, history and DOM transports.
// These checks do not establish browser layout, OS clipboard or native printing.
import {test, expect} from '@playwright/test';
import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {EditorState, AllSelection, TextSelection} from '@tiptap/pm/state';
import {DOMParser as ModelDOMParser, DOMSerializer, Fragment, Slice} from '@tiptap/pm/model';
import {history, closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
import {interactionExtensions, schema as oldSchema, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
const metadata = await loadTsModule('./ui/word-font-family-metadata.ts');
const styles = await loadTsModule('./ui/word-style-profile.ts');
const fonts = await loadTsModule('./ui/word-font-family.ts');
const breaks = await loadTsModule('./ui/word-page-break.ts');
const pagination = await loadTsModule('./ui/word-paragraph-pagination.ts');
const list = await loadTsModule('./ui/word-list-geometry.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const entries = () => [{name: 'Arial Black', family: 'swiss'}, {name: 'Unused Serif', family: 'roman'}];
const raw = (value = entries(), attrs = {}) => ({type: 'doc', attrs: {...attrs, fontFamilyMetadata: value}, content: [{type: 'paragraph'}]});
const extensions = (onError = () => {}) => [...interactionExtensions({onError}).filter(extension => extension.name !== 'pageBreak')
  .map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {
    return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);
  }}) : extension), metadata.WordFontFamilyMetadata.configure({onError}), breaks.PageBreak,
  breaks.PageBreakParagraph.configure({onError}), pagination.ParagraphPagination.configure({onError}),
  list.PreservedListGeometry.configure({onError}), empty.EmptyParagraphFont.configure({onError})];
const schema = getSchema(extensions());
const p = (text = '', name = undefined) => schema.node('paragraph', null,
  text ? schema.text(text, name ? [schema.mark('fontFamily', {name})] : []) : null);
const doc = (content = [p('Original', 'Arial Black')], value = entries(), attrs = {}) => schema.node('doc',
  {...attrs, ...(value === undefined ? {} : {fontFamilyMetadata: value})}, content);
const snapshot = node => pagination.omitDefaultParagraphPagination(list.omitDefaultListGeometry(baseSnapshot(node)));
const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), metadata.fontFamilyMetadataGuard(message => errors.push(message))]});
const badNames = ['', ' a', 'a ', '\ta', 'a\n', 'x'.repeat(32), '😀'.repeat(16), '\ud800', '\udc00', 'a\ud800b',
  '\x00', '\x1f', '\x7f', '\x80', '\x9f', '\ufffe', '\uffff', '\u00a0edge', 'edge\u3000'];
const invalid = [null, undefined, false, true, 0, 'roman', {}, [], [null], [false], [1], ['Arial'], [[]], [{}],
  [{name: 'Arial'}], [{family: 'swiss'}], [{name: 'Arial', family: 'swiss', other: true}],
  ...[null, undefined, true, 1, 'Roman', 'SWISS', 'serif', 'modern', '', {}, []].map(family => [{name: 'Arial', family}]),
  ...[...badNames, null, undefined, 1, {}, []].map(name => [{name, family: 'roman'}])];

test('exact bounded root entries accept both families and preserve literal non-ASCII names and order', () => {
  const value = [{name: '字'.repeat(31), family: 'roman'}, {name: '😀'.repeat(15) + 'a', family: 'swiss'},
    {name: 'É', family: 'roman'}, {name: 'é', family: 'swiss'}, {name: 'e\u0301', family: 'roman'},
    {name: 'Ａ', family: 'swiss'}, {name: 'A', family: 'roman'}, {name: 'İ', family: 'swiss'},
    {name: 'i', family: 'roman'}, {name: 'serif', family: 'swiss'}, {name: 'a";color:red;/*\\b', family: 'roman'}];
  expect(metadata.isFontFamilyMetadata(value)).toBe(true);
  expect(() => styles.documentStyleProfile(raw(value))).not.toThrow();
  expect(snapshot(doc([p()], value)).attrs.fontFamilyMetadata).toEqual(value);
  expect(JSON.stringify(snapshot(doc([p()], value)).attrs.fontFamilyMetadata)).toBe(JSON.stringify(value));
});

test('256 distinct metadata entries and 31 UTF16 units are accepted while 257 and 32 are rejected', () => {
  const value = Array.from({length: 256}, (_, index) => ({name: `Unused ${index}`, family: index % 2 ? 'swiss' : 'roman'}));
  expect(metadata.isFontFamilyMetadata(value)).toBe(true);
  expect(snapshot(doc([p()], value)).attrs.fontFamilyMetadata).toEqual(value);
  expect(() => styles.documentStyleProfile(raw([...value, {name: 'Overflow', family: 'roman'}]))).toThrow('字体类别');
  for (const name of ['a'.repeat(31), '😀'.repeat(15) + 'a']) expect(metadata.isFontFamilyMetadata([{name, family: 'swiss'}])).toBe(true);
  for (const name of ['a'.repeat(32), '😀'.repeat(16)]) expect(metadata.isFontFamilyMetadata([{name, family: 'swiss'}])).toBe(false);
});

test('duplicate exact names and ASCII aliases reject even when the categories agree and at the array limit', () => {
  for (const family of ['roman', 'swiss']) for (const name of ['Arial Black', 'ARIAL BLACK', 'arial black', 'aRiaL BlAcK']) {
    const value = [{name: 'Arial Black', family: 'swiss'}, {name, family}];
    expect(metadata.isFontFamilyMetadata(value)).toBe(false);
    expect(() => styles.documentStyleProfile(raw(value))).toThrow('字体类别');
  }
  const value = Array.from({length: 255}, (_, index) => ({name: `Font ${index}`, family: 'roman'}));
  expect(metadata.isFontFamilyMetadata([...value, {name: 'FONT 0', family: 'roman'}])).toBe(false);
});

test('external null empty wrong types incomplete extra fields and invalid literal names are never canonicalized', () => {
  for (const value of invalid) {
    expect(metadata.isFontFamilyMetadata(value), JSON.stringify(value)).toBe(false);
    expect(() => styles.documentStyleProfile({type: 'doc', attrs: {fontFamilyMetadata: value}, content: [{type: 'paragraph'}]})).toThrow('字体类别');
    expect(() => metadata.prepareFontFamilyMetadataDocument({type: 'doc', attrs: {fontFamilyMetadata: value}})).toThrow();
    expect(() => metadata.copyFontFamilyMetadata(value)).toThrow();
  }
  expect(metadata.isFontFamilyMetadata(new Array(1))).toBe(false);
  expect(metadata.isFontFamilyMetadata([Object.create({name: 'Arial', family: 'swiss'})])).toBe(false);
});

test('root-only metadata rejects nested doc block text atom and mark attributes before schema erasure', () => {
  for (const type of ['doc', 'paragraph', 'heading', 'text', 'hardBreak', 'image', 'pageBreak', 'table', 'listItem']) {
    for (const value of [entries(), null, []]) expect(() => metadata.validateFontFamilyMetadataDocument({type: 'doc',
      content: [{type, attrs: {fontFamilyMetadata: value}}]})).toThrow('字体类别');
  }
  for (const node of [{type: 'doc'}, {type: 'paragraph'}, {type: 'text', text: 'x'}]) {
    node.marks = [{type: 'bold', attrs: {fontFamilyMetadata: entries()}}];
    const input = node.type === 'doc' ? node : {type: 'doc', content: [node]};
    expect(() => metadata.validateFontFamilyMetadataDocument(input)).toThrow('字体类别');
  }
  expect(() => metadata.validateFontFamilyMetadataDocument({type: 'paragraph'})).toThrow('doc');
});

test('root and structure validation keep the existing total node and depth bounds', () => {
  const boundary = raw(); boundary.content = Array.from({length: 9999}, () => ({type: 'paragraph'}));
  expect(() => metadata.validateFontFamilyMetadataDocument(boundary)).not.toThrow();
  boundary.content.push({type: 'paragraph'}); expect(() => metadata.validateFontFamilyMetadataDocument(boundary)).toThrow('结构');
  const depth = raw(); let current = depth;
  for (let index = 0; index < 32; index++) {current.content = [{type: 'paragraph'}]; current = current.content[0];}
  expect(() => metadata.validateFontFamilyMetadataDocument(depth)).not.toThrow();
  current.content = [{type: 'paragraph'}]; expect(() => metadata.validateFontFamilyMetadataDocument(depth)).toThrow('结构');
});

test('internal null omission keeps exact historical JSON bytes for both profiles and never erases a nested null', () => {
  for (const styleProfile of [null, 'standard-v2']) {
    const original = oldSchema.node('doc', {styleProfile}, [oldSchema.node('paragraph', null, oldSchema.text('Historical'))]);
    const current = schema.nodeFromJSON(original.toJSON());
    expect(current.attrs.fontFamilyMetadata).toBeNull();
    expect(JSON.stringify(snapshot(current))).toBe(JSON.stringify(baseSnapshot(original)));
  }
  expect(metadata.omitDefaultFontFamilyMetadata({type: 'doc', attrs: {fontFamilyMetadata: null}, content: [{type: 'paragraph'}]}))
    .toEqual({type: 'doc', content: [{type: 'paragraph'}]});
  const nested = {type: 'doc', content: [{type: 'paragraph', attrs: {fontFamilyMetadata: null}}]};
  expect(metadata.omitDefaultFontFamilyMetadata(nested)).toBe(nested);
  expect(() => styles.documentStyleProfile(nested)).toThrow();
});

test('metadata is independent of active marks code headings empty baseline fonts and document compatibility', () => {
  for (const styleProfile of [undefined, 'standard-v2']) for (const split of [undefined, true]) {
    const attrs = {...(styleProfile ? {styleProfile} : {}), ...(split ? {splitPageBreakAndParagraphMark: split} : {})};
    const nodes = [p(), p('No explicit font'), p('Unlisted font', 'Unlisted literal'),
      schema.node('heading', {level: 2}, schema.text('Heading')), schema.node('paragraph', null, schema.text('Code', [schema.mark('code')])),
      schema.node('paragraph', {emptyParagraphFont: {family: 'Other baseline', halfPoints: 20, bold: false, italic: false}}), schema.node('pageBreak')];
    const saved = snapshot(doc(nodes, entries(), attrs));
    expect(saved.attrs.fontFamilyMetadata).toEqual(entries());
    expect(saved.content[2].content[0].marks[0].attrs.name).toBe('Unlisted literal');
    expect(saved.content[5].attrs.emptyParagraphFont.family).toBe('Other baseline');
    expect(schema.nodeFromJSON(saved).eq(doc(nodes, entries(), attrs))).toBe(true);
  }
});

test('incoming metadata is copied and frozen before schema loading without freezing caller-owned values', () => {
  const original = raw(entries(), {styleProfile: 'standard-v2'}), prepared = metadata.prepareFontFamilyMetadataDocument(original);
  expect(prepared.attrs.fontFamilyMetadata).not.toBe(original.attrs.fontFamilyMetadata);
  expect(prepared.attrs.fontFamilyMetadata[0]).not.toBe(original.attrs.fontFamilyMetadata[0]);
  expect(Object.isFrozen(original.attrs.fontFamilyMetadata)).toBe(false);
  expect(Object.isFrozen(prepared.attrs.fontFamilyMetadata)).toBe(true);
  expect(prepared.attrs.fontFamilyMetadata.every(Object.isFrozen)).toBe(true);
  original.attrs.fontFamilyMetadata[0].name = 'Caller mutation'; original.attrs.fontFamilyMetadata.push({name: 'Extra', family: 'roman'});
  expect(prepared.attrs.fontFamilyMetadata).toEqual(entries());
  expect(() => {prepared.attrs.fontFamilyMetadata[0].family = 'roman';}).toThrow();
  expect(snapshot(schema.nodeFromJSON(prepared)).attrs.fontFamilyMetadata).toEqual(entries());
});

test('standard-profile adoption and repeated snapshots are detached deep copies with stable order', () => {
  const original = raw(), upgraded = styles.withStandardWordStyle(original), node = schema.nodeFromJSON(upgraded);
  expect(upgraded.attrs.styleProfile).toBe('standard-v2');
  original.attrs.fontFamilyMetadata[0].family = 'roman'; expect(node.attrs.fontFamilyMetadata).toEqual(entries());
  const first = snapshot(node), second = snapshot(node);
  expect(first.attrs.fontFamilyMetadata).not.toBe(second.attrs.fontFamilyMetadata);
  expect(first.attrs.fontFamilyMetadata[0]).not.toBe(node.attrs.fontFamilyMetadata[0]);
  first.attrs.fontFamilyMetadata[0].name = 'Snapshot mutation'; first.attrs.fontFamilyMetadata.pop();
  expect(second.attrs.fontFamilyMetadata).toEqual(entries()); expect(node.attrs.fontFamilyMetadata).toEqual(entries());
  expect(snapshot(node).attrs.fontFamilyMetadata).toEqual(entries());
});

test('typing changing/resetting all run fonts and formatting never prunes dormant root entries', () => {
  const original = doc(), state = stateFor(original, new AllSelection(original));
  let edited = state.apply(closeHistory(fonts.createFontFamilyTransaction(state, 'Different literal')));
  expect(snapshot(edited.doc).attrs.fontFamilyMetadata).toEqual(entries());
  edited = edited.apply(fonts.createFontFamilyTransaction(edited, null));
  edited = edited.apply(edited.tr.addMark(1, edited.doc.content.size - 1, schema.mark('bold')));
  edited = edited.apply(edited.tr.insertText('Typed', 1));
  expect(snapshot(edited.doc).attrs.fontFamilyMetadata).toEqual(entries());
  expect(JSON.stringify(snapshot(edited.doc))).not.toContain('"type":"fontFamily"');
});

test('delete all text and undo redo retain unused metadata without an active font dependency', () => {
  const original = doc([p('Only active run', 'Arial Black'), p('Other')]);
  let state = stateFor(original, new AllSelection(original)); state = state.apply(closeHistory(state.tr.deleteSelection()));
  const cleared = state.doc, saved = snapshot(cleared);
  expect(saved).toEqual({type: 'doc', attrs: {fontFamilyMetadata: entries()}, content: [{type: 'paragraph', attrs: {textAlign: null}}]});
  expect(undoDepth(state)).toBe(1);
  expect(undo(state, transaction => {state = state.apply(transaction);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, transaction => {state = state.apply(transaction);})).toBe(true); expect(state.doc.eq(cleared)).toBe(true);
});

test('malformed root transactions reject atomically before history state and observers can change', () => {
  const errors = [], original = doc(), state = stateFor(original, undefined, errors);
  for (const value of invalid.filter(value => value !== null)) {
    const result = state.applyTransaction(state.tr.setDocAttribute('fontFamilyMetadata', value));
    expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undoDepth(result.state)).toBe(0);
  }
  expect(errors.length).toBe(invalid.filter(value => value !== null).length);
  expect(state.doc.eq(original)).toBe(true);
  // Null is the one legitimate internal unset value, including its inverse
  // history step when adding metadata to an initially unannotated document.
  let cleared = state.apply(closeHistory(state.tr.setDocAttribute('fontFamilyMetadata', null)));
  expect(snapshot(cleared.doc).attrs?.fontFamilyMetadata).toBeUndefined();
  expect(undo(cleared, transaction => {cleared = cleared.apply(transaction);})).toBe(true);
  expect(cleared.doc.eq(original)).toBe(true);
  expect(redo(cleared, transaction => {cleared = cleared.apply(transaction);})).toBe(true);
  expect(cleared.doc.attrs.fontFamilyMetadata).toBeNull();
  const absent = doc([p('Unannotated')], null); let added = stateFor(absent);
  added = added.apply(closeHistory(added.tr.setDocAttribute('fontFamilyMetadata', entries())));
  expect(snapshot(added.doc).attrs.fontFamilyMetadata).toEqual(entries());
  expect(undo(added, transaction => {added = added.apply(transaction);})).toBe(true);
  expect(added.doc.eq(absent)).toBe(true);
  expect(redo(added, transaction => {added = added.apply(transaction);})).toBe(true);
  expect(snapshot(added.doc).attrs.fontFamilyMetadata).toEqual(entries());
});

test('paste slices from documents with differing metadata keep destination root present absent and unused entries', () => {
  for (const sourceValue of [null, entries(), [{name: 'Foreign', family: 'roman'}]]) {
    for (const targetValue of [null, entries(), [{name: 'Destination only', family: 'swiss'}]]) {
      const source = doc([p('Pasted', 'Foreign')], sourceValue), target = doc([p('Replace')], targetValue);
      let state = stateFor(target, new AllSelection(target));
      state = state.apply(closeHistory(state.tr.replaceSelection(new Slice(source.content, 0, 0)).setMeta('paste', true)));
      expect(snapshot(state.doc).attrs?.fontFamilyMetadata).toEqual(targetValue ?? undefined);
      expect(state.doc.textContent).toBe('Pasted');
      expect(undo(state, transaction => {state = state.apply(transaction);})).toBe(true); expect(state.doc.eq(target)).toBe(true);
    }
  }
});

test('paste-tagged transactions cannot inject clear replace or reorder destination root metadata', () => {
  for (const targetValue of [null, entries()]) for (const marker of ['paste', 'uiEvent']) {
    const errors = [], state = stateFor(doc([p('Keep')], targetValue), undefined, errors);
    for (const value of [null, entries(), [...entries()].reverse(), [{name: 'Foreign', family: 'roman'}]]) {
      if (JSON.stringify(value) === JSON.stringify(targetValue)) continue;
      const candidate = state.tr.insertText('Rejected', 1).setDocAttribute('fontFamilyMetadata', value).setMeta(marker, marker === 'paste' ? true : 'paste');
      expect(state.applyTransaction(candidate).transactions).toEqual([]);
    }
    expect(errors.every(error => error.includes('粘贴不能更改'))).toBe(true); expect(state.doc.textContent).toBe('Keep');
  }
});

const editors = [];
function editorFor(document) {
  const errors = [], editor = new Editor({element: null, extensions: extensions(message => errors.push(message)),
    content: metadata.prepareFontFamilyMetadataDocument(snapshot(document))});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); editors.push(editor); return {editor, errors};
}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

test('real full plugin chain edit delete-all undo redo and draft/export observations preserve exact metadata', () => {
  const {editor, errors} = editorFor(doc([p('Original', 'Arial Black'), p('Last')], entries(), {styleProfile: 'standard-v2'}));
  const original = editor.state.doc, observed = [];
  editor.on('update', () => observed.push(snapshot(editor.state.doc)));
  editor.commands.selectAll(); expect(editor.commands.deleteSelection()).toBe(true);
  expect(snapshot(editor.state.doc).attrs.fontFamilyMetadata).toEqual(entries()); expect(editor.state.doc.textContent).toBe('');
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true);
  expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.textContent).toBe('');
  expect(editor.commands.insertContent({type: 'text', text: 'New text'})).toBe(true);
  expect(observed.length).toBeGreaterThanOrEqual(4);
  for (const payload of observed) {
    expect(payload.attrs.fontFamilyMetadata).toEqual(entries());
    const reopened = schema.nodeFromJSON(JSON.parse(JSON.stringify(payload)));
    expect(snapshot(reopened)).toEqual(payload);
  }
  expect(errors).toEqual([]);
});

test('real full plugin slice paste and rejected root paste do not leak a transient draft or history entry', () => {
  const {editor, errors} = editorFor(doc()), original = editor.state.doc, observed = [];
  editor.on('update', () => observed.push(snapshot(editor.state.doc)));
  const transaction = editor.state.tr.insertText('Bad', 1).setDocAttribute('fontFamilyMetadata', [{name: 'Foreign', family: 'roman'}]).setMeta('paste', true);
  editor.view.dispatch(transaction); expect(editor.state.doc.eq(original)).toBe(true); expect(observed).toEqual([]);
  expect(editor.commands.undo()).toBe(false); expect(errors).toHaveLength(1);
  editor.commands.selectAll();
  editor.view.dispatch(closeHistory(editor.state.tr.replaceSelection(new Slice(Fragment.from(p('Pasted', 'Foreign')), 0, 0)).setMeta('paste', true)));
  expect(observed).toHaveLength(1); expect(observed[0].attrs.fontFamilyMetadata).toEqual(entries());
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true);
});

class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; dataset = {};
  constructor(tag = 'div', attrs = {}) {
    this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = []; this.classList = {remove() {}};
    this.style = new Proxy({length: 0}, {get: (target, property) => property === 'cssText' ? this.attrs.style ?? '' : target[property] ?? '',
      set: (target, property, value) => {if (property === 'cssText') this.attrs.style = String(value); else target[property] = value; return true;}});
  }
  get firstChild() {return this.childNodes[0] ?? null;}
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
  closest(selector) {return this.matches(selector) ? this : this.parentNode?.closest?.(selector) ?? null;}
  querySelectorAll(selector) {return this.childNodes.flatMap(child => child.nodeType === 1 ? [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)] : []);}
  cloneNode() {const node = new Element(this.tagName, this.attrs); node.dataset = {...this.dataset}; for (const child of this.childNodes) node.appendChild(child.cloneNode(true)); return node;}
}
const fakeDocument = {createElement: tag => new Element(tag), createElementNS: (_, tag) => new Element(tag), createDocumentFragment: () => new Element('fragment'),
  createTextNode: text => ({nodeType: 3, nodeValue: text, get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}, cloneNode() {return fakeDocument.createTextNode(text);}})};
const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: fakeDocument});

test('actual HTML parser ignores valid malformed and adversarial root metadata without changing pasted text fonts', () => {
  for (const value of [JSON.stringify(entries()), 'null', '[]', '{bad', JSON.stringify([{name: 'Foreign', family: 'roman'}])]) {
    const wrapper = new Element('div', {[metadata.FONT_FAMILY_METADATA]: value}); wrapper.appendChild(serialize(p('Literal text')));
    const parsed = ModelDOMParser.fromSchema(schema).parse(wrapper);
    expect(parsed.attrs.fontFamilyMetadata).toBeNull(); expect(parsed.textContent).toBe('Literal text');
    const pasted = ModelDOMParser.fromSchema(schema).parseSlice(wrapper);
    for (const targetValue of [null, entries()]) {
      const target = doc([p('Replace')], targetValue); let state = stateFor(target, new AllSelection(target));
      state = state.apply(state.tr.replaceSelection(pasted).setMeta('paste', true));
      expect(snapshot(state.doc).attrs?.fontFamilyMetadata).toEqual(targetValue ?? undefined);
      expect(state.doc.textContent).toBe('Literal text');
    }
  }
});

test('inert root DOM metadata and literal CSS never substitute families infer weight or rename fonts', () => {
  const value = [{name: 'a";color:red;/*\\b', family: 'swiss'}];
  expect(metadata.fontFamilyMetadataDocumentDOM({fontFamilyMetadata: value})).toEqual({[metadata.FONT_FAMILY_METADATA]: JSON.stringify(value)});
  expect(metadata.fontFamilyMetadataDocumentDOM({fontFamilyMetadata: null})).toEqual({});
  const node = p('Literal', 'Arial Black'), element = serialize(node);
  expect(element.firstChild.getAttribute('style')).toBe('font-family: "Arial Black"');
  expect(element.firstChild.getAttribute('data-font-family')).toBe('Arial Black');
  expect(element.firstChild.hasAttribute('font-weight')).toBe(false);
  expect(fonts.fontFamilyCss('Arial Black')).toBe('"Arial Black"');
  expect(readFileSync('ui/style.css', 'utf8')).not.toContain(metadata.FONT_FAMILY_METADATA);
});

test('print clone captures inert metadata before font awaits without changing CSS fallback', async () => {
  const previous = globalThis.document, host = new Element(), source = new Element('div', {contenteditable: 'true',
    ...metadata.fontFamilyMetadataDocumentDOM({fontFamilyMetadata: entries()})});
  source.appendChild(serialize(p('Printed', 'Arial Black'))); let ready;
  globalThis.document = {...fakeDocument, getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const pending = preparePrintSnapshot(source); source.setAttribute(metadata.FONT_FAMILY_METADATA, 'changed'); ready(); await pending;
    expect(JSON.parse(host.firstChild.getAttribute(metadata.FONT_FAMILY_METADATA))).toEqual(entries());
    expect(host.firstChild.hasAttribute('contenteditable')).toBe(false);
    expect(host.firstChild.firstChild.firstChild.getAttribute('style')).toBe('font-family: "Arial Black"');
  } finally {globalThis.document = previous;}
});

test('all load draft print export paths use validated snapshot and disclose family substitution limitation', () => {
  expect(metadata.fontFamilyMetadataPreviewWarning(raw())).toContain('尚未据此调整缺失字体替代');
  expect(metadata.fontFamilyMetadataPreviewWarning({type: 'doc', content: [{type: 'paragraph'}]})).toBe('');
  const source = readFileSync('ui/authoring.ts', 'utf8');
  const create = source.slice(source.indexOf('async function create('));
  expect(create.indexOf('prepareFontFamilyMetadataDocument(')).toBeLessThan(create.indexOf('await canLeaveAuthoring()'));
  expect(create.indexOf('documentStyleProfile(initial)')).toBeLessThan(create.indexOf('closeAuthoring();'));
  expect(source).toContain('WordFontFamilyMetadata.configure({onError: message => showNotice(message, true)})');
  expect(source).toContain('...fontFamilyMetadataDocumentDOM(state.doc.attrs)');
  expect(source.match(/fontFamilyMetadataPreviewWarning\(content\)/g)).toHaveLength(2);
  expect(source).toContain('pageBreakPreviewWarning(doc) + fontFamilyMetadataPreviewWarning(doc)');
  expect(source).toContain('fontFamilyMetadataPreviewWarning(snapshotDocument(editor!))');
  expect(source.indexOf('fontFamilyMetadataPreviewWarning(snapshotDocument(editor))')).toBeLessThan(source.indexOf("'export_current_pdf'"));
  expect(source).toContain('queueDraft(snapshotDocument(owner))'); expect(source).toContain('doc = snapshotDocument(editor)');
  expect(source).toContain("invoke('save_draft', { doc })");
});
