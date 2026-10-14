import {test, expect} from '@playwright/test';
import {CommandManager} from '@tiptap/core';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection, mergeCells, toggleHeaderCell, toggleHeaderRow, toggleHeaderColumn} from '@tiptap/pm/tables';
import {closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {Fragment, Slice} from '@tiptap/pm/model';
import {rows, tables, schema, p, table, doc, positions, cells, flags, snapshot, stateAt, fullEditor, key, paste} from './word-table-row-pagination-harness.mjs';
const choose = (state, selection) => state.apply(state.tr.setSelection(selection));
const rowPositions = document => positions(document, ['tableRow']);
const apply = (state, value) => state.apply(closeHistory(tables.createTableRowPaginationTransaction(state, value)));
const editors = [];
const makeEditor = (...args) => {const result = fullEditor(...args); editors.push(result.editor); return result;};
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

test('strict raw tri-state rejects null undefined wrong types misplaced nodes and marks', () => {
  for (const value of [null, undefined, 'true', 'false', 0, 1, [], {}, NaN]) {
    const raw = snapshot(doc()); raw.content[0].content[0].attrs = {rowCantSplit: value};
    expect(() => tables.validateTableWidthsDocument(raw)).toThrow('行内不分页');
  }
  for (const type of ['doc', 'table', 'tableCell', 'tableHeader', 'paragraph', 'text', 'hardBreak']) {
    expect(() => rows.validateTableRowPaginationDocument({type: 'doc', content: [{type, attrs: {rowCantSplit: false}}]})).toThrow();
  }
  expect(() => rows.validateTableRowPaginationDocument({type: 'doc', content: [{type: 'paragraph', marks: [{type: 'bold', attrs: {rowCantSplit: true}}]}]})).toThrow();
  for (const value of [false, true]) {const raw = snapshot(doc([table([value])])); expect(() => tables.validateTableWidthsDocument(raw)).not.toThrow(); expect(flags(schema.nodeFromJSON(raw))).toEqual([value]);}
});
test('absence leaves legacy JSON DOM unchanged while false is never omitted', () => {
  const raw = {type: 'doc', content: [{type: 'table', content: [{type: 'tableRow', attrs: {rowMinHeightTwips: 0}, content: []}]}]};
  const defaults = structuredClone(raw); defaults.content[0].content[0].attrs.rowCantSplit = null;
  expect(rows.omitDefaultTableRowPagination(defaults)).toEqual(raw); expect(defaults.content[0].content[0].attrs.rowCantSplit).toBeNull();
  const attrs = rows.tableRowPaginationAttributes()[0].attributes.rowCantSplit;
  expect(attrs.renderHTML({})).toEqual({}); expect(attrs.renderHTML({rowCantSplit: false})).toEqual({'data-row-cant-split': 'false'});
  for (const value of ['', '0', '1', 'FALSE', ' true', 'false ', 'null']) expect(attrs.parseHTML({getAttribute: () => value})).toBe('invalid-row-pagination');
  expect(attrs.keepOnSplit).toBe(false);
});
test('top-level rectangles only; height header and supported cell lists stay independent', () => {
  for (const change of [raw => raw.content[0].content[1].content.pop(), raw => {raw.content[0].content[0].content[0].attrs.colspan = 2;},
    raw => {raw.content[0].content[0].content[0].attrs.rowspan = 2;}, raw => raw.content[0].content[0].content[0].content.push(snapshot(doc([table([null])])).content[0])]) {
    const raw = snapshot(doc()); change(raw); expect(() => tables.validateTableWidthsDocument(raw)).toThrow();
  }
  const nested = schema.node('bulletList', null, schema.node('listItem', null, [p('item'), table([false])]));
  expect(() => rows.validateTableRowPaginationState(doc([nested]))).toThrow();
  let state = stateAt(); state = apply(state, false); state = state.apply(tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0, cellVerticalAlign: 'center'}));
  expect(flags(state.doc)).toEqual([false, false, true]); expect(state.doc.firstChild.firstChild.attrs.rowMinHeightTwips).toBe(0);
  const raw = snapshot(state.doc); raw.content[0].content[0].content[0].content.push({type: 'bulletList', content: [{type: 'listItem', content: [{type: 'paragraph'}]}]});
  expect(() => tables.validateTableWidthsDocument(raw)).not.toThrow();
});
test('all selection kinds and backward ranges target rows exactly; outside ranges do not narrow', () => {
  const document = doc(), cell = cells(document), row = rowPositions(document);
  for (const [selection, count] of [[TextSelection.create(document, cell[0] + 2), 1], [TextSelection.create(document, cell[1] + 2, cell[4] + 3), 3],
    [TextSelection.create(document, cell[4] + 3, cell[1] + 2), 3], [CellSelection.create(document, cell[1], cell[4]), 3],
    [NodeSelection.create(document, 0), 3], [NodeSelection.create(document, row[1]), 1], [NodeSelection.create(document, cell[3]), 1]]) {
    const state = EditorState.create({doc: document, selection, plugins: stateAt(document).plugins}), selected = tables.selectedTableRowPagination(state), tr = tables.createTableRowPaginationTransaction(state, true);
    expect(selected.count).toBe(count); expect(tr.selection.eq(selection)).toBe(true); expect(tr.doc.textContent).toBe(document.textContent);
  }
  const outside = doc([p('before'), table(), p('after')]);
  for (const selection of [new AllSelection(outside), TextSelection.create(outside, 2), TextSelection.create(outside, 2, cells(outside)[3] + 2)]) expect(tables.selectedTableRowPagination(choose(stateAt(outside), selection))).toBeNull();
});
test('mixed false absent true reset no-op exact draft and one undo redo', () => {
  let state = stateAt(), original = state.doc; state = choose(state, NodeSelection.create(state.doc, 0));
  expect(tables.selectedTableRowPagination(state)).toMatchObject({mixed: true, value: null, hasExplicit: true});
  state = apply(state, false); state = state.apply(closeHistory(state.tr)); expect(flags(state.doc)).toEqual([false, false, false]);
  expect(tables.createTableRowPaginationTransaction(state, false)).toBeNull(); expect(undoDepth(state)).toBe(1);
  const saved = snapshot(state.doc); tables.validateTableWidthsDocument(saved); expect(snapshot(schema.nodeFromJSON(saved))).toEqual(saved);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); state = apply(state, null); expect(flags(state.doc)).toEqual([null, null, null]);
  expect(tables.createTableRowPaginationTransaction(state, null)).toBeNull();
});
for (const flag of [false, true]) {
  test(`flag ${flag}: row/column structural commands preserve ownership and new rows default absent`, () => {
    for (const action of ['addRowBefore', 'addRowAfter', 'deleteRow', 'addColumnBefore', 'addColumnAfter', 'deleteColumn', 'deleteTable']) {
      let state = stateAt(doc([table([flag, !flag, flag])]), 2), original = state.doc;
      const tr = tables.createTableStructureTransaction(state, action); const result = state.applyTransaction(closeHistory(tr)); expect(result.transactions.length).toBeGreaterThan(0); state = result.state;
      const expected = action === 'addRowBefore' ? [flag, null, !flag, flag] : action === 'addRowAfter' ? [flag, !flag, null, flag]
        : action === 'deleteRow' ? [flag, flag] : action === 'deleteTable' ? [] : [flag, !flag, flag];
      expect(flags(state.doc)).toEqual(expected); expect(undo(state, candidate => {state = state.apply(candidate);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
    }
  });
  test(`flag ${flag}: arbitrary erasure recreation partial row crossing and forged approval are rejected`, () => {
    const state = stateAt(doc([table([flag, !flag])])), row = rowPositions(state.doc), cell = cells(state.doc), before = state.doc;
    const candidates = [state.tr.setNodeAttribute(row[0], 'rowCantSplit', null), state.tr.setNodeAttribute(row[0], 'rowCantSplit', !flag),
      state.tr.replaceWith(row[0], row[0] + before.nodeAt(row[0]).nodeSize, schema.node('tableRow', null, before.nodeAt(row[0]).content)),
      state.tr.delete(cell[0] + 3, cell[2] + 3)];
    for (const candidate of candidates) {candidate.setMeta('rowPaginationApproved', true).setMeta('tableRowPagination', true); expect(state.applyTransaction(candidate).transactions).toEqual([]);}
    const approved = tables.createTableRowPaginationTransaction(state, !flag); approved.setNodeAttribute(row[0], 'rowCantSplit', null);
    expect(state.applyTransaction(approved).transactions).toEqual([]); expect(undoDepth(state)).toBe(0); expect(state.doc).toBe(before);
  });
  test(`flag ${flag}: full plugins typing paragraph Enter join heading marks clear undo retain row shell`, () => {
    const {editor, errors} = makeEditor(doc([table([flag, null])])), cell = cells(editor.state.doc)[0];
    editor.commands.setTextSelection(cell + 3); editor.commands.insertContent({type: 'text', text: 'typed'});
    expect(key(editor, 'Enter')).toBe(true); expect(editor.state.doc.firstChild.firstChild.firstChild.childCount).toBe(2); expect(flags(editor.state.doc)).toEqual([flag, null]);
    expect(key(editor, 'Backspace')).toBe(true); expect(editor.state.doc.firstChild.firstChild.firstChild.childCount).toBe(1);
    editor.commands.setHeading({level: 2}); editor.commands.setBold(); editor.commands.insertContent({type: 'text', text: 'bold'}); expect(flags(editor.state.doc)).toEqual([flag, null]);
    const at = cells(editor.state.doc)[0], textblock = editor.state.doc.nodeAt(at).firstChild;
    editor.commands.setTextSelection({from: at + 2, to: at + textblock.nodeSize}); editor.commands.deleteSelection(); editor.commands.insertContent({type: 'text', text: 'again'});
    expect(flags(editor.state.doc)).toEqual([flag, null]); expect(errors).toEqual([]);
    editor.commands.selectAll(); editor.commands.deleteSelection(); expect(flags(editor.state.doc)).toEqual([]); editor.commands.undo(); expect(flags(editor.state.doc)).toEqual([flag, null]);
  });
  test(`flag ${flag}: full plugin last-cell Tab is atomic and new row never inherits`, () => {
    const {editor, errors} = makeEditor(doc([table([flag])])), original = editor.state.doc;
    editor.commands.setTextSelection(cells(editor.state.doc).at(-1) + 2); expect(key(editor, 'Tab')).toBe(true); expect(flags(editor.state.doc)).toEqual([flag, null]);
    expect(undoDepth(editor.state)).toBe(1); editor.commands.undo(); expect(editor.state.doc.eq(original)).toBe(true); editor.commands.redo(); expect(flags(editor.state.doc)).toEqual([flag, null]); expect(errors).toEqual([]);
  });
  test(`flag ${flag}: header row column cell toggles preserve independent metadata`, () => {
    for (const command of [toggleHeaderRow, toggleHeaderColumn, toggleHeaderCell]) {
      let state = stateAt(doc([table([flag, !flag])]), 2); expect(command(state, tr => {state = state.apply(tr);})).toBe(true); expect(flags(state.doc)).toEqual([flag, !flag]);
    }
  });
  test(`flag ${flag}: flagged structured paste refuses before real tableEditing can drop row attrs`, () => {
    for (const open of [false, true]) {
      const {editor, errors} = makeEditor(doc([table([!flag])])), original = editor.state.doc; editor.commands.setTextSelection(cells(original)[0] + 2);
      const source = schema.nodeFromJSON(table([flag]).toJSON()), slice = open ? new Slice(source.content, 1, 1) : new Slice(Fragment.from(source), 0, 0);
      expect(paste(editor, slice)).toBe(true); expect(editor.state.doc).toBe(original); expect(errors.at(-1)).toContain('包含行分页设置'); expect(undoDepth(editor.state)).toBe(0);
    }
  });
  test(`flag ${flag}: ordinary unflagged structured cell paste retains target and new growth absent`, () => {
    const {editor, errors} = makeEditor(doc([table([flag])])), original = editor.state.doc;
    editor.commands.setTextSelection(cells(original)[0] + 2);
    const source = editor.schema.nodeFromJSON(table([null, null]).toJSON());
    expect(paste(editor, new Slice(Fragment.from(source), 0, 0))).toBe(true); expect(flags(editor.state.doc)).toEqual([flag, null]); expect(errors).toEqual([]);
    editor.commands.undo(); expect(editor.state.doc.eq(original)).toBe(true);
  });
  test(`flag ${flag}: closed top-level whole-table paste preserves all source metadata`, () => {
    const {editor, errors} = makeEditor(doc([p('')])), source = editor.schema.nodeFromJSON(table([flag, null, !flag]).toJSON());
    expect(paste(editor, new Slice(Fragment.from(source), 0, 0))).toBe(true); expect(flags(editor.state.doc)).toEqual([flag, null, !flag]); expect(errors).toEqual([]);
    editor.commands.undo(); expect(editor.state.doc.childCount).toBe(1); expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
  });
}
test('shared chain row flags and heights compose; can probes cannot grant or leak approval', () => {
  const {editor, errors} = makeEditor(doc([table([null, false])])); editor.commands.setTextSelection(cells(editor.state.doc)[0] + 2);
  const before = editor.state.doc; expect(editor.can().chain().setTableRowCantSplit(true).addRowAfter().run()).toBe(true); expect(editor.state.doc).toBe(before);
  expect(editor.chain().setTableRowCantSplit(true).setTableVerticalLayout({rowMinHeightTwips: 20}).addRowAfter().run()).toBe(true);
  expect(flags(editor.state.doc)).toEqual([true, null, false]); expect(editor.state.doc.firstChild.firstChild.attrs.rowMinHeightTwips).toBe(20);
  expect(undoDepth(editor.state)).toBe(1); expect(errors).toEqual([]);
  const beforeBad = editor.state.doc; expect(editor.chain().setTableRowCantSplit(false).command(({tr}) => {tr.setNodeAttribute(rowPositions(tr.doc)[0], 'rowCantSplit', null); return true;}).run()).toBe(true);
  expect(editor.state.doc).toBe(beforeBad); expect(errors.at(-1)).toContain('行内不分页');
});
test('failed structural and invalid row candidates leave shared chain untouched', () => {
  const commands = tables.WordTable.config.addCommands.call({options: {onWidthError() {}}, parent: () => ({})});
  let state = stateAt(), writes = 0; const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {const result = state.applyTransaction(tr); state = result.state; if (result.transactions.some(item => item.docChanged)) writes++;}}};
  const manager = new CommandManager({editor});
  expect(manager.can().setTableRowCantSplit('false')).toBe(false); expect(manager.chain().setTableRowCantSplit(undefined).run()).toBe(false); expect(writes).toBe(0);
  const token = tables.captureTableCellMargins(state); state = state.apply(state.tr.insertText('changed'));
  expect(tables.createTableRowPaginationTransaction(state, true, token)).toBeNull();
});
test('nested insert merge and arbitrary flagged row insertion fail candidate validation', () => {
  const state = stateAt(doc([table([false, true])])), cell = cells(state.doc);
  expect(state.applyTransaction(state.tr.insert(cell[0] + 1, table([null]))).transactions).toEqual([]);
  const selected = choose(state, CellSelection.create(state.doc, cell[0], cell[1])); let candidate;
  expect(mergeCells(selected, tr => {candidate = tr;})).toBe(true); expect(selected.applyTransaction(candidate).transactions).toEqual([]);
  const newRow = table([false]).firstChild;
  expect(state.applyTransaction(state.tr.insert(rowPositions(state.doc)[1], newRow)).transactions).toEqual([]);
});
test('actual extension priority places row refusal before tableEditing and locks block dispatch', () => {
  let locked = false; const {editor, errors} = makeEditor(doc([table([true])]), () => locked);
  const plugins = editor.state.plugins, tableAt = plugins.findIndex(plugin => plugin.key.startsWith('selectingCells'));
  const guardAt = plugins.findIndex(plugin => plugin.key.startsWith('tableRowPaginationGuard'));
  expect(guardAt).toBeGreaterThanOrEqual(0); expect(tableAt).toBeGreaterThan(guardAt);
  editor.commands.setTextSelection(cells(editor.state.doc).at(-1) + 2); const original = editor.state.doc; locked = true;
  key(editor, 'Tab'); expect(editor.state.doc).toBe(original); expect(editor.commands.setTableRowCantSplit(false)).toBe(false); expect(errors).toEqual([]);
});
test('naturally tall row content is legal and only existing structural budgets apply', () => {
  const raw = snapshot(doc([table([true])])); raw.content[0].content[0].content[0].content = Array.from({length: 700}, () => ({type: 'paragraph', content: [{type: 'text', text: 'tall'}]}));
  expect(() => tables.validateTableWidthsDocument(raw)).not.toThrow(); expect(() => rows.validateTableRowPaginationState(schema.nodeFromJSON(raw))).not.toThrow();
  expect(() => rows.validateTableRowPaginationState(doc([table([true]), p('x'.repeat(2 * 1024 * 1024))]))).toThrow();
  expect(() => rows.validateTableRowPaginationState(doc([table([true]), ...Array.from({length: 10000}, () => p())]))).toThrow();
});

test('typing after last-cell Tab has its own undo event and cannot erase row creation', () => {
  const {editor} = makeEditor(doc([table([false])])); editor.commands.setTextSelection(cells(editor.state.doc).at(-1) + 2);
  editor.commands.insertContent({type: 'text', text: 'before'}); const typed = editor.state.doc;
  expect(key(editor, 'Tab')).toBe(true); const inserted = editor.state.doc;
  editor.commands.insertContent({type: 'text', text: 'after'}); expect(editor.state.doc.textContent).toContain('after');
  editor.commands.undo(); expect(editor.state.doc.eq(inserted)).toBe(true);
  editor.commands.undo(); expect(editor.state.doc.eq(typed)).toBe(true);
});
for (const flag of [false, true]) test(`flag ${flag}: actual copied inline text never transfers enclosing source row flags`, () => {
  const {editor, errors} = makeEditor(doc([table([!flag])])); editor.commands.setTextSelection(cells(editor.state.doc)[0] + 2);
  const source = editor.schema.nodeFromJSON(doc([table([flag])]).toJSON()), sourceCell = cells(source)[0];
  const slice = TextSelection.create(source, sourceCell + 2, sourceCell + 4).content();
  expect(slice.openStart).toBe(4); expect(slice.openEnd).toBe(4);
  expect(paste(editor, slice)).toBe(true); expect(flags(editor.state.doc)).toEqual([!flag]); expect(editor.state.doc.firstChild.firstChild.firstChild.textContent).toBe('0:0:0'); expect(errors).toEqual([]);
});

for (const flag of [false, true]) test(`flag ${flag}: actual single multi and whole-table CellSelection copies retain row ownership and refuse`, () => {
  for (const [anchor, head] of [[0, 0], [0, 1], [0, 3], [3, 0]]) {
    const {editor, errors} = makeEditor(doc([table([!flag, null])])), source = editor.schema.nodeFromJSON(doc([table([flag, !flag])]).toJSON());
    editor.commands.setTextSelection(cells(editor.state.doc)[0] + 2); const original = editor.state.doc, writes = [];
    editor.on('update', () => writes.push(snapshot(editor.state.doc)));
    const sourceCells = cells(source), slice = CellSelection.create(source, sourceCells[anchor], sourceCells[head]).content();
    expect(slice.openStart).toBe(1); expect(slice.openEnd).toBe(1);
    expect(paste(editor, slice)).toBe(true); expect(editor.state.doc).toBe(original); expect(undoDepth(editor.state)).toBe(0);
    expect(writes).toEqual([]); expect(errors.at(-1)).toContain('包含行分页设置');
  }
});
for (const flag of [false, true]) for (const destination of ['body', 'absent', 'opposite']) test(`flag ${flag}: rich copied inline marks hardBreak and exact undo at ${destination} destination`, () => {
  const initial = destination === 'body' ? doc([p('target', {keepNext: false, spaceBefore: 20})])
    : doc([table([destination === 'absent' ? null : !flag])]);
  const {editor, errors} = makeEditor(initial), s = editor.schema;
  const rich = s.node('paragraph', null, [s.text('A', [s.mark('bold'), s.mark('textColor', {rgb: '123456'})]), s.node('hardBreak'), s.text('B', [s.mark('italic')])]);
  const source = s.node('doc', null, s.node('table', null, s.node('tableRow', {rowCantSplit: flag}, s.node('tableCell', null, rich))));
  const at = destination === 'body' ? 1 : cells(editor.state.doc)[0] + 2;
  editor.commands.setTextSelection(at); const before = editor.state.doc, previous = editor.state.selection.$from.parent;
  const sourceCell = cells(source)[0], slice = TextSelection.create(source, sourceCell + 2, sourceCell + 2 + rich.content.size).content();
  expect(slice.openStart).toBe(4); expect(slice.openEnd).toBe(4);
  expect(paste(editor, slice)).toBe(true); const pasted = editor.state.selection.$from.parent;
  expect(pasted.content.eq(rich.content.append(previous.content))).toBe(true); expect(pasted.attrs).toEqual(previous.attrs);
  expect(flags(editor.state.doc)).toEqual(destination === 'body' ? [] : [destination === 'absent' ? null : !flag]);
  expect(errors).toEqual([]); expect(undoDepth(editor.state)).toBe(1);
  editor.commands.undo(); expect(editor.state.doc.eq(before)).toBe(true);
  editor.commands.redo(); expect(editor.state.selection.$from.parent.content.eq(pasted.content)).toBe(true);
});
