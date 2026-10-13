import {test, expect} from '@playwright/test';
import {CommandManager, getSchema, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import Image from '@tiptap/extension-image';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {splitBlock, joinBackward} from '@tiptap/pm/commands';
import {DOMParser as ProseMirrorDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {wrapInList} from '@tiptap/pm/schema-list';
import {loadTsModule} from './load-ts.mjs';
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const {WordParagraphIndentControls} = await loadTsModule('./ui/word-paragraph-indent-controls.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const {ParagraphSpacing, omitDefaultParagraphSpacing} = await loadTsModule('./ui/word-paragraph-spacing.ts');
const {snapshotDocument} = await loadTsModule('./ui/word-style-profile.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const {FontSize} = await loadTsModule('./ui/word-font-size.ts');
const WordDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+', addAttributes: () => ({styleProfile: {default: null}})});
const schema = getSchema([StarterKit.configure({document: false}), WordDoc, TableKit.configure({table: false}), tables.WordTable,
  Image, ParagraphSpacing, FontSize, page.WordPageGeometry, indent.ParagraphIndent]);
const paragraph = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const heading = (text, attrs = {}) => schema.node('heading', {level: 2, ...attrs}, schema.text(text));
const doc = (content, attrs = {}) => schema.node('doc', attrs, content);
const table = (cells, attrs = {}) => schema.node('table', attrs, [schema.node('tableRow', null, cells.map(value => schema.node('tableCell', null, Array.isArray(value) ? value : [value])))]);
const list = value => schema.node('bulletList', null, [schema.node('listItem', null, [value])]);
const blocks = document => {const result = []; document.descendants((node, pos) => {if (['paragraph', 'heading'].includes(node.type.name)) result.push({node, pos});}); return result;};
const positions = (document, name) => {const result = []; document.descendants((node, pos) => {if (node.type.name === name) result.push(pos);}); return result;};
const snapshot = document => indent.omitDefaultParagraphIndent(tables.omitDefaultTableWidths(omitDefaultParagraphSpacing(snapshotDocument({getJSON: () => document.toJSON()}))));
const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), indent.paragraphIndentGuard(message => errors.push(message)), page.pageGeometryGuard(message => errors.push(message)), tables.tableWidthGuard(message => errors.push(message))]});
const all = document => stateFor(document, new AllSelection(document));
const apply = (state, patch) => state.apply(closeHistory(indent.createParagraphIndentTransaction(state, patch)));

test('exact signed point and CSS parsers reject coercion, rounding, negatives on side fields and invalid units', () => {
  for (const [input, expected] of [['0', 0], ['-0', 0], ['.05', 1], ['-.05', -1], [' -18.05 ', -361], ['1584', 31680], ['-1584', -31680]]) expect(indent.parseParagraphIndentPoints('firstLineIndentTwips', input)).toBe(expected);
  for (const field of indent.paragraphIndentFields) for (const input of ['', '+1', '1e2', '.001', '1584.05', 'NaN', 'Infinity', '1pt', '0.05000000000000001', '9'.repeat(65)]) expect(indent.parseParagraphIndentPoints(field, input)).toBeNull();
  for (const field of ['indentLeftTwips', 'indentRightTwips']) for (const input of ['-1', '-0']) expect(indent.parseParagraphIndentPoints(field, input)).toBeNull();
  expect(indent.parseParagraphIndentCss('firstLineIndentTwips', '-12px')).toBe(-180);
  expect(indent.parseParagraphIndentCss('indentLeftTwips', '0.05pt')).toBe(1);
  for (const input of ['1em', 'auto', 'calc(1pt)', '0.01px', '1%']) expect(indent.parseParagraphIndentCss('firstLineIndentTwips', input)).toBeNull();
  for (let twips = -31680; twips <= 31680; twips++) if (indent.parseParagraphIndentPoints('firstLineIndentTwips', String(twips / 20)) !== twips) throw new Error(`Roundtrip changed ${twips}`);
});

test('integer ranges null and misplaced raw attrs reject before schema defaulting', () => {
  for (const field of indent.paragraphIndentFields) for (const value of [null, undefined, true, '20', 1.5, NaN, Infinity, 31681, -31681]) {
    expect(() => indent.validateParagraphIndentDocument({type: 'doc', content: [{type: 'paragraph', attrs: {[field]: value}}]})).toThrow('段落缩进');
  }
  for (const type of ['doc', 'text', 'image', 'table', 'tableCell', 'bulletList']) expect(() => indent.validateParagraphIndentDocument({type: 'doc', content: [{type, attrs: {indentLeftTwips: 0}}]})).toThrow();
});

test('body fit checks first and subsequent lines, exact zero edge, one-twip residual, and legacy width', () => {
  const check = attrs => indent.validateParagraphIndentDocument({type: 'doc', content: [{type: 'paragraph', attrs}]});
  for (const attrs of [{indentLeftTwips: 9025}, {indentRightTwips: 9025}, {firstLineIndentTwips: 9025},
    {indentLeftTwips: 100, indentRightTwips: 8925, firstLineIndentTwips: -100}, {indentLeftTwips: 0, indentRightTwips: 0, firstLineIndentTwips: 0}]) expect(() => check(attrs)).not.toThrow();
  for (const attrs of [{indentLeftTwips: 9026}, {indentRightTwips: 9026}, {firstLineIndentTwips: 9026}, {firstLineIndentTwips: -1},
    {indentLeftTwips: 100, firstLineIndentTwips: -101}, {indentLeftTwips: 100, indentRightTwips: 8926, firstLineIndentTwips: -100},
    {indentLeftTwips: 100, indentRightTwips: 8900, firstLineIndentTwips: 26}]) expect(() => check(attrs)).toThrow('正文宽度');
});

test('fixed cell widths subtract both 108-twip side margins and use each own column', () => {
  const valid = table([paragraph('first', {indentLeftTwips: 783}), heading('second', {indentLeftTwips: 1783})], {columnWidthsTwips: [1000, 2000]});
  expect(() => indent.validateParagraphIndentDocument(snapshot(doc([valid])))).not.toThrow();
  for (const [column, width] of [[0, 784], [1, 1784]]) {
    const raw = snapshot(doc([valid])); raw.content[0].content[0].content[column].content[0].attrs.indentLeftTwips = width;
    expect(() => indent.validateParagraphIndentDocument(raw)).toThrow('正文宽度');
  }
  expect(() => indent.validateParagraphIndentDocument(snapshot(doc([table([paragraph('no space', {indentLeftTwips: 0})], {columnWidthsTwips: [216]})])))).toThrow();
});

test('automatic cell grids preserve legacy truncation and exact explicit-page remainder without materializing widths', () => {
  const automatic = table(Array.from({length: 7}, () => paragraph('cell'))), raw = snapshot(doc([automatic])).content[0];
  expect(indent.indentTableColumnWidths(raw, null)).toEqual(Array(7).fill(1285));
  const a5 = page.presetPageGeometry('A5', 'portrait');
  expect(indent.indentTableColumnWidths(raw, a5)).toEqual([788, 788, 787, 787, 787, 787, 787]);
  const positioned = {...raw, attrs: {tableAlign: 'left'}};
  expect(indent.indentTableColumnWidths(positioned, a5)).toEqual(Array(7).fill(1285));
  const single = doc([table([paragraph('cell', {indentRightTwips: 5294})])], {pageGeometry: a5});
  expect(() => indent.validateParagraphIndentDocument(snapshot(single))).not.toThrow();
  const saved = snapshot(single); saved.content[0].content[0].content[0].content[0].attrs.indentRightTwips++;
  expect(() => indent.validateParagraphIndentDocument(saved)).toThrow(); expect(snapshot(single).content[0].attrs?.columnWidthsTwips).toBeUndefined();
});

test('lists nested tables blockquotes and non-simple cell contexts reject only explicit new attrs', () => {
  const unsupported = [list(paragraph('item', {indentLeftTwips: 0})), schema.node('blockquote', null, [paragraph('quoted', {indentLeftTwips: 0})]),
    table([[paragraph('outer'), table([paragraph('inner', {indentLeftTwips: 0})])]]),
    table([[paragraph('outer', {indentLeftTwips: 0}), table([paragraph('inner')])]]), list(paragraph('list'))];
  for (const node of unsupported.slice(0, -1)) expect(() => indent.validateParagraphIndentDocument(snapshot(doc([node])))).toThrow();
  expect(() => indent.validateParagraphIndentDocument(snapshot(doc([unsupported.at(-1)])))).not.toThrow();
  for (const mutation of [raw => {raw.content[0].content[0].attrs.colspan = 2;}, raw => {raw.content[0].content[0].attrs.rowspan = 2;},
    raw => {raw.content[0].content[0].attrs.colwidth = [100];}, raw => {raw.content.push({type: 'tableRow', content: []});}]) {
    const raw = snapshot(doc([table([paragraph('cell', {indentLeftTwips: 0})])])).content[0]; mutation(raw);
    expect(() => indent.validateParagraphIndentDocument({type: 'doc', content: [raw]})).toThrow();
  }
});

test('absent fields preserve exact legacy V1/V2 JSON and DOM output; explicit zero remains durable', () => {
  for (const profile of [null, 'standard-v2']) {
    const old = {type: 'doc', ...(profile ? {attrs: {styleProfile: profile}} : {}), content: [{type: 'paragraph', attrs: {textAlign: null}, content: [{type: 'text', text: 'old'}]}]};
    const internal = structuredClone(old); Object.assign(internal.content[0].attrs, {indentLeftTwips: null, indentRightTwips: null, firstLineIndentTwips: null});
    expect(JSON.stringify(indent.omitDefaultParagraphIndent(internal))).toBe(JSON.stringify(old));
    expect(internal.content[0].attrs.indentLeftTwips).toBeNull();
  }
  expect(schema.nodes.paragraph.spec.toDOM(paragraph())[1]).toEqual({});
  const raw = snapshot(doc([paragraph('zero', {indentLeftTwips: 0, indentRightTwips: 0, firstLineIndentTwips: 0})]));
  expect(raw.content[0].attrs).toEqual({indentLeftTwips: 0, indentRightTwips: 0, firstLineIndentTwips: 0});
  expect(schema.nodeFromJSON(raw).eq(doc([paragraph('zero', {indentLeftTwips: 0, indentRightTwips: 0, firstLineIndentTwips: 0})]))).toBe(true);
});

test('DOM declarations preserve exact pt margins signed hanging and spacing simultaneously for print', () => {
  const node = heading('print', {indentLeftTwips: 361, indentRightTwips: 1, firstLineIndentTwips: -360, spaceBefore: 21, spaceAfter: 0, lineSpacing: 241});
  const attrs = schema.nodes.heading.spec.toDOM(node)[1];
  expect(attrs).toMatchObject({'data-indent-left-twips': '361', 'data-indent-right-twips': '1', 'data-first-line-indent-twips': '-360'});
  for (const fragment of ['margin-left: 18.05pt', 'margin-right: 0.05pt', 'text-indent: -18pt', 'margin-top: 1.05pt', 'margin-bottom: 0pt', 'line-height: 1.0041666666666667']) expect(attrs.style).toContain(fragment);
  const definitions = indent.ParagraphIndent.config.addGlobalAttributes()[0].attributes;
  const element = (style, data = {}) => ({style, getAttribute: name => data[name] ?? null});
  expect(definitions.firstLineIndentTwips.parseHTML(element({textIndent: '-12px'}))).toBe(-180);
  expect(definitions.firstLineIndentTwips.parseHTML(element({textIndent: '999pt'}, {'data-first-line-indent-twips': '-1'}))).toBe(-1);
  expect(definitions.indentLeftTwips.parseHTML(element({marginLeft: '-1pt'}))).toBe('invalid-paragraph-indent');
});

test('caret and empty-paragraph edits preserve text selection inline attrs marks and stored marks', () => {
  const marked = schema.node('paragraph', {spaceBefore: 20}, schema.text('abc', [schema.marks.bold.create(), schema.marks.fontSize.create({halfPoints: 36})]));
  const document = doc([marked, paragraph()]), bold = schema.marks.bold.create();
  let state = stateFor(document, TextSelection.create(document, 2)); state = state.apply(state.tr.setStoredMarks([bold]));
  const transaction = indent.createParagraphIndentTransaction(state, {indentLeftTwips: 361, firstLineIndentTwips: -360});
  expect(transaction.doc.firstChild.content.eq(marked.content)).toBe(true); expect(transaction.doc.firstChild.attrs.spaceBefore).toBe(20);
  expect(transaction.storedMarks).toEqual([bold]); expect(transaction.selection.eq(state.selection)).toBe(true); expect(state.doc).toBe(document);
  const empty = stateFor(document, TextSelection.atEnd(document)); expect(indent.createParagraphIndentTransaction(empty, {firstLineIndentTwips: 0}).doc.lastChild.attrs.firstLineIndentTwips).toBe(0);
});

test('forward backward partial multi-selection respects untouched end and supports headings empty blocks and cells', () => {
  const document = doc([paragraph('first'), paragraph(), heading('last')]), records = blocks(document);
  const state = stateFor(document, TextSelection.create(document, 3, records[2].pos + 1));
  const tr = indent.createParagraphIndentTransaction(state, {indentRightTwips: 100});
  expect(blocks(tr.doc).map(({node}) => node.attrs.indentRightTwips)).toEqual([100, 100, null]);
  const reverse = stateFor(document, TextSelection.create(document, records[2].pos + 1, 3));
  expect(indent.createParagraphIndentTransaction(reverse, {indentRightTwips: 100}).doc.eq(tr.doc)).toBe(true);
  const withTable = doc([paragraph('body'), table([[paragraph('one'), paragraph('two')], heading('cell')])]);
  expect(blocks(indent.createParagraphIndentTransaction(all(withTable), {indentLeftTwips: 1}).doc).map(({node}) => node.attrs.indentLeftTwips)).toEqual([1, 1, 1, 1]);
});

test('cell and table-node selections retain exact selection kind and affect only selected cell paragraphs', () => {
  const document = doc([paragraph('body'), table([[paragraph('one'), paragraph('two')], paragraph('three')])]), cells = positions(document, 'tableCell');
  const selected = stateFor(document, CellSelection.create(document, cells[0]));
  const tr = indent.createParagraphIndentTransaction(selected, {firstLineIndentTwips: 100});
  expect(blocks(tr.doc).map(({node}) => node.attrs.firstLineIndentTwips)).toEqual([null, 100, 100, null]); expect(tr.selection.eq(selected.selection)).toBe(true);
  const nodeState = stateFor(document, NodeSelection.create(document, document.firstChild.nodeSize));
  const nodeTr = indent.createParagraphIndentTransaction(nodeState, {indentLeftTwips: 10}); expect(nodeTr.selection).toBeInstanceOf(NodeSelection); expect(nodeTr.selection.eq(nodeState.selection)).toBe(true);
});

test('per-field default mixed reset and no-op behavior distinguishes omission from explicit zero', () => {
  const document = doc([paragraph('one', {indentLeftTwips: 0, indentRightTwips: 20}), heading('two', {indentRightTwips: 20})]), state = all(document);
  expect(indent.selectedParagraphIndent(state)).toMatchObject({count: 2, supported: true, fields: {indentLeftTwips: {value: null, mixed: true}, indentRightTwips: {value: 20, mixed: false}, firstLineIndentTwips: {value: null, mixed: false}}});
  expect(indent.createParagraphIndentTransaction(state, {})).toBeNull(); expect(indent.createParagraphIndentTransaction(state, {indentRightTwips: 20})).toBeNull();
  const reset = indent.createParagraphIndentTransaction(state, {indentLeftTwips: null}); expect(snapshot(reset.doc).content[0].attrs).toEqual({indentRightTwips: 20});
  expect(snapshot(reset.doc).content[1].attrs).toMatchObject({level: 2, indentRightTwips: 20});
});

test('multi-edit one undo redo and exact draft roundtrip retain zero negative first and unrelated attrs', () => {
  const original = doc([paragraph('one'), heading('two')], {styleProfile: 'standard-v2'}); let state = all(original);
  state = apply(state, {indentLeftTwips: 361, indentRightTwips: 0, firstLineIndentTwips: -360}); state = state.apply(closeHistory(state.tr));
  const changed = state.doc; expect(schema.nodeFromJSON(snapshot(changed)).eq(changed)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
  state = apply(state, {indentLeftTwips: null, indentRightTwips: null, firstLineIndentTwips: null});
  expect(snapshot(state.doc).content[0]).not.toHaveProperty('attrs'); expect(snapshot(state.doc).attrs.styleProfile).toBe('standard-v2');
});

test('invalid patch and mixed unsupported selections leave original doc chain transaction and undo untouched', () => {
  const state = all(doc([paragraph('body'), list(paragraph('list'))]));
  expect(indent.selectedParagraphIndent(state).supported).toBe(false);
  expect(() => indent.createParagraphIndentTransaction(state, {indentLeftTwips: 0})).toThrow(); expect(undo(state)).toBe(false);
  const safe = all(doc([paragraph('body')])), original = safe.doc;
  for (const patch of [{indentLeftTwips: -1}, {indentRightTwips: 9026}, {firstLineIndentTwips: -1}, {indentLeftTwips: 1, firstLineIndentTwips: -2}, {indentLeftTwips: undefined}, {bogus: 0}]) expect(() => indent.createParagraphIndentTransaction(safe, patch)).toThrow();
  const shared = safe.tr, chainable = Object.create(safe); Object.defineProperty(chainable, 'tr', {get: () => shared});
  expect(() => indent.createParagraphIndentTransaction(chainable, {indentLeftTwips: 9026})).toThrow(); expect(shared.steps).toEqual([]); expect(safe.doc).toBe(original); expect(undo(safe)).toBe(false);
});

test('page size orientation margins and reset candidates reject narrowing before any dispatch', () => {
  const landscape = page.presetPageGeometry('A4', 'landscape'), portrait = page.presetPageGeometry('A4', 'portrait');
  const cases = [[doc([paragraph('wide', {indentLeftTwips: 10000})], {pageGeometry: landscape}), portrait],
    [doc([paragraph('wide', {indentLeftTwips: 10000})], {pageGeometry: landscape}), null],
    [doc([paragraph('wide', {indentLeftTwips: 9025})]), {...portrait, marginRightTwips: 1441}],
    [doc([table([paragraph('cell', {indentLeftTwips: 6000})])]), page.presetPageGeometry('A5', 'portrait')]];
  for (const [document, geometry] of cases) {const state = stateFor(document); expect(() => page.createPageGeometryTransaction(state, geometry)).toThrow('段落缩进'); expect(state.doc).toBe(document); expect(undo(state)).toBe(false);}
});

test('table width reset column structure and positioning candidates reject narrowed cell containers atomically', () => {
  const atCell = document => stateFor(document, TextSelection.create(document, blocks(document)[0].pos + 1));
  const fixed = atCell(doc([table([paragraph('one', {indentLeftTwips: 1000})], {columnWidthsTwips: [2000]})]));
  expect(() => tables.createTableColumnWidthTransaction(fixed, 1216)).toThrow('段落缩进');
  const wide = atCell(doc([table([paragraph('one', {indentLeftTwips: 9000})], {columnWidthsTwips: [10000]})]));
  expect(() => tables.createTableColumnWidthTransaction(wide, null)).toThrow('段落缩进');
  const auto = atCell(doc([table([paragraph('one', {indentLeftTwips: 6000})])]));
  for (const action of ['addColumnBefore', 'addColumnAfter']) expect(() => tables.createTableStructureTransaction(auto, action)).toThrow('段落缩进');
  // In a 7-column automatic table setting explicit alignment drops the page
  // remainder; that one twip matters at the first cell's exact boundary.
  const a4 = page.presetPageGeometry('A4', 'portrait');
  const seven = atCell(doc([table([paragraph('edge', {indentLeftTwips: 1069}), ...Array.from({length: 6}, () => paragraph('cell'))])], {pageGeometry: a4}));
  expect(() => tables.createTablePositionTransaction(seven, {tableAlign: 'left'})).toThrow('段落缩进');
  for (const state of [fixed, wide, auto, seven]) expect(undo(state)).toBe(false);
});

test('Tiptap direct chain and can table commands reject invalid candidate without leaking intermediate steps', () => {
  const commands = tables.WordTable.config.addCommands.call({options: {onWidthError() {}}, parent: () => ({})});
  for (const run of [manager => manager.commands.addColumnAfter(), manager => manager.chain().addColumnAfter().run(),
    manager => manager.can().addColumnAfter(), manager => manager.can().chain().addColumnAfter().run()]) {
    const document = doc([table([paragraph('one', {indentLeftTwips: 6000})])]); let state = stateFor(document, TextSelection.create(document, 4));
    const editor = {extensionManager: {commands}, get state() {return state;}, view: {dispatch(tr) {state = state.apply(tr);}}};
    expect(run(new CommandManager({editor}))).toBe(false); expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

test('guard refuses invalid pasted context direct page or column shrink and unsupported list wrapping before history', () => {
  const errors = [], document = doc([paragraph('safe', {indentLeftTwips: 100})]), state = stateFor(document, TextSelection.create(document, 2), errors);
  for (const tr of [state.tr.setNodeMarkup(0, undefined, {...document.firstChild.attrs, firstLineIndentTwips: -101}),
    state.tr.insert(document.content.size, list(paragraph('bad', {indentLeftTwips: 0}))), state.tr.insert(document.content.size, table([paragraph('bad', {indentLeftTwips: 2000})], {columnWidthsTwips: [1000]}))]) {
    const result = state.applyTransaction(tr); expect(result.transactions).toEqual([]); expect(result.state).toBe(state);
  }
  let wrapped; wrapInList(schema.nodes.bulletList)(state, tr => {wrapped = tr;}); expect(wrapped).toBeTruthy(); expect(state.applyTransaction(wrapped).transactions).toEqual([]);
  expect(errors).toHaveLength(4); expect(undo(state)).toBe(false);
});

test('Enter splits retain current indent and Backspace merges without losing signed attrs or history', () => {
  const document = doc([paragraph('firstsecond', {indentLeftTwips: 360, firstLineIndentTwips: -180})]); let state = stateFor(document, TextSelection.create(document, 6));
  expect(splitBlock(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.childCount).toBe(2);
  expect(blocks(state.doc).map(({node}) => [node.attrs.indentLeftTwips, node.attrs.firstLineIndentTwips])).toEqual([[360, -180], [360, -180]]);
  expect(joinBackward(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(document)).toBe(true);
});

test('paragraph indent decoration adds authored table width and removes on reset while old tables stay untouched', () => {
  const document = doc([table([paragraph('cell')])]), state = stateFor(document, TextSelection.create(document, 4)), plugin = state.plugins[1];
  expect(plugin.props.decorations(state).find()).toEqual([]);
  const changed = apply(state, {indentLeftTwips: 0}); const attrs = plugin.props.decorations(changed).find()[0].type.attrs;
  expect(attrs).toEqual({'data-paragraph-indent-table': '', style: '--word-indent-table-width: 450pt'});
  const reset = apply(changed, {indentLeftTwips: null}); expect(plugin.props.decorations(reset).find()).toEqual([]);
});

test('prospective text depth nodes and draft bytes fail without mutating doc or history', () => {
  let deep = paragraph('deep'); for (let i = 0; i < 33; i++) deep = schema.node('blockquote', null, [deep]);
  for (const [document, message] of [[doc([paragraph('target'), deep]), '文档结构'], [doc(Array.from({length: 5000}, () => paragraph('x'))), '文档结构'],
    [doc([paragraph('中'.repeat(700000))]), '2 MiB'], [doc([paragraph('target'), schema.node('image', {src: 'x'.repeat(32 * 1024 * 1024)})]), '32 MiB']]) {
    const state = stateFor(document); expect(() => indent.createParagraphIndentTransaction(state, {indentLeftTwips: 1})).toThrow(message); expect(state.doc).toBe(document); expect(undo(state)).toBe(false);
  }
});

class Element {
  dataset = {}; attrs = {}; children = []; value = ''; placeholder = ''; disabled = false; textContent = ''; validity = {badInput: false}; listeners = new Map();
  addEventListener(type, listener) {this.listeners.set(type, [...this.listeners.get(type) ?? [], listener]);}
  emit(type, props = {}) {for (const listener of this.listeners.get(type) ?? []) listener({preventDefault() {}, ...props});}
  contains(other) {return other?.id?.startsWith('edit-paragraph-indent-');}
  replaceChildren(...children) {this.children = children;}
  removeAttribute(name) {delete this.attrs[name];}
  querySelectorAll() {return [];}
  cloneNode() {const clone = new Element(); clone.attrs = {...this.attrs}; clone.dataset = {...this.dataset}; clone.children = this.children.map(child => child.cloneNode()); return clone;}
}
function controlsHarness(run, initial = doc([paragraph('one'), paragraph('two')])) {
  const previousDocument = globalThis.document, previousElement = globalThis.HTMLElement, elements = new Map(), notices = [];
  const element = id => {if (!elements.has(id)) {const value = new Element(); value.id = id; elements.set(id, value);} return elements.get(id);};
  globalThis.document = {getElementById: element}; globalThis.HTMLElement = Element;
  let state = stateFor(initial), locked = false, writes = 0, controls, pendingSelection;
  let editor = {get state() {return state;}, view: {dispatch(tr) {const result = state.applyTransaction(tr); state = result.state; writes += Number(result.transactions.some(item => item.docChanged)); controls.update();}, focus() {}}};
  controls = new WordParagraphIndentControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message), syncSelection: () => {if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); controls.update(); pendingSelection = null;}}}); controls.update();
  const change = (tr, notify = true) => {state = state.apply(tr); if (notify) controls.update();};
  const fill = (field, value) => {const input = element(`edit-paragraph-indent-${field}`); input.emit('pointerdown'); input.emit('focus'); input.value = value; input.emit('input');};
  try {run({element, notices, controls, change, fill, get state() {return state;}, get writes() {return writes;}, lock(value) {locked = value; controls.update();},
    select(selection) {pendingSelection = selection;}, replace() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}});}
  finally {globalThis.document = previousDocument; globalThis.HTMLElement = previousElement;}
}

test('browser-free controls synchronize pending selection apply mixed fields once and preserve unrelated mixed blanks', () => {
  const document = doc([paragraph('one', {indentLeftTwips: 20}), paragraph('two', {indentLeftTwips: 40})]);
  controlsHarness(h => {
    h.select(new AllSelection(h.state.doc)); h.fill('right', '1.05');
    expect(h.element('edit-paragraph-indent-left').placeholder).toBe('混合');
    h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    expect(blocks(h.state.doc).map(({node}) => [node.attrs.indentLeftTwips, node.attrs.indentRightTwips])).toEqual([[20, 21], [40, 21]]);
    expect(undo(h.state, h.change)).toBe(true); expect(h.state.doc.eq(document)).toBe(true);
    expect(redo(h.state, h.change)).toBe(true); h.element('edit-paragraph-indent-right-reset').emit('click');
    expect(blocks(h.state.doc).map(({node}) => node.attrs.indentRightTwips)).toEqual([null, null]);
    h.fill('left', ''); h.element('edit-paragraph-indent-apply').emit('click'); expect(blocks(h.state.doc).map(({node}) => node.attrs.indentLeftTwips)).toEqual([null, null]);
  }, document);
});

test('browser-free controls group validation rejects hanging overflow and unsafe per-field reset atomically', () => {
  controlsHarness(h => {
    h.fill('left', '18.05'); h.fill('first', '-18'); h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    h.element('edit-paragraph-indent-left-reset').emit('click'); expect(h.writes).toBe(1); expect(h.notices.at(-1)).toContain('悬挂');
    h.fill('right', '450'); h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    h.fill('right', '.001'); h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    h.element('edit-paragraph-indent-reset').emit('click'); expect(h.writes).toBe(2); expect(snapshot(h.state.doc).content[0]).not.toHaveProperty('attrs');
  });
});

test('browser-free controls no-op default and explicit zero keep clean history, locks and unsupported selections disable all actions', () => {
  controlsHarness(h => {
    h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(0); expect(undo(h.state)).toBe(false);
    h.fill('left', '0'); h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    expect(h.element('edit-paragraph-indent-left').dataset.indentState).toBe('explicit'); h.element('edit-paragraph-indent-apply').emit('click'); expect(h.writes).toBe(1);
    h.lock(true); for (const id of ['left', 'right', 'first', 'apply', 'reset']) expect(h.element(`edit-paragraph-indent-${id}`).disabled).toBe(true);
    h.element('edit-paragraph-indent-reset').emit('click'); expect(h.writes).toBe(1);
  });
  controlsHarness(h => {expect(h.element('edit-paragraph-indent-apply').disabled).toBe(true); expect(h.element('edit-paragraph-indent-state').textContent).toContain('列表不支持');}, doc([list(paragraph('item'))]));
});

test('browser-free pending fields discard on document selection owner switch and Escape, reject silent stale token and IME Enter', () => {
  controlsHarness(h => {
    const input = h.element('edit-paragraph-indent-left'); h.fill('left', '18'); input.emit('keydown', {key: 'Enter', isComposing: true}); expect(h.writes).toBe(0);
    input.emit('keydown', {key: 'Escape'}); expect(input.value).toBe('');
    h.fill('left', '18'); h.change(h.state.tr.setSelection(TextSelection.atEnd(h.state.doc))); expect(input.value).toBe('');
    h.fill('left', '18'); h.change(h.state.tr.insertText('new')); expect(input.value).toBe('');
    h.fill('left', '18'); h.replace(); expect(input.value).toBe('');
    h.fill('left', '18'); h.change(h.state.tr.insertText('stale'), false); h.element('edit-paragraph-indent-apply').emit('click'); h.element('edit-paragraph-indent-reset').emit('click'); expect(h.writes).toBe(0);
  });
});

test('immutable print clone preserves signed inline declarations and table decoration through async font load', async () => {
  const previous = globalThis.document, host = new Element(), source = new Element(), p = new Element(), wrapper = new Element();
  p.attrs = schema.nodes.paragraph.spec.toDOM(paragraph('print', {indentLeftTwips: 360, firstLineIndentTwips: -180}))[1];
  wrapper.attrs = {'data-paragraph-indent-table': '', style: '--word-indent-table-width: 450pt'}; wrapper.children = [p]; source.children = [wrapper];
  let ready; const fonts = new Promise(resolve => {ready = resolve;}); globalThis.document = {getElementById: () => host, createElement: () => new Element(), fonts: {ready: fonts}};
  try {
    const pending = preparePrintSnapshot(source); p.attrs.style = 'text-indent: 99pt'; wrapper.attrs.style = 'changed'; ready(); await pending;
    expect(host.children[0].children[0].attrs.style).toBe('--word-indent-table-width: 450pt');
    expect(host.children[0].children[0].children[0].attrs.style).toContain('text-indent: -9pt');
  } finally {globalThis.document = previous;}
});

// This DOM shim drives real ProseMirror DOMParser/DOMSerializer, but has no
// CSS layout, browser selection or clipboard APIs. Actual-browser specs above
// remain discovery-only until a supported browser environment is available.
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

test('real ProseMirror DOM serialization and parsing preserve body heading signed indents exact zeros and spacing', () => {
  for (const original of [paragraph('text', {indentLeftTwips: 361, indentRightTwips: 0, firstLineIndentTwips: -360, spaceBefore: 1, lineSpacing: 241}),
    heading('heading', {indentLeftTwips: 0, indentRightTwips: 1, firstLineIndentTwips: 0})]) {
    const parsed = parseNode(serializeNode(original)); expect(parsed.firstChild.eq(original)).toBe(true);
    expect(() => indent.validateParagraphIndentDocument(snapshot(parsed))).not.toThrow();
  }
  const cssOnly = new ParseElement('p', {style: 'margin-left:18.05pt;margin-right:9px;text-indent:-18pt'}); cssOnly.appendChild(parseDocument.createTextNode('pasted'));
  expect(parseNode(cssOnly).firstChild.attrs).toMatchObject({indentLeftTwips: 361, indentRightTwips: 135, firstLineIndentTwips: -360});
});

test('malformed exact HTML metadata and unsupported active CSS survive as rejected evidence before paste commits', () => {
  for (const attrs of [{'data-indent-left-twips': 'null'}, {'data-indent-left-twips': '1.5'}, {'data-first-line-indent-twips': '-31681'},
    {style: 'margin-left:-1pt'}, {style: 'text-indent:2em'}, {'data-indent-right-twips': 'false', style: 'margin-right:0pt'}]) {
    const element = new ParseElement('p', attrs); element.appendChild(parseDocument.createTextNode('do not paste'));
    const parsed = parseNode(element), state = stateFor(doc([paragraph('keep')]));
    expect(() => indent.validateParagraphIndentDocument(snapshot(parsed))).toThrow();
    expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]); expect(undo(state)).toBe(false);
  }
});


test('raw paragraphs and headings with any explicit indentation including zero reject tab text only', () => {
  for (const type of ['paragraph', 'heading']) for (const field of indent.paragraphIndentFields) {
    const value = {type: 'doc', content: [{type, attrs: {...(type === 'heading' ? {level: 2} : {}), [field]: 0}, content: [{type: 'text', text: 'left\tright'}]}]};
    expect(() => indent.validateParagraphIndentDocument(value)).toThrow('制表符');
  }
  expect(() => indent.validateParagraphIndentDocument(snapshot(doc([paragraph('left\tright'), heading('heading\ttab')])))).not.toThrow();
});

test('applying indentation to a tab paragraph rejects the entire multi-selection before history or shared-chain mutation', () => {
  const original = doc([paragraph('plain'), paragraph('left\tright')]);
  const state = all(original), shared = state.tr, chainable = Object.create(state); Object.defineProperty(chainable, 'tr', {get: () => shared});
  for (const field of indent.paragraphIndentFields) {
    expect(() => indent.createParagraphIndentTransaction(chainable, {[field]: 0})).toThrow('制表符');
    expect(state.doc).toBe(original); expect(shared.steps).toEqual([]); expect(undo(state)).toBe(false);
  }
  const reset = indent.createParagraphIndentTransaction(state, {indentLeftTwips: null}); expect(reset).toBeNull();
});

test('typing or pasting tab into indented paragraph is blocked while unindented tabs remain accepted', () => {
  const original = doc([paragraph('indented', {indentLeftTwips: 0}), paragraph('plain')]);
  const state = stateFor(original), second = blocks(original)[1].pos + 1;
  for (const transaction of [state.tr.insertText('\t', 2), state.tr.replaceWith(2, 2, schema.text('pasted\ttab'))]) {
    const result = state.applyTransaction(transaction); expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undo(state)).toBe(false);
  }
  const accepted = state.applyTransaction(state.tr.insertText('left\tright', second)); expect(accepted.transactions).toHaveLength(1);
  expect(accepted.state.doc.lastChild.textContent).toContain('left\tright'); expect(accepted.state.doc.firstChild.eq(original.firstChild)).toBe(true);
});

test('normal newlines and hardBreak nodes still support indentation application and later editing', () => {
  const p = schema.node('paragraph', null, [schema.text('line one\nline two'), schema.node('hardBreak'), schema.text('line three')]);
  const original = doc([p, heading('heading\nnext')]); let state = all(original);
  state = apply(state, {indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180});
  expect(state.doc.firstChild.content.eq(p.content)).toBe(true); expect(() => indent.validateParagraphIndentDocument(snapshot(state.doc))).not.toThrow();
  const result = state.applyTransaction(state.tr.insertText('\n', 2)); expect(result.transactions).toHaveLength(1);
  const insertBreak = result.state.applyTransaction(result.state.tr.insert(2, schema.node('hardBreak'))); expect(insertBreak.transactions).toHaveLength(1);
});
