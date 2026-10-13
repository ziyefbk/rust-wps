import {test, expect} from '@playwright/test';
import {CommandManager, getSchema, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {history, closeHistory, undo, redo} from '@tiptap/pm/history';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
const page = await loadTsModule('./ui/word-page-geometry.ts');
const {WordPageControls} = await loadTsModule('./ui/word-page-controls.ts');
const {WordImage, omitDefaultImageDimensions, createImageDimensionsTransaction} = await loadTsModule('./ui/word-image-dimensions.ts');
const {WordTable, createTableColumnWidthTransaction, createTablePositionTransaction, createTableStructureTransaction} = await loadTsModule('./ui/word-table-widths.ts');
const {snapshotDocument, documentStyleProfile} = await loadTsModule('./ui/word-style-profile.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const {inspectWordImageMedia} = await loadTsModule('./ui/word-image-media.ts');
const WordDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+', addAttributes: () => ({styleProfile: {default: null}})});
const schema = getSchema([StarterKit.configure({document: false}), WordDoc, TableKit.configure({table: false}), WordTable, WordImage, page.WordPageGeometry]);
const geometry = extra => ({widthTwips: 11906, heightTwips: 16838, marginTopTwips: 1440, marginRightTwips: 1440, marginBottomTwips: 1440, marginLeftTwips: 1440, ...extra});
const paragraph = text => schema.node('paragraph', null, text ? schema.text(text) : null);
const doc = (layout = null, content = [paragraph('before'), paragraph('after')]) => schema.node('doc', {styleProfile: 'standard-v2', pageGeometry: layout}, content);
const src = `data:image/png;base64,${readFileSync('tests/fixtures/slide-image.png').toString('base64')}`;
const image = attrs => schema.node('image', {src, ...attrs});
const table = attrs => schema.node('table', attrs, [schema.node('tableRow', null, [schema.node('tableCell', null, [paragraph('cell')])])]);
const stateFor = document => EditorState.create({doc: document, plugins: [history(), page.pageGeometryGuard()]});
const snapshot = document => omitDefaultImageDimensions(snapshotDocument({getJSON: () => document.toJSON()}));

test('all presets and orientations allow exact ±2twip source units without canonicalizing', () => {
  for (const preset of ['A4', 'Letter', 'A5']) for (const orientation of ['portrait', 'landscape']) {
    const value = page.presetPageGeometry(preset, orientation);
    expect(page.isPageGeometry(value)).toBe(true); expect(page.pagePreset(value)).toBe(preset);
    for (const delta of [-2, -1, 0, 1, 2]) {
      const source = {...value, widthTwips: value.widthTwips + delta, heightTwips: value.heightTwips - delta};
      expect(page.isPageGeometry(source)).toBe(true);
      expect(page.documentPageGeometry({type: 'doc', attrs: {pageGeometry: source}})).toEqual(source);
    }
    expect(page.isPageGeometry({...value, widthTwips: value.widthTwips + 3})).toBe(false);
  }
});

test('six exact integer fields, content boundary and asymmetric margin bounds reject raw malformed models', () => {
  const invalid = [null, undefined, [], {}, 'page', {...geometry(), extra: true}, ...page.pageGeometryFields.flatMap(field => {
    const missing = geometry(); delete missing[field];
    return [missing, ...[null, '1440', true, 1.5, NaN, Infinity].map(value => geometry({[field]: value}))];
  }), geometry({widthTwips: 10000}), geometry({marginTopTwips: -1}), geometry({marginRightTwips: 4321}),
  geometry({widthTwips: 8391, heightTwips: 11906, marginLeftTwips: 4320, marginRightTwips: 2632})];
  for (const value of invalid) {expect(page.isPageGeometry(value)).toBe(false); expect(() => page.validatePageGeometryDocument({type: 'doc', attrs: {pageGeometry: value}})).toThrow();}
  expect(page.isPageGeometry(geometry({widthTwips: 8391, heightTwips: 11906, marginLeftTwips: 4320, marginRightTwips: 2631}))).toBe(true);
  expect(page.isPageGeometry(geometry({marginTopTwips: 0, marginRightTwips: 4320, marginBottomTwips: 4, marginLeftTwips: 0}))).toBe(true);
});

test('exact point margin parsing never rounds or accepts exponent/unit syntax', () => {
  for (const [input, twips] of [['0', 0], ['.05', 1], ['72', 1440], [' 216 ', 4320], ['0.05000', 1]]) expect(page.parsePageMarginPoints(input)).toBe(twips);
  for (const input of ['', '-1', '+1', '1e2', '216.05', '.001', '0.050000000000000001', '72pt', 'NaN', '9'.repeat(65)]) expect(page.parsePageMarginPoints(input)).toBeNull();
});

test('root-only attribute and null omission preserve absent V1/V2 JSON plus old profile behavior', () => {
  for (const profile of [undefined, 'standard-v2']) {
    const old = {type: 'doc', ...(profile ? {attrs: {styleProfile: profile}} : {}), content: [{type: 'paragraph'}]};
    const internal = {...old, attrs: {...old.attrs, styleProfile: profile ?? null, pageGeometry: null}};
    expect(JSON.stringify(snapshotDocument({getJSON: () => internal}))).toBe(JSON.stringify(old));
    expect(documentStyleProfile({...old, attrs: {...old.attrs, pageGeometry: geometry()}})).toBe(profile ?? null);
  }
  for (const type of ['paragraph', 'table', 'image', 'text']) expect(() => page.validatePageGeometryDocument({type: 'doc', content: [{type, attrs: {pageGeometry: geometry()}}]})).toThrow('页面');
  expect(() => documentStyleProfile({type: 'doc', attrs: {pageGeometry: null}})).toThrow('页面');
  expect(page.WordPageGeometry.config.addGlobalAttributes()[0].attributes.pageGeometry.parseHTML({style: {width: '100pt'}})).toBeNull();
});

test('page transaction is a single document-attribute step, preserves contents selection and stored marks', () => {
  let state = stateFor(doc()); state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 2, 4)).setStoredMarks([schema.marks.bold.create()]));
  const token = page.capturePageGeometry(state), transaction = page.createPageGeometryTransaction(state, geometry(), token);
  expect(transaction.steps).toHaveLength(1); expect(transaction.doc.content.eq(state.doc.content)).toBe(true);
  expect(transaction.selection.eq(state.selection)).toBe(true); expect(transaction.storedMarks).toEqual(state.storedMarks);
  expect(state.doc.attrs.pageGeometry).toBeNull();
  expect(page.createPageGeometryTransaction(state.apply(transaction), {...geometry()})).toBeNull();
});

test('stale document edits invalidate page tokens while selection-only moves remain valid', () => {
  const state = stateFor(doc()), token = page.capturePageGeometry(state);
  const selected = state.apply(state.tr.setSelection(TextSelection.atEnd(state.doc)));
  expect(page.matchesPageGeometry(selected, token)).toBe(true);
  const changed = state.apply(state.tr.insertText('x', 1));
  expect(page.matchesPageGeometry(changed, token)).toBe(false); expect(page.createPageGeometryTransaction(changed, geometry(), token)).toBeNull();
});

test('apply reset undo redo and draft roundtrip retain exact source twips atomically', () => {
  let state = stateFor(doc()), original = state.doc;
  const layout = geometry({widthTwips: 11907, heightTwips: 16837, marginTopTwips: 1, marginBottomTwips: 321});
  state = state.apply(closeHistory(page.createPageGeometryTransaction(state, layout))); state = state.apply(closeHistory(state.tr));
  const changed = state.doc;
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
  expect(schema.nodeFromJSON(snapshot(changed)).eq(changed)).toBe(true);
  state = state.apply(closeHistory(page.createPageGeometryTransaction(state, null))); state = state.apply(closeHistory(state.tr));
  expect(snapshot(state.doc).attrs).not.toHaveProperty('pageGeometry');
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.attrs.pageGeometry).toEqual(layout);
});

test('existing fixed and positioned automatic tables use page-aware conservative right-edge bound', () => {
  const a5 = page.presetPageGeometry('A5', 'portrait');
  const cases = [table({tableAlign: 'left'}), table({tableIndentTwips: 0}), table({columnWidthsTwips: [5600]}), table({columnWidthsTwips: [5400], tableIndentTwips: 112})];
  for (const value of cases) {
    const state = stateFor(doc(null, [value]));
    expect(() => page.createPageGeometryTransaction(state, a5)).toThrow('超出'); expect(undo(state)).toBe(false); expect(state.doc.attrs.pageGeometry).toBeNull();
  }
  for (const align of ['left', 'center', 'right']) {
    const state = stateFor(doc(null, [table({columnWidthsTwips: [5511], tableAlign: align, tableIndentTwips: 0})]));
    expect(() => page.createPageGeometryTransaction(state, a5)).not.toThrow();
  }
  expect(() => page.validatePageGeometryDocument(snapshot(doc(null, [table({columnWidthsTwips: [31000]})])))).not.toThrow();
});

test('explicit images use exact EMU bounds without scale crop or mutate-on-failure', () => {
  const a5 = page.presetPageGeometry('A5', 'portrait'), max = (a5.widthTwips - 2880) * 635;
  const state = stateFor(doc(null, [image({displayWidthEmu: max, displayHeightEmu: 100})]));
  expect(() => page.createPageGeometryTransaction(state, a5)).not.toThrow();
  for (const attrs of [{displayWidthEmu: max + 1, displayHeightEmu: 100}, {displayWidthEmu: 100, displayHeightEmu: (a5.heightTwips - 2880) * 635 + 1}]) {
    const current = stateFor(doc(null, [image(attrs)]));
    expect(() => page.createPageGeometryTransaction(current, a5)).toThrow('超出'); expect(current.doc.firstChild.attrs).toMatchObject(attrs);
  }
});

test('implicit PNG/JPEG frames resolve bounded intrinsic metadata and explicit-page natural decorations only', () => {
  const media = inspectWordImageMedia(src); expect(media.width).toBeGreaterThan(0); expect(media.height).toBeGreaterThan(0); expect(media.displayError).toBeNull();
  const natural = page.pageImageDimensions({src});
  expect(natural).toEqual({width: Math.round(media.width * Math.min(1, 600 / media.width, 800 / media.height) * 9525), height: Math.round(media.height * Math.min(1, 600 / media.width, 800 / media.height) * 9525)});
  const legacy = stateFor(doc(null, [image()])), explicit = stateFor(doc(geometry(), [image()]));
  expect(legacy.plugins[1].props.decorations(legacy)).toBeNull();
  const decoration = explicit.plugins[1].props.decorations(explicit).find()[0];
  expect(decoration.type.attrs).toEqual(page.pageNaturalImageDOM({src}));
  expect(decoration.type.attrs.style).toContain('--word-image-width:'); expect(snapshot(explicit.doc).content[0].attrs).not.toHaveProperty('displayWidthEmu');
  const reset = explicit.apply(page.createPageGeometryTransaction(explicit, null)); expect(reset.plugins[1].props.decorations(reset)).toBeNull();
  expect(page.pageNaturalImageDOM({displayWidthEmu: 1, displayHeightEmu: 1})).toEqual({});
});

test('page guard blocks direct geometry, paste, table expansion and image growth before history/drafts change', () => {
  const a5 = page.presetPageGeometry('A5', 'portrait'), errors = [];
  const state = EditorState.create({doc: doc(a5, [table({columnWidthsTwips: [5000]}), image({displayWidthEmu: 100, displayHeightEmu: 100})]), plugins: [history(), page.pageGeometryGuard(message => errors.push(message))]});
  for (const transaction of [state.tr.setDocAttribute('pageGeometry', null), state.tr.setDocAttribute('pageGeometry', {...a5, unknown: 1}), state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, columnWidthsTwips: [5600]}),
    state.tr.insert(state.doc.content.size, table({tableAlign: 'left'})), state.tr.setNodeMarkup(state.doc.firstChild.nodeSize, undefined, {...state.doc.child(1).attrs, displayWidthEmu: 4000000})]) {
    if (transaction.doc.attrs.pageGeometry === null) {expect(state.applyTransaction(transaction).transactions).toHaveLength(1); continue;}
    const result = state.applyTransaction(transaction); expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(state)).toBe(false);
  }
  expect(errors).toHaveLength(4);
});

test('page transaction rejects prospective depth text node and draft-byte limits atomically', () => {
  let deep = paragraph('deep'); for (let i = 0; i < 33; i++) deep = schema.node('blockquote', null, [deep]);
  for (const [document, message] of [[doc(null, [deep]), '文档结构'], [doc(null, Array.from({length: 5000}, () => paragraph('x'))), '文档结构'],
    [doc(null, [paragraph('中'.repeat(700000))]), '2 MiB'], [doc(null, [image({alt: 'x'.repeat(32 * 1024 * 1024)})]), '32 MiB']]) {
    const state = stateFor(document); expect(() => page.createPageGeometryTransaction(state, geometry())).toThrow(message); expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

test('physical sheet and print declarations retain exact asymmetry and absent declarations remain empty', () => {
  const value = geometry({widthTwips: 16837, heightTwips: 11907, marginTopTwips: 1, marginRightTwips: 20, marginBottomTwips: 321, marginLeftTwips: 1441});
  expect(page.pageGeometrySheetStyle(value)).toEqual({width: '841.85pt', minWidth: '841.85pt', maxWidth: 'none', minHeight: '595.35pt', padding: '0.05pt 1pt 16.05pt 72.05pt', border: '0'});
  expect(page.pageGeometryPrintCss(value)).toContain('size: 841.85pt 595.35pt; margin: 0.05pt 1pt 16.05pt 72.05pt');
  expect(page.pageGeometryPrintCss(value)).toContain('page: word-explicit-page');
  expect(page.pageGeometrySheetStyle(null)).toEqual({}); expect(page.pageGeometryPrintCss(null)).toBe('');
  expect(() => page.pageGeometryPrintCss({...value, widthTwips: NaN})).toThrow();
});

class Element {
  dataset = {}; attrs = {}; children = []; value = ''; disabled = false; textContent = ''; listeners = new Map();
  style = {removeProperty(property) {delete this[property.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase())];}};
  addEventListener(type, listener) {this.listeners.set(type, [...this.listeners.get(type) ?? [], listener]);}
  emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
  contains(other) {return other?.id?.startsWith('edit-page-');}
  replaceChildren(...children) {this.children = children;}
  removeAttribute(name) {delete this.attrs[name]; if (name === 'data-page-geometry') delete this.dataset.pageGeometry;}
  querySelectorAll() {return [];}
  cloneNode() {const clone = new Element(); clone.attrs = {...this.attrs}; clone.dataset = {...this.dataset}; return clone;}
}
function controlsHarness(run, initial = doc(geometry({widthTwips: 11907, heightTwips: 16837}))) {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) {const node = new Element(); node.id = id; elements.set(id, node);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  let state = stateFor(initial), locked = false, dispatches = 0, controls;
  let editor = {get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); dispatches++; controls.update();}, focus() {}}};
  controls = new WordPageControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message)}); controls.update();
  const change = (tr, notify = true) => {state = state.apply(tr); if (notify) controls.update();};
  try {run({element, notices, controls, change, get state() {return state;}, get dispatches() {return dispatches;}, lock(value) {locked = value; controls.update();},
    replace() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});}
  finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
}

test('browser-free controls preserve source twips during margin-only apply, orientation reversals and undo', () => {
  controlsHarness(h => {
    const margin = h.element('edit-page-margin-left'); margin.emit('focus'); margin.value = '80.05'; h.element('edit-page-apply').emit('click');
    expect(h.state.doc.attrs.pageGeometry).toEqual(geometry({widthTwips: 11907, heightTwips: 16837, marginLeftTwips: 1601}));
    h.element('edit-page-orientation').emit('focus'); h.element('edit-page-orientation').value = 'landscape'; h.element('edit-page-apply').emit('click');
    expect(h.state.doc.attrs.pageGeometry).toMatchObject({widthTwips: 16837, heightTwips: 11907, marginLeftTwips: 1601});
    expect(undo(h.state, h.change)).toBe(true); expect(h.state.doc.attrs.pageGeometry.widthTwips).toBe(11907);
    h.element('edit-page-reset').emit('click'); expect(h.state.doc.attrs.pageGeometry).toBeNull(); expect(h.element('authoring-sheet').style.width).toBeUndefined();
    expect(undo(h.state, h.change)).toBe(true); expect(h.element('authoring-sheet').style.width).toBe('595.35pt');
  });
});

test('browser-free pending fields discard on editor/document switch and Escape; silent stale tokens cannot commit', () => {
  controlsHarness(h => {
    const input = h.element('edit-page-margin-top'); input.emit('focus'); input.value = '99';
    h.change(h.state.tr.insertText('new', 1)); expect(input.value).toBe('72');
    input.emit('focus'); input.value = '99'; input.emit('keydown', {key: 'Escape'}); expect(input.value).toBe('72');
    input.emit('focus'); input.value = '99'; h.replace(); expect(input.value).toBe('72');
    h.change(h.state.tr.insertText('stale', 1), false); h.element('edit-page-apply').emit('click'); h.element('edit-page-reset').emit('click');
    expect(h.dispatches).toBe(0);
  });
});

test('browser-free locks, IME Enter, bad margins and content overflow never mutate or create undo', () => {
  controlsHarness(h => {
    h.lock(true);
    for (const id of ['edit-page-size', 'edit-page-orientation', 'edit-page-margin-top', 'edit-page-margin-right', 'edit-page-margin-bottom', 'edit-page-margin-left', 'edit-page-apply', 'edit-page-reset']) expect(h.element(id).disabled).toBe(true);
    h.element('edit-page-apply').emit('click'); h.element('edit-page-reset').emit('click'); expect(h.dispatches).toBe(0);
    h.lock(false); const top = h.element('edit-page-margin-top'); top.emit('focus'); top.value = '.001'; h.element('edit-page-apply').emit('click');
    expect(h.notices.at(-1)).toContain('0.05 pt'); expect(h.dispatches).toBe(0);
    top.value = '0'; top.emit('keydown', {key: 'Enter', isComposing: true}); expect(h.dispatches).toBe(0);
  });
  controlsHarness(h => {h.element('edit-page-size').emit('focus'); h.element('edit-page-size').value = 'A5'; h.element('edit-page-apply').emit('click'); expect(h.notices.at(-1)).toContain('超出'); expect(h.dispatches).toBe(0);}, doc(null, [table({tableAlign: 'left'})]));
});

test('print snapshot captures immutable exact geometry before async font loading and clears absent page style', async () => {
  const previous = globalThis.document, host = new Element(), source = new Element();
  let ready; const fonts = new Promise(resolve => {ready = resolve;});
  globalThis.document = {getElementById: () => host, createElement: () => new Element(), fonts: {ready: fonts}};
  try {
    const original = geometry({marginLeftTwips: 201}), captured = {...original};
    const pending = preparePrintSnapshot(source, original); original.marginLeftTwips = 99; source.dataset.pageGeometry = 'changed'; ready(); await pending;
    expect(host.children).toHaveLength(2); expect(host.children[0].textContent).toBe(page.pageGeometryPrintCss(captured));
    expect(JSON.parse(host.children[1].dataset.pageGeometry)).toEqual(captured);
    await preparePrintSnapshot(source); expect(host.children).toHaveLength(1); expect(host.children[0].dataset).not.toHaveProperty('pageGeometry');
  } finally {globalThis.document = previous;}
});


test('real Tiptap CommandManager direct chain and can never retain failed page steps in their shared transaction', () => {
  const commands = {setPage: geometry => ({state, tr, dispatch}) => {
    try {
      const candidate = page.createPageGeometryTransaction(state, geometry);
      if (candidate && dispatch) for (const step of candidate.steps) tr.step(step);
      return true;
    } catch {return false;}
  }};
  const harness = initial => {
    let state = stateFor(initial);
    const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr);}}};
    return {manager: new CommandManager({editor}), get state() {return state;}};
  };
  for (const run of [manager => manager.commands.setPage(page.presetPageGeometry('A5', 'portrait')),
    manager => manager.chain().setPage(page.presetPageGeometry('A5', 'portrait')).run(), manager => manager.can().setPage(page.presetPageGeometry('A5', 'portrait')),
    manager => manager.can().chain().setPage(page.presetPageGeometry('A5', 'portrait')).run()]) {
    const current = harness(doc(null, [table({tableIndentTwips: 0})])), before = current.state.doc;
    expect(run(current.manager)).toBe(false); expect(current.state.doc).toBe(before); expect(undo(current.state)).toBe(false);
  }
  const good = harness(doc()); expect(good.manager.can().setPage(geometry())).toBe(true); expect(good.state.doc.attrs.pageGeometry).toBeNull();
  expect(good.manager.chain().setPage(geometry()).run()).toBe(true); expect(good.state.doc.attrs.pageGeometry).toEqual(geometry());
});

test('page natural media fit refuses oversized implicit images and ambiguous metadata while absence remains unchanged', () => {
  const png = Buffer.from(src.split(',')[1], 'base64'); png.writeUInt32BE(600, 16); png.writeUInt32BE(300, 20);
  const sized = `data:image/png;base64,${png.toString('base64')}`;
  const original = stateFor(doc(null, [image({src: sized})]));
  expect(() => page.createPageGeometryTransaction(original, page.presetPageGeometry('A5', 'portrait'))).toThrow('超出');
  expect(original.doc.firstChild.attrs.src).toBe(sized); expect(original.doc.firstChild.attrs.displayWidthEmu).toBeNull();
  const chunk = Buffer.concat([Buffer.from([0, 0, 0, 0]), Buffer.from('eXIf'), Buffer.alloc(4)]);
  const ambiguous = `data:image/png;base64,${Buffer.concat([png.subarray(0, -12), chunk, png.subarray(-12)]).toString('base64')}`;
  const state = stateFor(doc(null, [image({src: ambiguous})]));
  expect(() => page.validatePageGeometryDocument(snapshot(state.doc))).not.toThrow();
  expect(() => page.createPageGeometryTransaction(state, geometry())).toThrow('方向'); expect(undo(state)).toBe(false);
  // Header scan declaration test only; fake JPEG below is deliberately not
  // offered as pixel decoding or actual renderer evidence.
  const jpeg = `data:image/jpeg;base64,${Buffer.from([255,216,255,192,0,8,8,0,30,0,60,1,255,217]).toString('base64')}`;
  expect(inspectWordImageMedia(jpeg)).toMatchObject({width: 60, height: 30, displayError: null});
  expect(page.pageImageDimensions({src: jpeg})).toEqual({width: 60 * 9525, height: 30 * 9525});
});


test('page-aware unpositioned automatic width derives Letter and A5 while preserving auto model and no-page base', () => {
  for (const [preset, orientation, expected] of [['Letter', 'portrait', 8640], ['A5', 'portrait', 5511], ['A4', 'landscape', 9000]]) {
    const layout = {...page.presetPageGeometry(preset, orientation), ...(preset === 'Letter' ? {marginLeftTwips: 1800, marginRightTwips: 1800} : {})}, original = stateFor(doc(null, [table({})]));
    const current = original.apply(page.createPageGeometryTransaction(original, layout));
    expect(page.pageTableWidthTwips(current.doc.firstChild.attrs, layout)).toBe(expected);
    expect(current.doc.firstChild.eq(original.doc.firstChild)).toBe(true);
    expect(current.doc.firstChild.attrs.columnWidthsTwips).toBeNull();
    const decoration = current.plugins[1].props.decorations(current).find()[0];
    expect(decoration.type.attrs).toEqual({'data-page-auto-table': '', style: `--word-table-width: ${expected / 20}pt`});
    const reset = current.apply(page.createPageGeometryTransaction(current, null));
    expect(reset.plugins[1].props.decorations(reset)).toBeNull(); expect(page.pageTableWidthTwips(reset.doc.firstChild.attrs, null)).toBe(9000);
  }
  const a5 = page.presetPageGeometry('A5', 'portrait');
  for (const attrs of [{tableAlign: 'left'}, {tableIndentTwips: 0}, {tableAlign: 'center'}, {tableAlign: 'right'}]) {
    expect(page.pageAutoTableDOM(attrs, a5)).toEqual({}); expect(page.pageTableWidthTwips(attrs, a5)).toBe(9000);
    expect(() => page.createPageGeometryTransaction(stateFor(doc(null, [table(attrs)])), a5)).toThrow('超出');
  }
});

test('page auto decoration recomputes after page changes and clears on fixed or positioned conversion and undo', () => {
  let current = stateFor(doc(null, [table({})]));
  current = current.apply(closeHistory(page.createPageGeometryTransaction(current, {...page.presetPageGeometry('Letter', 'portrait'), marginLeftTwips: 1800, marginRightTwips: 1800})));
  current = current.apply(closeHistory(current.tr));
  const get = () => current.plugins[1].props.decorations(current)?.find() ?? [];
  expect(get()[0].type.attrs.style).toContain('432pt');
  current = current.apply(closeHistory(page.createPageGeometryTransaction(current, page.presetPageGeometry('A5', 'portrait')))); current = current.apply(closeHistory(current.tr));
  expect(get()[0].type.attrs.style).toContain('275.55pt');
  expect(undo(current, tr => {current = current.apply(tr);})).toBe(true); expect(get()[0].type.attrs.style).toContain('432pt');
  current = current.apply(current.tr.setNodeMarkup(0, undefined, {...current.doc.firstChild.attrs, columnWidthsTwips: [5000]}));
  expect(get()).toEqual([]);
  current = current.apply(current.tr.setNodeMarkup(0, undefined, {...current.doc.firstChild.attrs, columnWidthsTwips: null}));
  expect(get()[0].type.attrs.style).toContain('432pt');
});

test('explicit pages refuse nested automatic tables without changing legacy nesting or first table dimensions', () => {
  const nested = schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableCell', null, [paragraph('outer'), table({})])])]);
  const current = stateFor(doc(null, [nested]));
  expect(() => page.validatePageGeometryDocument(snapshot(current.doc))).not.toThrow();
  expect(() => page.createPageGeometryTransaction(current, geometry())).toThrow('嵌套表格');
  expect(current.doc.firstChild).toBe(nested); expect(undo(current)).toBe(false);
  const listed = schema.node('bulletList', null, [schema.node('listItem', null, [paragraph('item'), table({})])]);
  expect(() => page.createPageGeometryTransaction(stateFor(doc(null, [listed])), geometry())).toThrow('正文顶层表格');
});


test('first fixed-column edit materializes the page-derived automatic grid with exact remainder and validates before mutation', () => {
  const cells = ['first', 'second'].map(text => schema.node('tableCell', null, [paragraph(text)]));
  const auto = schema.node('table', null, [schema.node('tableRow', null, cells)]);
  const a5 = page.presetPageGeometry('A5', 'portrait');
  let current = stateFor(doc(a5, [auto])); current = current.apply(current.tr.setSelection(TextSelection.create(current.doc, 4)));
  const changed = createTableColumnWidthTransaction(current, 2500);
  expect(changed.doc.firstChild.attrs.columnWidthsTwips).toEqual([2500, 2755]);
  expect(changed.doc.firstChild.textContent).toBe('firstsecond'); expect(changed.selection.eq(current.selection)).toBe(true);
  expect(() => createTableColumnWidthTransaction(current, 3000)).toThrow('超出'); expect(undo(current)).toBe(false);
  const legacy = stateFor(doc(null, [auto]));
  const selected = legacy.apply(legacy.tr.setSelection(TextSelection.create(legacy.doc, 4)));
  expect(createTableColumnWidthTransaction(selected, 2500).doc.firstChild.attrs.columnWidthsTwips).toEqual([2500, 4500]);
  const exact = {...a5, marginRightTwips: 720}; // 6231 split = 3116 + 3115
  let wide = stateFor(doc(exact, [auto])); wide = wide.apply(wide.tr.setSelection(TextSelection.create(wide.doc, 4)));
  expect(createTableColumnWidthTransaction(wide, 3116).doc.firstChild.attrs.columnWidthsTwips).toEqual([3116, 3115]);
});

test('table and image helper candidates enforce page bounds before direct chain or can dispatch', () => {
  const commands = WordTable.config.addCommands.call({options: {onWidthError() {}}, parent: () => ({})});
  for (const run of [m => m.commands.setTablePosition({tableAlign: 'left'}), m => m.chain().setTablePosition({tableAlign: 'left'}).run(),
    m => m.can().setTablePosition({tableAlign: 'left'}), m => m.can().chain().setTablePosition({tableAlign: 'left'}).run()]) {
    let state = stateFor(doc(page.presetPageGeometry('A5', 'portrait'), [table({})])); state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 4)));
    const original = state.doc, editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr);}}};
    expect(run(new CommandManager({editor}))).toBe(false); expect(state.doc).toBe(original); expect(undo(state)).toBe(false);
  }
  let state = stateFor(doc(page.presetPageGeometry('A5', 'portrait'), [table({columnWidthsTwips: [5000]})])); state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 4)));
  expect(() => createTableStructureTransaction(state, 'addColumnAfter')).toThrow('超出');
  expect(() => createTablePositionTransaction(state, {tableIndentTwips: 512})).toThrow('超出');
  let picture = stateFor(doc(page.presetPageGeometry('A5', 'portrait'), [image({displayWidthEmu: 100, displayHeightEmu: 100})]));
  picture = picture.apply(picture.tr.setSelection(NodeSelection.create(picture.doc, 0)));
  expect(() => createImageDimensionsTransaction(picture, {displayWidthEmu: 4000000, displayHeightEmu: 100})).toThrow('超出');
  expect(picture.doc.firstChild.attrs.displayWidthEmu).toBe(100); expect(undo(picture)).toBe(false);
});
