import {test, expect} from '@playwright/test';
import {CommandManager, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, tableEditing} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';
const margins = await loadTsModule('./ui/word-table-cell-margins.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const position = await loadTsModule('./ui/word-table-position.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const {WordTableCellMarginsControls} = await loadTsModule('./ui/word-table-cell-margins-controls.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const schema = getSchema([StarterKit, TableKit.configure({table: false}), tables.WordTable, page.WordPageGeometry, indent.ParagraphIndent]);
const p = (text = 'cell', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const table = (attrs = {}, columns = attrs.columnWidthsTwips?.length ?? 3, rows = 2, paragraphAttrs = {}) => schema.node('table', attrs,
  Array.from({length: rows}, (_, row) => schema.node('tableRow', null, Array.from({length: columns}, (_, column) =>
    schema.node(row ? 'tableCell' : 'tableHeader', null, p(`${row}:${column}`, paragraphAttrs))))));
const doc = (nodes, attrs = {}) => schema.node('doc', attrs, nodes);
const snap = document => indent.omitDefaultParagraphIndent(page.omitDefaultPageGeometry(tables.omitDefaultTableWidths(document.toJSON())));
const positions = (document, names) => {const found = []; document.descendants((node, pos) => {if (names.includes(node.type.name)) found.push(pos);}); return found;};
const cells = document => positions(document, ['tableCell', 'tableHeader']);
const stateAt = (document, index = 0) => EditorState.create({doc: document, selection: TextSelection.create(document, cells(document)[index] + 2),
  plugins: [history(), tableEditing(), tables.tableWidthGuard(), page.pageGeometryGuard(), indent.paragraphIndentGuard()]});
const choose = (state, selection) => state.apply(state.tr.setSelection(selection));
const apply = (state, value) => state.apply(closeHistory(tables.createTableCellMarginsTransaction(state, value)));
const pair = (left, right) => ({tableCellMarginsTwips: {left, right}});

// Real schemas, transactions, guards, histories, and DOM parser/serializer.
// Small DOM shims have no layout/focus/clipboard implementation. Actual browser
// acceptance is a separate discovery-only specification, not a claimed pass.
test('canonical pair and exact decimal parsing preserve zero without coercion rounding or partial values', () => {
  for (const value of [{left: 0, right: 0}, {left: 1, right: 31680}, {left: 108, right: 108}]) expect(margins.isTableCellMargins(value)).toBe(true);
  for (const value of [null, undefined, false, [], [0, 0], '0,0', {}, {left: 0}, {right: 0}, {left: 0, right: 0, top: 0},
    {left: -1, right: 0}, {left: 0, right: 31681}, {left: 1.5, right: 0}, {left: '0', right: 0}, {left: false, right: 0}, {left: null, right: 0},
    {left: NaN, right: 0}, {left: Infinity, right: 0}]) expect(margins.isTableCellMargins(value)).toBe(false);
  for (let value = 0; value <= 31680; value++) if (margins.parseTableCellMarginPoints(String(value / 20)) !== value) throw new Error(`Changed ${value}`);
  for (const value of ['', '-0', '-1', '+1', '.001', '1e2', '1pt', '1584.05', '0.05000000000000001']) expect(margins.parseTableCellMarginPoints(value)).toBeNull();
});

test('raw JSON rejects null malformed pairs and the attribute on any non-table before schema normalization', () => {
  for (const value of [null, undefined, {}, {left: 1}, {left: 1, right: 1, bottom: 0}, {left: '1', right: 1}, [1, 1]]) {
    const raw = snap(doc([table()])); raw.content[0].attrs = {tableCellMarginsTwips: value};
    expect(() => tables.validateTableWidthsDocument(raw)).toThrow('单元格左右边距');
  }
  for (const type of ['doc', 'paragraph', 'text', 'image', 'tableRow', 'tableCell', 'tableHeader', 'bulletList']) {
    const raw = {type: 'doc', content: [{type, attrs: pair(0, 0)}]};
    expect(() => tables.validateTableWidthsDocument(raw)).toThrow('单元格左右边距');
  }
});

test('explicit pairs reject ragged merged nested and non-top-level tables without narrowing legacy nesting', () => {
  const valid = snap(doc([table(pair(0, 0))]));
  for (const mutate of [raw => raw.content[0].content.pop(), raw => {raw.content[0].content[0].attrs.colspan = 2;},
    raw => {raw.content[0].content[0].attrs.rowspan = 2;}, raw => raw.content[0].content[0].content.push(snap(table({}, 1, 1)))]) {
    const raw = structuredClone(valid); mutate(raw.content[0]); expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
  }
  for (const wrapper of [child => schema.node('blockquote', null, child), child => schema.node('bulletList', null, [schema.node('listItem', null, [p(), child])])]) {
    expect(() => tables.validateTableWidthsDocument(snap(doc([wrapper(table(pair(0, 0)))])))).toThrow();
    expect(() => tables.validateTableWidthsDocument(snap(doc([wrapper(table())])))).not.toThrow();
  }
  const raw = snap(doc([table({}, 1, 1)])); raw.content[0].content[0].content[0].content.push(snap(table({}, 1, 1)));
  expect(() => tables.validateTableWidthsDocument(raw)).not.toThrow();
  raw.content[0].content[0].content[0].content[1].attrs = pair(0, 0);
  expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
});

test('every actual fixed or normalized automatic column must exceed the pair even with no paragraph indent', () => {
  for (const widths of [[1], [200, 201, 202], [31680]]) {
    const sum = Math.min(...widths) - 1;
    expect(() => tables.validateTableWidthsDocument(snap(doc([table({columnWidthsTwips: widths, ...pair(sum, 0)})])))).not.toThrow();
    expect(() => tables.validateTableWidthsDocument(snap(doc([table({columnWidthsTwips: widths, ...pair(sum + 1, 0)})])))).toThrow();
  }
  const a5 = page.presetPageGeometry('A5', 'portrait');
  const automatic = table(pair(0, 786), 7), raw = snap(doc([automatic], {pageGeometry: a5}));
  expect(margins.cellMarginsTableColumnWidths(raw.content[0], a5)).toEqual([788, 788, 787, 787, 787, 787, 787]);
  expect(() => tables.validateTableWidthsDocument(raw)).not.toThrow();
  raw.content[0].attrs.tableCellMarginsTwips.right = 787;
  expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
  expect(margins.cellMarginsTableColumnWidths(snap(table({}, 7)), null)).toEqual(Array(7).fill(1285));
  expect(margins.cellMarginsTableColumnWidths(snap(table({tableIndentTwips: 0}, 7)), a5)).toEqual(Array(7).fill(1285));
});

test('absent margin fields keep legacy automatic/fixed JSON DOM and default rendering unchanged', () => {
  const legacy = getSchema([StarterKit, TableKit]);
  const old = legacy.node('doc', null, [legacy.node('table', null, [legacy.node('tableRow', null, [legacy.node('tableCell', null, legacy.node('paragraph'))])])]);
  const restored = schema.nodeFromJSON(old.toJSON());
  expect(JSON.stringify(snap(restored))).toBe(JSON.stringify(old.toJSON()));
  expect(schema.nodes.table.spec.toDOM(restored.firstChild)).toEqual(legacy.nodes.table.spec.toDOM(old.firstChild));
  for (const attrs of [{columnWidthsTwips: [1000, 2000]}, {tableIndentTwips: 0}, {tableAlign: 'left'}]) {
    const value = table(attrs), output = schema.nodes.table.spec.toDOM(value);
    expect(snap(value).attrs).toEqual(attrs); expect(output[1]).not.toHaveProperty('data-table-cell-margins-twips');
    expect(output[1].style).not.toContain('--word-cell-margin');
  }
  expect(margins.tableCellMarginsDOM({}, true)).toEqual({}); expect(margins.tableCellMargins({})).toEqual({left: 108, right: 108});
});

test('legacy indent border origin uses the explicit left side while absent indent preserves historical border origin', () => {
  for (const [attrs, expected] of [[pair(360, 180), null], [{...pair(360, 180), tableAlign: 'left'}, '0pt'],
    [{...pair(360, 180), tableIndentTwips: 0}, '-18pt'], [{...pair(360, 180), tableIndentTwips: 720}, '18pt'],
    [{...pair(0, 0), tableIndentTwips: 720}, '36pt'], [{tableIndentTwips: 0}, '-5.4pt'],
    [{...pair(360, 180), tableAlign: 'center', tableIndentTwips: 0}, 'auto'], [{...pair(360, 180), tableAlign: 'right', tableIndentTwips: 0}, 'auto']]) {
    expect(position.tablePositionMargins(attrs)?.left ?? null).toBe(expected);
  }
});

test('caret cell forward/backward and table node selections edit only the current table and retain selection kind', () => {
  const document = doc([p('before'), table({columnWidthsTwips: [1000, 2000, 3000]}), p('between'), table(pair(10, 20)), p('after')]);
  const cell = cells(document), tablePos = positions(document, ['table'])[0];
  for (const selection of [TextSelection.create(document, cell[0] + 2), TextSelection.create(document, cell[4] + 3, cell[1] + 2),
    CellSelection.create(document, cell[0], cell[5]), CellSelection.create(document, cell[5], cell[0]), NodeSelection.create(document, cell[3]), NodeSelection.create(document, tablePos)]) {
    const state = EditorState.create({doc: document, selection, plugins: [history(), tables.tableWidthGuard()]}), transaction = tables.createTableCellMarginsTransaction(state, {left: 360, right: 180});
    expect(transaction.selection.eq(selection)).toBe(true); expect(transaction.steps).toHaveLength(1);
    expect(transaction.doc.child(1).attrs.tableCellMarginsTwips).toEqual({left: 360, right: 180});
    expect(transaction.doc.child(3).eq(document.child(3))).toBe(true); expect(transaction.doc.child(1).content.eq(document.child(1).content)).toBe(true);
  }
});

test('outside cross-table and all selections are disabled rather than silently editing every selected table', () => {
  const document = doc([p('before'), table(), p('between'), table(), p('after')]), cell = cells(document);
  for (const selection of [TextSelection.create(document, 2), new AllSelection(document), TextSelection.create(document, cell[0] + 2, cell.at(-1) + 3),
    TextSelection.create(document, cell.at(-1) + 3, cell[0] + 2)]) {
    const state = choose(stateAt(document), selection); expect(tables.selectedTableCellMargins(state)).toBeNull();
    expect(tables.createTableCellMarginsTransaction(state, {left: 1, right: 1})).toBeNull(); expect(undo(state)).toBe(false);
  }
});

test('top-level restriction disables current nested list-contained and merged legacy tables', () => {
  const raw = snap(table({}, 1, 1)); raw.content[0].content[0].content.push(snap(table({}, 1, 1)));
  const nested = doc([schema.nodeFromJSON(raw)]);
  const listed = doc([schema.node('bulletList', null, [schema.node('listItem', null, [p('list'), table()])])]);
  const merged = snap(doc([table({}, 1, 1)])); merged.content[0].content[0].content[0].attrs.colspan = 2;
  for (const [document, index] of [[nested, 0], [nested, 1], [listed, 0], [schema.nodeFromJSON(merged), 0]]) {
    const state = stateAt(document, index); expect(tables.selectedTableCellMargins(state).supported).toBe(false);
    expect(() => tables.createTableCellMarginsTransaction(state, {left: 0, right: 0})).toThrow(); expect(undo(state)).toBe(false);
  }
});

test('pair application reset draft roundtrip and one undo redo preserve stored marks and explicit zero/default-valued pairs', () => {
  for (const value of [{left: 0, right: 0}, {left: 108, right: 108}, {left: 361, right: 180}]) {
    let state = stateAt(doc([table({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 720})]));
    state = state.apply(state.tr.setStoredMarks([schema.marks.bold.create()])); const before = state.doc;
    const transaction = tables.createTableCellMarginsTransaction(state, value); expect(transaction.storedMarks).toEqual(state.storedMarks); expect(transaction.selection.eq(state.selection)).toBe(true);
    state = state.apply(closeHistory(transaction)); state = state.apply(closeHistory(state.tr));
    const saved = snap(state.doc), changed = state.doc;
    expect(saved.content[0].attrs.tableCellMarginsTwips).toEqual(value); tables.validateTableWidthsDocument(saved); expect(snap(schema.nodeFromJSON(saved))).toEqual(saved);
    expect(tables.createTableCellMarginsTransaction(state, {...value})).toBeNull();
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
    expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
    state = apply(state, null); expect(snap(state.doc).content[0].attrs).toEqual({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 720});
  }
  const state = stateAt(doc([table()])); expect(tables.createTableCellMarginsTransaction(state, null)).toBeNull(); expect(undo(state)).toBe(false);
});

test('stale document or selection tokens never apply and invalid candidates leave shared chain transactions empty', () => {
  const state = stateAt(doc([table()])), token = tables.captureTableCellMargins(state), shared = state.tr;
  const chain = Object.create(state); Object.defineProperty(chain, 'tr', {get: () => shared});
  expect(() => tables.createTableCellMarginsTransaction(chain, {left: 1500, right: 1500})).toThrow(); expect(shared.steps).toEqual([]); expect(undo(state)).toBe(false);
  for (const next of [state.apply(state.tr.insertText('new')), choose(state, TextSelection.create(state.doc, cells(state.doc)[1] + 2))]) {
    expect(tables.matchesTableCellMargins(next, token)).toBe(false); expect(tables.createTableCellMarginsTransaction(next, {left: 1, right: 2}, token)).toBeNull();
  }
});

test('paragraph fit subtracts the exact pair on all rows and rejects margin changes or resets atomically', () => {
  const document = doc([table({columnWidthsTwips: [1000], ...pair(0, 0)}, 1, 2, {indentLeftTwips: 999})]), state = stateAt(document);
  expect(() => indent.validateParagraphIndentDocument(snap(document))).not.toThrow();
  for (const value of [{left: 1, right: 0}, {left: 0, right: 1}, null]) expect(() => tables.createTableCellMarginsTransaction(state, value)).toThrow('段落缩进');
  expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  const exact = doc([table({columnWidthsTwips: [1000], ...pair(360, 180)}, 1, 2, {indentLeftTwips: 459})]);
  expect(() => indent.validateParagraphIndentDocument(snap(exact))).not.toThrow();
  const invalid = snap(exact); invalid.content[0].content[1].content[0].content[0].attrs.indentLeftTwips = 460;
  expect(() => indent.validateParagraphIndentDocument(invalid)).toThrow('段落缩进');
});

test('page column and structural narrowing reject positive-content loss before dispatch history or drafts', () => {
  const automatic = stateAt(doc([table(pair(900, 900), 3)]));
  expect(() => page.createPageGeometryTransaction(automatic, page.presetPageGeometry('A5', 'portrait'))).not.toThrow();
  const smaller = {...page.presetPageGeometry('A5', 'portrait'), marginLeftTwips: 1500, marginRightTwips: 1500};
  expect(() => page.createPageGeometryTransaction(automatic, smaller)).toThrow('单元格左右边距');
  expect(() => tables.createTableStructureTransaction(automatic, 'addColumnAfter')).not.toThrow();
  const nearlyFull = stateAt(doc([table(pair(1400, 1400), 3)]));
  expect(() => tables.createTableStructureTransaction(nearlyFull, 'addColumnAfter')).toThrow('单元格左右边距');
  const fixed = stateAt(doc([table({columnWidthsTwips: [2000, 2001], ...pair(1000, 999)})]));
  expect(() => tables.createTableColumnWidthTransaction(fixed, 1999)).toThrow('单元格左右边距');
  for (const state of [automatic, nearlyFull, fixed]) expect(undo(state)).toBe(false);
});

test('automatic-width reset and position changes validate final cell and page geometry before mutation', () => {
  const fixed = stateAt(doc([table({columnWidthsTwips: [10000, 10000], ...pair(3000, 2000)})]));
  expect(() => tables.createTableColumnWidthTransaction(fixed, null)).toThrow('单元格左右边距');
  const a5 = page.presetPageGeometry('A5', 'portrait');
  const custom = stateAt(doc([table(pair(900, 900), 3)], {pageGeometry: a5}));
  expect(() => tables.createTablePositionTransaction(custom, {tableAlign: 'left'})).toThrow('页面正文区域');
  expect(custom.doc.firstChild.attrs.tableAlign).toBeNull(); expect(undo(fixed)).toBe(false);
});

test('guards reject pasted/programmatic malformed pairs collapsed widths and misplaced shapes before history', () => {
  const document = doc([table()]), state = stateAt(document);
  for (const value of ['invalid-table-cell-margins', {left: 0}, {left: 1500, right: 1500}, {left: -1, right: 0}]) {
    const result = state.applyTransaction(state.tr.setNodeMarkup(0, undefined, {...document.firstChild.attrs, tableCellMarginsTwips: value}));
    expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(result.state)).toBe(false);
  }
  const narrower = stateAt(doc([table(pair(1400, 1400), 3)]));
  const next = narrower.tr.setDocAttribute('pageGeometry', page.presetPageGeometry('A5', 'portrait'));
  expect(narrower.applyTransaction(next).transactions).toEqual([]);
});

test('Tiptap direct chained and can commands preserve atomic validation and only dispatch successful pair changes', () => {
  const errors = [], commands = tables.WordTable.config.addCommands.call({options: {onWidthError: message => errors.push(message)}, parent: () => ({})});
  const harness = initial => {
    let state = initial, writes = 0; const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++;}}};
    return {manager: new CommandManager({editor}), get state() {return state;}, get writes() {return writes;}};
  };
  const good = harness(stateAt(doc([table()])));
  expect(good.manager.can().setTableCellMargins({left: 0, right: 0})).toBe(true); expect(good.writes).toBe(0);
  expect(good.manager.chain().setTableCellMargins({left: 0, right: 0}).addRowAfter().run()).toBe(true); expect(good.writes).toBe(1);
  expect(good.state.doc.firstChild.childCount).toBe(3); expect(good.state.doc.firstChild.attrs.tableCellMarginsTwips).toEqual({left: 0, right: 0});
  expect(good.manager.commands.resetTableCellMargins()).toBe(true);
  for (const run of [manager => manager.commands.setTableCellMargins({left: 1500, right: 1500}), manager => manager.chain().setTableCellMargins({left: 1500, right: 1500}).run(),
    manager => manager.can().setTableCellMargins({left: 1500, right: 1500}), manager => manager.can().chain().setTableCellMargins({left: 1500, right: 1500}).run()]) {
    const bad = harness(stateAt(doc([table()]))), before = bad.state.doc; expect(run(bad.manager)).toBe(false); expect(bad.state.doc).toBe(before); expect(undo(bad.state)).toBe(false);
  }
  expect(errors).toHaveLength(2);
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
  contains(other) {return other?.id?.startsWith('edit-table-cell-margin');}
  appendChild(child) {child.parentNode = this; this.children.push(child); return child;}
  removeChild(child) {this.children.splice(this.children.indexOf(child), 1); child.parentNode = null;}
  replaceChildren(...children) {this.children = []; children.forEach(child => this.appendChild(child));}
  setAttribute(name, value) {this.attrs[name] = value;}
  removeAttribute(name) {delete this.attrs[name];}
  querySelectorAll() {return [];}
  cloneNode(deep = true) {const copy = new Element(this.tag); copy.attrs = {...this.attrs}; Object.assign(copy.style, this.style); if (deep) this.children.forEach(child => copy.appendChild(child.cloneNode())); return copy;}
}
function controlsHarness(run, initial = doc([table(), p('after'), table(pair(1, 2))])) {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map();
  const element = id => {if (!elements.has(id)) {const node = new Element(); node.id = id; elements.set(id, node);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  let state = stateAt(initial), writes = 0, locked = false, pendingSelection, controls;
  const notices = []; let editor = {get state() {return state;}, view: {focus() {}, dispatch(tr) {state = state.apply(tr); if (tr.docChanged) writes++; controls.update();}}};
  controls = new WordTableCellMarginsControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message),
    syncSelection: () => {if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); controls.update(); pendingSelection = null;}}}); controls.update();
  const change = (tr, notify = true) => {state = state.apply(tr); if (notify) controls.update();};
  const fill = (side, value) => {const input = element(`edit-table-cell-margin-${side}`); input.emit('pointerdown'); input.emit('focus'); input.value = value; input.emit('input');};
  try {run({element, notices, controls, change, fill, get state() {return state;}, get writes() {return writes;}, lock(value) {locked = value; controls.update();},
    select(selection) {pendingSelection = selection;}, replace() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});}
  finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
}

test('browser-free controls apply the complete pair current table only and retain defaults explicit zero undo and redo', () => {
  controlsHarness(h => {
    const applyButton = h.element('edit-table-cell-margins-apply'); applyButton.emit('click'); expect(h.writes).toBe(0); expect(undo(h.state)).toBe(false);
    h.fill('left', '0'); applyButton.emit('click'); expect(h.writes).toBe(0); expect(h.notices.at(-1)).toContain('完整');
    h.fill('right', '0'); applyButton.emit('click'); expect(h.writes).toBe(1); expect(h.state.doc.firstChild.attrs.tableCellMarginsTwips).toEqual({left: 0, right: 0});
    expect(h.state.doc.lastChild.attrs.tableCellMarginsTwips).toEqual({left: 1, right: 2});
    expect(h.element('edit-table-cell-margin-left').dataset.marginState).toBe('explicit'); applyButton.emit('click'); expect(h.writes).toBe(1);
    expect(undo(h.state, h.change)).toBe(true); expect(h.element('edit-table-cell-margin-left').value).toBe('');
    expect(redo(h.state, h.change)).toBe(true); h.element('edit-table-cell-margins-reset').emit('click'); expect(h.writes).toBe(2);
    expect(snap(h.state.doc).content[0].attrs).toBeUndefined();
  });
});

test('browser-free control validation rejects partial bad-input and content-width failures without draft writes', () => {
  controlsHarness(h => {
    for (const [left, right] of [['1', ''], ['.001', '0'], ['150', '0'], ['-1', '1'], ['0', '1584.05']]) {
      h.fill('left', left); h.fill('right', right); h.element('edit-table-cell-margins-apply').emit('click'); expect(h.writes).toBe(0);
    }
    h.fill('left', '0'); h.fill('right', '0'); h.element('edit-table-cell-margin-left').validity.badInput = true;
    h.element('edit-table-cell-margins-apply').emit('click'); expect(h.writes).toBe(0); expect(undo(h.state)).toBe(false);
  });
});

test('browser-free pending inputs cancel on selection document owner Escape or focus exit and ignore stale tokens and IME', () => {
  controlsHarness(h => {
    const input = h.element('edit-table-cell-margin-left'); h.fill('left', '18'); h.fill('right', '9'); input.emit('keydown', {key: 'Enter', isComposing: true}); expect(h.writes).toBe(0);
    input.emit('keydown', {key: 'Escape'}); expect(input.value).toBe('');
    h.fill('left', '18'); h.element('table-cell-margins-controls').emit('focusout', {relatedTarget: h.element('edit-table-cell-margin-right')}); expect(input.value).toBe('18');
    h.element('table-cell-margins-controls').emit('focusout', {relatedTarget: null}); expect(input.value).toBe('');
    h.fill('left', '18'); h.change(h.state.tr.setSelection(TextSelection.create(h.state.doc, cells(h.state.doc)[1] + 2))); expect(input.value).toBe('');
    h.fill('left', '18'); h.change(h.state.tr.insertText('new')); expect(input.value).toBe('');
    h.fill('left', '18'); h.replace(); expect(input.value).toBe('');
    h.fill('left', '18'); h.fill('right', '9'); h.change(h.state.tr.insertText('stale'), false);
    h.element('edit-table-cell-margins-apply').emit('click'); h.element('edit-table-cell-margins-reset').emit('click'); expect(h.writes).toBe(0);
  });
});

test('browser-free controls resync pending DOM selection and enforce lock outside and cross-table disabled states', () => {
  controlsHarness(h => {
    h.select(TextSelection.create(h.state.doc, cells(h.state.doc)[6] + 2)); h.fill('left', '1'); h.fill('right', '2'); h.element('edit-table-cell-margins-apply').emit('click');
    expect(h.state.doc.firstChild.attrs.tableCellMarginsTwips).toBeNull(); expect(h.state.doc.lastChild.attrs.tableCellMarginsTwips).toEqual({left: 20, right: 40});
    h.lock(true); for (const id of ['edit-table-cell-margin-left', 'edit-table-cell-margin-right', 'edit-table-cell-margins-apply', 'edit-table-cell-margins-reset']) expect(h.element(id).disabled).toBe(true);
    h.element('edit-table-cell-margins-reset').emit('click'); expect(h.writes).toBe(1); h.lock(false);
    h.change(h.state.tr.setSelection(new AllSelection(h.state.doc))); expect(h.element('edit-table-cell-margins-apply').disabled).toBe(true);
    h.element('edit-table-cell-margins-apply').emit('click'); expect(h.writes).toBe(1);
  });
});

test('mounted node view removes all owned metadata variables width and legacy indent compensation after repeated resets', () => {
  const old = globalThis.document; globalThis.document = {createElement: tag => new Element(tag)};
  try {
    const view = new tables.FixedTableView(table({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 720, ...pair(360, 180)}), 25);
    expect(view.table.style).toMatchObject({'--word-cell-margin-left': '18pt', '--word-cell-margin-right': '9pt', marginLeft: '18pt', width: '400px'});
    for (let iteration = 0; iteration < 2; iteration++) {
      view.update(table(pair(0, 0))); expect(view.table.style).toMatchObject({width: '600px', tableLayout: 'auto', '--word-cell-margin-left': '0pt'});
      expect(view.table.attrs).toEqual({'data-table-cell-margins-twips': '0,0'});
      view.update(table()); expect(view.table.attrs).toEqual({});
      expect(view.table.style).toMatchObject({width: '', tableLayout: '', marginLeft: '', '--word-cell-margin-left': '', '--word-cell-margin-right': ''});
      view.update(table({tableIndentTwips: 0, ...pair(361, 1)})); expect(view.table.style.marginLeft).toBe('-18.05pt');
      view.update(table({tableIndentTwips: 0})); expect(view.table.style.marginLeft).toBe('-5.4pt'); expect(view.table.attrs).not.toHaveProperty('data-table-cell-margins-twips');
    }
  } finally {globalThis.document = old;}
});

test('print snapshot captures explicit pair immutably before font wait and resets future snapshots only', async () => {
  const old = globalThis.document, host = new Element(); let ready;
  globalThis.document = {createElement: tag => new Element(tag), getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const view = new tables.FixedTableView(table({tableIndentTwips: 720, ...pair(360, 180)}), 25), source = new Element(); source.appendChild(view.table);
    const pending = preparePrintSnapshot(source); view.update(table()); ready(); await pending;
    expect(host.firstChild.firstChild.attrs['data-table-cell-margins-twips']).toBe('360,180');
    expect(host.firstChild.firstChild.style).toMatchObject({'--word-cell-margin-left': '18pt', '--word-cell-margin-right': '9pt', marginLeft: '18pt'});
    await preparePrintSnapshot(source); expect(host.firstChild.firstChild.attrs).not.toHaveProperty('data-table-cell-margins-twips');
  } finally {globalThis.document = old;}
});

test('shared direct-cell CSS overrides default position and paragraph padding without altering old selectors', () => {
  const css = readFileSync('ui/style.css', 'utf8');
  expect(css).toContain('.tiptap table[data-table-position] td,.tiptap table[data-table-position] th { padding-left:5.4pt; padding-right:5.4pt; min-width:0; }');
  expect(css).toContain('.tiptap table[data-table-cell-margins-twips] > tbody > tr > td');
  expect(css).toContain('padding-left:var(--word-cell-margin-left); padding-right:var(--word-cell-margin-right); min-width:0;');
  expect(css.lastIndexOf('padding-left:var(--word-cell-margin-left)')).toBeGreaterThan(css.lastIndexOf('padding-left:5.4pt'));
  const html = readFileSync('ui/index.html', 'utf8');
  for (const side of ['left', 'right']) expect(html).toContain(`id="edit-table-cell-margin-${side}" type="number" min="0" max="1584" step="0.05"`);
});

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

test('real DOM serialization parsing roundtrips canonical pair exact zero and position without exposing raw JSON attrs', () => {
  for (const attrs of [pair(0, 0), pair(108, 108), {...pair(361, 180), columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 721}]) {
    const original = table(attrs), serialized = serializeNode(original), parsed = parseNode(serialized);
    expect(parsed.firstChild.eq(original)).toBe(true); expect(serialized.attrs).not.toHaveProperty('tableCellMarginsTwips');
    expect(serialized.attrs['data-table-cell-margins-twips']).toBe(`${attrs.tableCellMarginsTwips.left},${attrs.tableCellMarginsTwips.right}`);
    expect(() => tables.validateTableWidthsDocument(snap(parsed))).not.toThrow();
  }
});

test('malformed owned HTML metadata stays as invalid evidence and rejects the paste-like transaction', () => {
  for (const value of ['', '0', '0,', ',0', 'null', '0,0,0', '-1,0', '1.5,0', '31681,0', ' 0,0', '0,0 ', '0,false', '0,1e2']) {
    const element = serializeNode(table()); element.setAttribute('data-table-cell-margins-twips', value); const parsed = parseNode(element), state = stateAt(doc([table()]));
    expect(parsed.firstChild.attrs.tableCellMarginsTwips).toBe('invalid-table-cell-margins');
    expect(() => tables.validateTableWidthsDocument(snap(parsed))).toThrow();
    expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]); expect(undo(state)).toBe(false);
  }
  const foreign = serializeNode(table()); foreign.setAttribute('style', 'padding-left:18pt;padding-right:9pt');
  expect(parseNode(foreign).firstChild.attrs.tableCellMarginsTwips).toBeNull();
});

test('owned metadata on non-table HTML elements is rejected before parsing can silently discard it', () => {
  for (const tag of ['td', 'th', 'p', 'tr', 'div']) {
    const element = new ParseElement(tag, {'data-table-cell-margins-twips': '0,0'});
    expect(margins.validTableCellMarginsHTML({querySelectorAll: () => [element]})).toBe(false);
  }
  const valid = new ParseElement('table', {'data-table-cell-margins-twips': '0,108'});
  expect(margins.validTableCellMarginsHTML({querySelectorAll: () => [valid]})).toBe(true);
  const old = globalThis.DOMParser, errors = [];
  globalThis.DOMParser = class {parseFromString() {return {querySelectorAll: () => [new ParseElement('td', {'data-table-cell-margins-twips': '0,0'})]};}};
  try {
    const plugin = tables.tableWidthGuard(error => errors.push(error));
    expect(plugin.props.handlePaste(null, {clipboardData: {getData: () => '<td DATA-TABLE-CELL-MARGINS-TWIPS="0,0">bad</td>'}})).toBe(true);
    expect(errors).toHaveLength(1);
  } finally {globalThis.DOMParser = old;}
});
