import {mkdirSync, writeFileSync, readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {test, expect} from '@playwright/test';
import {Slice, Fragment} from '@tiptap/pm/model';
import {TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {undoDepth} from '@tiptap/pm/history';
import {schema, table, doc, cells, flags, snapshot, fullEditor, paste, key} from './word-table-row-pagination-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const colors = await loadTsModule('./ui/word-table-cell-colors.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const rawNoFill = () => {
  const raw = snapshot(doc());
  raw.content[0].content.forEach(row => row.content.forEach(cell => {cell.attrs = {...cell.attrs, cellFillMode: 'none'};}));
  return raw;
};
const attrList = editor => cells(editor.state.doc).map(pos => editor.state.doc.nodeAt(pos).attrs.cellFillMode);

// Actual installed application extension stack, not a mounted native browser.
test('full pipeline keeps exact no-fill draft data alongside row ownership through commands history and text edits', () => {
  const raw = rawNoFill(); tables.validateTableWidthsDocument(raw);
  const {editor, errors} = fullEditor(schema.nodeFromJSON(raw));
  try {
    editor.commands.setTextSelection(cells(editor.state.doc)[4] + 2);
    const before = snapshot(editor.state.doc), expected = structuredClone(before);
    expected.content[0].content[2].content[0].attrs.cellFillRgb = 'FFFFFF'; delete expected.content[0].content[2].content[0].attrs.cellFillMode;
    expect(editor.commands.setTableCellColors({cellFillRgb: 'FFFFFF'})).toBe(true);
    expect(snapshot(editor.state.doc)).toEqual(expected); expect(flags(editor.state.doc)).toEqual([null, false, true]);
    expect(editor.commands.undo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before);
    expect(editor.commands.redo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(expected);
    expect(editor.commands.setTableCellColors({cellFillMode: 'none', cellTextColorMode: 'auto'})).toBe(true);
    const none = snapshot(editor.state.doc); expect(none.content[0].content[2].content[0].attrs).toMatchObject({cellFillMode: 'none', cellTextColorMode: 'auto'});
    expect(editor.commands.setTableCellColors({cellTextColorMode: null})).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before);
    const persisted = snapshot(editor.state.doc); tables.validateTableWidthsDocument(persisted); expect(snapshot(schema.nodeFromJSON(persisted))).toEqual(persisted);
    expect(editor.commands.insertContent({type: 'text', text: 'typed'})).toBe(true); expect(attrList(editor)).toEqual(Array(6).fill('none'));
    expect(flags(editor.state.doc)).toEqual([null, false, true]); expect(editor.commands.undo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before);
    expect(editor.commands.resetTableCellColors()).toBe(true); expect(attrList(editor)[4]).toBeNull(); expect(flags(editor.state.doc)).toEqual([null, false, true]);
    expect(errors).toEqual([]);
  } finally {editor.destroy();}
});

test('full plugin mixed selection transition is one history event and conflicting command leaves shared state untouched', () => {
  const raw = rawNoFill(); delete raw.content[0].content[0].content[1].attrs.cellFillMode;
  const {editor, errors} = fullEditor(schema.nodeFromJSON(raw));
  try {
    const positions = cells(editor.state.doc); editor.commands.setCellSelection({anchorCell: positions[0], headCell: positions[5]});
    const before = snapshot(editor.state.doc), depth = undoDepth(editor.state);
    expect(tables.selectedTableCellColors(editor.state).cellFill.mixed).toBe(true);
    expect(editor.commands.setTableCellColors({cellFillMode: 'none', cellFillRgb: 'FFFFFF'})).toBe(false);
    expect(snapshot(editor.state.doc)).toEqual(before); expect(undoDepth(editor.state)).toBe(depth);
    expect(editor.commands.setTableCellColors({cellFillMode: 'none'})).toBe(true); expect(attrList(editor)).toEqual(Array(6).fill('none'));
    expect(undoDepth(editor.state)).toBe(depth + 1); expect(editor.commands.undo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before);
    expect(editor.commands.redo()).toBe(true); expect(attrList(editor)).toEqual(Array(6).fill('none')); expect(errors).toHaveLength(1);
  } finally {editor.destroy();}
});

test('full plugin structural and last-cell Tab operations preserve none survivors and new cells remain absent', () => {
  for (const action of ['addRowAfter', 'addColumnAfter', 'toggleHeaderCell', 'toggleHeaderRow', 'tab']) {
    const {editor, errors} = fullEditor(schema.nodeFromJSON(rawNoFill()));
    try {
      editor.commands.setTextSelection(cells(editor.state.doc).at(-1) + 2); const before = snapshot(editor.state.doc);
      expect(action === 'tab' ? key(editor, 'Tab') : editor.commands[action]()).toBe(true);
      const values = attrList(editor); expect(values.filter(value => value === 'none')).toHaveLength(6);
      if (action === 'addRowAfter' || action === 'tab') {expect(values.slice(-2)).toEqual([null, null]); expect(flags(editor.state.doc)).toEqual([null, false, true, null]);}
      if (action === 'addColumnAfter') expect(values.filter(value => value === null)).toHaveLength(3);
      tables.validateTableWidthsDocument(snapshot(editor.state.doc));
      expect(editor.commands.undo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before); expect(errors).toEqual([]);
    } finally {editor.destroy();}
  }
});

test('full plugin closed whole-table paste retains mode while flagged cell copy remains refused and inline copy stays narrow', () => {
  const {editor, errors} = fullEditor(doc());
  const source = editor.schema.nodeFromJSON(rawNoFill()).firstChild;
  try {
    const last = editor.state.doc.lastChild; editor.commands.setTextSelection(editor.state.doc.content.size - last.nodeSize + 1);
    expect(paste(editor, new Slice(Fragment.from(source), 0, 0))).toBe(true);
    expect(snapshot(editor.state.doc).content.filter(node => node.type === 'table').at(-1)).toEqual(snapshot(editor.schema.node('doc', null, source)).content[0]);
    editor.commands.undo(); editor.commands.setTextSelection(cells(editor.state.doc)[0] + 2);
    const before = snapshot(editor.state.doc), copyDoc = editor.schema.nodeFromJSON(rawNoFill()), copied = CellSelection.create(copyDoc, cells(copyDoc)[4]).content();
    expect(paste(editor, copied)).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before); expect(errors.length).toBeGreaterThan(0);
    const sourceDoc = editor.schema.nodeFromJSON(rawNoFill()), at = cells(sourceDoc)[4] + 2;
    const inline = TextSelection.create(sourceDoc, at, at + 1).content(); expect(inline.openStart).toBe(4); expect(inline.openEnd).toBe(4);
    expect(paste(editor, inline)).toBe(true); expect(attrList(editor)).toEqual(Array(6).fill(null));
    expect(flags(editor.state.doc)).toEqual([null, false, true]);
    expect(editor.commands.undo()).toBe(true); expect(snapshot(editor.state.doc)).toEqual(before);
  } finally {editor.destroy();}
});

// DOM-shaped preflight fixtures are deliberately not a native HTML parser.
class Element {
  constructor(tag = 'body', attrs = {}) {this.tagName = tag.toUpperCase(); this.attrs = attrs; this.children = [];}
  getAttribute(key) {return this.attrs[key] ?? null;} hasAttribute(key) {return key in this.attrs;}
  append(child) {this.children.push(child); child.parentElement = this; return child;}
  matches(selector) {return selector.split(',').some(part => {const match = /^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(part); return match && (!match[1] || match[1].toUpperCase() === this.tagName) && (!match[2] || this.hasAttribute(match[2]));});}
  closest(selector) {for (let node = this; node; node = node.parentElement) if (node.matches(selector)) return node; return null;}
  querySelectorAll(selector) {return this.children.flatMap(child => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]);}
}
const ownedRoot = () => {const root = new Element(); root.body = root; root.append(new Element('table')).append(new Element('tbody')).append(new Element('tr')).append(new Element('th', {'data-cell-fill-mode': 'none'})); return root;};
const parseAs = (root, run) => {const previous = globalThis.DOMParser; globalThis.DOMParser = class {parseFromString() {return root;}}; try {run();} finally {globalThis.DOMParser = previous;}};

test('raw lexical no-fill duplicates orphan misplaced malformed and merged metadata fail both guards before schema loss', () => {
  const valid = '<table><tr><th data-cell-fill-mode="none">x</th></tr></table>';
  parseAs(ownedRoot(), () => {
    expect(colors.isValidTableCellColorsHTML(valid)).toBe(true);
    expect(colors.isValidTableCellColorsHTML(valid.replace('="none"', '="none" data-cell-fill-mode="none"'))).toBe(false);
  });
  for (const mutate of [root => {root.querySelectorAll('th')[0].tagName = 'P';}, root => {root.querySelectorAll('th')[0].attrs['data-cell-fill-mode'] = 'NONE';},
    root => {root.querySelectorAll('th')[0].attrs['data-cell-fill-rgb'] = 'FFFFFF';}, root => {root.querySelectorAll('th')[0].attrs.colspan = '2';},
    root => {root.children = [];}, root => {root.querySelectorAll('th')[0].append(new Element('table'));}]) {
    const root = ownedRoot(); mutate(root); parseAs(root, () => {
      expect(colors.isValidTableCellColorsHTML(valid)).toBe(false);
      const {editor, errors} = fullEditor(doc()); const before = snapshot(editor.state.doc);
      try {
        editor.commands.setTextSelection(cells(editor.state.doc)[2] + 2);
        const transform = editor.extensionManager.transformPastedHTML(); expect(transform(valid)).toContain('data-cell-fill-mode="invalid"');
        expect(paste(editor, new Slice(Fragment.from(table([null])), 0, 0), {clipboardData: {getData: () => valid}})).toBe(true);
        expect(snapshot(editor.state.doc)).toEqual(before); expect(undoDepth(editor.state)).toBe(0); expect(errors).toHaveLength(1);
        const lateErrors = []; expect(tables.tableWidthGuard(value => lateErrors.push(value)).props.handlePaste(null, {clipboardData: {getData: () => valid}})).toBe(true); expect(lateErrors).toHaveLength(1);
      } finally {editor.destroy();}
    });
  }
});

test('ordinary CSS and metadata spelling in text comments quoted attrs rawtext retain exact unowned HTML behavior', () => {
  const root = new Element(); root.body = root;
  for (const html of ['<p>data-cell-fill-mode="none"</p>', '<!-- data-cell-fill-mode="none" -->', '<p title="data-cell-fill-mode=none">ordinary</p>',
    '<script>data-cell-fill-mode="none"</script>', '<td style="background-color:transparent">x</td>', '<td bgcolor="white">x</td>',
    `${'<div>'.repeat(40)}data-cell-fill-mode${'</div>'.repeat(40)}`, `data-cell-fill-mode${'x'.repeat(32 * 1024 * 1024)}`]) {
    parseAs(root, () => {expect(colors.isValidTableCellColorsHTML(html)).toBe(true); expect(colors.preflightTableCellColorsHTML(html)).toBe(html);});
  }
});


test('full-stack evidence driver freezes exact source edited history resets and reopened draft snapshots', ({}, testInfo) => {
  mkdirSync(testInfo.outputPath(), {recursive: true});
  const input = testInfo.outputPath('input.json'), output = testInfo.outputPath('evidence');
  const raw = rawNoFill(); raw.content[0].content[2].content[0].attrs.cellTextColorMode = 'auto';
  writeFileSync(input, JSON.stringify(raw));
  const result = spawnSync(process.execPath, ['tests/ui/edit-table-cell-no-fill.mjs', input, output, '0', '4', '{"cellFillRgb":"FFFFFF"}'], {encoding: 'utf8'});
  expect(result.status, result.stderr).toBe(0);
  const read = name => JSON.parse(readFileSync(`${output}/${name}.json`, 'utf8'));
  const before = read('before'), expected = structuredClone(before); const target = expected.content[0].content[2].content[0];
  delete target.attrs.cellFillMode; target.attrs.cellFillRgb = 'FFFFFF';
  expect(read('source')).toEqual(raw); expect(read('edited')).toEqual(expected); expect(read('undo')).toEqual(before);
  expect(read('redo')).toEqual(expected); expect(read('reopened')).toEqual(expected);
  const resetFill = structuredClone(expected); delete resetFill.content[0].content[2].content[0].attrs.cellFillRgb;
  expect(read('reset-fill')).toEqual(resetFill);
  const resetText = structuredClone(expected); delete resetText.content[0].content[2].content[0].attrs.cellTextColorMode;
  expect(read('reset-mode')).toEqual(resetText);
  delete resetFill.content[0].content[2].content[0].attrs.cellTextColorMode; expect(read('reset-all')).toEqual(resetFill);
});
