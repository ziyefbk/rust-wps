import {test, expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
import {AllSelection, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {Fragment, Slice} from '@tiptap/pm/model';
import {closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {joinBackward, joinForward, splitBlock} from '@tiptap/pm/commands';
import {wrapInList} from '@tiptap/pm/schema-list';
import {createWordBlockTypeTransaction, schema as oldSchema} from './word-block-type-harness.mjs';
import {pagination, list, empty, page, schema, p, h, doc, snapshot, stateFor, fullEditor, key} from './word-paragraph-pagination-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const {WordParagraphPaginationControls} = await loadTsModule('./ui/word-paragraph-pagination-controls.ts');
const fields = pagination.paragraphPaginationFields;
const flags = {keepNext: true, keepLines: false, pageBreakBefore: true};
const table = paragraph => schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableCell', null, [paragraph])])]);
const bullet = paragraph => schema.node('bulletList', null, [schema.node('listItem', null, [paragraph])]);
function apply(state, patch) {return state.applyTransaction(closeHistory(pagination.createParagraphPaginationTransaction(state, patch))).state;}
function historyCheck(state, original) {
  const edited = state.doc; expect(undoDepth(state)).toBe(1); expect(schema.nodeFromJSON(snapshot(edited)).eq(edited)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
}
for (const field of fields) test(`${field} raw strict booleans only and explicit false retained`, () => {
  for (const value of [null, undefined, 0, 1, 'false', 'true', [], {}, NaN]) expect(() => pagination.validateParagraphPaginationDocument({type: 'doc', content: [{type: 'paragraph', attrs: {[field]: value}}]})).toThrow('段落分页');
  for (const value of [true, false]) {
    const raw = {type: 'doc', content: [{type: 'paragraph', attrs: {[field]: value}}]};
    expect(() => pagination.validateParagraphPaginationDocument(raw)).not.toThrow(); expect(snapshot(schema.nodeFromJSON(raw)).content[0].attrs[field]).toBe(value);
  }
});
test('raw misplaced node, mark, nested list/cell, image and heading attrs fail before schema drops evidence', () => {
  for (const type of ['doc', 'text', 'image', 'hardBreak', 'pageBreak', 'table', 'tableCell', 'orderedList', 'listItem']) {
    expect(() => pagination.validateParagraphPaginationDocument({type: 'doc', content: [{type, attrs: {keepNext: false}}]})).toThrow();
  }
  for (const block of [bullet(p('list', flags)), table(h('cell', flags))]) expect(() => pagination.validateParagraphPaginationState(doc([block]))).toThrow();
  expect(() => pagination.validateParagraphPaginationDocument({type: 'doc', content: [{type: 'paragraph', attrs: flags, content: [{type: 'image'}]}]})).toThrow();
  expect(() => pagination.validateParagraphPaginationDocument({type: 'doc', content: [{type: 'paragraph', marks: [{type: 'bold', attrs: {keepLines: false}}]}]})).toThrow();
  expect(() => pagination.validateParagraphPaginationState(doc([bullet(p('ordinary')), table(p('ordinary'))]))).not.toThrow();
});
test('absent attrs preserve exact pre-extension JSON and DOM for legacy and standard profiles', () => {
  for (const styleProfile of [null, 'standard-v2']) {
    const old = {type: 'doc', ...(styleProfile ? {attrs: {styleProfile}} : {}), content: [{type: 'paragraph', attrs: {textAlign: null}, content: [{type: 'text', text: 'old'}]}]};
    const schemaDefaults = structuredClone(old); Object.assign(schemaDefaults.content[0].attrs, {keepNext: null, keepLines: null, pageBreakBefore: null});
    expect(JSON.stringify(pagination.omitDefaultParagraphPagination(schemaDefaults))).toBe(JSON.stringify(old));
    expect(schemaDefaults.content[0].attrs.keepNext).toBeNull();
  }
  expect(schema.nodes.paragraph.spec.toDOM(p())[1]).toEqual(oldSchema.nodes.paragraph.spec.toDOM(oldSchema.nodes.paragraph.create())[1]); expect(schema.nodes.heading.spec.toDOM(h())[1]).toEqual(oldSchema.nodes.heading.spec.toDOM(oldSchema.nodes.heading.create({level: 2}))[1]);
});
test('DOM metadata strict parsing and rendering preserve true false default without continuous layout claims', () => {
  const attrs = schema.nodes.heading.spec.toDOM(h('Heading', flags))[1];
  expect(attrs).toMatchObject({'data-word-keep-next': 'true', 'data-word-keep-lines': 'false', 'data-word-page-break-before': 'true'});
  expect(attrs.style ?? '').not.toContain('break');
  const definitions = pagination.ParagraphPagination.config.addGlobalAttributes()[0].attributes;
  for (const field of fields) {
    for (const raw of [null, 'true', 'false']) expect(definitions[field].parseHTML({getAttribute: () => raw})).toBe(raw === null ? null : raw === 'true');
    for (const raw of ['', 'TRUE', '0', 'null', ' true']) expect(definitions[field].parseHTML({getAttribute: () => raw})).toBe('invalid-paragraph-pagination');
    expect(definitions[field].keepOnSplit).toBe(field !== 'pageBreakBefore');
  }
});
test('single and multi-paragraph patches are detached atomic history edits preserving selection marks and unrelated attrs', () => {
  const original = doc([p('abc', {spaceAfter: 100}), h('def', {indentLeftTwips: 80})], {styleProfile: 'standard-v2'});
  for (const selection of [TextSelection.create(original, 2), new AllSelection(original), TextSelection.create(original, 8, 2)]) {
    let state = stateFor(original, selection); state = state.apply(state.tr.setStoredMarks([schema.mark('bold')]));
    const shared = state.tr, chain = Object.create(state); Object.defineProperty(chain, 'tr', {get: () => shared});
    const candidate = pagination.createParagraphPaginationTransaction(chain, flags);
    expect(shared.steps).toEqual([]); expect(candidate.selection.eq(selection)).toBe(true); expect(candidate.storedMarks).toEqual(state.storedMarks);
    const result = state.applyTransaction(closeHistory(candidate)); expect(result.transactions).toHaveLength(1); expect(result.state.doc.firstChild.attrs.spaceAfter).toBe(100);
    expect(result.state.doc.lastChild.attrs.indentLeftTwips).toBe(80); historyCheck(result.state, original);
  }
});
test('mixed default true false states are independent and endpoint at following text start excludes it', () => {
  const original = doc([p('a', {keepNext: true}), p('b', {keepNext: false}), h('c')]);
  const selected = pagination.selectedParagraphPagination(stateFor(original, new AllSelection(original)));
  expect(selected.supported).toBe(true); expect(selected.fields.keepNext).toEqual({value: null, mixed: true, explicit: true});
  expect(selected.fields.keepLines).toEqual({value: null, mixed: false, explicit: false});
  const state = stateFor(original, TextSelection.create(original, 1, 7)); const next = apply(state, {keepNext: false});
  expect(next.doc.child(0).attrs.keepNext).toBe(false); expect(next.doc.child(1).attrs.keepNext).toBe(false); expect(next.doc.child(2).attrs.keepNext).toBeNull();
});
test('field reset and all reset remove attrs without clearing unrelated formatting and undo restores false', () => {
  const original = doc([h('a', {...flags, keepNext: false, spaceAfter: 120})]);
  let state = apply(stateFor(original), {keepNext: null}); expect(snapshot(state.doc).content[0].attrs).toMatchObject({keepLines: false, pageBreakBefore: true, spaceAfter: 120});
  expect(snapshot(state.doc).content[0].attrs.keepNext).toBeUndefined(); historyCheck(state, original);
  state = apply(stateFor(original), {keepNext: null, keepLines: null, pageBreakBefore: null});
  for (const field of fields) expect(snapshot(state.doc).content[0].attrs[field]).toBeUndefined(); historyCheck(state, original);
});
test('unsupported or mixed list/cell/image/page-break selections refuse entire candidate without history', () => {
  const image = schema.node('image', {src: 'data:image/png;base64,iVBORw0KGgo='});
  for (const object of [bullet(p('item')), table(p('cell')), image, schema.node('pageBreak')]) {
    const original = doc([p('outside'), object]), state = stateFor(original, new AllSelection(original));
    expect(pagination.selectedParagraphPagination(state).supported).toBe(false);
    expect(() => pagination.createParagraphPaginationTransaction(state, {keepNext: true})).toThrow(); expect(undoDepth(state)).toBe(0);
  }
  const original = doc([table(p('cell'))]); const selection = CellSelection.create(original, 2);
  expect(() => pagination.createParagraphPaginationTransaction(stateFor(original, selection), {keepLines: false})).toThrow();
});
test('bad patch fields, coercions and stale doc or selection tokens never mutate the document', () => {
  const original = doc([p('abc')]), state = stateFor(original), token = pagination.captureParagraphPagination(state);
  for (const patch of [{keepNext: 0}, {keepLines: undefined}, {pageBreakBefore: 'true'}, {other: true}]) expect(() => pagination.createParagraphPaginationTransaction(state, patch)).toThrow();
  expect(pagination.createParagraphPaginationTransaction(state, {})).toBeNull();
  expect(pagination.createParagraphPaginationTransaction(state.apply(state.tr.setSelection(TextSelection.create(original, 2))), flags, token)).toBeNull();
  expect(pagination.createParagraphPaginationTransaction(state.apply(state.tr.insertText('x')), flags, token)).toBeNull();
});
for (const type of ['paragraph', 'heading']) for (const at of [0, 2, 4]) test(`bounded ${type} Enter at ${at} owns break-before only on leading block`, () => {
  for (const pageBreakBefore of [true, false]) {
    const original = doc([(type === 'heading' ? h : p)('abcd', {...flags, pageBreakBefore})]); const state = stateFor(original, TextSelection.create(original, at + 1));
    const candidate = pagination.createParagraphPaginationSplitTransaction(state), result = state.applyTransaction(closeHistory(candidate));
    expect(result.transactions).toHaveLength(1); const next = result.state.doc;
    expect(next.childCount).toBe(2); expect(next.firstChild.attrs.pageBreakBefore).toBe(pageBreakBefore); expect(next.lastChild.attrs.pageBreakBefore).toBeNull();
    expect(next.firstChild.textContent).toBe('abcd'.slice(0, at)); expect(next.lastChild.textContent).toBe('abcd'.slice(at));
    expect(next.lastChild.attrs.keepNext).toBe(true); expect(next.lastChild.attrs.keepLines).toBe(false);
    expect(next.lastChild.type.name).toBe(type === 'heading' && at === 4 ? 'paragraph' : type); historyCheck(result.state, original);
  }
});
test('Enter replacing selected text preserves leading ownership and rejects cross-block selection', () => {
  const original = doc([p('abcd', flags), p('after')]);
  const state = stateFor(original, TextSelection.create(original, 2, 4)), result = state.applyTransaction(pagination.createParagraphPaginationSplitTransaction(state));
  expect(result.state.doc.child(0).textContent).toBe('a'); expect(result.state.doc.child(1).textContent).toBe('d'); expect(result.state.doc.child(1).attrs.pageBreakBefore).toBeNull();
  expect(() => pagination.createParagraphPaginationSplitTransaction(stateFor(original, TextSelection.create(original, 2, 8)))).toThrow();
});
test('upstream generic split cannot duplicate break-before and forged metadata cannot authorize it', () => {
  const original = doc([p('abcd', flags)]), state = stateFor(original, TextSelection.create(original, 3)); let candidate;
  expect(splitBlock(state, tr => {candidate = tr;})).toBe(true); expect(state.applyTransaction(candidate).transactions).toEqual([]);
  candidate = state.tr.split(3).setMeta('safePaginationSplit', true); expect(state.applyTransaction(candidate).transactions).toEqual([]);
  candidate = pagination.createParagraphPaginationSplitTransaction(state); candidate.join(candidate.doc.firstChild.nodeSize);
  expect(state.applyTransaction(candidate).transactions).toEqual([]);
});
for (const side of ['leading', 'trailing', 'both']) test(`${side} flagged merge and partial cross-block delete reject atomically`, () => {
  const original = doc([p('first', side !== 'trailing' ? flags : {}), h('last', side !== 'leading' ? {...flags, keepNext: false} : {})]);
  for (const [position, command] of [[original.firstChild.nodeSize + 1, joinBackward], [original.firstChild.nodeSize - 1, joinForward]]) {
    const state = stateFor(original, TextSelection.create(original, position)); let candidate;
    expect(command(state, tr => {candidate = tr;})).toBe(true); expect(state.applyTransaction(candidate).transactions).toEqual([]); expect(undoDepth(state)).toBe(0);
  }
  const state = stateFor(original, TextSelection.create(original, 3, original.firstChild.nodeSize + 3));
  expect(state.applyTransaction(state.tr.deleteSelection()).transactions).toEqual([]);
});
test('ordinary inline edits, clear text, block deletion and entire document clear remain possible with undo', () => {
  const original = doc([p('first', flags), p('after')]);
  for (const selection of [TextSelection.create(original, 1, 6), NodeSelection.create(original, 0), new AllSelection(original)]) {
    const state = stateFor(original, selection), result = state.applyTransaction(closeHistory(state.tr.deleteSelection()));
    expect(result.transactions).toHaveLength(1); historyCheck(result.state, original);
  }
  const state = stateFor(original); const result = state.applyTransaction(closeHistory(state.tr.insertText('typed'))); expect(result.transactions).toHaveLength(1); historyCheck(result.state, original);
});
test('app body heading conversions preserve true false absent and unsupported list moves reject', () => {
  const original = doc([p('first', flags), h('second', {keepNext: false})]);
  for (const level of [0, 1, 2, 3]) {
    const state = stateFor(original, new AllSelection(original)), candidate = createWordBlockTypeTransaction(state, level);
    const result = state.applyTransaction(closeHistory(candidate)); expect(result.transactions).toHaveLength(1);
    expect(result.state.doc.firstChild.attrs).toMatchObject(flags); expect(result.state.doc.lastChild.attrs.keepNext).toBe(false); historyCheck(result.state, original);
  }
  const state = stateFor(doc([p('first', flags)])); let candidate;
  expect(wrapInList(schema.nodes.bulletList)(state, tr => {candidate = tr;})).toBe(true); expect(state.applyTransaction(candidate).transactions).toEqual([]);
  const target = table(p('first', flags)); expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, target)).transactions).toEqual([]);
});
test('raw attribute removal is refused; explicit app reset and unrelated shell changes retain ownership', () => {
  const original = doc([p('first', flags)]), state = stateFor(original);
  expect(state.applyTransaction(state.tr.setNodeMarkup(0, undefined, {...original.firstChild.attrs, keepNext: null})).transactions).toEqual([]);
  expect(state.applyTransaction(state.tr.setNodeAttribute(0, 'keepNext', null)).transactions).toEqual([]);
  expect(state.applyTransaction(state.tr.addMark(1, 3, schema.mark('bold'))).transactions).toHaveLength(1);
  expect(state.applyTransaction(state.tr.setNodeMarkup(0, undefined, {...original.firstChild.attrs, spaceAfter: 240})).transactions).toHaveLength(1);
  expect(state.applyTransaction(pagination.createParagraphPaginationTransaction(state, {keepNext: null})).transactions).toHaveLength(1);
});
test('closed full clipboard slices remain generic while incompatible open shapes reject before fitting', () => {
  const errors = [], guard = pagination.paragraphPaginationGuard(message => errors.push(message));
  const state = stateFor(doc([p('destination', flags)])); const view = {state, editable: true, composing: false, dispatch() {}};
  const paste = slice => guard.props.handlePaste(view, {}, slice);
  expect(paste(new Slice(Fragment.from(p('safe', flags)), 0, 0))).toBe(false);
  for (const slice of [new Slice(Fragment.fromArray([p('open', flags), p('next', flags)]), 1, 1),
    new Slice(Fragment.from(p('one side', flags)), 1, 0), new Slice(Fragment.from(bullet(p('bad', flags))), 0, 0),
    new Slice(Fragment.from(table(p('bad', flags))), 0, 0)]) expect(paste(slice)).toBe(true);
  expect(paste(new Slice(Fragment.from(p('ordinary')), 1, 1))).toBe(false); expect(errors).toHaveLength(4);
});
for (const direction of [-1, 1]) test(`compatible body merge direction ${direction} keeps explicit false, all attrs and marked text`, () => {
  const first = schema.node('paragraph', {spaceAfter: 120, indentLeftTwips: 80}, schema.text('First', [schema.mark('bold')]));
  const second = schema.node('paragraph', {spaceAfter: 120, indentLeftTwips: 80, keepNext: false, keepLines: false, pageBreakBefore: false}, schema.text('Second', [schema.mark('italic')]));
  const original = doc([first, second, p('after')]); const position = direction < 0 ? first.nodeSize + 1 : first.nodeSize - 1;
  const state = stateFor(original, TextSelection.create(original, position)), candidate = pagination.createParagraphPaginationMergeTransaction(state, direction);
  const result = state.applyTransaction(candidate); expect(result.transactions).toHaveLength(1); const merged = result.state.doc.firstChild;
  expect(merged.attrs).toMatchObject({spaceAfter: 120, indentLeftTwips: 80, keepNext: false, keepLines: false, pageBreakBefore: false});
  expect(merged.content.eq(first.content.append(second.content))).toBe(true); expect(result.state.selection.from).toBe(first.nodeSize - 1); historyCheck(result.state, original);
});
test('compatible heading merges use true default and never erase explicit heading false', () => {
  for (const [firstFlags, secondFlags, expected] of [[{}, {keepNext: true, keepLines: false, pageBreakBefore: false}, true],
    [{keepNext: false}, {keepNext: false, keepLines: false}, false]]) {
    const original = doc([h('First', firstFlags), h('Second', secondFlags)]), state = stateFor(original, TextSelection.create(original, original.firstChild.nodeSize + 1));
    const result = state.applyTransaction(pagination.createParagraphPaginationMergeTransaction(state, -1)); expect(result.transactions).toHaveLength(1);
    expect(result.state.doc.firstChild.attrs.keepNext).toBe(expected); historyCheck(result.state, original);
  }
  const mismatch = doc([h('default'), h('false', {keepNext: false})]);
  expect(() => pagination.createParagraphPaginationMergeTransaction(stateFor(mismatch, TextSelection.create(mismatch, mismatch.firstChild.nodeSize + 1)), -1)).toThrow();
});
test('merge candidates refuse differing type level spacing indentation font baseline and pagination ownership', () => {
  const base = {keepNext: false, keepLines: false, pageBreakBefore: false};
  for (const second of [p('right', {...base, keepLines: true}), p('right', {...base, pageBreakBefore: true}),
    p('right', {...base, spaceAfter: 20}), p('right', {...base, indentLeftTwips: 20}), h('right', base)]) {
    const original = doc([p('left', base), second]), state = stateFor(original, TextSelection.create(original, original.firstChild.nodeSize + 1));
    expect(() => pagination.createParagraphPaginationMergeTransaction(state, -1)).toThrow(); expect(undoDepth(state)).toBe(0);
  }
  const original = doc([h('left', base), h('right', {...base, level: 3})]);
  expect(() => pagination.createParagraphPaginationMergeTransaction(stateFor(original, TextSelection.create(original, original.firstChild.nodeSize + 1)), -1)).toThrow();
});
test('approved merge proof cannot authorize appended attr erasure or unsupported list movement', () => {
  const base = {keepNext: false, keepLines: false, pageBreakBefore: false}, original = doc([p('a', base), p('b', base)]);
  const state = stateFor(original, TextSelection.create(original, original.firstChild.nodeSize + 1));
  const candidate = pagination.createParagraphPaginationMergeTransaction(state, -1); candidate.setNodeAttribute(0, 'keepNext', null);
  expect(state.applyTransaction(candidate).transactions).toEqual([]);
});
test('ordinary inline paste copies exact marks and hardBreaks while preserving destination shell regardless of source flags', () => {
  const source = schema.node('heading', {level: 2, keepNext: true, keepLines: true, pageBreakBefore: true, spaceAfter: 400},
    [schema.text('Pasted', [schema.mark('bold')]), schema.node('hardBreak'), schema.text('text', [schema.mark('italic')])]);
  const original = doc([p('abcd', {keepNext: false, keepLines: false, pageBreakBefore: false, indentLeftTwips: 20, spaceAfter: 120})]);
  for (const [from, to] of [[1, 1], [3, 3], [2, 4], [5, 5]]) {
    const state = stateFor(original, TextSelection.create(original, from, to)), slice = new Slice(Fragment.from(source), 1, 1);
    const candidate = pagination.createParagraphPaginationInlinePasteTransaction(state, slice), result = state.applyTransaction(candidate); expect(result.transactions).toHaveLength(1);
    expect(result.state.doc.firstChild.attrs).toEqual(original.firstChild.attrs);
    const expected = original.firstChild.content.cut(0, from - 1).append(source.content).append(original.firstChild.content.cut(to - 1));
    expect(result.state.doc.firstChild.content.eq(expected)).toBe(true); historyCheck(result.state, original);
  }
});
test('inline paste refuses nested or cross-block destinations and raw malformed source attrs before dropping shell', () => {
  const slice = new Slice(Fragment.from(p('paste', flags)), 1, 1);
  for (const original of [doc([bullet(p('list'))]), doc([table(p('cell'))])]) expect(() => pagination.createParagraphPaginationInlinePasteTransaction(stateFor(original), slice)).toThrow();
  const original = doc([p('first', flags), p('last')]);
  expect(() => pagination.createParagraphPaginationInlinePasteTransaction(stateFor(original, new AllSelection(original)), slice)).toThrow();
  for (const attrs of [{keepNext: 'false'}, {...flags, emptyParagraphFont: 'invalid'}, {...flags, indentLeftTwips: 'invalid'}, {...flags, listGeometry: 'invalid'}]) {
    const malformed = new Slice(Fragment.from(p('paste', attrs)), 1, 1);
    expect(() => pagination.createParagraphPaginationInlinePasteTransaction(stateFor(original), malformed)).toThrow();
  }
});
test('document node depth text and encoded JSON budgets reject candidates before dispatch', () => {
  expect(() => pagination.validateParagraphPaginationState(doc(Array.from({length: 10001}, () => p())))).toThrow('结构');
  expect(() => pagination.createParagraphPaginationTransaction(stateFor(doc([p('中'.repeat(700000))])), flags)).toThrow('2 MiB');
  let raw = {type: 'paragraph', attrs: flags}; for (let depth = 0; depth < 33; depth++) raw = {type: 'wrapper', content: [raw]};
  expect(() => pagination.validateParagraphPaginationDocument({type: 'doc', content: [raw]})).toThrow();
  const huge = schema.node('image', {src: `data:image/png;base64,${'A'.repeat(33 * 1024 * 1024)}`});
  expect(() => pagination.createParagraphPaginationTransaction(stateFor(doc([p('target'), huge])), flags)).toThrow('32 MiB');
});
const editors = [];
const makeEditor = (...args) => {const result = fullEditor(...args); editors.push(result.editor); return result;};
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});
test('actual registered Enter and full middleware history preserve pagination with empty baseline cleanup', () => {
  for (const content of ['', 'abcd']) {
    const font = {family: 'Arial', halfPoints: 24, bold: false, italic: true};
    const original = doc([p(content, {...flags, ...(!content ? {emptyParagraphFont: font} : {})}), p('after')]);
    const {editor, errors} = makeEditor(original); const before = editor.state.doc;
    editor.commands.setTextSelection(1 + Math.floor(content.length / 2)); expect(key(editor, 'Enter')).toBe(true);
    expect(editor.state.doc.childCount).toBe(3); expect(editor.state.doc.child(0).attrs.pageBreakBefore).toBe(true); expect(editor.state.doc.child(1).attrs.pageBreakBefore).toBeNull();
    expect(editor.state.doc.child(0).attrs.emptyParagraphFont).toBeNull(); expect(editor.state.doc.child(1).attrs.emptyParagraphFont).toBeNull();
    const after = editor.state.doc; expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
    expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(after)).toBe(true); expect(errors).toEqual([]);
  }
});
test('registered Enter is a separate undo event between immediate preceding and following typing', () => {
  const {editor, errors} = makeEditor(doc([p('abcdef', flags), p('after')])); const original = editor.state.doc;
  editor.commands.setTextSelection(3); editor.view.dispatch(editor.state.tr.insertText('BEFORE')); const typed = editor.state.doc;
  expect(key(editor, 'Enter')).toBe(true); const split = editor.state.doc;
  editor.view.dispatch(editor.state.tr.insertText('AFTER')); const final = editor.state.doc; expect(undoDepth(editor.state)).toBe(3);
  for (const expected of [split, typed, original]) {expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
  for (const expected of [typed, split, final]) {expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
  expect(errors).toEqual([]);
});
test('registered empty first heading Backspace retains false pagination in existing body conversion path', () => {
  const {editor, errors} = makeEditor(doc([h('', {...flags, keepNext: false}), p('after')])); const original = editor.state.doc;
  editor.commands.setTextSelection(1); expect(key(editor, 'Backspace')).toBe(true); expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
  expect(editor.state.doc.firstChild.attrs).toMatchObject({...flags, keepNext: false});
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(errors).toEqual([]);
});
test('registered Backspace undoes a later flagged heading input rule before incompatible paragraph merge', () => {
  const base = {keepNext: false, keepLines: false, pageBreakBefore: false};
  const {editor, errors} = makeEditor(doc([p('previous', base), p('Tail', base)]));
  const start = editor.state.doc.firstChild.nodeSize + 1; editor.commands.setTextSelection(start); editor.view.dispatch(editor.state.tr.insertText('##'));
  const cursor = start + 2; const handled = editor.state.plugins.filter(plugin => plugin.spec.isInputRules).some(plugin => plugin.props.handleTextInput(editor.view, cursor, cursor, ' '));
  expect(handled).toBe(true); expect(editor.state.doc.child(1).type.name).toBe('heading');
  expect(key(editor, 'Backspace')).toBe(true); expect(editor.state.doc.child(1).type.name).toBe('paragraph');
  expect(editor.state.doc.child(1).textContent).toBe('## Tail'); expect(editor.state.doc.child(1).attrs).toMatchObject(base); expect(errors).toEqual([]);
});
test('actual registered Enter respects readonly lock and composition without consuming unflagged ordinary path', () => {
  for (const mode of ['readonly', 'locked', 'composing']) {
    const {editor, errors} = makeEditor(doc([p('abc', flags)]), () => mode === 'locked'); const before = editor.state.doc;
    if (mode === 'readonly') editor.setEditable(false, false); if (mode === 'composing') editor.view.composing = true;
    expect(key(editor, 'Enter')).toBe(true); expect(editor.state.doc.eq(before)).toBe(true); expect(undoDepth(editor.state)).toBe(0); expect(errors).toEqual([]);
  }
});
test('full extension chain retained decimal list still splits and body controls refuse mixed selection', () => {
  const item = schema.node('listItem', null, p('item', {listGeometry: {leftTwips: 720, rightTwips: 0, hangingTwips: 360}}));
  const preserved = schema.node('orderedList', {start: 7, markerFont: {fontFamily: 'Arial', fontSize: 22}}, [item]);
  const {editor, errors} = makeEditor(doc([preserved, p('outside', flags)], {pageGeometry: page.presetPageGeometry('Letter', 'portrait')}));
  editor.commands.setTextSelection(5); expect(key(editor, 'Enter')).toBe(true); expect(editor.state.doc.firstChild.childCount).toBe(2); expect(errors).toEqual([]);
  expect(snapshot(editor.state.doc).content[0].content[1].content[0].attrs.keepNext).toBeUndefined();
  editor.view.dispatch(editor.state.tr.setSelection(new AllSelection(editor.state.doc))); expect(() => pagination.createParagraphPaginationTransaction(editor.state, flags)).toThrow();
});
test('registered compatible Backspace and Delete merge retain exact undo redo between adjacent typing', () => {
  for (const keyName of ['Backspace', 'Delete']) {
    const base = {keepNext: false, keepLines: false, pageBreakBefore: false};
    const {editor, errors} = makeEditor(doc([p('first', base), p('second', base), p('after')])); const original = editor.state.doc;
    editor.commands.setTextSelection(2); editor.view.dispatch(editor.state.tr.insertText('BEFORE')); const typed = editor.state.doc;
    editor.commands.setTextSelection(typed.firstChild.nodeSize + (keyName === 'Backspace' ? 1 : -1)); expect(key(editor, keyName)).toBe(true); const merged = editor.state.doc;
    expect(merged.childCount).toBe(2); editor.view.dispatch(editor.state.tr.insertText('AFTER')); const final = editor.state.doc; expect(undoDepth(editor.state)).toBe(3);
    for (const expected of [merged, typed, original]) {expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
    for (const expected of [typed, merged, final]) {expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
    expect(errors).toEqual([]);
  }
});
test('registered inline clipboard handler preserves destination flags marks and atomic history and honors lock', () => {
  const source = schema.node('heading', {level: 2, ...flags}, schema.text('PASTE', [schema.mark('bold')]));
  for (const locked of [false, true]) {
    const {editor, errors} = makeEditor(doc([p('target', {keepNext: false, keepLines: false, pageBreakBefore: false})]), () => locked);
    editor.commands.setTextSelection(3); const original = editor.state.doc;
    const handled = editor.state.plugins.some(plugin => plugin.props.handlePaste?.(editor.view, {}, new Slice(Fragment.from(editor.schema.nodeFromJSON(source.toJSON())), 1, 1)));
    expect(handled).toBe(true);
    if (locked) {expect(editor.state.doc.eq(original)).toBe(true); expect(undoDepth(editor.state)).toBe(0);}
    else {expect(editor.state.doc.firstChild.textContent).toBe('taPASTErget'); expect(editor.state.doc.firstChild.attrs).toEqual(original.firstChild.attrs);
      const pasted = editor.state.doc; expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(original)).toBe(true); expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(pasted)).toBe(true);}
    expect(errors).toEqual([]);
  }
});
test('actual shortcut and markdown input preserve flags and their input-rule undo', () => {
  const {editor, errors} = makeEditor(doc([p('Tail', {...flags, keepNext: false}), p('after')]));
  expect(key(editor, '2', {ctrlKey: true, altKey: true})).toBe(true); expect(editor.state.doc.firstChild.type.name).toBe('heading'); expect(editor.state.doc.firstChild.attrs.keepNext).toBe(false);
  expect(editor.commands.undo()).toBe(true); editor.commands.setTextSelection(1); editor.view.dispatch(closeHistory(editor.state.tr.insertText('##')));
  editor.view.dispatch(closeHistory(editor.state.tr)); const hashed = editor.state.doc;
  const handled = editor.state.plugins.filter(plugin => plugin.spec.isInputRules).some(plugin => plugin.props.handleTextInput(editor.view, 3, 3, ' '));
  expect(handled).toBe(true); expect(editor.state.doc.firstChild.attrs).toMatchObject({...flags, keepNext: false});
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe(`## Tail`); expect(editor.state.doc.firstChild.attrs.keepNext).toBe(false); expect(errors).toEqual([]);
});
test('print CSS explicitly overrides heading default for false after/inside/before; immutable clone retains data attrs', () => {
  const css = readFileSync('ui/style.css', 'utf8'), print = css.slice(css.indexOf('@media print'));
  expect(print).toContain('#print-document h1,#print-document h2,#print-document h3 { break-after:avoid;');
  for (const [field, prop, yes] of [['keep-next', 'after', 'avoid'], ['keep-lines', 'inside', 'avoid'], ['page-break-before', 'before', 'page']]) {
    expect(print).toContain(`#print-document [data-word-${field}="true"] { break-${prop}:${yes}; }`);
    expect(print).toContain(`#print-document [data-word-${field}="false"] { break-${prop}:auto; }`);
  }
  const code = readFileSync('ui/printing.ts', 'utf8'); expect(code).toContain('source.cloneNode(true)'); expect(code).not.toContain('data-word-keep');
});
class Element {
  constructor() {this.handlers = {}; this.value = ''; this.disabled = false; this.dataset = {}; this.textContent = '';}
  addEventListener(name, callback) {(this.handlers[name] ??= []).push(callback);}
  emit(name, event = {}) {for (const callback of this.handlers[name] ?? []) callback(event);}
}
function controlsHarness(run) {
  const previous = globalThis.document, elements = new Map(), element = id => {if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id);};
  globalThis.document = {getElementById: element};
  let state = stateFor(doc([p('first', flags), h('second', {keepNext: false})])), locked = false, controls, pendingSelection;
  const notices = [], editor = {get state() {return state;}, isEditable: true, view: {composing: false, focus() {}, dispatch(tr) {state = state.apply(tr); controls.update();}}};
  controls = new WordParagraphPaginationControls({editor: () => editor, locked: () => locked, notice: message => notices.push(message), syncSelection: () => {
    if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); pendingSelection = null; controls.update();}
  }}); controls.update();
  try {run({element, editor, notices, controls, get state() {return state;}, lock(value) {locked = value; controls.update();},
    select(selection) {pendingSelection = selection;}, change(tr) {state = state.apply(tr); controls.update();},
    set(id, value) {element(id).emit('pointerdown'); element(id).value = value; element(id).emit('change');}});} finally {globalThis.document = previous;}
}
test('three-state controls edit false default and mixed with exact history and selection sync', () => controlsHarness(h => {
  const select = h.element('edit-keep-next'); expect(select.value).toBe('true'); h.set('edit-keep-next', 'false'); expect(h.state.doc.firstChild.attrs.keepNext).toBe(false);
  expect(h.element('edit-pagination-reset').disabled).toBe(false); h.set('edit-keep-next', 'default'); expect(h.state.doc.firstChild.attrs.keepNext).toBeNull();
  h.select(new AllSelection(h.state.doc)); select.emit('pointerdown'); expect(select.value).toBe('mixed'); select.value = 'true'; select.emit('change');
  expect(h.state.doc.firstChild.attrs.keepNext).toBe(true); expect(h.state.doc.lastChild.attrs.keepNext).toBe(true);
  h.element('edit-pagination-reset').emit('click'); for (const node of [h.state.doc.firstChild, h.state.doc.lastChild]) for (const field of fields) expect(node.attrs[field]).toBeNull(); expect(h.notices).toEqual([]);
}));
test('controls pending stale selection and readonly lock composition block edits', () => controlsHarness(h => {
  for (const mode of ['lock', 'readonly', 'composing', 'stale']) {
    const original = h.state.doc, select = h.element('edit-keep-next'); select.emit('pointerdown');
    if (mode === 'lock') h.lock(true); if (mode === 'readonly') h.editor.isEditable = false; if (mode === 'composing') h.editor.view.composing = true;
    if (mode === 'stale') h.change(h.state.tr.setSelection(TextSelection.create(h.state.doc, 2)));
    select.value = 'false'; select.emit('change'); expect(h.state.doc.eq(original)).toBe(true);
    h.lock(false); h.editor.isEditable = true; h.editor.view.composing = false;
  }
}));
test('raw HTML preflight rejects unsupported tags nested contexts and malformed values while retaining false', () => {
  const previous = globalThis.DOMParser;
  function run(element) {globalThis.DOMParser = class {parseFromString() {return {querySelectorAll: () => [element]};}}; return pagination.preflightParagraphPaginationHTML('original');}
  const good = {tagName: 'P', closest: () => null, querySelector: () => null, getAttribute: name => name === 'data-word-keep-next' ? 'false' : null};
  try {
    expect(run(good)).toBe('original');
    for (const changed of [{tagName: 'SPAN'}, {closest: () => ({})}, {querySelector: () => ({})}, {getAttribute: () => '0'}]) expect(run({...good, ...changed})).toContain('invalid');
  } finally {globalThis.DOMParser = previous;}
});
