import {test, expect} from '@playwright/test';
import {CommandManager, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, tableEditing, mergeCells, toggleHeaderCell, toggleHeaderRow} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {loadTsModule} from './load-ts.mjs';
const colors = await loadTsModule('./ui/word-table-cell-colors.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const {TextColor} = await loadTsModule('./ui/word-text-color.ts');
const {WordTableCellColorControls} = await loadTsModule('./ui/word-table-cell-color-controls.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const schema = getSchema([StarterKit, TableKit.configure({table: false}), tables.WordTable, page.WordPageGeometry, TextColor]);
const p = (text = 'cell', marks = []) => schema.node('paragraph', null, text ? schema.text(text, marks) : null);
const table = (fills = Array(6).fill(null), modes = Array(6).fill(null), attrs = {}) => schema.node('table', attrs,
  [0, 1].map(row => schema.node('tableRow', null, [0, 1, 2].map(column => schema.node(row ? 'tableCell' : 'tableHeader',
    {cellFillRgb: fills[row * 3 + column], cellTextColorMode: modes[row * 3 + column]}, p(`${row}:${column}`))))));
const doc = (nodes = [table()], attrs = {}) => schema.node('doc', attrs, nodes);
const snap = node => page.omitDefaultPageGeometry(tables.omitDefaultTableWidths(node.toJSON()));
const positions = (document, names) => {const found = []; document.descendants((node, pos) => {if (names.includes(node.type.name)) found.push(pos);}); return found;};
const cells = document => positions(document, ['tableCell', 'tableHeader']);
const stateAt = (document = doc(), index = 0) => EditorState.create({doc: document, selection: TextSelection.create(document, cells(document)[index] + 2), plugins: [history(), tableEditing(), tables.tableWidthGuard(), page.pageGeometryGuard()]});
const choose = (state, selection) => state.apply(state.tr.setSelection(selection));
const apply = (state, patch) => {state = state.apply(closeHistory(tables.createTableCellColorsTransaction(state, patch))); return state.apply(closeHistory(state.tr));};
const fills = state => cells(state.doc).map(pos => state.doc.nodeAt(pos).attrs.cellFillRgb);
const modes = state => cells(state.doc).map(pos => state.doc.nodeAt(pos).attrs.cellTextColorMode);

// No browser is launched: real schemas, transactions, histories, HTML
// parser/serializer and UI-event shims establish contracts, not visual parity.
test('fill RGB is canonical and automatic color is an independent literal declaration', () => {
  for (const value of ['000000', 'FFFFFF', '123ABC']) expect(colors.isCellFillRgb(value)).toBe(true);
  for (const value of [null, undefined, false, 0, '#123ABC', '123abc', '123', '12345678', '', {}, []]) expect(colors.isCellFillRgb(value)).toBe(false);
  for (const value of ['#12abef', '12abef', ' #12ABEF ']) expect(colors.parseCellFill(value)).toBe('12ABEF');
  for (const value of ['red', '#ABC', '#12345G', 'rgb(0,0,0)', 'auto', 'transparent']) expect(colors.parseCellFill(value)).toBeNull();
  expect(colors.isCellTextColorMode('auto')).toBe(true);
  for (const value of [null, undefined, 'AUTO', ' auto', 'black', 'FFFFFF', false, {}]) expect(colors.isCellTextColorMode(value)).toBe(false);
});

test('raw input rejects malformed null wrong-node or unsupported structural color declarations', () => {
  for (const [field, values] of [['cellFillRgb', [null, undefined, false, '#000000', 'ffffff', 'red', '', 123456]], ['cellTextColorMode', [null, undefined, false, 'AUTO', 'white', '']]]) {
    for (const value of values) {
      const raw = snap(doc()); raw.content[0].content[0].content[0].attrs = {[field]: value}; expect(() => tables.validateTableWidthsDocument(raw)).toThrow('单元格底色');
    }
    for (const type of ['doc', 'table', 'tableRow', 'paragraph', 'text']) expect(() => tables.validateTableWidthsDocument({type: 'doc', content: [{type, attrs: {[field]: field === 'cellFillRgb' ? '000000' : 'auto'}}]})).toThrow('单元格底色');
  }
  for (const mutate of [raw => raw.content[0].content[1].content.pop(), raw => {raw.content[0].content[0].content[0].attrs.colspan = 2;},
    raw => {raw.content[0].content[0].content[0].attrs.rowspan = 2;}, raw => raw.content[0].content[0].content[0].content.push(snap(table()))]) {
    const raw = snap(doc([table(['000000'])])); mutate(raw); expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
  }
  const listed = schema.node('bulletList', null, schema.node('listItem', null, [p('list'), table(['000000'])]));
  expect(() => tables.validateTableWidthsDocument(snap(doc([listed])))).toThrow();
  const legacy = snap(doc()); legacy.content[0].content[0].content[0].content.push(snap(table()));
  expect(() => tables.validateTableWidthsDocument(legacy)).not.toThrow();
  legacy.content[0].content[0].content[0].content[1].content[0].content[0].attrs = {cellTextColorMode: 'auto'};
  expect(() => tables.validateTableWidthsDocument(legacy)).toThrow();
});

test('absent fill and mode preserve old document JSON and cell DOM with no profile changes', () => {
  const legacy = getSchema([StarterKit, TableKit]);
  const raw = {type: 'doc', content: [{type: 'table', content: [{type: 'tableRow', content: [{type: 'tableCell', content: [{type: 'paragraph'}]}]}]}]};
  const old = legacy.nodeFromJSON(raw), restored = schema.nodeFromJSON(old.toJSON());
  expect(JSON.stringify(snap(restored))).toBe(JSON.stringify(old.toJSON()));
  const nextCell = restored.firstChild.firstChild.firstChild, oldCell = old.firstChild.firstChild.firstChild;
  expect(schema.nodes.tableCell.spec.toDOM(nextCell)).toEqual(legacy.nodes.tableCell.spec.toDOM(oldCell));
  expect(colors.cellFillDOM({})).toEqual({}); expect(colors.cellTextColorModeDOM({})).toEqual({});
});

test('caret forward reverse text ranges CellSelection and table row cell NodeSelection retain exact scope and kind', () => {
  const document = doc(), cell = cells(document), row = positions(document, ['tableRow']);
  const selections = [[TextSelection.create(document, cell[1] + 2), 1], [TextSelection.create(document, cell[1] + 2, cell[4] + 3), 4],
    [TextSelection.create(document, cell[4] + 3, cell[1] + 2), 4], [TextSelection.create(document, cell[0] + 2, cell[1] + 2), 1],
    [CellSelection.create(document, cell[1], cell[5]), 4], [CellSelection.create(document, cell[5], cell[1]), 4],
    [NodeSelection.create(document, 0), 6], [NodeSelection.create(document, row[1]), 3], [NodeSelection.create(document, cell[3]), 1]];
  for (const [selection, count] of selections) {
    const state = choose(stateAt(document), selection), selected = tables.selectedTableCellColors(state);
    expect(selected.cells).toHaveLength(count);
    const transaction = tables.createTableCellColorsTransaction(state, {cellFillRgb: '123ABC', cellTextColorMode: 'auto'});
    expect(transaction.selection.eq(state.selection)).toBe(true); expect(transaction.steps).toHaveLength(count);
    const next = state.apply(transaction); expect(fills(next).filter(Boolean)).toEqual(Array(count).fill('123ABC')); expect(modes(next).filter(Boolean)).toEqual(Array(count).fill('auto'));
    expect(next.doc.textContent).toBe(document.textContent);
  }
});

test('outside all cross-table and unsupported tables cannot receive cell color edits', () => {
  const document = doc([p('before'), table(), p('between'), table(), p('after')]), cell = cells(document);
  for (const selection of [TextSelection.create(document, 2), new AllSelection(document), TextSelection.create(document, cell[0] + 2, cell.at(-1) + 3)]) {
    const state = choose(stateAt(document), selection); expect(tables.selectedTableCellColors(state)).toBeNull(); expect(tables.createTableCellColorsTransaction(state, {cellFillRgb: '000000'})).toBeNull();
  }
  const raw = snap(doc()); raw.content[0].content[0].content[0].attrs.colspan = 2;
  const state = stateAt(schema.nodeFromJSON(raw)); expect(tables.selectedTableCellColors(state).supported).toBe(false);
  expect(() => tables.createTableCellColorsTransaction(state, {cellFillRgb: '000000'})).toThrow();
});

test('mixed defaults explicit white and auto remain independent with no-op resets and one undo redo', () => {
  let state = stateAt(doc([table(['FFFFFF', null, '000000'], ['auto'])])); state = choose(state, NodeSelection.create(state.doc, 0));
  for (const field of ['cellFillRgb', 'cellTextColorMode']) expect(tables.selectedTableCellColors(state)[field]).toEqual({value: null, mixed: true, hasExplicit: true});
  state = state.apply(state.tr.setStoredMarks([schema.marks.bold.create()])); const before = state.doc;
  const transaction = tables.createTableCellColorsTransaction(state, {cellFillRgb: 'ABCDEF', cellTextColorMode: 'auto'});
  expect(transaction.storedMarks).toEqual(state.storedMarks); state = state.apply(closeHistory(transaction)); state = state.apply(closeHistory(state.tr));
  const changed = state.doc, saved = snap(changed); tables.validateTableWidthsDocument(saved); expect(snap(schema.nodeFromJSON(saved))).toEqual(saved);
  expect(tables.createTableCellColorsTransaction(state, {cellFillRgb: 'ABCDEF', cellTextColorMode: 'auto'})).toBeNull();
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
  state = apply(state, {cellFillRgb: null}); expect(fills(state)).toEqual(Array(6).fill(null)); expect(modes(state)).toEqual(Array(6).fill('auto'));
  state = apply(state, {cellFillRgb: '000000'}); state = apply(state, {cellTextColorMode: null}); expect(fills(state)).toEqual(Array(6).fill('000000')); expect(modes(state)).toEqual(Array(6).fill(null));
});

test('formatting preserves explicit text colors links content and stored marks without adding derived RGB', () => {
  const marked = p('explicit', [schema.marks.textColor.create({rgb: 'FF0000'}), schema.marks.bold.create(), schema.marks.link.create({href: 'https://example.com/'})]);
  const document = doc([schema.node('table', null, schema.node('tableRow', null, schema.node('tableCell', null, [p('ordinary'), marked])))]);
  const state = stateAt(document), content = document.firstChild.firstChild.firstChild.content;
  const next = apply(state, {cellFillRgb: '000000', cellTextColorMode: 'auto'});
  expect(next.doc.firstChild.firstChild.firstChild.content.eq(content)).toBe(true);
  expect(JSON.stringify(snap(next.doc))).not.toContain('FFFFFF');
  expect(snap(next.doc).content[0].content[0].content[0].attrs.cellTextColorMode).toBe('auto');
});

test('add delete and header conversion maintain surviving cell colors and new cells default with atomic undo', () => {
  const originalFills = ['111111', '222222', '333333', '444444', '555555', '666666'], originalModes = ['auto', null, 'auto', null, 'auto', null];
  for (const action of ['addRowBefore', 'addRowAfter', 'addColumnBefore', 'addColumnAfter', 'deleteRow', 'deleteColumn']) {
    const state = stateAt(doc([table(originalFills, originalModes, {columnWidthsTwips: [1000, 1200, 1400], tableCellMarginsTwips: {left: 0, right: 0}})]), 4);
    let next = state.apply(closeHistory(tables.createTableStructureTransaction(state, action))); tables.validateTableWidthsDocument(snap(next.doc));
    expect(next.doc.firstChild.attrs.tableCellMarginsTwips).toEqual({left: 0, right: 0});
    if (action.startsWith('addRow')) {
      const at = action === 'addRowBefore' ? 1 : 2; next.doc.firstChild.child(at).forEach(cell => {expect(cell.attrs.cellFillRgb).toBeNull(); expect(cell.attrs.cellTextColorMode).toBeNull();});
      expect(fills(next).filter(Boolean)).toEqual(originalFills);
    }
    if (action.startsWith('addColumn')) {
      const at = action === 'addColumnBefore' ? 1 : 2; next.doc.firstChild.forEach(row => {expect(row.child(at).attrs.cellFillRgb).toBeNull(); expect(row.child(at).attrs.cellTextColorMode).toBeNull();});
      expect(fills(next).filter(Boolean)).toEqual(originalFills);
    }
    if (action === 'deleteRow') expect(fills(next)).toEqual(originalFills.slice(0, 3));
    if (action === 'deleteColumn') expect(fills(next)).toEqual(['111111', '333333', '444444', '666666']);
    expect(undo(next, tr => {next = next.apply(tr);})).toBe(true); expect(next.doc.eq(state.doc)).toBe(true);
  }
  for (const command of [toggleHeaderCell, toggleHeaderRow]) {
    let state = stateAt(doc([table(originalFills, originalModes)]), 4), before = state.doc;
    command(state, tr => {state = state.apply(closeHistory(tr));}); expect(fills(state)).toEqual(originalFills); expect(modes(state)).toEqual(originalModes);
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
  }
});

test('invalid candidates stale tokens unsupported merge and failed commands never mutate history or chain drafts', () => {
  const errors = [], commands = tables.WordTable.config.addCommands.call({options: {onWidthError: message => errors.push(message)}, parent: () => ({})});
  const harness = () => {
    let state = stateAt(), writes = 0; const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++;}}};
    return {manager: new CommandManager({editor}), get state() {return state;}, get writes() {return writes;}};
  };
  const good = harness(); expect(good.manager.can().setTableCellColors({cellFillRgb: '000000'})).toBe(true); expect(good.writes).toBe(0);
  expect(good.manager.chain().setTableCellColors({cellFillRgb: '000000', cellTextColorMode: 'auto'}).addRowAfter().run()).toBe(true); expect(good.writes).toBe(1);
  expect(good.manager.commands.resetTableCellColors()).toBe(true);
  for (const run of [manager => manager.commands.setTableCellColors({cellFillRgb: 'black'}), manager => manager.chain().setTableCellColors({cellTextColorMode: 'white'}).run(),
    manager => manager.can().setTableCellColors({cellFillRgb: null, cellTextColorMode: undefined})]) {
    const bad = harness(); expect(run(bad.manager)).toBe(false); expect(bad.writes).toBe(0); expect(undo(bad.state)).toBe(false);
  }
  expect(errors).toHaveLength(2);
  const state = stateAt(), token = tables.captureTableCellMargins(state), stale = state.apply(state.tr.insertText('x'));
  expect(tables.createTableCellColorsTransaction(stale, {cellFillRgb: '000000'}, token)).toBeNull();
  for (const patch of [{bad: 0}, {cellFillRgb: undefined}, {cellTextColorMode: false}]) expect(() => tables.createTableCellColorsTransaction(state, patch)).toThrow();
  let colored = stateAt(doc([table(['000000'])])); colored = choose(colored, CellSelection.create(colored.doc, cells(colored.doc)[0], cells(colored.doc)[1]));
  let candidate; mergeCells(colored, tr => {candidate = tr;}); expect(colored.applyTransaction(candidate).transactions).toEqual([]); expect(undo(colored)).toBe(false);
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
  contains(other) {return other?.id?.startsWith('edit-cell-');}
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

function controlsHarness(run, initial = doc([table(), p('after'), table(['000000', 'FFFFFF'], ['auto'])])) {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map();
  const element = id => {if (!elements.has(id)) {const node = new Element(); node.id = id; elements.set(id, node);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  let state = stateAt(initial), writes = 0, locked = false, pendingSelection, controls;
  const notices = []; let editor = {get state() {return state;}, view: {focus() {}, dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++; controls.update();}}};
  controls = new WordTableCellColorControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message),
    syncSelection: () => {if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); controls.update(); pendingSelection = null;}}}); controls.update();
  const change = (tr, notify = true) => {state = state.apply(tr); if (notify) controls.update();};
  const fill = value => {const input = element('edit-cell-fill'); input.emit('pointerdown'); input.emit('focus'); input.value = value; input.emit('input');};
  const mode = value => {const select = element('edit-cell-text-color-mode'); select.emit('pointerdown'); select.value = value; select.emit('change');};
  try {run({element, notices, controls, change, fill, mode, get state() {return state;}, get writes() {return writes;}, lock(value) {locked = value; controls.update();},
    select(selection) {pendingSelection = selection;}, replace() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});}
  finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
}

test('controls normalize hex picker and independent default resets with single undo and clear preview warning', () => {
  controlsHarness(h => {
    const button = h.element('edit-cell-fill-apply'); button.emit('click'); expect(h.writes).toBe(0);
    h.fill('#aa12ef'); button.emit('click'); expect(h.writes).toBe(1); expect(fills(h.state)[0]).toBe('AA12EF');
    expect(h.element('edit-cell-fill').dataset.fillState).toBe('explicit'); expect(h.element('edit-cell-fill').value).toBe('#AA12EF');
    h.mode('auto'); expect(modes(h.state)[0]).toBe('auto'); expect(h.element('edit-cell-text-color-mode').dataset.modeState).toBe('explicit');
    h.element('edit-cell-fill-reset').emit('click'); expect(fills(h.state)[0]).toBeNull(); expect(modes(h.state)[0]).toBe('auto');
    expect(undo(h.state, h.change)).toBe(true); expect(fills(h.state)[0]).toBe('AA12EF'); expect(modes(h.state)[0]).toBe('auto');
    expect(redo(h.state, h.change)).toBe(true);
    const picker = h.element('edit-cell-fill-picker'); picker.emit('pointerdown'); picker.value = '#000000'; picker.emit('input'); expect(h.writes).toBe(3);
    picker.emit('change'); expect(h.writes).toBe(4); expect(fills(h.state)[0]).toBe('000000');
    h.element('edit-cell-text-color-mode-reset').emit('click'); expect(modes(h.state)[0]).toBeNull(); expect(fills(h.state)[0]).toBe('000000');
    expect(h.element('edit-cell-color-state').textContent).toContain('近似'); expect(h.element('edit-cell-color-state').textContent).toContain('链接保留');
  });
});

test('mixed UI states untouched Apply Enter and field-specific resets preserve unrelated colors', () => {
  controlsHarness(h => {
    h.change(h.state.tr.setSelection(CellSelection.create(h.state.doc, cells(h.state.doc)[0], cells(h.state.doc)[5])));
    expect(h.element('edit-cell-fill').dataset.fillState).toBe('mixed'); expect(h.element('edit-cell-fill').placeholder).toBe('混合');
    expect(h.element('edit-cell-text-color-mode').value).toBe('mixed'); h.element('edit-cell-fill-apply').emit('click');
    h.element('edit-cell-fill').emit('keydown', {key: 'Enter'}); expect(h.writes).toBe(0);
    h.fill('123456'); h.element('edit-cell-fill').emit('keydown', {key: 'Enter'}); expect(fills(h.state)).toEqual(Array(6).fill('123456')); expect(modes(h.state)).toEqual(['auto', null, null, null, null, null]);
    h.mode('auto'); expect(modes(h.state)).toEqual(Array(6).fill('auto'));
    h.fill(''); h.element('edit-cell-fill-apply').emit('click'); expect(fills(h.state)).toEqual(Array(6).fill(null)); expect(modes(h.state)).toEqual(Array(6).fill('auto'));
    h.mode(''); expect(modes(h.state)).toEqual(Array(6).fill(null));
  }, doc([table(['FFFFFF', null, '000000'], ['auto'])]));
});

test('controls cancel stale editing on focus exit Escape selection document changes and block IME locks and invalid input', () => {
  controlsHarness(h => {
    const input = h.element('edit-cell-fill');
    for (const value of ['#12', 'red', 'auto', '#GG0000', '12345678']) {h.fill(value); h.element('edit-cell-fill-apply').emit('click');}
    expect(h.writes).toBe(0); expect(h.notices).toHaveLength(5); h.mode('white'); h.mode('mixed'); expect(h.writes).toBe(0);
    h.fill('123456'); input.emit('keydown', {key: 'Enter', isComposing: true}); input.emit('keydown', {key: 'Enter', keyCode: 229}); expect(h.writes).toBe(0);
    input.emit('keydown', {key: 'Escape'}); expect(input.value).toBe('');
    h.fill('123456'); h.element('table-cell-color-controls').emit('focusout', {relatedTarget: h.element('edit-cell-fill-apply')}); expect(input.value).toBe('123456');
    h.element('table-cell-color-controls').emit('focusout', {relatedTarget: null}); expect(input.value).toBe('');
    h.fill('123456'); h.change(h.state.tr.setSelection(TextSelection.create(h.state.doc, cells(h.state.doc)[1] + 2))); expect(input.value).toBe('');
    h.fill('123456'); h.change(h.state.tr.insertText('new')); expect(input.value).toBe('');
    h.fill('123456'); h.replace(); expect(input.value).toBe('');
    h.lock(true); for (const id of ['edit-cell-fill', 'edit-cell-fill-picker', 'edit-cell-fill-apply', 'edit-cell-fill-reset', 'edit-cell-text-color-mode', 'edit-cell-text-color-mode-reset']) expect(h.element(id).disabled).toBe(true);
    h.fill('123456'); h.element('edit-cell-fill-apply').emit('click'); h.mode('auto'); expect(h.writes).toBe(0); h.lock(false);
    h.fill('123456'); h.change(h.state.tr.insertText('stale'), false); h.element('edit-cell-fill-apply').emit('click'); h.mode('auto'); expect(h.writes).toBe(0);
  });
});

test('controls resync current DOM selection and disable outside and cross-table editing', () => {
  controlsHarness(h => {
    h.select(TextSelection.create(h.state.doc, cells(h.state.doc)[7] + 2)); h.fill('123456'); h.element('edit-cell-fill-apply').emit('click');
    expect(fills(h.state)[0]).toBeNull(); expect(fills(h.state)[7]).toBe('123456'); h.mode('auto'); expect(modes(h.state)[7]).toBe('auto');
    h.change(h.state.tr.setSelection(new AllSelection(h.state.doc))); expect(h.element('edit-cell-fill').disabled).toBe(true); expect(h.element('edit-cell-text-color-mode').disabled).toBe(true);
    h.element('edit-cell-fill-apply').emit('click'); expect(h.writes).toBe(2);
  });
});

test('dark light missing-fill automatic preview is derived without materializing marks during DOM roundtrip', () => {
  for (const [fill, expected] of [['000000', 'FFFFFF'], ['123456', 'FFFFFF'], ['FFFFFF', '000000'], ['FFFF00', '000000'], [null, '000000']]) {
    expect(colors.cellAutoPreviewRgb(fill)).toBe(expected);
    const original = table([fill], ['auto']), element = serializeNode(original), parsed = parseNode(element);
    expect(parsed.firstChild.eq(original)).toBe(true); tables.validateTableWidthsDocument(snap(parsed));
    const cell = element.children.find(child => child.tagName === 'TBODY').firstChild.firstChild;
    expect(cell.attrs['data-cell-text-color-mode']).toBe('auto'); expect(cell.style.cssText).toContain(`--word-cell-auto-preview: #${expected}`);
    expect(cell.style.cssText).not.toMatch(/(?:^|;)\s*color\s*:/); expect(JSON.stringify(snap(parsed))).not.toContain('textColor');
  }
  expect(colors.cellTextColorModeDOM({cellFillRgb: '000000'})).toEqual({});
  expect(colors.cellFillDOM({cellTextColorMode: 'auto'})).toEqual({});
});

test('owned metadata rejects wrong elements and invalid values before clipboard schema erases evidence', () => {
  for (const [field, invalid] of [['data-cell-fill-rgb', ['', 'ffffff', '#FFFFFF', 'auto', 'null']], ['data-cell-text-color-mode', ['', 'AUTO', 'white', ' auto', 'null']]]) {
    for (const value of invalid) {
      const element = serializeNode(table()), cell = element.children.find(child => child.tagName === 'TBODY').firstChild.firstChild; cell.setAttribute(field, value);
      const parsed = parseNode(element), state = stateAt(); expect(() => tables.validateTableWidthsDocument(snap(parsed))).toThrow();
      expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]); expect(undo(state)).toBe(false);
    }
    for (const tag of ['p', 'tr', 'table', 'span']) {
      const root = new ParseElement(); root.appendChild(new ParseElement(tag, {[field]: field.includes('fill') ? '000000' : 'auto'})); expect(colors.validTableCellColorsHTML(root)).toBe(false);
      const previous = globalThis.DOMParser, errors = []; globalThis.DOMParser = class {parseFromString() {return root;}};
      try {expect(tables.tableWidthGuard(message => errors.push(message)).props.handlePaste(null, {clipboardData: {getData: () => `<${tag} ${field.toUpperCase()}="x">x</${tag}>`}})).toBe(true); expect(errors).toHaveLength(1);}
      finally {globalThis.DOMParser = previous;}
    }
  }
});

test('foreign background CSS never silently becomes supported fill or automatic mode', () => {
  const element = serializeNode(table()), cell = element.children.find(child => child.tagName === 'TBODY').firstChild.firstChild;
  cell.setAttribute('style', 'background-color:#000000;--word-cell-auto-preview:#FFFFFF'); cell.setAttribute('bgcolor', '#123456');
  const parsed = parseNode(element), result = parsed.firstChild.firstChild.firstChild;
  expect(result.attrs.cellFillRgb).toBeNull(); expect(result.attrs.cellTextColorMode).toBeNull(); expect(result.textContent).toBe('0:0');
});

test('fill and derived auto printing declarations remain immutable across later reset and font await', async () => {
  const previous = globalThis.document, host = new Element(); let ready;
  globalThis.document = {createElement: tag => new Element(tag), getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const source = new Element(), cell = new Element('td'); source.appendChild(cell);
    cell.attrs = schema.nodes.tableCell.spec.toDOM(schema.node('tableCell', {cellFillRgb: '000000', cellTextColorMode: 'auto'}, p()))[1];
    const pending = preparePrintSnapshot(source); cell.attrs = {}; ready(); await pending;
    expect(host.firstChild.firstChild.attrs['data-cell-fill-rgb']).toBe('000000'); expect(host.firstChild.firstChild.attrs['data-cell-text-color-mode']).toBe('auto');
    expect(host.firstChild.firstChild.attrs.style).toContain('background-color: #000000'); expect(host.firstChild.firstChild.attrs.style).toContain('--word-cell-auto-preview: #FFFFFF');
    await preparePrintSnapshot(source); expect(host.firstChild.firstChild.attrs).toEqual({});
  } finally {globalThis.document = previous;}
});

test('shared preview CSS uses inheritance so explicit RGB and links win and legacy unmarked cells are unchanged', () => {
  const css = readFileSync('ui/style.css', 'utf8'), html = readFileSync('ui/index.html', 'utf8');
  expect(css).toContain('.tiptap [data-cell-text-color-mode="auto"] { color:var(--word-cell-auto-preview); }');
  expect(css).toContain('#print-document .tiptap [data-cell-text-color-mode="auto"] :is(p,h1,h2,h3,code) { color:inherit; }');
  expect(css).not.toContain(':is(p,h1,h2,h3,a,code)'); expect(css).toContain('.tiptap [data-cell-fill-rgb].selectedCell { outline:2px solid #477fc6;');
  for (const rule of css.matchAll(/[^{}]*data-cell-text-color-mode[^{}]*\{([^}]*)\}/g)) expect(rule[1]).not.toContain('!important');
  expect(html).toContain('id="edit-cell-fill-picker" type="color"'); expect(html).toContain('id="edit-cell-text-color-mode"');
  expect(colors.cellAutoColorPreviewWarning).toContain('不代表 Word / LibreOffice');
  const explicit = schema.text('red code', [schema.marks.textColor.create({rgb: 'FF0000'}), schema.marks.code.create()]);
  const cell = schema.node('tableCell', {cellFillRgb: '000000', cellTextColorMode: 'auto'}, schema.node('paragraph', null, explicit));
  const element = serializeNode(schema.node('table', null, schema.node('tableRow', null, cell)));
  const span = element.querySelectorAll('[data-text-color]')[0]; expect(span.attrs['data-text-color']).toBe('FF0000'); expect(span.style.cssText).toBe('color: #FF0000');
});

test('candidate structure text and draft budgets reject color changes before shared transaction mutation', () => {
  for (const document of [doc([table(), ...Array.from({length: 10000}, () => p(''))]), doc([table(), p('x'.repeat(2 * 1024 * 1024))]),
    doc([table(), p('x', [schema.marks.link.create({href: `https://example.com/${'a'.repeat(32 * 1024 * 1024)}`})])])]) {
    const state = stateAt(document), shared = state.tr; expect(() => tables.createTableCellColorsTransaction(state, {cellFillRgb: '000000', cellTextColorMode: 'auto'})).toThrow();
    expect(state.doc).toBe(document); expect(shared.steps).toEqual([]); expect(undo(state)).toBe(false);
  }
});

test('typing before and after toolbar colors forms independent undo boundaries', () => {
  let state = stateAt(); state = state.apply(state.tr.insertText('before')); const typed = state.doc;
  state = apply(state, {cellFillRgb: '000000', cellTextColorMode: 'auto'}); const colored = state.doc; state = state.apply(state.tr.insertText('after'));
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(colored)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(typed)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.textContent).not.toContain('before');
});


test('native picker lifecycle rejects late input and change after selection document owner changes or Escape', () => {
  controlsHarness(h => {
    const picker = h.element('edit-cell-fill-picker');
    for (const invalidate of [() => h.change(h.state.tr.setSelection(TextSelection.create(h.state.doc, cells(h.state.doc)[1] + 2))),
      () => h.change(h.state.tr.insertText('new')), () => h.replace(), () => picker.emit('keydown', {key: 'Escape'})]) {
      picker.emit('pointerdown'); picker.emit('focus'); picker.value = '#123456'; picker.emit('input'); invalidate();
      picker.value = '#123456'; picker.emit('input'); picker.emit('change'); expect(h.writes).toBe(0);
    }
    picker.emit('focus'); picker.value = '#123456'; picker.emit('input'); picker.emit('change'); expect(h.writes).toBe(1); expect(fills(h.state)[1]).toBe('123456');
    picker.value = '#ABCDEF'; picker.emit('change'); expect(h.writes).toBe(1);
  });
});


test('evidence helper preserves all imported font mark attrs despite null prototypes and refuses silently dropped data', ({}, testInfo) => {
  const raw = {type: 'doc', attrs: {styleProfile: 'standard-v2'}, content: [{type: 'table', content: [{type: 'tableRow', content: [0, 1].map(index => ({
    type: 'tableCell', attrs: {cellFillRgb: index ? 'FFFFFF' : '000000', cellTextColorMode: 'auto'}, content: [{type: 'paragraph',
      attrs: {spaceBefore: 0, spaceAfter: 0, lineSpacing: 240}, content: [{type: 'text', text: `Source ${index}`, marks: [
        {type: 'fontSize', attrs: {halfPoints: 24}}, {type: 'fontFamily', attrs: {name: 'Liberation Serif'}}, {type: 'textColor', attrs: {rgb: 'C03030'}},
      ]}]}]}))}]}]};
  mkdirSync(testInfo.outputPath(), {recursive: true});
  const input = testInfo.outputPath('helper-input.json'), out = testInfo.outputPath('helper-output');
  writeFileSync(input, JSON.stringify(raw));
  const result = spawnSync(process.execPath, ['tests/ui/edit-table-cell-colors.mjs', input, out, '0', '0', '{"cellFillRgb":"156082","cellTextColorMode":"auto"}'], {encoding: 'utf8'});
  expect(result.status, result.stderr).toBe(0);
  const read = name => JSON.parse(readFileSync(`${out}/${name}.json`, 'utf8'));
  const before = read('before'), edited = read('edited'); expect(read('undo')).toEqual(before); expect(read('redo')).toEqual(edited);
  const beforeCells = before.content[0].content[0].content, editedCells = edited.content[0].content[0].content;
  expect(editedCells[0].content).toEqual(beforeCells[0].content); expect(editedCells[1]).toEqual(beforeCells[1]);
  expect(editedCells[0].attrs).toEqual({...beforeCells[0].attrs, cellFillRgb: '156082'});
  expect(editedCells[0].content[0].content[0].marks).toEqual(expect.arrayContaining(raw.content[0].content[0].content[0].content[0].content[0].marks));
  raw.content[0].content[0].content[0].content[0].content[0].marks[1].attrs.unmodeled = 'must not disappear';
  writeFileSync(input, JSON.stringify(raw));
  const rejected = spawnSync(process.execPath, ['tests/ui/edit-table-cell-colors.mjs', input, testInfo.outputPath('must-not-write')], {encoding: 'utf8'});
  expect(rejected.status).not.toBe(0); expect(rejected.stderr).toContain('Lost mark fontFamily.unmodeled');
});
