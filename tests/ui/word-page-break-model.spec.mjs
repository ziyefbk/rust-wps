// Real ProseMirror/Tiptap state, commands, plugins, DOM parser/serializer and
// immutable print cloning. No browser layout, OS clipboard or native printing.
import {test, expect} from '@playwright/test';
import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {EditorState, AllSelection, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {DOMParser as ModelDOMParser, DOMSerializer, Fragment, Slice} from '@tiptap/pm/model';
import {history, closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
import {interactionExtensions, snapshot as baseSnapshot, schema as oldSchema} from './word-block-type-harness.mjs';
const breaks = await loadTsModule('./ui/word-page-break.ts');
const styles = await loadTsModule('./ui/word-style-profile.ts');
const pagination = await loadTsModule('./ui/word-paragraph-pagination.ts');
const list = await loadTsModule('./ui/word-list-geometry.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const configured = (onError = () => {}) => interactionExtensions({onError}).filter(extension => extension.name !== 'pageBreak')
  .map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {
    return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);
  }}) : extension);
const extensions = (onError = () => {}) => [...configured(onError), breaks.PageBreak, breaks.PageBreakParagraph.configure({onError}),
  pagination.ParagraphPagination.configure({onError}), list.PreservedListGeometry.configure({onError}), empty.EmptyParagraphFont.configure({onError})];
const schema = getSchema(extensions());
const metrics = extra => ({font: {family: 'Calibri', halfPoints: 22}, spaceBefore: 0, spaceAfter: 200, lineSpacing: 276, ...extra});
const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const b = value => schema.node('pageBreak', value === undefined ? {} : {breakParagraph: value});
const doc = (content, split = undefined, attrs = {}) => schema.node('doc', {...attrs, ...(split === undefined ? {} : {splitPageBreakAndParagraphMark: split})}, content);
const snapshot = node => pagination.omitDefaultParagraphPagination(list.omitDefaultListGeometry(baseSnapshot(node)));
const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), breaks.pageBreakGuard(message => errors.push(message))]});
const raw = (value, split = undefined) => ({type: 'doc', ...(split === undefined ? {} : {attrs: {splitPageBreakAndParagraphMark: split}}),
  content: [{type: 'pageBreak', ...(value === undefined ? {} : {attrs: {breakParagraph: value}})}]});
const invalid = [null, false, 1, 'value', [], {}, {font: {family: 'Arial', halfPoints: 22}}, {...metrics(), other: 1},
  ...[null, {}, {family: 'Arial'}, {family: 'Arial', halfPoints: 22, bold: false}].map(font => metrics({font})),
  ...[1, 3277, 3.5, '22', NaN, Infinity].map(halfPoints => metrics({font: {family: 'Arial', halfPoints}})),
  ...['', ' a', 'a ', 'x'.repeat(32), 'bad\nfont', '\ud800', '😀'.repeat(16)].map(family => metrics({font: {family, halfPoints: 22}})),
  ...[-1, 31681, 1.5, '200', null, NaN, Infinity].flatMap(value => [metrics({spaceBefore: value}), metrics({spaceAfter: value})]),
  ...[119, 2401, 240.5, '276', null, NaN, Infinity].map(lineSpacing => metrics({lineSpacing}))];

test('exact complete metrics accept boundaries and reject partial fields coercions extra fields and bad fonts', () => {
  for (const value of [metrics(), metrics({font: {family: '字'.repeat(31), halfPoints: 2}, spaceBefore: 31680, spaceAfter: 0, lineSpacing: 120}),
    metrics({font: {family: 'Liberation Serif', halfPoints: 3276}, spaceAfter: 31680, lineSpacing: 2400})]) {
    expect(breaks.isBreakParagraph(value)).toBe(true);
    for (const split of [undefined, true]) expect(() => breaks.validatePageBreakDocument(raw(value, split))).not.toThrow();
  }
  for (const value of invalid) {
    expect(breaks.isBreakParagraph(value)).toBe(false);
    expect(() => breaks.validatePageBreakDocument(raw(value))).toThrow('分页段');
  }
  for (const key of Object.keys(metrics())) {const value = metrics(); delete value[key]; expect(() => breaks.validatePageBreakDocument(raw(value))).toThrow();}
});

test('root omission and true are independent from no bare formatted and mixed atom states', () => {
  for (const split of [undefined, true]) for (const nodes of [[p()], [b()], [b(metrics())], [b(), p(), b(metrics()), b()]]) {
    const original = doc(nodes, split, {styleProfile: 'standard-v2'}), saved = snapshot(original);
    expect(() => styles.documentStyleProfile(saved)).not.toThrow();
    expect(saved.attrs.splitPageBreakAndParagraphMark).toBe(split);
    expect(schema.nodeFromJSON(JSON.parse(JSON.stringify(saved))).eq(original)).toBe(true);
  }
  for (const value of [null, false, undefined, 0, 1, 'true', [], {}]) {
    const document = {type: 'doc', attrs: {splitPageBreakAndParagraphMark: value}, content: [{type: 'paragraph'}]};
    expect(() => styles.documentStyleProfile(document)).toThrow('分页段');
  }
});

test('raw validation refuses wrong node mark nesting content and unknown atom attributes before schema erasure', () => {
  for (const type of ['doc', 'paragraph', 'heading', 'text', 'hardBreak', 'image', 'table', 'listItem']) {
    expect(() => breaks.validatePageBreakDocument({type: 'doc', content: [{type, attrs: {breakParagraph: metrics()}}]})).toThrow();
    expect(() => breaks.validatePageBreakDocument({type: 'doc', content: [{type, attrs: {splitPageBreakAndParagraphMark: true}}]})).toThrow();
  }
  for (const changed of [{text: ''}, {text: 'x'}, {content: [{type: 'text', text: 'x'}]}, {content: null}, {marks: [{type: 'bold'}]},
    {attrs: {breakParagraph: metrics(), spaceAfter: 200}}]) {
    const document = raw(metrics()); Object.assign(document.content[0], changed);
    expect(() => breaks.validatePageBreakDocument(document)).toThrow();
  }
  for (const field of ['breakParagraph', 'splitPageBreakAndParagraphMark']) {
    expect(() => breaks.validatePageBreakDocument({type: 'doc', content: [{type: 'paragraph', marks: [{type: 'bold', attrs: {[field]: true}}]}]})).toThrow();
  }
  const nested = {type: 'doc', content: [{type: 'bulletList', content: [{type: 'listItem', content: [{type: 'paragraph'}, raw(metrics()).content[0]]}]}]};
  expect(() => breaks.validatePageBreakDocument(nested)).toThrow();
  const source = readFileSync('ui/authoring.ts', 'utf8');
  expect(source.indexOf('documentStyleProfile(initial)')).toBeLessThan(source.indexOf('closeAuthoring();', source.indexOf('async function create(')));
});

test('missing state has exact historical snapshot shape for both profiles without default null attrs', () => {
  for (const styleProfile of [null, 'standard-v2']) {
    const old = oldSchema.node('doc', {styleProfile}, [oldSchema.node('paragraph'), oldSchema.node('pageBreak'), oldSchema.node('paragraph', null, oldSchema.text('same'))]);
    const current = schema.nodeFromJSON(old.toJSON());
    expect(JSON.stringify(snapshot(current))).toBe(JSON.stringify(baseSnapshot(old)));
    expect(snapshot(current).content[1]).toEqual({type: 'pageBreak'});
    expect(current.child(1).attrs.breakParagraph).toBeNull();
    expect(current.attrs.splitPageBreakAndParagraphMark).toBeNull();
  }
  const original = raw(metrics(), true), clone = structuredClone(original);
  expect(breaks.omitDefaultPageBreak(original)).toEqual(original); expect(original).toEqual(clone);
  expect(() => breaks.validatePageBreakDocument(raw(null))).toThrow();
});

test('ordinary typing formatting snapshots draft and export JSON retain the exact source-bound atom and root', () => {
  const original = doc([p('first'), p(), b(metrics()), p(), p('after')], true, {styleProfile: 'standard-v2'});
  let state = stateFor(original); state = state.apply(closeHistory(state.tr.insertText('edit', 2)));
  state = state.apply(state.tr.addMark(1, 3, schema.mark('bold')));
  const saved = snapshot(state.doc), copied = JSON.parse(JSON.stringify(saved));
  expect(saved.attrs.splitPageBreakAndParagraphMark).toBe(true);
  expect(saved.content[2]).toEqual(raw(metrics()).content[0]);
  expect(saved.content.map(node => node.type)).toEqual(['paragraph', 'paragraph', 'pageBreak', 'paragraph', 'paragraph']);
  expect(saved.content[1].content).toBeUndefined(); expect(saved.content[3].content).toBeUndefined();
  expect(schema.nodeFromJSON(copied).eq(state.doc)).toBe(true);
  const source = readFileSync('ui/authoring.ts', 'utf8');
  expect(source).toContain('queueDraft(snapshotDocument(owner))');
  expect(source).toContain('doc = snapshotDocument(editor)');
  expect(source).toContain("invoke('save_draft', { doc })");
});

test('atom selection deletion undo and redo preserve adjacent genuine empty blocks and independent metrics', () => {
  for (const split of [undefined, true]) {
    const original = doc([p(), b(metrics()), p(), b(), p('after')], split);
    let state = stateFor(original, NodeSelection.create(original, 2));
    const selected = state.selection.content(); expect(selected.content.firstChild.eq(original.child(1))).toBe(true);
    state = state.apply(closeHistory(state.tr.deleteSelection())); const deleted = state.doc;
    expect(snapshot(deleted).content.map(node => node.type)).toEqual(['paragraph', 'paragraph', 'pageBreak', 'paragraph']);
    expect(snapshot(deleted).attrs?.splitPageBreakAndParagraphMark).toBe(split); expect(undoDepth(state)).toBe(1);
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
    expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(deleted)).toBe(true);
  }
});

test('delete-all retains dormant document intent and one undo restores every atom and true empty paragraph', () => {
  const original = doc([b(metrics()), p(), b(), b(metrics({spaceAfter: 0})), p()], true);
  let state = stateFor(original, new AllSelection(original)); state = state.apply(closeHistory(state.tr.deleteSelection()));
  expect(snapshot(state.doc)).toMatchObject({attrs: {splitPageBreakAndParagraphMark: true}, content: [{type: 'paragraph'}]});
  expect(breaks.pageBreakPreviewWarning(snapshot(state.doc))).toBe('');
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
});

test('same and different document clipboard atoms keep metrics without changing the destination root', () => {
  for (const sourceSplit of [undefined, true]) for (const destinationSplit of [undefined, true]) for (const value of [undefined, metrics()]) {
    const source = doc([p(), b(value), p()], sourceSplit), slice = NodeSelection.create(source, 2).content();
    const destination = doc([p('destination')], destinationSplit); let state = stateFor(destination);
    const guard = breaks.pageBreakGuard(); expect(guard.props.handlePaste({state}, {}, slice)).toBe(false);
    state = state.apply(closeHistory(state.tr.insert(state.doc.content.size, slice.content).setMeta('paste', true)));
    expect(state.doc.lastChild.eq(source.child(1))).toBe(true);
    expect(snapshot(state.doc).attrs?.splitPageBreakAndParagraphMark).toBe(destinationSplit);
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(destination)).toBe(true);
  }
});

test('malformed atom mutations and clipboard sentinels reject before history draft observers or fitting', () => {
  const original = doc([p('before'), b(metrics()), p('after')], true), errors = [], state = stateFor(original, undefined, errors);
  for (const value of invalid.filter(value => value !== null)) {
    const transaction = state.tr.setNodeAttribute(original.firstChild.nodeSize, 'breakParagraph', value);
    expect(state.applyTransaction(transaction).transactions).toEqual([]); expect(state.doc.eq(original)).toBe(true); expect(undoDepth(state)).toBe(0);
    const slice = new Slice(Fragment.from(b(value)), 0, 0);
    expect(breaks.pageBreakGuard().props.handlePaste({state}, {}, slice)).toBe(true);
  }
  for (const value of [false, 'true', 1]) expect(state.applyTransaction(state.tr.setDocAttribute('splitPageBreakAndParagraphMark', value)).transactions).toEqual([]);
  expect(errors.length).toBeGreaterThan(0);
});

const editors = [];
function editorFor(document) {
  const errors = [], editor = new Editor({element: null, extensions: extensions(message => errors.push(message)), content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); editors.push(editor); return {editor, errors};
}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

test('real full plugin manual insertion stays usable in flagged documents with bare default metrics and exact undo', () => {
  for (const split of [undefined, true]) for (const content of [[p('before'), b(metrics()), p('after')], [p()]]) {
    const initial = doc(content, split, {styleProfile: 'standard-v2'}), {editor, errors} = editorFor(initial), original = editor.state.doc;
    expect(snapshot(original)).toEqual(snapshot(initial));
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    expect(editor.chain().insertContent({type: 'pageBreak'}).createParagraphNear().run()).toBe(true);
    const saved = snapshot(editor.state.doc), atoms = saved.content.filter(node => node.type === 'pageBreak');
    expect(atoms.at(-1)).toEqual({type: 'pageBreak'}); expect(saved.attrs.splitPageBreakAndParagraphMark).toBe(split);
    if (content.length > 1) expect(atoms[0]).toEqual(raw(metrics()).content[0]);
    expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
  }
});

test('real full plugin atom deletion and complete slice replacement retain one history step and exact draft payload', () => {
  const initial = doc([p(), b(metrics()), p(), p('after')], true), {editor, errors} = editorFor(initial), original = editor.state.doc, observed = [];
  expect(snapshot(original)).toEqual(snapshot(initial));
  editor.on('update', () => observed.push(snapshot(editor.state.doc)));
  editor.commands.setNodeSelection(2); expect(editor.commands.deleteSelection()).toBe(true);
  expect(observed).toHaveLength(1); expect(observed[0].attrs.splitPageBreakAndParagraphMark).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true);
  const pasted = editor.schema.node('pageBreak', {breakParagraph: metrics({font: {family: 'Arial', halfPoints: 32}})});
  editor.commands.setNodeSelection(2); editor.view.dispatch(closeHistory(editor.state.tr.replaceSelection(new Slice(Fragment.from(pasted), 0, 0)).setMeta('paste', true)));
  expect(snapshot(editor.state.doc).content[1].attrs.breakParagraph.font).toEqual({family: 'Arial', halfPoints: 32});
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
});

class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; dataset = {};
  constructor(tag = 'div', attrs = {}) {
    this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = []; this.classList = {remove() {}};
    this.style = new Proxy({length: 0}, {get: (target, property) => property === 'cssText' ? this.attrs.style ?? '' : target[property] ?? ''});
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
const parse = element => {const root = new Element(); root.appendChild(element); return ModelDOMParser.fromSchema(schema).parse(root);};

test('actual atom DOM serializer/parser carries literal metrics without fake text CSS offsets or imported root flags', () => {
  for (const value of [undefined, metrics(), metrics({font: {family: 'a";color:red;/*\\b', halfPoints: 25}})]) {
    const original = b(value), element = serialize(original);
    expect(element.childNodes).toEqual([]); expect(element.getAttribute('style')).toBeNull();
    expect(element.getAttribute(breaks.BREAK_PARAGRAPH_METADATA)).toBe(value === undefined ? null : JSON.stringify(value));
    expect(parse(element).firstChild.eq(original)).toBe(true);
  }
  const oldDOM = ['div', {'data-page-break': '', class: 'page-break', 'aria-label': '手动分页符'}];
  expect(schema.nodes.pageBreak.spec.toDOM(b())).toEqual(oldDOM);
  const wrapper = new Element('div', {[breaks.SPLIT_PAGE_BREAK_METADATA]: 'true'}); wrapper.appendChild(serialize(b(metrics())));
  const result = parse(wrapper); expect(result.firstChild.attrs.breakParagraph).toEqual(metrics()); expect(result.attrs.splitPageBreakAndParagraphMark).toBeNull();
});

test('malformed clipboard HTML retains rejecting sentinel and wrong tags nested contexts or children never lose evidence', () => {
  for (const value of [...invalid.map(value => JSON.stringify(value)), '{bad', ' '.repeat(4097)]) {
    const element = new Element('div', {'data-page-break': '', [breaks.BREAK_PARAGRAPH_METADATA]: value});
    const parsed = parse(element); expect(parsed.firstChild.attrs.breakParagraph).toBe('invalid-break-paragraph');
    expect(() => breaks.validatePageBreakState(parsed)).toThrow();
  }
  const previous = globalThis.DOMParser;
  const run = element => {const body = new Element(); body.appendChild(element); globalThis.DOMParser = class {parseFromString() {return body;}}; return breaks.preflightPageBreakHTML('unchanged');};
  try {
    expect(run(serialize(b(metrics())))).toBe('unchanged');
    for (const tag of ['p', 'span', 'img', 'table']) expect(run(new Element(tag, {[breaks.BREAK_PARAGRAPH_METADATA]: JSON.stringify(metrics())}))).toContain('="invalid"');
    for (const tag of ['p', 'li', 'td', 'th', 'table']) {const wrapper = new Element(tag); wrapper.appendChild(serialize(b(metrics()))); expect(run(wrapper)).toContain('="invalid"');}
    for (const child of [new Element('br'), fakeDocument.createTextNode(' '), new Element('span')]) {
      const atom = serialize(b(metrics())); atom.appendChild(child); expect(run(atom)).toContain('="invalid"'); expect(parse(atom).firstChild.attrs.breakParagraph).toBe('invalid-break-paragraph');
    }
    expect(run(new Element('div', {[breaks.SPLIT_PAGE_BREAK_METADATA]: 'false'}))).toContain('="invalid"');
    expect(run(new Element('div', {[breaks.SPLIT_PAGE_BREAK_METADATA]: 'true'}))).toBe('unchanged');
  } finally {globalThis.DOMParser = previous;}
});

test('print snapshot captures root and atom metadata before awaits, without inventing physical line layout', async () => {
  const saved = globalThis.document, host = new Element(), source = new Element('div', {contenteditable: 'true', ...breaks.pageBreakDocumentDOM({splitPageBreakAndParagraphMark: true})});
  const atom = serialize(b(metrics())); source.appendChild(atom); let ready;
  globalThis.document = {...fakeDocument, getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const pending = preparePrintSnapshot(source);
    source.removeAttribute(breaks.SPLIT_PAGE_BREAK_METADATA); atom.setAttribute(breaks.BREAK_PARAGRAPH_METADATA, 'changed'); ready(); await pending;
    expect(host.firstChild.getAttribute(breaks.SPLIT_PAGE_BREAK_METADATA)).toBe('true');
    expect(JSON.parse(host.firstChild.firstChild.getAttribute(breaks.BREAK_PARAGRAPH_METADATA))).toEqual(metrics());
    expect(host.firstChild.childNodes).toHaveLength(1); expect(host.firstChild.firstChild.childNodes).toHaveLength(0);
    expect(host.firstChild.firstChild.getAttribute('style')).toBeNull(); expect(host.firstChild.hasAttribute('contenteditable')).toBe(false);
  } finally {globalThis.document = saved;}
});

test('honest preview warning is limited to actual affected breaks and both print/PDF paths expose it', () => {
  for (const document of [raw(metrics()), raw(metrics(), true), raw(undefined, true)]) expect(breaks.pageBreakPreviewWarning(document)).toContain('打印和 PDF 暂不复现');
  for (const document of [raw(undefined), {type: 'doc', attrs: {splitPageBreakAndParagraphMark: true}, content: [{type: 'paragraph'}]}]) expect(breaks.pageBreakPreviewWarning(document)).toBe('');
  expect(breaks.pageBreakDocumentDOM({})).toEqual({});
  expect(breaks.pageBreakDocumentDOM({splitPageBreakAndParagraphMark: true})).toEqual({[breaks.SPLIT_PAGE_BREAK_METADATA]: 'true'});
  const source = readFileSync('ui/authoring.ts', 'utf8');
  expect(source).toContain('...pageBreakDocumentDOM(state.doc.attrs)');
  expect(source).toContain('pageBreakPreviewWarning(snapshotDocument(editor!))');
  expect(source).toContain('const pageBreakWarning = pageBreakPreviewWarning(snapshotDocument(editor))');
  expect(source.indexOf('if (pageBreakWarning) showNotice(pageBreakWarning)')).toBeLessThan(source.indexOf("'export_current_pdf'"));
  const css = readFileSync('ui/style.css', 'utf8');
  expect(css).toContain('#print-document .page-break { break-after:page; height:0; margin:0; border:0; }');
  expect(css).not.toContain('25.45'); expect(css).not.toContain(breaks.BREAK_PARAGRAPH_METADATA);
});
