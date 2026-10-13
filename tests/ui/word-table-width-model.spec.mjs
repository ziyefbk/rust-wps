import {test, expect} from '@playwright/test';
import {CommandManager, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import Image from '@tiptap/extension-image';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, tableEditing} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';

const widths = await loadTsModule('./ui/word-table-widths.ts');
const {WordTable, FixedTableView, isTableColumnWidths, parseTableColumnWidthPoints, parseTableColumnWidthsMetadata,
  validateTableWidthsDocument, omitDefaultTableWidths, fixedTableDOM, selectedTableColumn,
  createTableColumnWidthTransaction, createTableStructureTransaction, tableWidthGuard} = widths;
const schema = getSchema([StarterKit, TableKit.configure({table: false}), WordTable, Image]);
const paragraph = text => schema.node('paragraph', null, text ? schema.text(text) : null);
const makeTable = (widths = null, rows = 3, columns = widths?.length ?? 3) => schema.node('table', {columnWidthsTwips: widths},
  Array.from({length: rows}, (_, row) => schema.node('tableRow', null,
    Array.from({length: columns}, (_, column) => schema.node(row ? 'tableCell' : 'tableHeader', null, [paragraph(`${row}:${column}`)])))));
const doc = children => schema.node('doc', null, children);
const cells = document => {
  const result = [];
  document.descendants((node, pos) => { if (['tableCell', 'tableHeader'].includes(node.type.name)) result.push({node, pos}); });
  return result;
};
const stateAt = (document, cell = 0, plugins = [history(), tableEditing(), tableWidthGuard()]) => EditorState.create({
  doc: document, selection: TextSelection.create(document, cells(document)[cell].pos + 2), plugins,
});
const snap = document => omitDefaultTableWidths(document.toJSON());
const change = (state, transaction) => state.apply(closeHistory(transaction));
const tableOf = state => { let result; state.doc.descendants(node => { if (node.type.name === 'table') {result ??= node; return false;} }); return result; };

test('point input is exact to one twip and rejects zero, fractional twips, non-finite values and malformed units', () => {
  for (const [input, expected] of [['.05', 1], ['1.10', 22], ['75', 1500], ['1584', 31680], [' 2.05 ', 41]]) expect(parseTableColumnWidthPoints(input)).toBe(expected);
  for (const input of ['', '0', '-1', '+1', '1e2', '1pt', '0.01', '1584.05', 'NaN', 'Infinity', '0.05000000000000001', '9'.repeat(65)]) {
    expect(parseTableColumnWidthPoints(input)).toBeNull();
  }
  for (let twips = 1; twips <= 31680; twips++) if (parseTableColumnWidthPoints(String(twips / 20)) !== twips) throw new Error(`Roundtrip changed ${twips}`);
});

test('canonical array validates per-column, count and sum limits without accepting coercions', () => {
  for (const valid of [[1], [31680], [1500, 2011], Array(50).fill(1), [1, 31679]]) expect(isTableColumnWidths(valid)).toBe(true);
  for (const invalid of [null, undefined, [], Array(3), [1, , 2], [0], [-1], [1.5], ['20'], [true], [NaN], [Infinity], [31681], [20000, 11681], Array(51).fill(1)]) {
    expect(isTableColumnWidths(invalid)).toBe(false);
  }
});

test('HTML accepts only bounded exact twip metadata, and CSS rendering keeps 15 twips per pixel', () => {
  expect(parseTableColumnWidthsMetadata('1500,2011,1')).toEqual([1500, 2011, 1]);
  expect(parseTableColumnWidthsMetadata(Array(50).fill(600).join(','))).toEqual(Array(50).fill(600));
  for (const value of [null, '', '1500px', '1.5,20', '1, 2', '0,1', '1e3', '20000,20000', '9'.repeat(301)]) expect(parseTableColumnWidthsMetadata(value)).toBeNull();
  const render = fixedTableDOM([1500, 2011, 1]);
  expect(render[1]).toEqual({'data-column-widths-twips': '1500,2011,1', style: 'table-layout: fixed; width: 234.13333333333333px; min-width: 0'});
  expect(render[2]).toEqual(['colgroup', {}, ['col', {style: 'width: 100px'}], ['col', {style: 'width: 134.06666666666666px'}], ['col', {style: 'width: 0.06666666666666667px'}]]);
  const table = makeTable([1500, 2011, 1]);
  expect(schema.nodes.table.spec.toDOM(table)).toEqual(render);
  expect(schema.nodes.table.spec.toDOM(table)[1]).not.toHaveProperty('columnWidthsTwips');
  expect(() => fixedTableDOM([0])).toThrow('固定列宽');
});

test('tables without new attributes retain legacy serialized model and DOM declarations', () => {
  const legacySchema = getSchema([StarterKit, TableKit, Image]);
  const legacy = legacySchema.nodeFromJSON(snap(doc([makeTable()])));
  const upgraded = schema.nodeFromJSON(legacy.toJSON());
  expect(JSON.stringify(snap(upgraded))).toBe(JSON.stringify(legacy.toJSON()));
  expect(schema.nodes.table.spec.toDOM(upgraded.firstChild)).toEqual(legacySchema.nodes.table.spec.toDOM(legacy.firstChild));
  expect(upgraded.firstChild.attrs.columnWidthsTwips).toBeNull();
});

test('incoming invalid, misplaced, ragged, merged and nested fixed geometry is rejected', () => {
  for (const value of [null, '1200', [0], [1.5], [1500], [20000, 20000, 1]]) {
    const incoming = snap(doc([makeTable()])); incoming.content[0].attrs = {columnWidthsTwips: value};
    expect(() => validateTableWidthsDocument(incoming)).toThrow('固定列宽');
  }
  expect(() => validateTableWidthsDocument({type: 'doc', attrs: {columnWidthsTwips: [1]}, content: []})).toThrow('固定列宽');
  for (const mutation of [
    table => table.content[1].content.pop(),
    table => { table.content[0].content[0].attrs.colspan = 2; },
    table => { table.content[0].content[0].attrs.rowspan = 2; },
    table => { table.content[0].content[0].attrs.colwidth = [100]; },
    table => { table.content[0].content[0].attrs.colwidth = [1500]; },
    table => { table.content[0].content[0].content.push(snap(makeTable(null, 1, 1))); },
  ]) {
    const incoming = snap(doc([makeTable([1500, 1500, 1500])])); mutation(incoming.content[0]);
    expect(() => validateTableWidthsDocument(incoming)).toThrow();
  }
  const outer = snap(makeTable(null, 1, 1)); outer.content[0].content[0].content.push(snap(makeTable([1500])));
  expect(() => validateTableWidthsDocument(doc([schema.nodeFromJSON(outer)]).toJSON())).toThrow('固定列宽');
  expect(() => validateTableWidthsDocument(snap(doc([makeTable(Array(50).fill(10), 500)])))).not.toThrow();
});

test('snapshots remove only new internal defaults and fixed-cell null widths without losing profile or values', () => {
  const incoming = snap(doc([makeTable([1, 2011, 3000]), makeTable()]));
  incoming.attrs = {styleProfile: 'standard-v2'};
  const internal = schema.nodeFromJSON(incoming).toJSON();
  const saved = omitDefaultTableWidths({...internal, attrs: incoming.attrs});
  expect(saved.attrs).toEqual({styleProfile: 'standard-v2'});
  expect(saved.content[0].attrs).toEqual({columnWidthsTwips: [1, 2011, 3000]});
  expect(saved.content[0].content[0].content[0].attrs).not.toHaveProperty('colwidth');
  expect(saved.content[1]).not.toHaveProperty('attrs');
  expect(saved.content[1].content[0].content[0].attrs).toHaveProperty('colwidth', null);
  expect(internal.content[1].attrs).toHaveProperty('columnWidthsTwips', null);
  expect(snap(schema.nodeFromJSON(saved))).toEqual({...saved, attrs: undefined});
});

test('caret changes one canonical column for every row without touching cell content or pixel attrs', () => {
  const original = doc([paragraph('before'), makeTable([1500, 2200, 3100]), paragraph('after')]);
  const state = stateAt(original, 4);
  expect(selectedTableColumn(state)).toMatchObject({column: 1, row: 1, widths: [1500, 2200, 3100]});
  const transaction = createTableColumnWidthTransaction(state, 2011);
  const updated = transaction.doc.child(1);
  expect(updated.attrs.columnWidthsTwips).toEqual([1500, 2011, 3100]);
  expect(updated.content.eq(original.child(1).content)).toBe(true);
  expect(transaction.selection.eq(state.selection)).toBe(true);
  expect(cells(transaction.doc).every(({node}) => node.attrs.colwidth === null)).toBe(true);
  expect(state.doc).toBe(original);
  expect(createTableColumnWidthTransaction(state.apply(transaction), 2011)).toBeNull();
});

test('auto geometry initializes a 450pt evenly distributed table and reset omits geometry', () => {
  for (const columns of [1, 3, 7, 23, 50]) {
    const state = stateAt(doc([makeTable(null, 2, columns)]), columns - 1);
    const expected = Array.from({length: columns}, (_, i) => Math.floor(9000 / columns) + Number(i < 9000 % columns));
    expected[columns - 1] = 41;
    const set = createTableColumnWidthTransaction(state, 41);
    expect(set.doc.firstChild.attrs.columnWidthsTwips).toEqual(expected);
    const reset = createTableColumnWidthTransaction(state.apply(set), null);
    expect(snap(reset.doc).content[0]).not.toHaveProperty('attrs');
    expect(reset.doc.eq(state.doc)).toBe(true);
  }
  expect(createTableColumnWidthTransaction(stateAt(doc([makeTable()])), null)).toBeNull();
});

test('current column follows caret and cell-selection head in either direction, with selection preserved', () => {
  const document = doc([makeTable([1100, 2200, 3300])]);
  const positions = cells(document);
  for (const [anchor, head, column] of [[0, 8, 2], [8, 0, 0], [1, 7, 1]]) {
    const selection = CellSelection.create(document, positions[anchor].pos, positions[head].pos);
    const state = EditorState.create({doc: document, selection});
    expect(selectedTableColumn(state).column).toBe(column);
    const transaction = createTableColumnWidthTransaction(state, 501);
    expect(transaction.doc.firstChild.attrs.columnWidthsTwips[column]).toBe(501);
    expect(transaction.selection.eq(selection)).toBe(true);
  }
  for (let column = 0; column < 3; column++) {
    const selection = NodeSelection.create(document, positions[3 + column].pos);
    const state = EditorState.create({doc: document, selection});
    expect(selectedTableColumn(state).column).toBe(column);
    expect(createTableColumnWidthTransaction(state, 501).doc.firstChild.attrs.columnWidthsTwips[column]).toBe(501);
  }
  const withAfter = doc([makeTable([1]), paragraph('outside')]);
  for (const selection of [new AllSelection(withAfter), NodeSelection.create(withAfter, 0), TextSelection.atEnd(withAfter)]) {
    const state = EditorState.create({doc: withAfter, selection});
    expect(selectedTableColumn(state)).toBeNull();
    expect(createTableColumnWidthTransaction(state, 20)).toBeNull();
  }
});

for (const action of ['addColumnBefore', 'addColumnAfter']) test(`${action} retains unequal widths at the first, middle and last columns`, () => {
  for (let column = 0; column < 3; column++) {
    const state = stateAt(doc([makeTable([1101, 2202, 3303])]), 3 + column);
    const transaction = createTableStructureTransaction(state, action);
    const expected = [1101, 2202, 3303]; expected.splice(column + Number(action === 'addColumnAfter'), 0, expected[column]);
    expect(transaction.doc.firstChild.attrs.columnWidthsTwips).toEqual(expected);
    for (let row = 0; row < 3; row++) expect(transaction.doc.firstChild.child(row).childCount).toBe(4);
    expect(() => validateTableWidthsDocument(snap(transaction.doc))).not.toThrow();
    expect(state.doc.firstChild.child(0).childCount).toBe(3);
  }
});

test('deleting first, middle and last unequal columns retains the matching widths and caret in the surviving table', () => {
  for (let column = 0; column < 3; column++) {
    const state = stateAt(doc([makeTable([1101, 2202, 3303]), makeTable([9000])]), 6 + column);
    const transaction = createTableStructureTransaction(state, 'deleteColumn');
    const expected = [1101, 2202, 3303]; expected.splice(column, 1);
    expect(transaction.doc.firstChild.attrs.columnWidthsTwips).toEqual(expected);
    expect(selectedTableColumn(state.apply(transaction))).toMatchObject({pos: 0, row: 2, column: Math.min(column, 1), widths: expected});
    expect(transaction.doc.lastChild.eq(state.doc.lastChild)).toBe(true);
  }
});

test('multi-column deletion removes the full selected rectangle in one coherent transaction', () => {
  const document = doc([makeTable([1100, 2200, 3300, 4400])]), positions = cells(document);
  const state = EditorState.create({doc: document, selection: CellSelection.create(document, positions[1].pos, positions[10].pos), plugins: [tableWidthGuard()]});
  const transaction = createTableStructureTransaction(state, 'deleteColumn');
  expect(state.applyTransaction(transaction).transactions).toHaveLength(1);
  expect(transaction.doc.firstChild.attrs.columnWidthsTwips).toEqual([1100, 4400]);
  expect(transaction.doc.firstChild.child(2).childCount).toBe(2);
});

test('row insertion and deletion retain geometry and keep terminal row caret inside its table', () => {
  for (const action of ['addRowBefore', 'addRowAfter', 'deleteRow']) for (let row = 0; row < 3; row++) {
    const state = stateAt(doc([makeTable([1101, 2202, 3303]), makeTable([9000])]), row * 3 + 2);
    const transaction = createTableStructureTransaction(state, action);
    expect(transaction.doc.firstChild.attrs.columnWidthsTwips).toEqual([1101, 2202, 3303]);
    expect(transaction.doc.firstChild.childCount).toBe(action === 'deleteRow' ? 2 : 4);
    expect(selectedTableColumn(state.apply(transaction)).pos).toBe(0);
    expect(() => validateTableWidthsDocument(snap(transaction.doc))).not.toThrow();
  }
});

test('last-column, last-row and explicit table deletion remove geometry with the table and undo restores it', () => {
  for (const [action, rows, columns] of [['deleteColumn', 3, 1], ['deleteRow', 1, 3], ['deleteTable', 3, 3]]) {
    const original = doc([makeTable(Array(columns).fill(1500), rows), paragraph('after')]);
    let state = stateAt(original);
    state = change(state, createTableStructureTransaction(state, action));
    expect(tableOf(state)).toBeUndefined();
    expect(state.doc.textContent).toBe('after');
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
    expect(state.doc.eq(original)).toBe(true);
  }
});

test('width, reset and structural changes each undo/redo once without merging with prior or later typing', () => {
  for (const make of [state => createTableColumnWidthTransaction(state, 2011), state => createTableColumnWidthTransaction(state, null),
    state => createTableStructureTransaction(state, 'addColumnAfter'), state => createTableStructureTransaction(state, 'deleteColumn'),
    state => createTableStructureTransaction(state, 'addRowAfter'), state => createTableStructureTransaction(state, 'deleteRow')]) {
    let state = stateAt(doc([makeTable([1101, 2202, 3303])]), 4);
    state = state.apply(state.tr.insertText('prior'));
    const original = state.doc;
    state = change(state, make(state)); state = state.apply(closeHistory(state.tr));
    const edited = state.doc;
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
    expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
    state = state.apply(closeHistory(state.tr)); state = state.apply(state.tr.insertText('later'));
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
  }
});

test('sum, range, size and unsupported geometry rejection leaves document, selection and undo history unchanged', () => {
  const state = stateAt(doc([makeTable([15000, 16000])]));
  for (const value of [0, -1, 0.5, NaN, Infinity, 31681, '1200', undefined, 16000]) {
    expect(() => createTableColumnWidthTransaction(state, value)).toThrow('固定列宽');
    expect(undo(state)).toBe(false);
    expect(state.doc.firstChild.attrs.columnWidthsTwips).toEqual([15000, 16000]);
  }
  expect(() => createTableStructureTransaction(state, 'addColumnAfter')).toThrow('固定列宽');
  expect(() => createTableStructureTransaction(stateAt(doc([makeTable(Array(50).fill(1))])), 'addColumnBefore')).toThrow('50 列');
  expect(() => createTableStructureTransaction(stateAt(doc([makeTable([1], 500)])), 'addRowAfter')).toThrow('500 行');
  const pixelTable = structuredClone(makeTable().toJSON()); pixelTable.content[0].content[0].attrs.colwidth = [100];
  expect(() => createTableColumnWidthTransaction(stateAt(doc([schema.nodeFromJSON(pixelTable)])), 1000)).toThrow('colwidth');
});

test('whole prospective document structure, text and byte budgets are enforced before mutation', () => {
  const large = stateAt(doc([makeTable([1500]), ...Array.from({length: 5000}, () => paragraph('x'))]));
  expect(() => createTableColumnWidthTransaction(large, 1501)).toThrow('文档结构');
  const text = stateAt(doc([makeTable([1500]), paragraph('中'.repeat(700000))]));
  expect(() => createTableColumnWidthTransaction(text, 1501)).toThrow('2 MiB');
  let nested = paragraph('deep');
  for (let depth = 0; depth < 33; depth++) nested = schema.node('blockquote', null, [nested]);
  expect(() => createTableColumnWidthTransaction(stateAt(doc([makeTable([1500]), nested])), 1501)).toThrow('文档结构');
  const image = stateAt(doc([makeTable([1500]), schema.node('image', {src: 'A'.repeat(32 * 1024 * 1024)})]));
  expect(() => createTableColumnWidthTransaction(image, 1501)).toThrow('32 MiB');
  expect(undo(image)).toBe(false);
});

test('guard rejects pasted or programmatic inconsistent tables atomically without adding undo entries', () => {
  const errors = [];
  const state = stateAt(doc([makeTable([1500, 2200, 3300])]), 0, [history(), tableWidthGuard(error => errors.push(error))]);
  const invalid = state.tr.setNodeMarkup(0, undefined, {columnWidthsTwips: [1500]});
  const filtered = state.applyTransaction(invalid);
  expect(filtered.state).toBe(state); expect(filtered.transactions).toEqual([]); expect(errors).toHaveLength(1);
  expect(undo(filtered.state)).toBe(false);
  const cell = cells(state.doc)[0];
  expect(state.applyTransaction(state.tr.setNodeMarkup(cell.pos, undefined, {...cell.node.attrs, colwidth: [100]})).transactions).toEqual([]);
  const valid = createTableStructureTransaction(state, 'addColumnAfter');
  expect(state.applyTransaction(valid).transactions).toHaveLength(1);
});

test('paste-like transactions with supported geometry but excessive nodes, depth, text or image bytes are rejected', () => {
  const state = stateAt(doc([makeTable([1500])]));
  let deep = paragraph('nested');
  for (let depth = 0; depth < 33; depth++) deep = schema.node('blockquote', null, [deep]);
  for (const pasted of [doc([makeTable(Array(50).fill(10), 500)]),
    doc([makeTable([1500]), paragraph('中'.repeat(700000))]), doc([makeTable([1500]), deep]),
    doc([makeTable([1500]), schema.node('image', {src: 'A'.repeat(32 * 1024 * 1024)})])]) {
    const transaction = state.tr.replaceWith(0, state.doc.content.size, pasted.content);
    const result = state.applyTransaction(transaction);
    expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(result.state)).toBe(false);
  }
});

test('legacy structural edits still omit new geometry and preserve auto behavior', () => {
  for (const action of ['addColumnAfter', 'deleteColumn', 'addRowAfter', 'deleteRow']) {
    const transaction = createTableStructureTransaction(stateAt(doc([makeTable()])), action);
    expect(snap(transaction.doc).content[0]).not.toHaveProperty('attrs');
  }
});

test('Tiptap CommandManager direct, chained and capability calls never leak rejected structural steps', () => {
  const errors = [];
  const extension = WordTable.configure({onWidthError: error => errors.push(error)});
  const rawCommands = extension.config.addCommands.call({options: extension.options, parent: () => ({})});
  const harness = initial => {
    let current = initial, dispatches = 0;
    const editor = {extensionManager: {commands: rawCommands}, get state() {return current;},
      view: {dispatch(tr) {current = current.apply(tr); dispatches++;}}};
    return {manager: new CommandManager({editor}), get state() {return current;}, get dispatches() {return dispatches;}};
  };
  const safe = harness(stateAt(doc([makeTable([1101, 2202, 3303])]), 1));
  expect(safe.manager.can().addColumnAfter()).toBe(true);
  expect(safe.state.doc.firstChild.child(0).childCount).toBe(3);
  expect(safe.manager.chain().addColumnAfter().addRowAfter().run()).toBe(true);
  expect(safe.state.doc.firstChild.attrs.columnWidthsTwips).toEqual([1101, 2202, 2202, 3303]);
  expect(safe.state.doc.firstChild.childCount).toBe(4); expect(safe.dispatches).toBe(1);
  const validDirect = harness(stateAt(doc([makeTable([1101, 2202, 3303])]), 1));
  expect(validDirect.manager.commands.deleteColumn()).toBe(true);
  expect(validDirect.state.doc.firstChild.attrs.columnWidthsTwips).toEqual([1101, 3303]);
  const oversized = doc([makeTable([1500]), ...Array.from({length: 5000}, () => paragraph('x'))]);
  for (const run of [manager => manager.commands.addRowAfter(), manager => manager.chain().addRowAfter().run(),
    manager => manager.can().addRowAfter(), manager => manager.can().chain().addRowAfter().run()]) {
    const rejected = harness(stateAt(oversized));
    const selection = rejected.state.selection;
    expect(run(rejected.manager)).toBe(false);
    expect(rejected.state.doc).toBe(oversized);
    expect(rejected.state.selection.eq(selection)).toBe(true);
    expect(undo(rejected.state)).toBe(false);
  }
  expect(errors).toHaveLength(2);
});

test('mounted non-resizable table view follows canonical geometry on mount, update, row change and reset', () => {
  // Minimal DOM contract checks only; these are not browser layout measurements.
  class Element {
    children = []; attrs = {}; parentNode = null;
    style = {width: '', minWidth: '', setProperty(key, value) {this[key === 'min-width' ? 'minWidth' : key] = value;}};
    ownerDocument = {createElement: () => new Element()};
    get firstChild() {return this.children[0] ?? null;}
    get nextSibling() {return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null;}
    appendChild(child) {child.parentNode = this; this.children.push(child); return child;}
    removeChild(child) {this.children.splice(this.children.indexOf(child), 1); child.parentNode = null;}
    replaceChildren(...children) {this.children = []; children.forEach(child => this.appendChild(child));}
    setAttribute(key, value) {this.attrs[key] = value;}
    removeAttribute(key) {delete this.attrs[key];}
  }
  const old = globalThis.document; globalThis.document = {createElement: () => new Element()};
  try {
    const view = new FixedTableView(makeTable([1500, 2011]), 25);
    expect(view.table.style.width).toBe(`${3511 / 15}px`);
    expect(view.colgroup.children.map(col => col.style.width)).toEqual(['100px', `${2011 / 15}px`]);
    expect(view.table.attrs['data-column-widths-twips']).toBe('1500,2011');
    expect(view.update(makeTable([3000, 6000, 1500], 4))).toBe(true);
    expect(view.colgroup.children.map(col => col.style.width)).toEqual(['200px', '400px', '100px']);
    expect(view.table.style.width).toBe('700px');
    expect(view.update(makeTable(null, 4))).toBe(true);
    expect(view.table.attrs).not.toHaveProperty('data-column-widths-twips');
    expect(view.table.style.width).toBe(''); expect(view.table.style.minWidth).toBe('75px');
    expect(view.table.style.tableLayout).toBe('');
    expect(view.colgroup.children.map(col => col.style.width)).toEqual(['', '', '']);
    expect(view.update(makeTable([3000, 6000, 1500], 4))).toBe(true);
    expect(view.colgroup.children.map(col => col.style.width)).toEqual(['200px', '400px', '100px']);
    expect(view.update(makeTable(null, 4))).toBe(true);
    expect(view.colgroup.children.map(col => col.style.width)).toEqual(['', '', '']);
    expect(view.update(paragraph('no table'))).toBe(false);
  } finally {globalThis.document = old;}
});
