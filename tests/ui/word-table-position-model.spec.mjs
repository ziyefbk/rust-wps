import {test, expect} from '@playwright/test';
import {CommandManager, getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import Image from '@tiptap/extension-image';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, tableEditing} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {readFileSync} from 'node:fs';
import {loadTsModule} from './load-ts.mjs';

const {WordTable, FixedTableView, createTablePositionTransaction, createTableColumnWidthTransaction,
  createTableStructureTransaction, omitDefaultTableWidths, validateTableWidthsDocument, tableWidthGuard} = await loadTsModule('./ui/word-table-widths.ts');
const {isTableAlignment, isTableIndent, parseTableAlignMetadata, parseTableIndentMetadata, parseTableIndentPoints,
  selectedTablePosition, selectedPositionTables, tablePositionMargins, tablePositionDOM} = await loadTsModule('./ui/word-table-position.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const schema = getSchema([StarterKit, TableKit.configure({table: false}), WordTable, Image]);
const p = text => schema.node('paragraph', null, text ? schema.text(text) : null);
const table = (attrs = {}, rows = 2, cols = attrs.columnWidthsTwips?.length ?? 3) => schema.node('table', attrs,
  Array.from({length: rows}, (_, row) => schema.node('tableRow', null,
    Array.from({length: cols}, (_, col) => schema.node(row ? 'tableCell' : 'tableHeader', null, [p(`${row}:${col}`)])))));
const doc = nodes => schema.node('doc', null, nodes);
const snap = document => omitDefaultTableWidths(document.toJSON());
const cells = document => {
  const found = []; document.descendants((node, pos) => {if (['tableCell', 'tableHeader'].includes(node.type.name)) found.push({node, pos});}); return found;
};
const stateAt = (document, cell = 0, plugins = [history(), tableEditing(), tableWidthGuard()]) => EditorState.create({doc: document,
  selection: TextSelection.create(document, cells(document)[cell].pos + 2), plugins});
const all = document => EditorState.create({doc: document, selection: new AllSelection(document), plugins: [history(), tableWidthGuard()]});
const attrs = state => selectedPositionTables(state).map(({table}) => ({...table.attrs}));
const apply = (state, patch) => state.apply(closeHistory(createTablePositionTransaction(state, patch)));

// These use real ProseMirror schemas, selections, transactions, history and
// Tiptap commands without launching a browser. DOM shims below test contracts,
// not computed layout, visual fidelity or native focus.
test('position scalar and decimal validation is exact, bounded and non-coercing', () => {
  for (const value of ['left', 'center', 'right']) expect(isTableAlignment(value)).toBe(true);
  for (const value of [null, undefined, '', 'justify', 'LEFT', ' left ', 0, false]) expect(isTableAlignment(value)).toBe(false);
  for (const value of [0, 1, 720, 31680]) expect(isTableIndent(value)).toBe(true);
  for (const value of [null, undefined, -1, .1, 31681, '0', false, NaN, Infinity]) expect(isTableIndent(value)).toBe(false);
  for (const [value, expected] of [['0', 0], ['.05', 1], ['36', 720], ['1584', 31680], [' 1.1 ', 22]]) expect(parseTableIndentPoints(value)).toBe(expected);
  for (const value of ['', '-1', '+1', '0.01', '1e2', '1pt', '1584.05', 'NaN', 'Infinity', '0.05000000000000001']) expect(parseTableIndentPoints(value)).toBeNull();
  for (let value = 0; value <= 31680; value++) if (parseTableIndentPoints(String(value / 20)) !== value) throw new Error(`Changed ${value}`);
});

test('clipboard parsers accept exact own metadata and never infer alignment or indentation from CSS', () => {
  for (const value of ['left', 'center', 'right']) expect(parseTableAlignMetadata(value)).toBe(value);
  for (const value of [null, '', 'justify', 'start', 'end', 'center ', 'CENTER']) expect(parseTableAlignMetadata(value)).toBeNull();
  for (const [value, expected] of [['0', 0], ['1', 1], ['720', 720], ['31680', 31680]]) expect(parseTableIndentMetadata(value)).toBe(expected);
  for (const value of [null, '', '-1', '0.1', '31681', '1e2', '720pt', ' 720', '9'.repeat(100)]) expect(parseTableIndentMetadata(value)).toBeNull();
  const extensionAttrs = WordTable.config.addAttributes.call({parent: () => ({})});
  const foreign = {style: {marginLeft: '36pt', marginRight: 'auto', textAlign: 'center'}, getAttribute: () => null};
  expect(extensionAttrs.tableAlign.parseHTML(foreign)).toBeNull();
  expect(extensionAttrs.tableIndentTwips.parseHTML(foreign)).toBeNull();
  expect(extensionAttrs.tableAlign.rendered).toBe(false); expect(extensionAttrs.tableIndentTwips.rendered).toBe(false);
});

test('legacy auto and fixed table JSON/DOM stay byte-identical when position is absent', () => {
  const legacySchema = getSchema([StarterKit, TableKit, Image]);
  const old = legacySchema.nodeFromJSON(snap(doc([table()])));
  const upgraded = schema.nodeFromJSON(old.toJSON());
  expect(JSON.stringify(snap(upgraded))).toBe(JSON.stringify(old.toJSON()));
  expect(schema.nodes.table.spec.toDOM(upgraded.firstChild)).toEqual(legacySchema.nodes.table.spec.toDOM(old.firstChild));
  expect(tablePositionDOM(upgraded.firstChild.attrs, true)).toEqual({});
  const fixed = snap(doc([table({columnWidthsTwips: [1000, 2000, 3000]})]));
  expect(fixed.content[0].attrs).toEqual({columnWidthsTwips: [1000, 2000, 3000]});
  expect(JSON.stringify(snap(schema.nodeFromJSON(fixed)))).toBe(JSON.stringify(fixed));
});

test('incoming JSON rejects null, invalid types and misplaced positioning before schema normalization', () => {
  for (const [field, values] of [['tableAlign', [null, '', 'justify', 'start', 0, false, [], {}]],
    ['tableIndentTwips', [null, '', '0', -1, .5, 31681, NaN, Infinity, [], {}]]]) {
    for (const value of values) {
      const input = snap(doc([table()])); input.content[0].attrs = {[field]: value};
      expect(() => validateTableWidthsDocument(input)).toThrow('表格位置');
    }
    for (const type of ['doc', 'paragraph', 'tableRow', 'tableCell', 'text']) {
      expect(() => validateTableWidthsDocument({type, attrs: {[field]: field === 'tableAlign' ? 'left' : 0}})).toThrow('表格位置');
    }
  }
  for (const tableAlign of ['left', 'center', 'right']) expect(() => validateTableWidthsDocument(snap(doc([table({tableAlign, tableIndentTwips: 0})])))).not.toThrow();
});

test('positioned geometry rejects ragged, merged and nested tables, including explicit zero alone', () => {
  for (const newAttrs of [{tableAlign: 'left'}, {tableIndentTwips: 0}, {tableAlign: 'right', tableIndentTwips: 0}]) {
    for (const mutate of [node => node.content[1].content.pop(), node => {node.content[0].content[0].attrs.colspan = 2;},
      node => {node.content[0].content[0].attrs.rowspan = 2;}, node => {node.content[0].content[0].content.push(snap(table({}, 1, 1)));}]) {
      const input = snap(doc([table(newAttrs)])); mutate(input.content[0]);
      expect(() => validateTableWidthsDocument(input)).toThrow();
    }
    const outer = snap(table({}, 1, 1)); outer.content[0].content[0].content.push(snap(table(newAttrs)));
    expect(() => validateTableWidthsDocument({type: 'doc', content: [outer]})).toThrow();
  }
  const oldNested = snap(table({}, 1, 1)); oldNested.content[0].content[0].content.push(snap(table({}, 1, 1)));
  expect(() => validateTableWidthsDocument({type: 'doc', content: [oldNested]})).not.toThrow();
});

test('total width plus indent checks exact fixed and 9000-twip auto limits', () => {
  for (const attr of [{tableIndentTwips: 22680}, {columnWidthsTwips: [1], tableIndentTwips: 31679},
    {columnWidthsTwips: [10000, 20000], tableAlign: 'left', tableIndentTwips: 1680}]) expect(() => validateTableWidthsDocument(snap(doc([table(attr)])))).not.toThrow();
  for (const attr of [{tableIndentTwips: 22681}, {columnWidthsTwips: [1], tableIndentTwips: 31680},
    {columnWidthsTwips: [10000, 20000], tableIndentTwips: 1681}, {tableAlign: 'center', tableIndentTwips: 1}, {tableAlign: 'right', tableIndentTwips: 720}]) {
    expect(() => validateTableWidthsDocument(snap(doc([table(attr)])))).toThrow('表格位置');
  }
});

test('caret, cell and node selections target the whole current table without selecting neighbours', () => {
  const document = doc([p('before'), table({tableAlign: 'right'}), p('between'), table({tableIndentTwips: 720}), p('after')]);
  const positions = cells(document);
  const selections = [TextSelection.create(document, positions[2].pos + 2), CellSelection.create(document, positions[0].pos, positions[5].pos),
    CellSelection.create(document, positions[5].pos, positions[0].pos), NodeSelection.create(document, positions[1].pos), NodeSelection.create(document, document.firstChild.nodeSize)];
  for (const selection of selections) {
    const state = EditorState.create({doc: document, selection});
    expect(selectedTablePosition(state)).toMatchObject({count: 1, supported: true, tableAlign: {value: 'right', mixed: false}, tableIndentTwips: {value: null, mixed: false}});
    const tr = createTablePositionTransaction(state, {tableAlign: 'left'});
    expect(tr.selection.eq(selection)).toBe(true); expect(tr.doc.child(1).attrs.tableAlign).toBe('left');
    expect(tr.doc.child(3).attrs.tableIndentTwips).toBe(720);
    expect(tr.doc.child(1).content.eq(document.child(1).content)).toBe(true);
  }
  const outside = EditorState.create({doc: document, selection: TextSelection.create(document, 2)});
  expect(selectedTablePosition(outside)).toMatchObject({count: 0, supported: false, hasExplicit: false});
  expect(createTablePositionTransaction(outside, {tableAlign: 'center'})).toBeNull();
});

test('multi-table forward/backward and all selection expose per-field mixed versus explicit/default states', () => {
  const document = doc([table({tableIndentTwips: 0}), p('between'), table({tableAlign: 'left'}), table({tableAlign: 'right', tableIndentTwips: 0})]);
  const positions = cells(document);
  for (const selection of [new AllSelection(document), TextSelection.create(document, positions[0].pos + 2, positions.at(-1).pos + 3),
    TextSelection.create(document, positions.at(-1).pos + 3, positions[0].pos + 2)]) {
    const state = EditorState.create({doc: document, selection});
    expect(selectedTablePosition(state)).toMatchObject({count: 3, supported: true, tableAlign: {value: null, mixed: true}, tableIndentTwips: {value: null, mixed: true}});
    const changed = apply(state, {tableAlign: 'center'});
    expect(attrs(changed).map(attr => attr.tableAlign)).toEqual(['center', 'center', 'center']);
    expect(attrs(changed).map(attr => attr.tableIndentTwips)).toEqual([0, null, 0]);
    expect(changed.selection.eq(selection)).toBe(true);
  }
  expect(selectedTablePosition(stateAt(doc([table()])))).toMatchObject({tableAlign: {value: null, mixed: false}, tableIndentTwips: {value: null, mixed: false}, hasExplicit: false});
});

test('legacy unsupported tables disable position edits without changing content', () => {
  const raw = snap(table({}, 1, 1)); raw.content[0].content[0].content.push(snap(table({}, 1, 1)));
  const document = doc([schema.nodeFromJSON(raw)]);
  for (const state of [stateAt(document), stateAt(document, 1), all(document)]) {
    expect(selectedTablePosition(state).supported).toBe(false);
    expect(() => createTablePositionTransaction(state, {tableAlign: 'left'})).toThrow('表格位置');
    expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

test('center/right atomically remove positive indent while retaining explicit zero and widths', () => {
  for (const alignment of ['center', 'right']) for (const indent of [0, 720]) {
    const state = stateAt(doc([table({columnWidthsTwips: [1000, 2000, 3000], tableAlign: 'left', tableIndentTwips: indent})]));
    const tr = createTablePositionTransaction(state, {tableAlign: alignment});
    expect(tr.steps).toHaveLength(1);
    expect(tr.doc.firstChild.attrs).toMatchObject({tableAlign: alignment, tableIndentTwips: indent ? null : 0, columnWidthsTwips: [1000, 2000, 3000]});
    expect(tr.doc.firstChild.content.eq(state.doc.firstChild.content)).toBe(true);
    const changed = state.apply(tr); let undone;
    expect(undo(changed, tr => {undone = changed.apply(tr);})).toBe(true); expect(undone.doc.eq(state.doc)).toBe(true);
  }
});

test('positive indent on center/right and contradictory patches reject without silently changing alignment', () => {
  for (const alignment of ['center', 'right']) {
    const state = stateAt(doc([table({tableAlign: alignment})]));
    expect(() => createTablePositionTransaction(state, {tableIndentTwips: 1})).toThrow('正缩进');
    expect(attrs(state)[0].tableAlign).toBe(alignment); expect(undo(state)).toBe(false);
    const zero = apply(state, {tableIndentTwips: 0}); expect(attrs(zero)[0].tableIndentTwips).toBe(0);
    expect(() => createTablePositionTransaction(state, {tableAlign: alignment, tableIndentTwips: 720})).toThrow('表格位置');
  }
  for (const patch of [{bad: 1}, {tableAlign: undefined}, {tableIndentTwips: undefined}, {tableIndentTwips: '720'}, {tableAlign: 'justify'}]) {
    expect(() => createTablePositionTransaction(stateAt(doc([table()])), patch)).toThrow('表格位置');
  }
});

test('separate alignment/indent resets preserve explicit zero, fixed widths and byte-stable drafts', () => {
  const state = stateAt(doc([table({columnWidthsTwips: [1000, 2000, 3000], tableAlign: 'left', tableIndentTwips: 0})]));
  const alignReset = apply(state, {tableAlign: null});
  expect(snap(alignReset.doc).content[0].attrs).toEqual({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 0});
  const indentReset = apply(state, {tableIndentTwips: null});
  expect(snap(indentReset.doc).content[0].attrs).toEqual({columnWidthsTwips: [1000, 2000, 3000], tableAlign: 'left'});
  const both = apply(state, {tableAlign: null, tableIndentTwips: null});
  expect(snap(both.doc).content[0].attrs).toEqual({columnWidthsTwips: [1000, 2000, 3000]});
  const incoming = JSON.parse(JSON.stringify(snap(state.doc)));
  validateTableWidthsDocument(incoming); expect(snap(schema.nodeFromJSON(incoming))).toEqual(incoming);
  expect(createTablePositionTransaction(state, {tableIndentTwips: 0})).toBeNull();
  expect(createTablePositionTransaction(state, {})).toBeNull();
});

test('caret position changes preserve stored marks, selection, paragraphs and all cell content', () => {
  const document = doc([table({columnWidthsTwips: [1000, 2000, 3000]}), p('after')]);
  const bold = schema.marks.bold.create();
  const state = EditorState.create({doc: document, selection: stateAt(document).selection, storedMarks: [bold]});
  const tr = createTablePositionTransaction(state, {tableIndentTwips: 1});
  expect(tr.storedMarks).toEqual([bold]); expect(tr.selection.eq(state.selection)).toBe(true);
  expect(tr.doc.firstChild.content.eq(document.firstChild.content)).toBe(true); expect(tr.doc.lastChild.eq(document.lastChild)).toBe(true);
  expect(state.doc).toBe(document);
});

test('multi-table bound failure is atomic even after an earlier candidate was valid', () => {
  const document = doc([table({columnWidthsTwips: [1500]}), table({columnWidthsTwips: [31000]})]);
  const state = all(document), selection = state.selection;
  expect(() => createTablePositionTransaction(state, {tableIndentTwips: 681})).toThrow('表格位置');
  expect(state.doc).toBe(document); expect(state.selection.eq(selection)).toBe(true); expect(undo(state)).toBe(false);
  const good = apply(state, {tableIndentTwips: 680}); expect(attrs(good).map(attr => attr.tableIndentTwips)).toEqual([680, 680]);
});

test('width and structural edits include indentation in their total budget and retain position on success', () => {
  const state = stateAt(doc([table({columnWidthsTwips: [15000, 15000], tableIndentTwips: 1680})]));
  expect(() => createTableColumnWidthTransaction(state, 15001)).toThrow('表格位置');
  expect(() => createTableStructureTransaction(state, 'addColumnAfter')).toThrow();
  const changed = state.apply(createTableColumnWidthTransaction(state, 14999));
  expect(attrs(changed)[0]).toMatchObject({columnWidthsTwips: [14999, 15000], tableIndentTwips: 1680});
  const narrow = stateAt(doc([table({columnWidthsTwips: [1], tableIndentTwips: 31679})]));
  expect(() => createTableColumnWidthTransaction(narrow, null)).toThrow('表格位置');
  const safe = stateAt(doc([table({columnWidthsTwips: [1000, 2000, 3000], tableAlign: 'left', tableIndentTwips: 720})]), 1);
  for (const action of ['addColumnBefore', 'addColumnAfter', 'deleteColumn', 'addRowBefore', 'addRowAfter', 'deleteRow']) {
    const tr = createTableStructureTransaction(safe, action);
    expect(tr.doc.firstChild.attrs).toMatchObject({tableAlign: 'left', tableIndentTwips: 720});
    expect(() => validateTableWidthsDocument(snap(tr.doc))).not.toThrow();
  }
});

test('position and reset undo/redo once, separate from earlier and later typing', () => {
  for (const patch of [{tableAlign: 'center'}, {tableIndentTwips: 1}, {tableIndentTwips: null}, {tableAlign: null, tableIndentTwips: null}]) {
    let state = stateAt(doc([table({tableAlign: 'left', tableIndentTwips: 720})]), 2);
    state = state.apply(state.tr.insertText('before'));
    const before = state.doc; state = apply(state, patch); state = state.apply(closeHistory(state.tr)); const changed = state.doc;
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(before)).toBe(true);
    expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
    state = state.apply(closeHistory(state.tr)); state = state.apply(state.tr.insertText('after'));
    expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
  }
});

test('programmatic and paste-like invalid geometry never enter history or autosave', () => {
  const errors = [], state = stateAt(doc([table()]), 0, [history(), tableWidthGuard(error => errors.push(error))]);
  for (const badAttrs of [{tableAlign: 'justify'}, {tableIndentTwips: 22681}, {tableAlign: 'right', tableIndentTwips: 1}]) {
    const tr = state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, ...badAttrs});
    const result = state.applyTransaction(tr);
    expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(result.state)).toBe(false);
  }
  const outer = snap(table({tableIndentTwips: 0}, 1, 1)); outer.content[0].content[0].content.push(snap(table({}, 1, 1)));
  const result = state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, schema.nodeFromJSON(outer)));
  expect(result.transactions).toEqual([]); expect(errors).toHaveLength(4);
});

test('prospective position edits enforce whole-document node, text and image budgets', () => {
  for (const document of [doc([table(), ...Array.from({length: 5000}, () => p('x'))]), doc([table(), p('中'.repeat(700000))]),
    doc([table(), schema.node('image', {src: 'A'.repeat(32 * 1024 * 1024)})])]) {
    const state = stateAt(document);
    expect(() => createTablePositionTransaction(state, {tableAlign: 'center'})).toThrow();
    expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

test('actual Tiptap CommandManager direct/chained/can position commands preserve atomic validation', () => {
  const errors = [], extension = WordTable.configure({onWidthError: error => errors.push(error)});
  const commands = extension.config.addCommands.call({options: extension.options, parent: () => ({})});
  const harness = state => {
    let current = state, dispatches = 0;
    const editor = {extensionManager: {commands}, get state() {return current;}, view: {dispatch(tr) {current = current.apply(tr); dispatches++;}}};
    return {manager: new CommandManager({editor}), get state() {return current;}, get dispatches() {return dispatches;}};
  };
  const good = harness(stateAt(doc([table({tableIndentTwips: 720})])));
  expect(good.manager.can().setTablePosition({tableAlign: 'center'})).toBe(true); expect(good.dispatches).toBe(0);
  expect(attrs(good.state)[0].tableIndentTwips).toBe(720);
  expect(good.manager.chain().setTablePosition({tableAlign: 'center'}).addRowAfter().run()).toBe(true);
  expect(attrs(good.state)[0]).toMatchObject({tableAlign: 'center', tableIndentTwips: null});
  expect(good.state.doc.firstChild.childCount).toBe(3); expect(good.dispatches).toBe(1);
  expect(good.manager.commands.resetTablePosition()).toBe(true); expect(selectedTablePosition(good.state).hasExplicit).toBe(false);
  for (const run of [m => m.commands.setTablePosition({tableIndentTwips: 1}), m => m.chain().setTablePosition({tableIndentTwips: 1}).run(),
    m => m.can().setTablePosition({tableIndentTwips: 1}), m => m.can().chain().setTablePosition({tableIndentTwips: 1}).run()]) {
    const bad = harness(stateAt(doc([table({tableAlign: 'right'})]))), before = bad.state.doc;
    expect(run(bad.manager)).toBe(false); expect(bad.state.doc).toBe(before); expect(undo(bad.state)).toBe(false);
  }
  expect(errors).toHaveLength(2);
});

test('serialized DOM keeps absent vs explicit zero text-edge origins and whole-table alignment', () => {
  for (const [attr, margins] of [[{}, null], [{tableAlign: 'left'}, {left: '0pt', right: '0pt'}],
    [{tableIndentTwips: 0}, {left: '-5.4pt', right: '0pt'}], [{tableIndentTwips: 720}, {left: '30.6pt', right: '0pt'}],
    [{tableAlign: 'center', tableIndentTwips: 0}, {left: 'auto', right: 'auto'}], [{tableAlign: 'right'}, {left: 'auto', right: '0pt'}]]) {
    expect(tablePositionMargins(attr)).toEqual(margins);
    const auto = schema.nodes.table.spec.toDOM(table(attr));
    if (!margins) {expect(auto[1]).not.toHaveProperty('data-table-position'); continue;}
    expect(auto[1]).toHaveProperty('data-table-position', '');
    expect(auto[1].style).toContain('table-layout: auto'); expect(auto[1].style).toContain('width: 600px');
    expect(auto[1].style).toContain(`margin-left: ${margins.left}`); expect(auto[1].style).toContain(`margin-right: ${margins.right}`);
    const fixed = schema.nodes.table.spec.toDOM(table({...attr, columnWidthsTwips: [1000, 2000, 3000]}));
    expect(fixed[1].style).toContain('table-layout: fixed'); expect(fixed[1].style).toContain('width: 400px');
    expect(fixed[1].style).toContain(`margin-left: ${margins.left}`);
    expect(fixed[1]).not.toHaveProperty('tableAlign'); expect(fixed[1]).not.toHaveProperty('tableIndentTwips');
  }
});

class Element {
  constructor(tag = 'div') {this.tag = tag;}
  children = []; attrs = {}; parentNode = null;
  style = {width: '', minWidth: '', tableLayout: '', marginLeft: '', marginRight: '', setProperty(key, value) {this[key.replace(/-([a-z])/g, (_, char) => char.toUpperCase())] = value;}};
  ownerDocument = {createElement: tag => new Element(tag)};
  classList = {remove() {}};
  get firstChild() {return this.children[0] ?? null;}
  get nextSibling() {return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null;}
  appendChild(child) {child.parentNode = this; this.children.push(child); return child;}
  removeChild(child) {this.children.splice(this.children.indexOf(child), 1); child.parentNode = null;}
  replaceChildren(...children) {this.children = []; children.forEach(child => this.appendChild(child));}
  setAttribute(key, value) {this.attrs[key] = value;}
  removeAttribute(key) {delete this.attrs[key];}
  querySelectorAll() {return [];}
  cloneNode(deep) {const copy = new Element(this.tag); copy.attrs = {...this.attrs}; Object.assign(copy.style, this.style); if (deep) this.children.forEach(child => copy.appendChild(child.cloneNode(true))); return copy;}
}

test('mounted TableView repeatedly resets all owned margins, metadata and auto/fixed geometry', () => {
  const old = globalThis.document; globalThis.document = {createElement: tag => new Element(tag)};
  try {
    const view = new FixedTableView(table({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 0}), 25);
    expect(view.table.style.marginLeft).toBe('-5.4pt'); expect(view.table.style.width).toBe('400px');
    for (let iteration = 0; iteration < 2; iteration++) {
      expect(view.update(table({tableAlign: 'center'}, 3))).toBe(true);
      expect(view.table.style).toMatchObject({width: '600px', tableLayout: 'auto', marginLeft: 'auto', marginRight: 'auto'});
      expect(view.table.attrs).toHaveProperty('data-table-align', 'center'); expect(view.table.attrs).not.toHaveProperty('data-table-indent-twips');
      expect(view.colgroup.children.map(col => col.style.width)).toEqual(['', '', '']);
      view.update(table());
      expect(view.table.style).toMatchObject({width: '', minWidth: '75px', tableLayout: '', marginLeft: '', marginRight: ''});
      expect(view.table.attrs).toEqual({});
      view.update(table({tableAlign: 'right', tableIndentTwips: 0}));
      expect(view.table.style.marginRight).toBe('0pt'); expect(view.table.attrs['data-table-indent-twips']).toBe('0');
      view.update(table({tableIndentTwips: 720, columnWidthsTwips: [1000, 2000, 3000]}));
      expect(view.table.style).toMatchObject({width: '400px', tableLayout: 'fixed', marginLeft: '30.6pt', marginRight: '0pt'});
      view.update(table());
      expect(view.table.style).toMatchObject({width: '', minWidth: '75px', tableLayout: '', marginLeft: '', marginRight: ''});
      expect(view.table.attrs).toEqual({});
      view.update(table({columnWidthsTwips: [1000, 2000, 3000]}));
      expect(view.table.style).toMatchObject({width: '400px', tableLayout: 'fixed', marginLeft: '', marginRight: ''});
      expect(view.table.attrs).not.toHaveProperty('data-table-position');
    }
    expect(view.update(p('no table'))).toBe(false);
  } finally {globalThis.document = old;}
});

test('real print snapshot clones canonical positioning immutably and shared CSS limits padding to positioned tables', async () => {
  const host = new Element(), old = globalThis.document;
  globalThis.document = {createElement: tag => new Element(tag), getElementById: () => host, fonts: {ready: Promise.resolve()}};
  try {
    const view = new FixedTableView(table({columnWidthsTwips: [1000, 2000, 3000], tableIndentTwips: 720}), 25);
    const source = new Element(); source.appendChild(view.table); source.setAttribute('contenteditable', 'true');
    await preparePrintSnapshot(source);
    const printed = host.firstChild.firstChild;
    expect(printed.style.marginLeft).toBe('30.6pt'); expect(printed.attrs['data-table-indent-twips']).toBe('720');
    expect(host.firstChild.attrs).not.toHaveProperty('contenteditable');
    view.update(table()); expect(printed.style.marginLeft).toBe('30.6pt');
    await preparePrintSnapshot(source);
    expect(host.firstChild.firstChild.style.marginLeft).toBe(''); expect(host.firstChild.firstChild.attrs).not.toHaveProperty('data-table-position');
    const css = readFileSync('ui/style.css', 'utf8');
    expect(css).toContain('.tiptap table[data-table-position] td,.tiptap table[data-table-position] th { padding-left:5.4pt; padding-right:5.4pt; min-width:0; }');
    expect(css).toContain('padding:8px;');
  } finally {globalThis.document = old;}
});
