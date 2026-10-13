import {test, expect} from '@playwright/test';
import {getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Image from '@tiptap/extension-image';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';

const {WordImage, isImageDimensions, parseImageDimensionPoints, formatImageDimensionPoints, parseImageDimensionsMetadata,
  validateImageDimensionsDocument, omitDefaultImageDimensions, imageDimensionsDOM, selectedWordImage,
  captureImageSelection, matchesImageSelection, createImageDimensionsTransaction, imageDimensionsGuard,
  imageDimensionsWithRatio, naturalImageDimensions} = await loadTsModule('./ui/word-image-dimensions.ts');
const schema = getSchema([StarterKit, TableKit, WordImage]);
const source = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jhtsAAAAASUVORK5CYII=';
const pair = (width = 952500, height = 476250) => ({displayWidthEmu: width, displayHeightEmu: height});
const image = (dimensions = null, src = source) => schema.node('image', {src, alt: 'original', title: 'retain', ...dimensions});
const paragraph = (text = '') => schema.node('paragraph', null, text ? schema.text(text) : null);
const doc = (...nodes) => schema.node('doc', null, nodes);
const snapshot = document => omitDefaultImageDimensions(document.toJSON());
const selected = (document, pos = 0, plugins = [history(), imageDimensionsGuard()]) => EditorState.create({doc: document, selection: NodeSelection.create(document, pos), plugins});

test('point decimals and exact fractions roundtrip every EMU without silently rounding user input', () => {
  for (const [value, expected] of [['.01', 127], ['1', 12700], ['75', 952500], ['450', 5715000], ['1/12700', 1], [' 5714999 / 12700 ', 5714999]]) {
    expect(parseImageDimensionPoints(value, 'displayWidthEmu')).toBe(expected);
  }
  expect(parseImageDimensionPoints('600', 'displayHeightEmu')).toBe(7620000);
  for (const value of [1, 3, 127, 635, 9525, 1234567, 5714999, 5715000, 7620000]) {
    expect(parseImageDimensionPoints(formatImageDimensionPoints(value), value > 5715000 ? 'displayHeightEmu' : 'displayWidthEmu')).toBe(value);
  }
  for (const value of ['', '0', '-1', '+1', '1e2', 'NaN', 'Infinity', '1pt', '450.01', '.0001', '0.010000000000000001', '1/2', '0/12700', '9'.repeat(65)]) {
    expect(parseImageDimensionPoints(value, 'displayWidthEmu')).toBeNull();
  }
});

test('raw JSON validation rejects null, missing companion, wrong types, misplaced and constrained dimensions before schema coercion', () => {
  const incoming = attrs => ({type: 'doc', content: [{type: 'image', attrs: {src: source, ...attrs}}]});
  for (const attrs of [{displayWidthEmu: null, displayHeightEmu: null}, {displayWidthEmu: 1}, {displayHeightEmu: 1},
    pair('1', 1), pair(1, '1'), pair(0, 1), pair(1, 0), pair(1.5, 1), pair(1, NaN), pair(Infinity, 1), pair(5715001, 1), pair(1, 7620001), pair(true, false)]) {
    expect(() => validateImageDimensionsDocument(incoming(attrs))).toThrow('图片显示尺寸');
  }
  expect(() => validateImageDimensionsDocument(incoming(pair(1, 1)))).not.toThrow();
  expect(() => validateImageDimensionsDocument(incoming(pair(5715000, 7620000)))).not.toThrow();
  for (const type of ['doc', 'paragraph', 'table', 'text']) expect(() => validateImageDimensionsDocument({type, attrs: pair()})).toThrow('图片显示尺寸');
  for (const parent of [schema.node('bulletList', null, [schema.node('listItem', null, [paragraph('item'), image(pair())])]),
    schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableCell', null, [paragraph(), image(pair())])])]),
    schema.node('blockquote', null, [image(pair())])]) {
    expect(() => validateImageDimensionsDocument(snapshot(doc(parent)))).toThrow('图片显示尺寸');
  }
});

test('only a complete internal default pair is omitted and legacy JSON plus DOM stay byte-identical', () => {
  const oldSchema = getSchema([StarterKit, TableKit, Image.configure({allowBase64: true, resize: false})]);
  const old = oldSchema.node('doc', null, [oldSchema.node('image', {src: source, alt: 'legacy', title: 'title'})]);
  const upgraded = schema.nodeFromJSON(old.toJSON());
  expect(JSON.stringify(snapshot(upgraded))).toBe(JSON.stringify(old.toJSON()));
  expect(schema.nodes.image.spec.toDOM(upgraded.firstChild)).toEqual(oldSchema.nodes.image.spec.toDOM(old.firstChild));
  const internal = doc(image(pair(1, 2))).toJSON(), saved = omitDefaultImageDimensions(internal);
  expect(saved.content[0].attrs).toMatchObject(pair(1, 2));
  expect(omitDefaultImageDimensions({type: 'image', attrs: {displayWidthEmu: 1, displayHeightEmu: null}}).attrs).toEqual({displayWidthEmu: 1, displayHeightEmu: null});
  expect(internal.content[0].attrs).toMatchObject(pair(1, 2));
  expect(() => validateImageDimensionsDocument(snapshot(doc(schema.node('bulletList', null, [schema.node('listItem', null, [paragraph(), image()])]))))).not.toThrow();
});

test('owned HTML metadata is paired, exact, and arbitrary width/CSS never becomes canonical geometry', () => {
  expect(parseImageDimensionsMetadata(null, null)).toBeNull();
  expect(parseImageDimensionsMetadata('1', '7620000')).toEqual(pair(1, 7620000));
  for (const values of [['1', null], [null, '1'], ['0', '1'], ['5715001', '1'], ['1', '7620001'], ['1.0', '2'], ['1e2', '2'], [' 1', '2']]) {
    expect(() => parseImageDimensionsMetadata(...values)).toThrow('图片显示尺寸');
  }
  const attrs = WordImage.config.addAttributes.call({parent: () => ({})});
  const fake = values => ({getAttribute: name => values[name] ?? null, style: {width: '120px', height: '40px'}});
  const arbitrary = fake({width: '120', height: '40'});
  for (const field of ['displayWidthEmu', 'displayHeightEmu']) expect(attrs[field].parseHTML(arbitrary)).toBeNull();
  const partial = fake({'data-display-width-emu': '952500'});
  for (const field of ['displayWidthEmu', 'displayHeightEmu']) expect(attrs[field].parseHTML(partial)).toBe('invalid-image-dimensions');
});

test('explicit DOM uses exact EMU conversion and overrides responsive/print geometry without changing original bytes', () => {
  const sized = image(pair(5715000, 7620000));
  const output = schema.nodes.image.spec.toDOM(sized);
  expect(output[0]).toBe('img');
  expect(output[1]).toMatchObject({src: source, alt: 'original', title: 'retain', 'data-display-width-emu': '5715000', 'data-display-height-emu': '7620000'});
  expect(output[1].style).toContain('width: 600px'); expect(output[1].style).toContain('height: 800px');
  expect(output[1].style).toContain('max-width: none'); expect(output[1].style).toContain('max-height: none');
  expect(output[1].style).toContain('object-fit: fill');
  expect(output[1]).not.toHaveProperty('displayWidthEmu');
  expect(imageDimensionsDOM(pair(1, 1)).style).toContain(`width: ${1 / 9525}px`);
  expect(imageDimensionsDOM({})).toEqual({});
});

test('selection-specific transaction preserves selected node, surrounding content, original source and metadata', () => {
  const original = doc(paragraph('before'), image(), image(pair()), paragraph('after'));
  const state = selected(original, original.firstChild.nodeSize);
  const token = captureImageSelection(state), transaction = createImageDimensionsTransaction(state, pair(1234567, 765432), token);
  expect(transaction.steps).toHaveLength(1); expect(transaction.selection.eq(state.selection)).toBe(true);
  expect(transaction.doc.child(1).attrs).toMatchObject({...pair(1234567, 765432), src: source, alt: 'original', title: 'retain'});
  for (const index of [0, 2, 3]) expect(transaction.doc.child(index).eq(original.child(index))).toBe(true);
  expect(state.doc).toBe(original);
  expect(createImageDimensionsTransaction(state.apply(transaction), pair(1234567, 765432))).toBeNull();
});

test('caret, all-selection and nested images cannot resize; legacy nested image remains untouched', () => {
  const document = doc(image(), paragraph('text'));
  for (const selection of [TextSelection.atEnd(document), new AllSelection(document)]) {
    const state = EditorState.create({doc: document, selection});
    expect(selectedWordImage(state)).toBeNull(); expect(createImageDimensionsTransaction(state, pair())).toBeNull();
  }
  const nested = doc(schema.node('bulletList', null, [schema.node('listItem', null, [paragraph(), image()])]));
  let position; nested.descendants((node, pos) => {if (node.type.name === 'image') position = pos;});
  const state = selected(nested, position);
  expect(selectedWordImage(state).supported).toBe(false); expect(captureImageSelection(state)).toBeNull();
  expect(() => createImageDimensionsTransaction(state, pair())).toThrow('图片显示尺寸');
  expect(() => createImageDimensionsTransaction(state, null)).toThrow('图片显示尺寸');
});

test('stale selection, document edits and deleted or replaced images invalidate target tokens', () => {
  const state = selected(doc(image(), image(), paragraph('after'))), token = captureImageSelection(state);
  expect(matchesImageSelection(state, token)).toBe(true);
  for (const next of [state.apply(state.tr.setSelection(NodeSelection.create(state.doc, 1))),
    state.apply(state.tr.insertText('x', 3)), state.apply(state.tr.delete(0, 1)),
    state.apply(createImageDimensionsTransaction(state, pair()))]) {
    expect(matchesImageSelection(next, token)).toBe(false);
    expect(createImageDimensionsTransaction(next, pair(1, 1), token)).toBeNull();
  }
});

test('set and reset each undo and redo atomically, preserving bytes and keeping surrounding typing separate', () => {
  let state = selected(doc(image(), paragraph('text')));
  state = state.apply(state.tr.insertText('prior', 3));
  state = state.apply(state.tr.setSelection(NodeSelection.create(state.doc, 0)));
  const original = state.doc;
  state = state.apply(closeHistory(createImageDimensionsTransaction(state, pair(1, 2)))); state = state.apply(closeHistory(state.tr));
  const resized = state.doc;
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(resized)).toBe(true);
  state = state.apply(closeHistory(createImageDimensionsTransaction(state, null))); state = state.apply(closeHistory(state.tr));
  expect(snapshot(state.doc).content[0].attrs).not.toHaveProperty('displayWidthEmu'); expect(state.doc.eq(original)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(resized)).toBe(true);
  state = state.apply(closeHistory(state.tr)); state = state.apply(state.tr.insertText('later', state.doc.content.size - 1));
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(resized)).toBe(true);
});

test('ratio calculations preserve exact entered axis, round companion to nearest EMU and reject overflow', () => {
  expect(imageDimensionsWithRatio(101, 'displayWidthEmu', {width: 2, height: 1})).toEqual(pair(101, 51));
  expect(imageDimensionsWithRatio(101, 'displayHeightEmu', {width: 2, height: 1})).toEqual(pair(202, 101));
  expect(imageDimensionsWithRatio(5715000, 'displayWidthEmu', {width: 1, height: 2})).toBeNull();
  expect(imageDimensionsWithRatio(1, 'displayWidthEmu', {width: 999, height: 1})).toBeNull();
  expect(naturalImageDimensions(1200, 400)).toEqual(pair(5715000, 1905000));
  expect(naturalImageDimensions(400, 1600)).toEqual(pair(1905000, 7620000));
  expect(naturalImageDimensions(1, 1)).toEqual(pair(9525, 9525));
  for (const values of [[0, 1], [1, 0], [1.5, 1], [Infinity, 1], [1000001, 1]]) expect(naturalImageDimensions(...values)).toBeNull();
});

test('invalid requests reject atomically without document, selection or undo changes', () => {
  const state = selected(doc(image()));
  for (const dimensions of [undefined, {}, pair(null, null), pair(1), pair(0, 1), pair(1, 7620001), {...pair(), extra: 1}]) {
    if (isImageDimensions(dimensions) && !('extra' in dimensions)) continue;
    expect(() => createImageDimensionsTransaction(state, dimensions)).toThrow('图片显示尺寸');
    expect(state.doc.firstChild.attrs.src).toBe(source); expect(undo(state)).toBe(false);
  }
});

test('prospective structure, depth, text and draft-byte budgets reject before changes or history', () => {
  let nested = paragraph('deep'); for (let count = 0; count < 33; count++) nested = schema.node('blockquote', null, [nested]);
  for (const [document, error] of [[doc(image(), ...Array.from({length: 5000}, () => paragraph('x'))), '文档结构'],
    [doc(image(), nested), '文档结构'], [doc(image(), paragraph('中'.repeat(700000))), '2 MiB'],
    [doc(schema.node('image', {src: source, alt: 'A'.repeat(32 * 1024 * 1024)})), '32 MiB']]) {
    const state = selected(document);
    expect(() => createImageDimensionsTransaction(state, pair())).toThrow(error);
    expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

test('guard blocks malformed metadata, partial pairs, constrained moves and oversized pasted documents atomically', () => {
  const errors = [], state = selected(doc(image(pair()), paragraph()), 0, [history(), imageDimensionsGuard(message => errors.push(message))]);
  const bad = [state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, displayHeightEmu: null}),
    state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, displayWidthEmu: 'invalid-image-dimensions'}),
    state.tr.replaceWith(0, state.doc.content.size, schema.node('blockquote', null, [image(pair())])),
    state.tr.insert(state.doc.content.size, paragraph('中'.repeat(700000)))];
  for (const transaction of bad) {
    const result = state.applyTransaction(transaction);
    expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(result.state)).toBe(false);
  }
  expect(errors).toHaveLength(bad.length);
  expect(state.applyTransaction(createImageDimensionsTransaction(state, pair(1, 1))).transactions).toHaveLength(1);
});

const {WordImageControls} = await loadTsModule('./ui/word-image-controls.ts');
async function controlsHarness(run, initial = doc(image(), image(null, 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAACQd1PeAAAAD0lEQVR42mP4z8DAwMAAAA0IAQCuEcnWAAAAAElFTkSuQmCC'), paragraph('after'))) {
  class Element {
    listeners = new Map(); value = ''; checked = false; disabled = false; placeholder = ''; textContent = '';
    addEventListener(type, listener) {this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);}
    emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
    contains(other) {return this.id === 'image-dimension-controls' && other?.id?.startsWith('edit-image-');}
  }
  const elements = new Map(), oldDocument = globalThis.document, oldWindow = globalThis.window, oldHTMLElement = globalThis.HTMLElement;
  const element = id => {if (!elements.has(id)) {const node = new Element(); node.id = id; elements.set(id, node);} return elements.get(id);};
  const decodes = [], notices = [];
  class DeferredImage {decode() {return new Promise((resolve, reject) => decodes.push({resolve: (width, height) => {this.naturalWidth = width; this.naturalHeight = height; resolve();}, reject}));}}
  globalThis.document = {getElementById: element}; globalThis.window = {Image: DeferredImage}; globalThis.HTMLElement = Element;
  element('edit-image-keep-ratio').checked = true;
  let state = selected(initial), locked = false, dispatches = 0, controls;
  let editor = {get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); dispatches++; controls.update();}, focus() {}}};
  controls = new WordImageControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message)});
  controls.update();
  const settle = async () => {await Promise.resolve(); await Promise.resolve(); await Promise.resolve();};
  try {
    await run({element, controls, decodes, notices, settle, get state() {return state;}, get dispatches() {return dispatches;},
      select(pos) {state = state.apply(state.tr.setSelection(NodeSelection.create(state.doc, pos))); controls.update();},
      change(tr) {state = state.apply(tr); controls.update();}, lock(value) {locked = value; controls.update();},
      replaceOwner() {editor = {...editor}; controls.reset(); controls.update();}});
  } finally {globalThis.document = oldDocument; globalThis.window = oldWindow; globalThis.HTMLElement = oldHTMLElement;}
}

test('browser-free controls initialize natural dimensions without mutations, resize one image, reset and undo atomically', async () => {
  await controlsHarness(async h => {
    expect(h.decodes).toHaveLength(1); expect(h.element('edit-image-natural-ratio').disabled).toBe(true);
    h.decodes[0].resolve(200, 100); await h.settle();
    expect(h.element('edit-image-width').value).toBe('150'); expect(h.element('edit-image-height').value).toBe('75');
    expect(h.dispatches).toBe(0);
    const width = h.element('edit-image-width'); width.emit('focus'); width.value = '75'; width.emit('input');
    expect(h.element('edit-image-height').value).toBe('37.5');
    h.element('edit-image-dimensions-apply').emit('click');
    expect(h.state.doc.firstChild.attrs).toMatchObject(pair()); expect(h.state.doc.child(1).attrs.displayWidthEmu).toBeNull();
    expect(h.dispatches).toBe(2);
    h.element('edit-image-dimensions-reset').emit('click');
    expect(snapshot(h.state.doc).content[0].attrs).not.toHaveProperty('displayWidthEmu');
    expect(undo(h.state, tr => h.change(tr))).toBe(true); expect(h.state.doc.firstChild.attrs).toMatchObject(pair());
  });
});

test('browser-free controls reject stale decoded dimensions after a different image or document is selected', async () => {
  await controlsHarness(async h => {
    h.select(1); expect(h.decodes).toHaveLength(2);
    h.decodes[0].resolve(400, 100); await h.settle();
    expect(h.element('edit-image-width').value).toBe(''); expect(h.element('edit-image-natural-ratio').disabled).toBe(true);
    h.decodes[1].resolve(60, 120); await h.settle();
    expect(h.element('edit-image-width').value).toBe('45'); expect(h.element('edit-image-height').value).toBe('90');
    expect(h.dispatches).toBe(0);
    h.select(0); const pending = h.decodes.at(-1); h.replaceOwner();
    pending.resolve(100, 200); await h.settle();
    expect(h.element('edit-image-natural-ratio').disabled).toBe(true); expect(h.dispatches).toBe(0);
  });
});

test('browser-free async decode never replaces pending input and a stale draft does not move to the next image', async () => {
  await controlsHarness(async h => {
    h.element('edit-image-keep-ratio').checked = false;
    const width = h.element('edit-image-width'); width.emit('focus'); width.value = '42'; width.emit('input');
    h.element('edit-image-height').value = '24';
    h.decodes[0].resolve(200, 100); await h.settle();
    expect(width.value).toBe('42'); expect(h.element('edit-image-height').value).toBe('24');
    h.select(1); expect(width.value).toBe('');
    h.element('edit-image-dimensions-apply').emit('click');
    expect(h.dispatches).toBe(0); expect(h.notices.at(-1)).toContain('成对设置');
  });
});

test('browser-free controls enforce every lock on apply/reset/restore and preserve the model despite synthetic clicks', async () => {
  await controlsHarness(async h => {
    h.decodes[0].resolve(200, 100); await h.settle();
    h.element('edit-image-keep-ratio').checked = false;
    h.element('edit-image-width').emit('focus'); h.element('edit-image-width').value = '42'; h.element('edit-image-height').value = '24';
    h.lock(true);
    for (const id of ['edit-image-width', 'edit-image-height', 'edit-image-keep-ratio', 'edit-image-dimensions-apply', 'edit-image-dimensions-reset', 'edit-image-natural-ratio']) {
      expect(h.element(id).disabled).toBe(true); h.element(id).emit('click');
    }
    expect(h.dispatches).toBe(0); expect(h.state.doc.firstChild.attrs).toMatchObject(pair());
  }, doc(image(pair()), paragraph('after')));
});

test('browser-free controls reject invalid/excessive ratio changes without mutating or silently breaking checked ratio', async () => {
  await controlsHarness(async h => {
    h.decodes[0].resolve(100, 400); await h.settle();
    const width = h.element('edit-image-width'); width.emit('focus'); width.value = '450'; width.emit('input');
    expect(h.element('edit-image-height').value).toBe(''); h.element('edit-image-dimensions-apply').emit('click');
    expect(h.dispatches).toBe(0);
    width.value = '75'; width.emit('input'); h.element('edit-image-height').value = '75';
    h.element('edit-image-dimensions-apply').emit('click'); expect(h.notices.at(-1)).toContain('不符合当前比例'); expect(h.dispatches).toBe(0);
    h.element('edit-image-keep-ratio').checked = false; h.element('edit-image-dimensions-apply').emit('click');
    expect(h.state.doc.firstChild.attrs).toMatchObject(pair(952500, 952500));
    h.element('edit-image-natural-ratio').emit('click'); expect(h.state.doc.firstChild.attrs).toMatchObject(pair(952500, 3810000));
  });
});

test('browser-free controls handle failed decode and require explicit choice to set an unknown ratio', async () => {
  await controlsHarness(async h => {
    h.decodes[0].reject(new Error('bad PNG')); await h.settle();
    expect(h.element('edit-image-natural-ratio').disabled).toBe(true);
    h.element('edit-image-width').emit('focus'); h.element('edit-image-width').value = '75'; h.element('edit-image-height').value = '75';
    h.element('edit-image-dimensions-apply').emit('click'); expect(h.dispatches).toBe(0); expect(h.notices.at(-1)).toContain('尚未读取');
    h.element('edit-image-keep-ratio').checked = false; h.element('edit-image-dimensions-apply').emit('click');
    expect(h.state.doc.firstChild.attrs).toMatchObject(pair(952500, 952500));
  });
});

const {inspectWordImageMedia, validateImageDisplayMedia} = await loadTsModule('./ui/word-image-media.ts');
const mediaSource = (bytes, mime = 'png') => `data:image/${mime};base64,${Buffer.from(bytes).toString('base64')}`;
const pngChunk = (type, body = []) => {const size = Buffer.alloc(4); size.writeUInt32BE(body.length); return Buffer.concat([size, Buffer.from(type), Buffer.from(body), Buffer.alloc(4)]);};
const pngWith = (tag, body = []) => mediaSource(Buffer.concat([Buffer.from(source.split(',')[1], 'base64').subarray(0, -12), pngChunk(tag, body), pngChunk('IEND')]));

test('stock pixel attributes reject in JSON/transactions and HTML pixel sizes normalize without inferring EMU', () => {
  const state = selected(doc(image()));
  for (const attrs of [{width: 100}, {height: 20}, {...pair(), width: 10}, {...pair(), height: '20'}]) {
    const input = {type: 'doc', content: [{type: 'image', attrs: {src: source, ...attrs}}]};
    expect(() => validateImageDimensionsDocument(input)).toThrow('像素');
    const result = state.applyTransaction(state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, ...attrs}));
    expect(result.state).toBe(state); expect(result.transactions).toEqual([]); expect(undo(result.state)).toBe(false);
  }
  const attrs = WordImage.config.addAttributes.call({parent: () => ({width: {default: null}, height: {default: null}})});
  for (const key of ['width', 'height']) expect(attrs[key].parseHTML({getAttribute: () => '120'})).toBeNull();
});

test('bounded media checks reject strict-base64, per-image, aggregate and count overflow before changes', () => {
  for (const src of ['https://example.com/image.png', 'data:image/png;base64,AA=', 'data:image/png;base64,AB==', 'data:image/png;base64,AA==AA==', 'data:image/png;base64,AA==\n']) {
    expect(() => inspectWordImageMedia(src)).toThrow();
  }
  const big = mediaSource(Buffer.alloc(8 * 1024 * 1024 + 1));
  expect(() => validateImageDimensionsDocument({type: 'doc', content: [{type: 'image', attrs: {src: big}}]})).toThrow('8 MiB');
  const seven = mediaSource(Buffer.alloc(7 * 1024 * 1024));
  const overAggregate = doc(image(), image(null, seven), image(null, seven), image(null, seven));
  expect(() => createImageDimensionsTransaction(selected(overAggregate), pair())).toThrow('20 MiB');
  const overCount = doc(...Array.from({length: 101}, () => image()));
  expect(() => createImageDimensionsTransaction(selected(overCount), pair())).toThrow('100 张');
  const state = selected(doc(image()));
  for (const document of [overAggregate, overCount, doc(image(null, big))]) {
    const result = state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, document.content));
    expect(result.state).toBe(state); expect(result.transactions).toEqual([]); expect(undo(result.state)).toBe(false);
  }
});

test('sized PNG rejects EXIF and APNG chunks, truncation and trailing data while unsized metadata remains legacy', () => {
  expect(() => validateImageDisplayMedia(source)).not.toThrow();
  for (const src of [...['eXIf', 'acTL', 'fcTL', 'fdAT'].map(tag => pngWith(tag)),
    mediaSource(Buffer.from(source.split(',')[1], 'base64').subarray(0, -1)),
    mediaSource(Buffer.concat([Buffer.from(source.split(',')[1], 'base64'), Buffer.from([0])]))]) {
    expect(() => validateImageDisplayMedia(src)).toThrow('元数据');
    expect(() => validateImageDimensionsDocument(snapshot(doc(image(null, src))))).not.toThrow();
    const state = selected(doc(image(null, src)));
    expect(() => createImageDimensionsTransaction(state, pair())).toThrow('元数据'); expect(undo(state)).toBe(false);
  }
});

test('JPEG marker scan rejects APP1 before and after entropy, malformed markers and trailing data', () => {
  // Metadata-only fixtures exercise the scanner, not image decoding/rendering.
  const valid = [255, 216, 255, 192, 0, 11, 8, 0, 1, 0, 1, 1, 1, 17, 0, 255, 218, 0, 2, 42, 255, 0, 23, 255, 208, 44, 255, 217];
  expect(() => validateImageDisplayMedia(mediaSource(valid, 'jpeg'))).not.toThrow();
  for (const bytes of [[255, 216, 255, 225, 0, 2, 255, 217], [...valid.slice(0, -2), 255, 225, 0, 2, 255, 217],
    [...valid, 0], valid.slice(0, -1), [255, 216, 255, 218, 0, 1, 255, 217], [255, 216, 255, 0, 255, 217]]) {
    expect(() => validateImageDisplayMedia(mediaSource(bytes, 'jpeg'))).toThrow('元数据');
  }
});

test('natural ratio controls never decode unsupported EXIF/animation media and explicit resize stays atomic', async () => {
  await controlsHarness(async h => {
    await h.settle(); expect(h.decodes).toHaveLength(0); expect(h.element('edit-image-natural-ratio').disabled).toBe(true);
    expect(h.element('edit-image-dimensions-apply').disabled).toBe(true); expect(h.element('edit-image-dimension-state').textContent).toContain('暂不支持设置尺寸');
    h.element('edit-image-keep-ratio').checked = false;
    h.element('edit-image-width').emit('focus'); h.element('edit-image-width').value = '75'; h.element('edit-image-height').value = '75';
    h.element('edit-image-dimensions-apply').emit('click');
    expect(h.dispatches).toBe(0); expect(h.notices.at(-1)).toContain('元数据'); expect(undo(h.state)).toBe(false);
  }, doc(image(null, pngWith('eXIf')), paragraph()));
});

test('real print snapshot clones exact image style and media immutably without changing selection or original bytes', async () => {
  const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
  class Element {
    constructor(tag = 'div', attrs = {}) {this.tag = tag; this.attrs = attrs; this.children = []; this.classList = {remove() {}};}
    get src() {return this.attrs.src;}
    removeAttribute(key) {delete this.attrs[key];}
    querySelectorAll(query) {return this.children.flatMap(child => [...(query === child.tag ? [child] : []), ...child.querySelectorAll(query)]);}
    cloneNode() {const copy = new Element(this.tag, {...this.attrs}); copy.children = this.children.map(child => child.cloneNode(true)); return copy;}
    decode() {return Promise.resolve();}
    replaceChildren(...children) {this.children = children;}
  }
  const root = new Element('div', {contenteditable: 'true'}), host = new Element();
  root.children.push(new Element('img', {src: source, ...imageDimensionsDOM(pair(5715000, 7620000))}));
  const previous = globalThis.document;
  globalThis.document = {getElementById: () => host, fonts: {ready: Promise.resolve()}};
  try {
    await preparePrintSnapshot(root);
    const printed = host.children[0].children[0];
    expect(printed.attrs).toEqual(root.children[0].attrs); expect(printed.src).toBe(source);
    expect(printed.attrs.style).toContain('width: 600px; height: 800px; max-width: none; max-height: none; object-fit: fill');
    root.children[0].attrs = {src: source};
    expect(printed.attrs['data-display-width-emu']).toBe('5715000'); expect(host.children[0].attrs).not.toHaveProperty('contenteditable');
  } finally {globalThis.document = previous;}
});

test('explicit display metadata and natural ratio reject oversized pixel headers before image decoding or mutation', () => {
  const png = (width, height) => {const bytes = Buffer.from(source.split(',')[1], 'base64'); bytes.writeUInt32BE(width, 16); bytes.writeUInt32BE(height, 20); return mediaSource(bytes);};
  const jpeg = (width, height) => mediaSource([255, 216, 255, 192, 0, 11, 8, height >> 8, height & 255, width >> 8, width & 255, 1, 1, 17, 0, 255, 218, 0, 2, 42, 255, 217], 'jpeg');
  // These are bounded header probes, not CRC/pixel-decode fixtures.
  for (const make of [png, jpeg]) for (const [width, height] of [[0, 1], [1, 0], [20001, 1], [1, 20001], [20000, 2001], [10000, 10000]]) {
    const src = make(width, height);
    expect(() => validateImageDisplayMedia(src)).toThrow('元数据');
    expect(naturalImageDimensions(width, height)).toBeNull();
    const state = selected(doc(image(null, src)));
    expect(() => createImageDimensionsTransaction(state, pair())).toThrow('元数据'); expect(undo(state)).toBe(false);
    expect(() => validateImageDimensionsDocument(snapshot(state.doc))).not.toThrow();
  }
  for (const make of [png, jpeg]) expect(() => validateImageDisplayMedia(make(20000, 2000))).not.toThrow();
  expect(naturalImageDimensions(20000, 2000)).toEqual(pair(5715000, 571500));
});
