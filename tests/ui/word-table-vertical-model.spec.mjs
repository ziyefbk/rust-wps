import {test, expect} from '@playwright/test';
import {CommandManager, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, tableEditing, mergeCells, toggleHeaderCell, toggleHeaderRow} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
const vertical = await loadTsModule('./ui/word-table-vertical-layout.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const {WordTableVerticalControls} = await loadTsModule('./ui/word-table-vertical-controls.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const schema = getSchema([StarterKit, TableKit.configure({table: false}), tables.WordTable, page.WordPageGeometry]);
const p = (text = 'cell') => schema.node('paragraph', null, text ? schema.text(text) : null);
const table = (heights = [null, null], aligns = [null, null, null, null, null, null], attrs = {}) => schema.node('table', attrs,
  heights.map((height, row) => schema.node('tableRow', {rowMinHeightTwips: height}, [0, 1, 2].map(column =>
    schema.node(row ? 'tableCell' : 'tableHeader', {cellVerticalAlign: aligns[row * 3 + column]}, p(`${row}:${column}`))))));
const doc = (nodes = [table()], attrs = {}) => schema.node('doc', attrs, nodes);
const snap = node => page.omitDefaultPageGeometry(tables.omitDefaultTableWidths(node.toJSON()));
const positions = (document, names) => {const found = []; document.descendants((node, pos) => {if (names.includes(node.type.name)) found.push(pos);}); return found;};
const cells = document => positions(document, ['tableCell', 'tableHeader']);
const stateAt = (document = doc(), index = 0) => EditorState.create({doc: document, selection: TextSelection.create(document, cells(document)[index] + 2), plugins: [history(), tableEditing(), tables.tableWidthGuard(), page.pageGeometryGuard()]});
const choose = (state, selection) => state.apply(state.tr.setSelection(selection));
const apply = (state, patch) => state.apply(closeHistory(tables.createTableVerticalTransaction(state, patch)));
const heights = state => Array.from({length: state.doc.firstChild.childCount}, (_, i) => state.doc.firstChild.child(i).attrs.rowMinHeightTwips);
const aligns = state => cells(state.doc).map(pos => state.doc.nodeAt(pos).attrs.cellVerticalAlign);

// Browser-free tests use real ProseMirror/Tiptap schemas, commands, validation,
// histories and DOM parser/serializer with tiny shims. They prove no layout,
// focus, native clipboard, pagination or real browser rendering claim.
test('vertical scalar metadata and exact point parsing preserve zero top and all integer twips', () => {
  for (const value of [0, 1, 13958, 31680]) expect(vertical.isRowMinHeight(value)).toBe(true);
  for (const value of [null, undefined, false, '0', -1, .5, 31681, NaN, Infinity, {}, []]) expect(vertical.isRowMinHeight(value)).toBe(false);
  for (const value of ['top', 'center', 'bottom']) expect(vertical.isCellVerticalAlignment(value)).toBe(true);
  for (const value of [null, '', 'middle', 'both', 'TOP', ' top', 0, false, {}]) expect(vertical.isCellVerticalAlignment(value)).toBe(false);
  for (let value = 0; value <= 31680; value++) if (vertical.parseRowMinHeightPoints(String(value / 20)) !== value) throw new Error(`Changed ${value}`);
  for (const value of ['', '-0', '-1', '+1', '.001', '1e2', '1pt', '1584.05', '0.05000000000000001']) expect(vertical.parseRowMinHeightPoints(value)).toBeNull();
  for (const value of [null, '', '-1', '1.5', '31681', '1e2', ' 0', '0pt']) expect(vertical.parseRowMinHeightMetadata(value)).toBeNull();
});

test('raw JSON rejects null malformed wrong-node attrs and unresolved structure before schema defaults', () => {
  for (const [field, values] of [['rowMinHeightTwips', [null, undefined, false, '0', -1, .5, 31681]], ['cellVerticalAlign', [null, undefined, '', false, 'middle', 'both']]]) {
    for (const value of values) {
      const raw = snap(doc()), target = field === 'rowMinHeightTwips' ? raw.content[0].content[0] : raw.content[0].content[0].content[0];
      target.attrs = {...target.attrs, [field]: value}; expect(() => tables.validateTableWidthsDocument(raw)).toThrow('行最小高度');
    }
    for (const type of ['doc', 'table', 'paragraph', 'text', field === 'rowMinHeightTwips' ? 'tableCell' : 'tableRow']) {
      expect(() => tables.validateTableWidthsDocument({type: 'doc', content: [{type, attrs: {[field]: field === 'rowMinHeightTwips' ? 0 : 'top'}}]})).toThrow('行最小高度');
    }
  }
  for (const mutate of [raw => raw.content[0].content[1].content.pop(), raw => {raw.content[0].content[0].content[0].attrs.colspan = 2;},
    raw => {raw.content[0].content[0].content[0].attrs.rowspan = 2;}, raw => raw.content[0].content[0].content[0].content.push(snap(table()))]) {
    const raw = snap(doc([table([0, null])])); mutate(raw); expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
  }
  const listed = schema.node('bulletList', null, schema.node('listItem', null, [p('list'), table([0, null])]));
  expect(() => tables.validateTableWidthsDocument(snap(doc([listed])))).toThrow();
  const old = snap(doc()); old.content[0].content[0].content[0].content.push(snap(table()));
  expect(() => tables.validateTableWidthsDocument(old)).not.toThrow();
  old.content[0].content[0].content[0].content[1].content[0].attrs = {rowMinHeightTwips: 0};
  expect(() => tables.validateTableWidthsDocument(old)).toThrow();
});

test('row minima fit actual page body and page shrinking/reset is checked atomically', () => {
  expect(() => tables.validateTableWidthsDocument(snap(doc([table([13958, 0])])))).not.toThrow();
  expect(() => tables.validateTableWidthsDocument(snap(doc([table([13959, 0])])))).toThrow('页面正文高度');
  const a5 = page.presetPageGeometry('A5', 'portrait'), limit = a5.heightTwips - a5.marginTopTwips - a5.marginBottomTwips;
  expect(() => tables.validateTableWidthsDocument(snap(doc([table([limit, 0])], {pageGeometry: a5})))).not.toThrow();
  expect(() => tables.validateTableWidthsDocument(snap(doc([table([limit + 1, 0])], {pageGeometry: a5})))).toThrow();
  const state = stateAt(doc([table([limit + 1, null])]));
  expect(() => page.createPageGeometryTransaction(state, a5)).toThrow('页面正文高度'); expect(undo(state)).toBe(false);
  expect(state.applyTransaction(state.tr.setDocAttribute('pageGeometry', a5)).transactions).toEqual([]);
  const large = {...page.presetPageGeometry('A4', 'portrait'), marginTopTwips: 0, marginBottomTwips: 0};
  expect(() => page.createPageGeometryTransaction(stateAt(doc([table([15000, 0])], {pageGeometry: large})), null)).toThrow();
});

test('legacy row cell and complete document JSON DOM stay unchanged with all new fields absent', () => {
  const legacy = getSchema([StarterKit, TableKit]);
  const raw = {type: 'doc', content: [{type: 'table', content: [{type: 'tableRow', content: [{type: 'tableCell', content: [{type: 'paragraph'}]}]}]}]};
  const old = legacy.nodeFromJSON(raw), restored = schema.nodeFromJSON(old.toJSON());
  expect(JSON.stringify(snap(restored))).toBe(JSON.stringify(old.toJSON()));
  for (const [oldNode, next] of [[old.firstChild.firstChild, restored.firstChild.firstChild], [old.firstChild.firstChild.firstChild, restored.firstChild.firstChild.firstChild]]) {
    expect(schema.nodes[next.type.name].spec.toDOM(next)).toEqual(legacy.nodes[oldNode.type.name].spec.toDOM(oldNode));
  }
  expect(vertical.rowMinHeightDOM({})).toEqual({}); expect(vertical.cellVerticalAlignDOM({})).toEqual({});
});

test('caret and text ranges target current rows and actually touched cells in either direction', () => {
  const document = doc(), cell = cells(document);
  for (const [from, to, rows, count] of [[cell[1] + 2, cell[1] + 2, 1, 1], [cell[1] + 2, cell[4] + 3, 2, 4],
    [cell[4] + 3, cell[1] + 2, 2, 4], [cell[0] + 2, cell[1] + 2, 1, 1]]) {
    const state = choose(stateAt(document), TextSelection.create(document, from, to)), selected = tables.selectedTableVerticalLayout(state);
    expect(selected.rows).toHaveLength(rows); expect(selected.cells).toHaveLength(count);
    const transaction = tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 720, cellVerticalAlign: 'center'});
    expect(transaction.selection.eq(state.selection)).toBe(true); expect(transaction.steps).toHaveLength(rows + count);
    expect(transaction.doc.textContent).toBe(document.textContent);
  }
});

test('rectangular CellSelection and table row cell node selections retain exact kind and scope', () => {
  const document = doc(), cell = cells(document), row = positions(document, ['tableRow']);
  for (const [selection, rows, count] of [[CellSelection.create(document, cell[1], cell[5]), 2, 4], [CellSelection.create(document, cell[5], cell[1]), 2, 4],
    [NodeSelection.create(document, 0), 2, 6], [NodeSelection.create(document, row[1]), 1, 3], [NodeSelection.create(document, cell[3]), 1, 1]]) {
    const state = choose(stateAt(document), selection), selected = tables.selectedTableVerticalLayout(state);
    expect(selected.rows).toHaveLength(rows); expect(selected.cells).toHaveLength(count);
    expect(tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0, cellVerticalAlign: 'top'}).selection.eq(state.selection)).toBe(true);
  }
});

test('outside all cross-table and unsupported legacy geometry disable edits', () => {
  const document = doc([p('before'), table(), p('between'), table(), p('after')]), cell = cells(document);
  for (const selection of [TextSelection.create(document, 2), new AllSelection(document), TextSelection.create(document, cell[0] + 2, cell.at(-1) + 3)]) {
    const state = choose(stateAt(document), selection); expect(tables.selectedTableVerticalLayout(state)).toBeNull();
    expect(tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0})).toBeNull();
  }
  const raw = snap(doc()); raw.content[0].content[0].content[0].attrs.colspan = 2;
  const unsupported = stateAt(schema.nodeFromJSON(raw)); expect(tables.selectedTableVerticalLayout(unsupported).supported).toBe(false);
  expect(() => tables.createTableVerticalTransaction(unsupported, {cellVerticalAlign: 'center'})).toThrow();
});

test('mixed states distinguish implicit default explicit zero and explicit top without editing neighboring cells', () => {
  let state = stateAt(doc([table([0, null], ['top', null, 'bottom', 'center', 'top', null])]));
  state = choose(state, NodeSelection.create(state.doc, 0));
  const selected = tables.selectedTableVerticalLayout(state);
  expect(selected.rowMinHeightTwips).toEqual({value: null, mixed: true, hasExplicit: true});
  expect(selected.cellVerticalAlign).toEqual({value: null, mixed: true, hasExplicit: true});
  state = apply(state, {rowMinHeightTwips: 0}); expect(heights(state)).toEqual([0, 0]);
  expect(aligns(state)).toEqual(['top', null, 'bottom', 'center', 'top', null]);
  state = choose(state, NodeSelection.create(state.doc, cells(state.doc)[1])); state = apply(state, {cellVerticalAlign: 'top'});
  expect(aligns(state)).toEqual(['top', 'top', 'bottom', 'center', 'top', null]);
});

test('combined patch draft roundtrip no-op reset one undo redo preserves text marks and selection', () => {
  let state = stateAt(); state = choose(state, NodeSelection.create(state.doc, 0));
  state = state.apply(state.tr.setStoredMarks([schema.marks.bold.create()])); const before = state.doc;
  const transaction = tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0, cellVerticalAlign: 'top'});
  expect(transaction.storedMarks).toEqual(state.storedMarks); state = state.apply(closeHistory(transaction)); state = state.apply(closeHistory(state.tr));
  const changed = state.doc, saved = snap(changed); tables.validateTableWidthsDocument(saved); expect(snap(schema.nodeFromJSON(saved))).toEqual(saved);
  expect(tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0, cellVerticalAlign: 'top'})).toBeNull();
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
  state = apply(state, {rowMinHeightTwips: null, cellVerticalAlign: null}); expect(snap(state.doc)).toEqual(snap(before));
});

test('new row/column defaults never duplicate neighboring vertical declarations and surviving attrs remain intact', () => {
  for (const action of ['addRowBefore', 'addRowAfter', 'addColumnBefore', 'addColumnAfter', 'deleteRow', 'deleteColumn']) {
    const state = stateAt(doc([table([720, 960], ['top', 'center', 'bottom', 'bottom', 'center', 'top'], {columnWidthsTwips: [1000, 1200, 1400], tableCellMarginsTwips: {left: 0, right: 0}})]), 4);
    let next = state.apply(closeHistory(tables.createTableStructureTransaction(state, action)));
    tables.validateTableWidthsDocument(snap(next.doc));
    const nextTable = next.doc.firstChild;
    expect(nextTable.attrs.tableCellMarginsTwips).toEqual({left: 0, right: 0});
    if (action.startsWith('addRow')) {
      const at = action === 'addRowBefore' ? 1 : 2; expect(nextTable.child(at).attrs.rowMinHeightTwips).toBeNull();
      nextTable.child(at).forEach(cell => expect(cell.attrs.cellVerticalAlign).toBeNull());
      expect(heights(next).filter(value => value !== null)).toEqual([720, 960]);
    }
    if (action.startsWith('addColumn')) {
      const at = action === 'addColumnBefore' ? 1 : 2;
      expect(heights(next)).toEqual([720, 960]); nextTable.forEach(row => expect(row.child(at).attrs.cellVerticalAlign).toBeNull());
    }
    if (action === 'deleteRow') expect(heights(next)).toEqual([720]);
    if (action === 'deleteColumn') expect(aligns(next)).toEqual(['top', 'bottom', 'bottom', 'top']);
    expect(undo(next, tr => {next = next.apply(tr);})).toBe(true); expect(next.doc.eq(state.doc)).toBe(true);
  }
});

test('header toggles preserve authored row minima and cell alignment while type changes remain undoable', () => {
  for (const command of [toggleHeaderCell, toggleHeaderRow]) {
    let state = stateAt(doc([table([720, 960], ['top', 'center', 'bottom', 'bottom', 'center', 'top'])]), 4), before = state.doc;
    expect(command(state, tr => {state = state.apply(closeHistory(tr));})).toBe(true);
    expect(heights(state)).toEqual([720, 960]); expect(aligns(state)).toEqual(['top', 'center', 'bottom', 'bottom', 'center', 'top']);
    tables.validateTableWidthsDocument(snap(state.doc)); expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
  }
});

test('invalid or oversized candidate cannot leak into shared chains history or draft writes', () => {
  const errors = [], commands = tables.WordTable.config.addCommands.call({options: {onWidthError: message => errors.push(message)}, parent: () => ({})});
  const harness = () => {
    let state = stateAt(), writes = 0; const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++;}}};
    return {manager: new CommandManager({editor}), get state() {return state;}, get writes() {return writes;}};
  };
  const good = harness(); expect(good.manager.can().setTableVerticalLayout({rowMinHeightTwips: 0})).toBe(true); expect(good.writes).toBe(0);
  expect(good.manager.chain().setTableVerticalLayout({rowMinHeightTwips: 720, cellVerticalAlign: 'center'}).addRowAfter().run()).toBe(true); expect(good.writes).toBe(1);
  expect(good.manager.commands.resetTableVerticalLayout()).toBe(true);
  for (const run of [manager => manager.commands.setTableVerticalLayout({rowMinHeightTwips: 13959}), manager => manager.chain().setTableVerticalLayout({cellVerticalAlign: 'both'}).run(),
    manager => manager.can().setTableVerticalLayout({rowMinHeightTwips: -1}), manager => manager.can().chain().setTableVerticalLayout({rowMinHeightTwips: 31681}).run()]) {
    const bad = harness(); expect(run(bad.manager)).toBe(false); expect(bad.writes).toBe(0); expect(undo(bad.state)).toBe(false);
  }
  expect(errors).toHaveLength(2);
  const state = stateAt(), token = tables.captureTableCellMargins(state), stale = state.apply(state.tr.insertText('x'));
  expect(tables.createTableVerticalTransaction(stale, {rowMinHeightTwips: 720}, token)).toBeNull();
  for (const patch of [{bad: 0}, {rowMinHeightTwips: undefined}, {cellVerticalAlign: false}]) expect(() => tables.createTableVerticalTransaction(state, patch)).toThrow();
  expect(state.tr.steps).toEqual([]); expect(undo(state)).toBe(false);
});

class Element {
  constructor(tag = 'div') {this.tag = tag;}
  dataset = {}; attrs = {}; children = []; value = ''; placeholder = ''; disabled = false; textContent = ''; validity = {badInput: false}; listeners = new Map(); parentNode = null;
  style = {width: '', minWidth: '', tableLayout: '', marginLeft: '', marginRight: '', setProperty(key, value) {this[key] = value;}};
  ownerDocument = {createElement: tag => new Element(tag)}; classList = {remove() {}};
  get firstChild() {return this.children[0] ?? null;}
  get nextSibling() {return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null;}
  addEventListener(type, listener) {this.listeners.set(type, [...this.listeners.get(type) ?? [], listener]);}
  emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
  contains(other) {return other?.id?.startsWith('edit-row-min-height') || other?.id?.startsWith('edit-cell-vertical-align');}
  appendChild(child) {child.parentNode = this; this.children.push(child); return child;}
  removeChild(child) {this.children.splice(this.children.indexOf(child), 1); child.parentNode = null;}
  replaceChildren(...children) {this.children = []; children.forEach(child => this.appendChild(child));}
  setAttribute(name, value) {this.attrs[name] = value;}
  removeAttribute(name) {delete this.attrs[name];}
  querySelectorAll() {return [];}
  cloneNode(deep = true) {const copy = new Element(this.tag); copy.attrs = {...this.attrs}; Object.assign(copy.style, this.style); if (deep) this.children.forEach(child => copy.appendChild(child.cloneNode())); return copy;}
}

class ParseElement {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml';
  constructor(tag = 'div', attrs = {}) {
    this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.childNodes = [];
    this.style = new Proxy({length: 0}, {get: (object, key) => {
      if (key in object) return object[key];
      const property = String(key).replace(/[A-Z]/g, ch => `-${ch.toLowerCase()}`);
      const value = (this.attrs.style ?? '').split(';').map(part => part.split(':')).find(([name]) => name.trim() === property);
      return value?.[1]?.trim() ?? '';
    }});
  }
  get children() {return this.childNodes.filter(node => node.nodeType === 1);}
  get parentElement() {return this.parentNode?.nodeType === 1 ? this.parentNode : null;}
  closest(selector) {for (let node = this; node; node = node.parentElement) if (node.matches(selector)) return node; return null;}
  querySelectorAll(selector) {
    const result = [];
    const visit = node => {for (const child of node.children) {if (selector === 'colgroup > col' ? child.tagName === 'COL' && child.parentElement?.tagName === 'COLGROUP' : child.matches(selector)) result.push(child); visit(child);}};
    visit(this); return result;
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
const serializeNode = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: parseDocument});
const parseNode = element => {const root = new ParseElement(); root.appendChild(element); return ProseMirrorDOMParser.fromSchema(schema).parse(root);};

function controlsHarness(run, initial = doc([table(), p('after'), table([720, 960], ['top', 'center', 'bottom', 'bottom', 'center', 'top'])])) {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map();
  const element = id => {if (!elements.has(id)) {const node = new Element(); node.id = id; elements.set(id, node);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  let state = stateAt(initial), writes = 0, locked = false, pendingSelection, controls;
  const notices = []; let editor = {get state() {return state;}, view: {focus() {}, dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++; controls.update();}}};
  controls = new WordTableVerticalControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message),
    syncSelection: () => {if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); controls.update(); pendingSelection = null;}}}); controls.update();
  const change = (tr, notify = true) => {state = state.apply(tr); if (notify) controls.update();};
  const fill = value => {const input = element('edit-row-min-height'); input.emit('pointerdown'); input.emit('focus'); input.value = value; input.emit('input');};
  const align = value => {const select = element('edit-cell-vertical-align'); select.emit('pointerdown'); select.value = value; select.emit('change');};
  try {run({element, notices, controls, change, fill, align, get state() {return state;}, get writes() {return writes;}, lock(value) {locked = value; controls.update();},
    select(selection) {pendingSelection = selection;}, replace() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});}
  finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
}

test('browser-free controls apply current row minimum and current cell alignment separately with zero top and undo', () => {
  controlsHarness(h => {
    const button = h.element('edit-row-min-height-apply'); button.emit('click'); expect(h.writes).toBe(0);
    h.fill('0'); button.emit('click'); expect(h.writes).toBe(1); expect(heights(h.state)).toEqual([0, null]);
    expect(h.element('edit-row-min-height').dataset.heightState).toBe('explicit');
    h.align('top'); expect(h.writes).toBe(2); expect(aligns(h.state)[0]).toBe('top');
    expect(h.element('edit-cell-vertical-align').dataset.alignState).toBe('explicit');
    expect(undo(h.state, h.change)).toBe(true); expect(heights(h.state)).toEqual([0, null]); expect(aligns(h.state)[0]).toBeNull();
    expect(redo(h.state, h.change)).toBe(true); h.element('edit-row-min-height-reset').emit('click'); expect(heights(h.state)).toEqual([null, null]);
    expect(aligns(h.state)[0]).toBe('top'); h.element('edit-cell-vertical-align-reset').emit('click'); expect(aligns(h.state)[0]).toBeNull();
    expect(h.element('edit-row-min-height-reset').disabled).toBe(true); expect(h.element('edit-cell-vertical-align-reset').disabled).toBe(true);
  });
});

test('browser-free mixed values are visible and untouched Apply does not reset selected row minima', () => {
  controlsHarness(h => {
    h.change(h.state.tr.setSelection(CellSelection.create(h.state.doc, cells(h.state.doc)[0], cells(h.state.doc)[5])));
    expect(h.element('edit-row-min-height').dataset.heightState).toBe('mixed'); expect(h.element('edit-row-min-height').placeholder).toBe('混合');
    expect(h.element('edit-cell-vertical-align').value).toBe('mixed');
    h.element('edit-row-min-height-apply').emit('click'); expect(h.writes).toBe(0);
    h.element('edit-row-min-height').emit('focus'); h.element('edit-row-min-height').emit('keydown', {key: 'Enter'}); expect(h.writes).toBe(0);
    h.fill('18.05'); h.element('edit-row-min-height').emit('keydown', {key: 'Enter'}); expect(heights(h.state)).toEqual([361, 361]);
    expect(aligns(h.state)).toEqual(['top', null, 'bottom', null, 'center', null]);
    h.align('center'); expect(aligns(h.state)).toEqual(Array(6).fill('center'));
    h.fill(''); h.element('edit-row-min-height-apply').emit('click'); expect(heights(h.state)).toEqual([null, null]);
  }, doc([table([0, null], ['top', null, 'bottom', null, 'center', null])]));
});

test('browser-free height validation rejects bad-input fractional overflow and invalid alignment without mutation', () => {
  controlsHarness(h => {
    for (const value of ['.001', '-1', '698', '1584.05', '1e2']) {h.fill(value); h.element('edit-row-min-height-apply').emit('click'); expect(h.writes).toBe(0);}
    h.fill('0'); h.element('edit-row-min-height').validity.badInput = true; h.element('edit-row-min-height-apply').emit('click'); expect(h.writes).toBe(0);
    h.align('middle'); h.align('mixed'); expect(h.writes).toBe(0); expect(undo(h.state)).toBe(false); expect(h.notices).toHaveLength(6);
  });
});

test('browser-free controls cancel pending values on Escape focus exit selection document owner changes and ignore stale tokens IME', () => {
  controlsHarness(h => {
    const input = h.element('edit-row-min-height'); h.fill('18'); input.emit('keydown', {key: 'Enter', isComposing: true}); input.emit('keydown', {key: 'Enter', keyCode: 229}); expect(h.writes).toBe(0);
    input.emit('keydown', {key: 'Escape'}); expect(input.value).toBe('');
    h.fill('18'); h.element('table-vertical-controls').emit('focusout', {relatedTarget: h.element('edit-cell-vertical-align')}); expect(input.value).toBe('18');
    h.element('table-vertical-controls').emit('focusout', {relatedTarget: null}); expect(input.value).toBe('');
    h.fill('18'); h.change(h.state.tr.setSelection(TextSelection.create(h.state.doc, cells(h.state.doc)[1] + 2))); expect(input.value).toBe('');
    h.fill('18'); h.change(h.state.tr.insertText('new')); expect(input.value).toBe('');
    h.fill('18'); h.replace(); expect(input.value).toBe('');
    h.fill('18'); h.change(h.state.tr.insertText('stale'), false);
    h.element('edit-row-min-height-apply').emit('click'); h.element('edit-row-min-height-reset').emit('click'); h.align('top'); expect(h.writes).toBe(0);
  });
});

test('browser-free controls resync pending DOM selection and enforce lock outside and cross-table disabled state', () => {
  controlsHarness(h => {
    h.select(TextSelection.create(h.state.doc, cells(h.state.doc)[7] + 2)); h.fill('1'); h.element('edit-row-min-height-apply').emit('click');
    expect(h.state.doc.firstChild.firstChild.attrs.rowMinHeightTwips).toBeNull(); expect(h.state.doc.lastChild.firstChild.attrs.rowMinHeightTwips).toBe(20);
    h.align('bottom'); expect(h.state.doc.lastChild.firstChild.child(1).attrs.cellVerticalAlign).toBe('bottom');
    h.lock(true); for (const id of ['edit-row-min-height', 'edit-row-min-height-apply', 'edit-row-min-height-reset', 'edit-cell-vertical-align', 'edit-cell-vertical-align-reset']) expect(h.element(id).disabled).toBe(true);
    h.fill('18'); h.element('edit-row-min-height-apply').emit('click'); h.align('top'); expect(h.writes).toBe(2); h.lock(false);
    h.change(h.state.tr.setSelection(new AllSelection(h.state.doc))); expect(h.element('edit-row-min-height-apply').disabled).toBe(true);
    h.element('edit-row-min-height-apply').emit('click'); expect(h.writes).toBe(2);
  });
});

test('real DOM serializer/parser preserves exact minima alignments and explicit zero without raw attrs leaking', () => {
  for (const [height, alignment] of [[0, 'top'], [1, 'center'], [13958, 'bottom']]) {
    const original = table([height, null], [alignment, null, null, null, null, null]), element = serializeNode(original), parsed = parseNode(element);
    expect(parsed.firstChild.eq(original)).toBe(true); tables.validateTableWidthsDocument(snap(parsed));
    const row = element.children.find(child => child.tagName === 'TBODY').firstChild;
    expect(row.attrs['data-row-min-height-twips']).toBe(String(height)); expect(row.style.cssText).toBe(`height: ${height / 20}pt`);
    expect(row.firstChild.attrs['data-cell-vertical-align']).toBe(alignment); expect(row.firstChild.style.cssText).toBe(`vertical-align: ${alignment === 'center' ? 'middle' : alignment}`);
    expect(row.attrs).not.toHaveProperty('rowMinHeightTwips'); expect(row.firstChild.attrs).not.toHaveProperty('cellVerticalAlign');
  }
});

test('malformed owned height and alignment metadata survives schema parsing as invalid evidence and rejects paste transaction', () => {
  for (const [field, values] of [['data-row-min-height-twips', ['', '-1', '1.5', '31681', '1e2', ' 0', '0pt', 'null']], ['data-cell-vertical-align', ['', 'middle', 'both', 'TOP', 'top ', 'null']]]) {
    for (const value of values) {
      const element = serializeNode(table()), row = element.children.find(child => child.tagName === 'TBODY').firstChild;
      (field.includes('row-') ? row : row.firstChild).setAttribute(field, value);
      const parsed = parseNode(element), state = stateAt(); expect(() => tables.validateTableWidthsDocument(snap(parsed))).toThrow();
      expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]); expect(undo(state)).toBe(false);
    }
  }
});

test('owned metadata on wrong HTML elements rejects before parser can silently erase it', () => {
  for (const [field, value, tags] of [['data-row-min-height-twips', '0', ['p', 'td', 'th', 'table']], ['data-cell-vertical-align', 'top', ['p', 'tr', 'table', 'span']]]) {
    for (const tag of tags) {
      const root = new ParseElement(); root.appendChild(new ParseElement(tag, {[field]: value})); expect(vertical.validTableVerticalHTML(root)).toBe(false);
      const previous = globalThis.DOMParser, errors = []; globalThis.DOMParser = class {parseFromString() {return root;}};
      try {expect(tables.tableWidthGuard(error => errors.push(error)).props.handlePaste(null, {clipboardData: {getData: () => `<${tag} ${field.toUpperCase()}="${value}">x</${tag}>`}})).toBe(true); expect(errors).toHaveLength(1);}
      finally {globalThis.DOMParser = previous;}
    }
  }
});

test('foreign CSS exact heights valign and styles are never silently promoted to canonical metadata', () => {
  const element = serializeNode(table()), row = element.children.find(child => child.tagName === 'TBODY').firstChild;
  row.setAttribute('style', 'height:36pt;max-height:36pt;overflow:hidden'); row.firstChild.setAttribute('style', 'vertical-align:middle'); row.firstChild.setAttribute('valign', 'bottom');
  const parsed = parseNode(element); expect(parsed.firstChild.firstChild.attrs.rowMinHeightTwips).toBeNull(); expect(parsed.firstChild.firstChild.firstChild.attrs.cellVerticalAlign).toBeNull();
  const output = serializeNode(parsed.firstChild); expect(output.children.find(child => child.tagName === 'TBODY').firstChild.attrs).toEqual({});
});

test('minimum height and alignment printing snapshot stays immutable across later reset and font await', async () => {
  const previous = globalThis.document, host = new Element(); let ready;
  globalThis.document = {createElement: tag => new Element(tag), getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const source = new Element(), row = new Element('tr'), cell = new Element('td'); source.appendChild(row); row.appendChild(cell);
    Object.assign(row.attrs, vertical.rowMinHeightDOM({rowMinHeightTwips: 721})); Object.assign(cell.attrs, vertical.cellVerticalAlignDOM({cellVerticalAlign: 'center'}));
    const pending = preparePrintSnapshot(source); row.attrs = {}; cell.attrs = {}; ready(); await pending;
    expect(host.firstChild.firstChild.attrs).toEqual({'data-row-min-height-twips': '721', style: 'height: 36.05pt'});
    expect(host.firstChild.firstChild.firstChild.attrs).toEqual({'data-cell-vertical-align': 'center', style: 'vertical-align: middle'});
    await preparePrintSnapshot(source); expect(host.firstChild.firstChild.attrs).toEqual({}); expect(host.firstChild.firstChild.firstChild.attrs).toEqual({});
  } finally {globalThis.document = previous;}
});

test('shared table formatting CSS contract permits natural content growth and retains default legacy top alignment', () => {
  const css = readFileSync('ui/style.css', 'utf8'), html = readFileSync('ui/index.html', 'utf8');
  expect(css).toContain('.tiptap tr[data-row-min-height-twips] { display:table-row; }');
  expect(css).toContain('.tiptap td[data-cell-vertical-align],.tiptap th[data-cell-vertical-align] { display:table-cell; }');
  expect(css).toContain('padding:8px; vertical-align:top;');
  for (const rule of css.matchAll(/[^{}]*(?:data-row-min-height-twips|data-cell-vertical-align)[^{}]*\{([^}]*)\}/g)) {
    expect(rule[1]).not.toMatch(/overflow|max-height|height\s*:|display\s*:\s*(?:block|flex|grid)/);
  }
  expect(vertical.rowMinHeightDOM({rowMinHeightTwips: 20}).style).toBe('height: 1pt');
  expect(html).toContain('id="edit-row-min-height" type="number" min="0" max="1584" step="0.05"');
  expect(html).toContain('<option value="mixed" disabled>混合</option>');
});

test('programmatic merge or invalid pasted height refuses the entire transaction before history', () => {
  let state = stateAt(doc([table([720, null], ['center', null, null, null, null, null])]));
  state = choose(state, CellSelection.create(state.doc, cells(state.doc)[0], cells(state.doc)[1]));
  const before = state; let candidate;
  expect(mergeCells(state, tr => {candidate = tr;})).toBe(true);
  expect(state.applyTransaction(candidate).transactions).toEqual([]); expect(state).toBe(before); expect(undo(state)).toBe(false);
  for (const value of ['invalid-row-min-height', -1, 13959, 31681, 1.5]) {
    const row = positions(state.doc, ['tableRow'])[0], node = state.doc.nodeAt(row);
    expect(state.applyTransaction(state.tr.setNodeMarkup(row, undefined, {...node.attrs, rowMinHeightTwips: value})).transactions).toEqual([]);
  }
});

test('candidate structural text and draft limits reject before mutation without touching shared transaction', () => {
  const cases = [doc([table(), ...Array.from({length: 10000}, () => p(''))]), doc([table(), p('x'.repeat(2 * 1024 * 1024))]),
    doc([table(), schema.node('paragraph', null, schema.text('x', [schema.marks.link.create({href: `https://example.com/${'a'.repeat(32 * 1024 * 1024)}`})]))])];
  for (const document of cases) {
    const state = stateAt(document), before = state.doc, shared = state.tr;
    expect(() => tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 720, cellVerticalAlign: 'center'})).toThrow();
    expect(state.doc).toBe(before); expect(shared.steps).toEqual([]); expect(undo(state)).toBe(false);
  }
});

test('typing before and after toolbar-format history remains independently undoable', () => {
  let state = stateAt(); state = state.apply(state.tr.insertText('before'));
  const typed = state.doc;
  state = apply(state, {rowMinHeightTwips: 720, cellVerticalAlign: 'center'}); state = state.apply(closeHistory(state.tr));
  const formatted = state.doc; state = state.apply(state.tr.insertText('after'));
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(formatted)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(typed)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.textContent).not.toContain('before');
});
