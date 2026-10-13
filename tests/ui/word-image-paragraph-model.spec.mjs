import {test, expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
import {getSchema, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Image from '@tiptap/extension-image';
import {TableKit} from '@tiptap/extension-table';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {EditorState, NodeSelection, TextSelection, AllSelection} from '@tiptap/pm/state';
import {history, undo, redo, closeHistory} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';

const {isImageParagraph, isImageParagraphFont, defaultImageParagraph, parseImageParagraphMetadata, imageParagraphDOM,
  validateImageParagraphDocument, omitDefaultImageParagraph, imageParagraphFontStyle} = await loadTsModule('./ui/word-image-paragraph.ts');
const {WordImage, createImageParagraphTransaction, createImageDimensionsTransaction, captureImageSelection,
  imageDimensionsGuard, validateImageDimensionsDocument, omitDefaultImageDimensions} = await loadTsModule('./ui/word-image-dimensions.ts');
const {WordImageParagraphControls} = await loadTsModule('./ui/word-image-paragraph-controls.ts');
const ProfileDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+', addAttributes: () => ({styleProfile: {default: null}})});
const schema = getSchema([StarterKit.configure({document: false}), ProfileDoc, TableKit, WordImage]);
const src = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jhtsAAAAASUVORK5CYII=';
const layout = extra => ({spaceBefore: 0, spaceAfter: 120, lineSpacing: 360, textAlign: 'left', ...extra});
const font = extra => ({family: 'Arial', halfPoints: 22, bold: false, italic: false, ...extra});
const withFonts = extra => layout({paragraphFont: font(), runFont: font({halfPoints: 20}), ...extra});
const image = (imageParagraph, attrs = {}) => schema.node('image', {src, alt: 'same bytes', title: 'same title', ...attrs, ...(imageParagraph === undefined ? {} : {imageParagraph})});
const paragraph = text => schema.node('paragraph', null, text ? schema.text(text) : []);
const doc = (...content) => schema.node('doc', {styleProfile: 'standard-v2'}, content);
const select = (document, pos = 0) => EditorState.create({doc: document, selection: NodeSelection.create(document, pos), plugins: [history(), imageDimensionsGuard()]});
const snapshot = node => omitDefaultImageDimensions(node.toJSON());
const jsonWith = (value, attrs = {}) => ({type: 'doc', content: [{type: 'image', attrs: {src, ...attrs, imageParagraph: value}}]});

const invalidLayouts = [null, false, [], 'left', {}, {spaceBefore: 0}, layout({spaceAfter: null}), layout({spaceAfter: -1}),
  layout({spaceBefore: 31681}), layout({spaceBefore: .1}), layout({spaceBefore: '0'}), layout({lineSpacing: 119}),
  layout({lineSpacing: 2401}), layout({lineSpacing: '240'}), layout({textAlign: 'start'}), layout({textAlign: ['left']}),
  layout({unexpected: true}), layout({paragraphFont: font()}), layout({runFont: font()}),
  layout({paragraphFont: null, runFont: null}), withFonts({paragraphFont: font({halfPoints: 1})}),
  withFonts({runFont: font({halfPoints: 3277})}), withFonts({runFont: font({bold: 'false'})}),
  withFonts({runFont: {...font(), extra: 1}}), withFonts({paragraphFont: {family: 'Arial', halfPoints: 22, bold: false}}),
  ...['', ' leading', 'trailing ', 'a'.repeat(32), '\ud800', 'bad\nfont'].map(family => withFonts({runFont: font({family})}))];

test('complete image paragraph schema accepts zeroes and exact boundaries and rejects raw partial/null/unknown/wrong-typed metadata', () => {
  for (const value of [layout(), layout({spaceBefore: 31680, spaceAfter: 0, lineSpacing: 120, textAlign: 'justify'}),
    withFonts({paragraphFont: font({halfPoints: 2}), runFont: font({halfPoints: 3276, bold: true, italic: true})})]) {
    expect(isImageParagraph(value)).toBe(true); expect(() => validateImageDimensionsDocument(jsonWith(value))).not.toThrow();
  }
  for (const value of invalidLayouts) {
    expect(isImageParagraph(value), JSON.stringify(value)).toBe(false);
    expect(() => validateImageParagraphDocument(jsonWith(value))).toThrow('图片段落');
    expect(() => validateImageDimensionsDocument(jsonWith(value))).toThrow('图片段落');
  }
  expect(isImageParagraphFont(font({family: '字'.repeat(31)}))).toBe(true);
  expect(isImageParagraphFont(font({family: '😀'.repeat(16)}))).toBe(false);
});

test('image paragraph metadata is root-only; misplaced data and list/table images reject before editor normalization', () => {
  for (const type of ['doc', 'paragraph', 'heading', 'text', 'table', 'tableCell']) {
    expect(() => validateImageParagraphDocument({type, attrs: {imageParagraph: layout()}})).toThrow('图片段落');
  }
  for (const container of [schema.node('blockquote', null, [image(layout())]),
    schema.node('bulletList', null, [schema.node('listItem', null, [paragraph(), image(layout())])]),
    schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableCell', null, [image(layout())])])])]) {
    expect(() => validateImageDimensionsDocument(snapshot(doc(container)))).toThrow('图片段落');
  }
  expect(() => validateImageDimensionsDocument(snapshot(doc(schema.node('blockquote', null, [image()]))))).not.toThrow();
});

test('absent metadata preserves exact legacy JSON and image DOM; only schema null is omitted without mutating original', () => {
  const oldSchema = getSchema([StarterKit, Image.configure({allowBase64: true, resize: false})]);
  const oldImage = oldSchema.node('image', {src, alt: 'same bytes', title: 'same title'});
  const current = schema.nodeFromJSON(oldImage.toJSON());
  expect(JSON.stringify(snapshot(current))).toBe(JSON.stringify(oldImage.toJSON()));
  expect(schema.nodes.image.spec.toDOM(current)).toEqual(oldSchema.nodes.image.spec.toDOM(oldImage));
  const internal = image().toJSON(); expect(internal.attrs.imageParagraph).toBeNull();
  expect(omitDefaultImageParagraph(internal).attrs).not.toHaveProperty('imageParagraph');
  expect(internal.attrs.imageParagraph).toBeNull();
  expect(omitDefaultImageParagraph({type: 'image', attrs: {imageParagraph: {spaceBefore: 0}}}).attrs.imageParagraph).toEqual({spaceBefore: 0});
});

test('explicit DOM contains a paragraph line box, nested font run and inline baseline image with independent geometry', () => {
  const value = withFonts({spaceBefore: 80, spaceAfter: 240, lineSpacing: 241, textAlign: 'center',
    paragraphFont: font({family: 'Liberation Serif', halfPoints: 24}), runFont: font({halfPoints: 16, bold: true, italic: true})});
  const output = schema.nodes.image.spec.toDOM(image(value, {displayWidthEmu: 95250, displayHeightEmu: 95250}));
  expect(output[0]).toBe('div'); expect(JSON.parse(output[1]['data-image-paragraph'])).toEqual(value);
  expect(output[1].style).toContain('margin: 4pt 0 12pt'); expect(output[1].style).toContain(`line-height: ${241 / 240}`);
  expect(output[1].style).toContain('text-align: center'); expect(output[1].style).toContain('font-family: "Liberation Serif"');
  expect(output[2][0]).toBe('span'); expect(output[2][1].style).toContain('font-size: 8pt');
  expect(output[2][1].style).toContain('font-weight: 700; font-style: italic');
  const picture = output[2][2]; expect(picture[0]).toBe('img'); expect(picture[1].src).toBe(src);
  expect(picture[1].style).toContain('width: 10px'); expect(picture[1].style).toContain('height: 10px');
  expect(picture[1].style).toContain('display: inline'); expect(picture[1].style).toContain('vertical-align: baseline');
  expect(picture[1].style).toContain('margin: 0'); expect(picture[1].style).not.toContain('margin-top');
  expect(picture[1]).not.toHaveProperty('imageParagraph');
});

test('layout-only metadata retains legacy responsive image CSS and never invents explicit browser dimensions', () => {
  const output = schema.nodes.image.spec.toDOM(image(layout())), picture = output[2][2];
  expect(picture[1].src).toBe(src);
  expect(picture[1]).not.toHaveProperty('data-display-width-emu'); expect(picture[1]).not.toHaveProperty('data-display-height-emu');
  expect(picture[1].style).toBe('display: inline; vertical-align: baseline; margin: 0');
  const css = readFileSync('ui/style.css', 'utf8');
  expect(css).toContain('.tiptap img { max-width:100%; max-height:800px; height:auto; }');
  // This declaration check does not measure browser geometry. Without paired
  // EMU, the responsive image may differ from Rust's historical natural-size
  // 600x800 ceiling. Only paragraph metadata is explicit in this case.
  expect(snapshot(image(layout())).attrs).not.toHaveProperty('displayWidthEmu');
});

test('font CSS is encoded as a literal string on both line-box and run without style-merging injection', () => {
  const family = 'a";color:red;/*\\b';
  const value = withFonts({paragraphFont: font({family}), runFont: font({family})});
  expect(isImageParagraph(value)).toBe(true);
  const output = schema.nodes.image.spec.toDOM(image(value));
  const encoded = 'font-family: "a\\";color:red;/*\\\\b"';
  expect(output[1].style).toContain(encoded); expect(output[2][1].style).toContain(encoded);
  expect(imageParagraphFontStyle(font({family}))).toContain(encoded);
});

// Minimal standards-shaped DOM only; real ProseMirror serializer/parser and
// schema run here. No browser layout or focus is exercised by this fixture.
class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; style = {length: 0};
  constructor(tag = 'div', attrs = {}) {this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = []; this.classList = {remove: () => {}}; Object.defineProperty(this.style, 'cssText', {get: () => this.attrs.style, set: value => {this.attrs.style = value;}});}
  get firstChild() {return this.childNodes[0] ?? null;}
  get firstElementChild() {return this.childNodes.find(child => child.nodeType === 1) ?? null;}
  get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}
  get src() {return this.attrs.src;}
  getAttribute(name) {return this.attrs[name] ?? null;}
  hasAttribute(name) {return name in this.attrs;}
  setAttribute(name, value) {this.attrs[name] = String(value);}
  removeAttribute(name) {delete this.attrs[name];}
  appendChild(child) {child.parentNode = this; this.childNodes.push(child); return child;}
  replaceChildren(...children) {this.childNodes = []; for (const child of children) this.appendChild(child);}
  matches(selector) {const match = /^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(selector); return !!match && (!match[1] || this.tagName.toLowerCase() === match[1]) && (!match[2] || this.hasAttribute(match[2]));}
  querySelectorAll(query) {return this.childNodes.flatMap(child => child.nodeType === 1 ? [...(child.matches(query) ? [child] : []), ...child.querySelectorAll(query)] : []);}
  cloneNode() {const node = new Element(this.tagName, this.attrs); for (const child of this.childNodes) node.appendChild(child.cloneNode(true)); return node;}
  decode() {return Promise.resolve();}
}
const fakeDocument = {createElement: tag => new Element(tag), createElementNS: (_, tag) => new Element(tag), createDocumentFragment: () => new Element('fragment'),
  createTextNode: value => ({nodeType: 3, nodeValue: value, cloneNode() {return fakeDocument.createTextNode(value);}})};
const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: fakeDocument});
const parse = element => {const root = new Element(); root.appendChild(element); return ProseMirrorDOMParser.fromSchema(schema).parse(root);};

test('real ProseMirror DOM clipboard roundtrip keeps wrapper, media, complete fonts and exact dimensions as one image', () => {
  const original = image(withFonts({lineSpacing: 241, textAlign: 'right'}), {displayWidthEmu: 95251, displayHeightEmu: 95253});
  const element = serialize(original), parsed = parse(element);
  expect(parsed.childCount).toBe(1); expect(parsed.firstChild.eq(original)).toBe(true);
  expect(() => validateImageDimensionsDocument(snapshot(parsed))).not.toThrow();
  const legacy = parse(serialize(image())); expect(snapshot(legacy.firstChild)).toEqual(snapshot(image()));
  const oldSchema = getSchema([StarterKit, Image.configure({allowBase64: true, resize: false})]);
  const numericLegacy = new Element('div'); numericLegacy.appendChild(new Element('img', {src, alt: '123', title: 'true'}));
  const oldParsed = ProseMirrorDOMParser.fromSchema(oldSchema).parse(numericLegacy);
  const currentParsed = ProseMirrorDOMParser.fromSchema(schema).parse(numericLegacy);
  expect(snapshot(currentParsed.firstChild)).toEqual(oldParsed.firstChild.toJSON());
});

test('HTML metadata rejects malformed/partial values and malformed mixed wrappers atomically instead of silently stripping them', () => {
  expect(parseImageParagraphMetadata(null)).toBeNull();
  expect(parseImageParagraphMetadata(JSON.stringify(layout()))).toEqual(layout());
  for (const value of [...invalidLayouts.map(value => JSON.stringify(value)), 'broken', ' '.repeat(4097)]) {
    expect(() => parseImageParagraphMetadata(value)).toThrow('图片段落');
    const element = serialize(image(layout())); element.setAttribute('data-image-paragraph', value);
    const parsed = parse(element); expect(parsed.firstChild.attrs.imageParagraph).toBe('invalid-image-paragraph');
    const state = select(doc(image()));
    expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]);
  }
  for (const damage of [element => element.appendChild(new Element('img', {src})),
    element => element.firstChild.appendChild(new Element('img', {src})),
    element => element.appendChild(fakeDocument.createTextNode('mixed text')),
    element => {element.tagName = element.nodeName = 'P';},
    element => element.firstChild.firstChild.setAttribute('data-image-paragraph', 'bad')]) {
    const element = serialize(image(layout())); damage(element);
    const parsed = parse(element); expect(parsed.firstChild.attrs.imageParagraph).toBe('invalid-image-paragraph');
  }
  const unowned = new Element('img', {src, style: 'margin-top:100pt;text-align:right;line-height:2'});
  expect(parse(unowned).firstChild.attrs.imageParagraph).toBeNull();
});

test('one-step layout edits preserve media, dimensions, font pair, NodeSelection and surrounding blank paragraphs', () => {
  const original = doc(paragraph(), image(withFonts(), {displayWidthEmu: 95251, displayHeightEmu: 95253}), paragraph(), image(), paragraph('after'));
  const state = select(original, 2), token = captureImageSelection(state), value = withFonts({spaceBefore: 80, textAlign: 'right'});
  const transaction = createImageParagraphTransaction(state, value, token);
  expect(transaction.steps).toHaveLength(1); expect(transaction.selection.eq(state.selection)).toBe(true);
  expect(transaction.doc.child(1).attrs).toMatchObject({src, displayWidthEmu: 95251, displayHeightEmu: 95253, imageParagraph: value});
  for (const index of [0, 2, 3, 4]) expect(transaction.doc.child(index).eq(original.child(index))).toBe(true);
  expect(state.doc).toBe(original);
  expect(createImageParagraphTransaction(state.apply(transaction), {...value, runFont: {...value.runFont}})).toBeNull();
  const resized = createImageDimensionsTransaction(state, {displayWidthEmu: 100, displayHeightEmu: 200});
  expect(resized.doc.child(1).attrs.imageParagraph).toEqual(withFonts());
});

test('carets/ranges/nested image selections and stale tokens never apply layout to a different target', () => {
  const document = doc(image(), image(), paragraph('after')), state = select(document), token = captureImageSelection(state);
  for (const selection of [TextSelection.atEnd(document), new AllSelection(document)]) {
    expect(createImageParagraphTransaction(EditorState.create({doc: document, selection}), layout())).toBeNull();
  }
  for (const changed of [state.apply(state.tr.setSelection(NodeSelection.create(document, 1))),
    state.apply(state.tr.insertText('x', document.content.size - 1)), state.apply(state.tr.delete(0, 1)),
    state.apply(createImageDimensionsTransaction(state, {displayWidthEmu: 1, displayHeightEmu: 1}))]) {
    expect(createImageParagraphTransaction(changed, layout(), token)).toBeNull();
  }
  const nested = doc(schema.node('blockquote', null, [image()]));
  expect(() => createImageParagraphTransaction(select(nested, 1), layout())).toThrow('图片段落');
});

test('layout apply/reset has atomic undo/redo and draft roundtrip while dimension reset preserves paragraph fonts', () => {
  let state = select(doc(image(), paragraph('after'))), original = state.doc;
  state = state.apply(closeHistory(createImageParagraphTransaction(state, withFonts()))); state = state.apply(closeHistory(state.tr));
  const explicit = state.doc;
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(explicit)).toBe(true);
  expect(schema.nodeFromJSON(snapshot(explicit)).eq(explicit)).toBe(true);
  state = state.apply(closeHistory(createImageParagraphTransaction(state, null))); state = state.apply(closeHistory(state.tr));
  expect(snapshot(state.doc).content[0].attrs).not.toHaveProperty('imageParagraph'); expect(state.doc.eq(original)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(explicit)).toBe(true);
  state = state.apply(createImageDimensionsTransaction(state, {displayWidthEmu: 100, displayHeightEmu: 200}));
  state = state.apply(createImageDimensionsTransaction(state, null));
  expect(state.doc.firstChild.attrs.imageParagraph).toEqual(withFonts());
});

test('prospective invalid metadata, structure/depth, text and draft budgets reject before changes or history', () => {
  const state = select(doc(image(), paragraph()));
  for (const value of invalidLayouts.filter(value => value !== null)) {
    expect(() => createImageParagraphTransaction(state, value)).toThrow('图片段落'); expect(undo(state)).toBe(false);
  }
  let nested = paragraph('deep'); for (let count = 0; count < 33; count++) nested = schema.node('blockquote', null, [nested]);
  for (const [document, message] of [[doc(image(), ...Array.from({length: 5000}, () => paragraph('x'))), '文档结构'],
    [doc(image(), nested), '文档结构'], [doc(image(), paragraph('中'.repeat(700000))), '2 MiB'],
    [doc(image(undefined, {alt: 'A'.repeat(32 * 1024 * 1024)})), '32 MiB'],
    [doc(...Array.from({length: 101}, () => image())), '100 张']]) {
    const current = select(document); expect(() => createImageParagraphTransaction(current, layout())).toThrow(message);
    expect(current.doc).toBe(document); expect(undo(current)).toBe(false);
  }
});

test('document guard blocks invalid direct metadata and moving explicit image paragraphs into list/table containers', () => {
  const state = select(doc(image(layout()), paragraph()));
  for (const transaction of [state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, imageParagraph: {spaceAfter: 1}}),
    state.tr.replaceWith(0, state.doc.content.size, schema.node('blockquote', null, [image(layout())]))]) {
    const result = state.applyTransaction(transaction); expect(result.state).toBe(state); expect(result.transactions).toEqual([]); expect(undo(state)).toBe(false);
  }
});

function controlsHarness(run, initial = doc(image(withFonts()), image(layout({textAlign: 'right'})), paragraph('after'))) {
  class Input extends Element {
    value = ''; disabled = false; textContent = ''; listeners = new Map();
    addEventListener(type, listener) {this.listeners.set(type, [...this.listeners.get(type) ?? [], listener]);}
    emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
    contains(other) {return other?.id?.startsWith('edit-image-');}
  }
  const oldDocument = globalThis.document, oldHTMLElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) {const input = new Input(); input.id = id; elements.set(id, input);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Input;
  let state = select(initial), locked = false, dispatches = 0, controls;
  let editor = {get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); dispatches++; controls.update();}, focus() {}}};
  controls = new WordImageParagraphControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message)}); controls.update();
  const change = tr => {state = state.apply(tr); controls.update();};
  try {run({element, notices, controls, change, get state() {return state;}, get dispatches() {return dispatches;},
    select(pos, notify = true) {state = state.apply(state.tr.setSelection(NodeSelection.create(state.doc, pos))); if (notify) controls.update();},
    lock(value) {locked = value; controls.update();}, replaceOwner() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});
  } finally {globalThis.document = oldDocument; globalThis.HTMLElement = oldHTMLElement;}
}

test('browser-free selected-image controls apply exact fields, preserve imported fonts, reset and undo once', () => {
  controlsHarness(h => {
    expect(h.dispatches).toBe(0); expect(h.element('edit-image-line-spacing').value).toBe('1.5');
    h.element('edit-image-space-before').emit('focus'); h.element('edit-image-space-before').value = '4.05';
    h.element('edit-image-space-after').value = '0'; h.element('edit-image-line-spacing').value = '241/240'; h.element('edit-image-alignment').value = 'center';
    h.element('edit-image-paragraph-apply').emit('click');
    expect(h.dispatches).toBe(2); expect(h.state.doc.firstChild.attrs.imageParagraph).toEqual(withFonts({spaceBefore: 81, spaceAfter: 0, lineSpacing: 241, textAlign: 'center'}));
    expect(h.state.doc.child(1).attrs.imageParagraph).toEqual(layout({textAlign: 'right'}));
    h.element('edit-image-paragraph-reset').emit('click'); expect(h.state.doc.firstChild.attrs.imageParagraph).toBeNull();
    expect(h.notices.at(-1)).toContain('基线字体'); expect(undo(h.state, h.change)).toBe(true);
    expect(h.state.doc.firstChild.attrs.imageParagraph.paragraphFont).toEqual(font());
  });
});

test('browser-free controls discard pending fields on selection/document changes and Escape; reject silent stale selection', () => {
  controlsHarness(h => {
    const before = h.element('edit-image-space-before'); before.emit('focus'); before.value = '99';
    h.select(1); expect(before.value).toBe('0'); expect(h.element('edit-image-alignment').value).toBe('right');
    before.emit('focus'); before.value = '99'; before.emit('keydown', {key: 'Escape'}); expect(before.value).toBe('0');
    before.emit('focus'); before.value = '99'; h.replaceOwner(); expect(before.value).toBe('0');
    h.select(0, false); h.element('edit-image-paragraph-apply').emit('click'); h.element('edit-image-paragraph-reset').emit('click');
    expect(h.dispatches).toBe(0);
  });
});

test('browser-free controls enforce saving/leaving/print locks, invalid input and IME Enter without mutations', () => {
  controlsHarness(h => {
    h.lock(true);
    for (const id of ['edit-image-space-before', 'edit-image-space-after', 'edit-image-line-spacing', 'edit-image-alignment', 'edit-image-paragraph-apply', 'edit-image-paragraph-reset']) {
      expect(h.element(id).disabled).toBe(true); h.element(id).emit('click');
    }
    expect(h.dispatches).toBe(0); h.lock(false);
    h.element('edit-image-line-spacing').emit('focus'); h.element('edit-image-line-spacing').value = '11';
    h.element('edit-image-paragraph-apply').emit('click'); expect(h.notices.at(-1)).toContain('0.5–10'); expect(h.dispatches).toBe(0);
    h.element('edit-image-line-spacing').value = '2'; h.element('edit-image-line-spacing').emit('keydown', {key: 'Enter', isComposing: true});
    expect(h.dispatches).toBe(0);
  });
});

test('browser-free controls show current profile defaults without upgrading untouched images and disable nested selections', () => {
  expect(defaultImageParagraph(null)).toEqual(layout({spaceAfter: 180, lineSpacing: 456}));
  expect(defaultImageParagraph('standard-v2')).toEqual(layout());
  controlsHarness(h => {expect(h.element('edit-image-space-after').value).toBe('6'); expect(h.state.doc.firstChild.attrs.imageParagraph).toBeNull();
    expect(h.element('edit-image-paragraph-reset').disabled).toBe(true); expect(h.dispatches).toBe(0);}, doc(image(), paragraph()));
  const nested = doc(schema.node('blockquote', null, [image()]));
  controlsHarness(h => {h.select(1); expect(h.element('edit-image-paragraph-apply').disabled).toBe(true);
    h.element('edit-image-paragraph-apply').emit('click'); expect(h.dispatches).toBe(0);}, nested);
});

test('real immutable print clone keeps wrapper layout, fonts, inline baseline, image dimensions and media after editor changes', async () => {
  const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
  const source = new Element('div', {contenteditable: 'true'}), host = new Element(), previous = globalThis.document;
  source.appendChild(serialize(image(withFonts({spaceBefore: 80, textAlign: 'right'}), {displayWidthEmu: 95250, displayHeightEmu: 95250})));
  globalThis.document = {getElementById: () => host, fonts: {ready: Promise.resolve()}};
  try {
    await preparePrintSnapshot(source);
    const printed = host.firstChild.firstChild;
    expect(printed.attrs).toEqual(source.firstChild.attrs); expect(printed.firstChild.attrs).toEqual(source.firstChild.firstChild.attrs);
    expect(printed.firstChild.firstChild.attrs).toEqual(source.firstChild.firstChild.firstChild.attrs);
    expect(printed.firstChild.firstChild.src).toBe(src); expect(printed.firstChild.firstChild.attrs.style).toContain('vertical-align: baseline');
    source.firstChild.attrs.style = 'display:none'; source.firstChild.firstChild.firstChild.attrs.style = 'width:1px';
    expect(printed.attrs.style).toContain('margin: 4pt 0 6pt'); expect(printed.firstChild.firstChild.attrs.style).toContain('width: 10px');
    expect(host.firstChild.attrs).not.toHaveProperty('contenteditable');
  } finally {globalThis.document = previous;}
});
