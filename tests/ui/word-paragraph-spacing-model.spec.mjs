import {test, expect} from '@playwright/test';
import {getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import Image from '@tiptap/extension-image';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {closeHistory, history, redo, undo} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';

const spacing = await loadTsModule('./ui/word-paragraph-spacing.ts');
const {ParagraphSpacing, createParagraphSpacingTransaction, selectedParagraphSpacing, omitDefaultParagraphSpacing,
  validateParagraphSpacingDocument, parseParagraphSpacePoints, parseParagraphSpaceCss, parseLineSpacingLines,
  parseLineSpacingCss, formatLineSpacingLines} = spacing;
const schema = getSchema([StarterKit, TableKit, ParagraphSpacing]);
const paragraph = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const heading = (text, attrs = {}) => schema.node('heading', {level: 2, ...attrs}, schema.text(text));
const doc = children => schema.node('doc', null, children);
const blocks = document => {
  const result = [];
  document.descendants((node, pos) => { if (['paragraph', 'heading'].includes(node.type.name)) result.push({node, pos}); });
  return result;
};
const all = document => EditorState.create({doc: document, selection: new AllSelection(document), plugins: [history()]});

test('paragraph points parse exact twips and reject fractional, non-finite and out-of-range values', () => {
  for (const [input, units] of [['0', 0], ['.05', 1], ['12', 240], ['1584', 31680], [' 1.10 ', 22]]) {
    expect(parseParagraphSpacePoints(input)).toBe(units);
  }
  for (const input of ['', '-1', '+1', '1e1', '1.001', '1584.05', 'NaN', 'Infinity', '0.05000000000000001', '9'.repeat(65)]) {
    expect(parseParagraphSpacePoints(input)).toBeNull();
  }
  expect(parseParagraphSpaceCss('12px')).toBe(180);
  expect(parseParagraphSpaceCss('0.05pt')).toBe(1);
  expect(parseParagraphSpaceCss('0')).toBe(0);
  for (const css of ['-1pt', '1em', 'auto', 'calc(1pt)', '0.01px', '1585pt']) expect(parseParagraphSpaceCss(css)).toBeNull();
});

test('every canonical line spacing displays and reparses exactly including repeating fractions', () => {
  for (let value = 120; value <= 2400; value++) expect(parseLineSpacingLines(formatLineSpacingLines(value))).toBe(value);
  expect(formatLineSpacingLines(241)).toBe('241/240');
  expect(formatLineSpacingLines(276)).toBe('1.15');
  expect(parseLineSpacingLines(' 241 / 240 ')).toBe(241);
  expect(parseLineSpacingCss('150%')).toBe(360);
  expect(parseLineSpacingCss('1.5')).toBe(360);
  for (const input of ['0', '.49', '10.01', '120/120', '119/240', '2401/240', '1.0042', '1e0', 'NaN']) {
    expect(parseLineSpacingLines(input)).toBeNull();
  }
  for (const input of ['normal', '12pt', '24px', '49%', '1000.1%', 'calc(1.5)', '241/240']) expect(parseLineSpacingCss(input)).toBeNull();
});

test('spacing extension renders exact canonical metadata and inline print-safe CSS without default styles', () => {
  const p = paragraph('Print', {spaceBefore: 1, spaceAfter: 31680, lineSpacing: 241});
  const rendered = schema.nodes.paragraph.spec.toDOM(p);
  expect(rendered[1]).toEqual({'data-space-before': '1', 'data-space-after': '31680', 'data-line-spacing': '241',
    style: 'margin-top: 0.05pt; margin-bottom: 1584pt; line-height: 1.0041666666666667'});
  expect(schema.nodes.paragraph.spec.toDOM(paragraph())[1]).toEqual({});
  expect(schema.nodes.heading.spec.toDOM(heading('Head', {lineSpacing: 360}))[1].style).toBe('line-height: 1.5');
});

test('HTML parsers retain canonical line metadata and only accept supported CSS forms', () => {
  const attrs = ParagraphSpacing.config.addGlobalAttributes.call(ParagraphSpacing)[0].attributes;
  const element = (style, data = {}) => ({style, getAttribute: key => data[key] ?? null});
  expect(attrs.lineSpacing.parseHTML(element({lineHeight: '1.0041666666666667'}, {'data-line-spacing': '241'}))).toBe(241);
  expect(attrs.spaceBefore.parseHTML(element({marginTop: '9px'}))).toBe(135);
  expect(attrs.spaceAfter.parseHTML(element({marginBottom: '1.05pt'}))).toBe(21);
  expect(attrs.lineSpacing.parseHTML(element({lineHeight: '125%'}))).toBe(300);
  expect(attrs.lineSpacing.parseHTML(element({lineHeight: '24px'}, {'data-line-spacing': '119'}))).toBeNull();
  expect(attrs.spaceAfter.parseHTML(element({marginBottom: '-1pt'}, {'data-space-after': 'NaN'}))).toBeNull();
});

test('caret changes one paragraph including an empty paragraph and keeps marks and inline content', () => {
  const bold = schema.marks.bold.create();
  const original = doc([schema.node('paragraph', {spaceAfter: 120}, schema.text('abc', [bold])), paragraph()]);
  const state = EditorState.create({doc: original, selection: TextSelection.create(original, 2), storedMarks: [bold]});
  const transaction = createParagraphSpacingTransaction(state, {spaceBefore: 0, lineSpacing: 360});
  expect(transaction.doc.child(0).attrs).toMatchObject({spaceBefore: 0, spaceAfter: 120, lineSpacing: 360});
  expect(transaction.doc.child(0).content.eq(original.child(0).content)).toBe(true);
  expect(transaction.doc.child(1).attrs.spaceBefore).toBeNull();
  expect(transaction.storedMarks).toEqual([bold]);
  expect(state.doc).toBe(original);
  const emptyState = EditorState.create({doc: original, selection: TextSelection.create(original, 6)});
  expect(createParagraphSpacingTransaction(emptyState, {spaceAfter: 20}).doc.child(1).attrs.spaceAfter).toBe(20);
});

test('partial multi-block selections include heading and intervening empty paragraph but exclude an untouched end', () => {
  const original = doc([paragraph('first'), paragraph(), heading('last')]);
  const positions = blocks(original);
  const range = (from, to) => EditorState.create({doc: original, selection: TextSelection.create(original, from, to)});
  const toStart = createParagraphSpacingTransaction(range(3, positions[2].pos + 1), {spaceAfter: 160});
  expect(blocks(toStart.doc).map(({node}) => node.attrs.spaceAfter)).toEqual([160, 160, null]);
  const intoHeading = createParagraphSpacingTransaction(range(3, positions[2].pos + 2), {lineSpacing: 480});
  expect(blocks(intoHeading.doc).map(({node}) => node.attrs.lineSpacing)).toEqual([480, 480, 480]);
  expect(intoHeading.doc.child(2).attrs.level).toBe(2);
  const backwards = createParagraphSpacingTransaction(range(positions[2].pos + 2, 3), {lineSpacing: 480});
  expect(backwards.doc.eq(intoHeading.doc)).toBe(true);
});

test('all-selected paragraphs include nested lists, headings and table cells without altering wrappers', () => {
  const original = doc([heading('head'), schema.node('bulletList', null, [schema.node('listItem', null, [paragraph('list')])]),
    schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableHeader', null, [paragraph('header')]),
      schema.node('tableCell', null, [paragraph('cell'), paragraph('second')])])])]);
  const transaction = createParagraphSpacingTransaction(all(original), {spaceBefore: 20, spaceAfter: 40, lineSpacing: 276});
  expect(blocks(transaction.doc).map(({node}) => ({...node.attrs, level: undefined}))).toEqual(Array(5).fill({spaceBefore: 20, spaceAfter: 40, lineSpacing: 276, level: undefined}));
  expect(transaction.doc.child(1).type.name).toBe('bulletList');
  expect(transaction.doc.child(2).child(0).child(0).attrs).toEqual(original.child(2).child(0).child(0).attrs);
  expect(transaction.doc.textContent).toBe(original.textContent);
});

test('cell selections change only selected cells, including every paragraph within a cell', () => {
  const table = schema.node('table', null, [schema.node('tableRow', null, [
    schema.node('tableCell', null, [paragraph('one'), paragraph('two')]),
    schema.node('tableCell', null, [paragraph('three')]),
  ])]);
  const original = doc([paragraph('outside'), table]);
  const cells = [];
  original.descendants((node, pos) => { if (node.type.name === 'tableCell') cells.push(pos); });
  const state = EditorState.create({doc: original, selection: CellSelection.create(original, cells[0])});
  const transaction = createParagraphSpacingTransaction(state, {lineSpacing: 600});
  expect(blocks(transaction.doc).map(({node}) => node.attrs.lineSpacing)).toEqual([null, 600, 600, null]);
  expect(transaction.selection).toBeInstanceOf(CellSelection);
  expect(createParagraphSpacingTransaction(EditorState.create({doc: original, selection: NodeSelection.create(original, original.child(0).nodeSize)}), {spaceAfter: 10}).doc.child(0).attrs.spaceAfter).toBeNull();
});

test('mixed states are per-field, no-op patches stay clean, and resets restore omitted defaults', () => {
  const original = doc([paragraph('one', {spaceBefore: 0, spaceAfter: 120, lineSpacing: 241}), heading('two', {spaceAfter: 120, lineSpacing: 241})]);
  const state = all(original);
  expect(selectedParagraphSpacing(state)).toEqual({spaceBefore: {value: null, mixed: true}, spaceAfter: {value: 120, mixed: false}, lineSpacing: {value: 241, mixed: false}});
  expect(createParagraphSpacingTransaction(state, {})).toBeNull();
  expect(createParagraphSpacingTransaction(state, {spaceAfter: 120, lineSpacing: 241})).toBeNull();
  const reset = createParagraphSpacingTransaction(state, {spaceBefore: null, spaceAfter: null, lineSpacing: null});
  const saved = omitDefaultParagraphSpacing(reset.doc.toJSON());
  expect(saved.content[0]).not.toHaveProperty('attrs');
  expect(saved.content[1].attrs).toEqual({level: 2});
  expect(selectedParagraphSpacing(state.apply(reset)).spaceBefore).toEqual({value: null, mixed: false});
});

test('one grouped undo/redo covers all selected paragraphs while later typing stays separate', () => {
  const original = doc([paragraph('one'), heading('two')]);
  let state = all(original);
  state = state.apply(closeHistory(createParagraphSpacingTransaction(state, {spaceBefore: 40, spaceAfter: 100, lineSpacing: 360})));
  state = state.apply(closeHistory(state.tr));
  const spaced = state.doc;
  expect(undo(state, transaction => { state = state.apply(transaction); })).toBe(true);
  expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, transaction => { state = state.apply(transaction); })).toBe(true);
  expect(state.doc.eq(spaced)).toBe(true);
  state = state.apply(closeHistory(state.tr.setSelection(TextSelection.create(state.doc, 2))));
  state = state.apply(state.tr.insertText('x'));
  expect(undo(state, transaction => { state = state.apply(transaction); })).toBe(true);
  expect(state.doc.eq(spaced)).toBe(true);
});

test('invalid patches and oversized prospective documents fail before mutating state or undo history', () => {
  const state = all(doc([paragraph('safe')]));
  const original = state.doc;
  for (const patch of [{spaceBefore: -1}, {spaceAfter: 31681}, {spaceBefore: .5}, {lineSpacing: 119}, {lineSpacing: 2401}, {spaceBefore: 40, lineSpacing: 2401}, {lineSpacing: NaN}, {spaceBefore: undefined}, {bogus: 1}]) {
    expect(() => createParagraphSpacingTransaction(state, patch)).toThrow('段前');
    expect(state.doc).toBe(original);
    expect(undo(state)).toBe(false);
  }
  const manyNodes = all(doc(Array.from({length: 5000}, () => paragraph('a'))));
  expect(() => createParagraphSpacingTransaction(manyNodes, {spaceBefore: 1})).toThrow('文档结构');
  const manyBytes = all(doc([paragraph('中'.repeat(700000))]));
  expect(() => createParagraphSpacingTransaction(manyBytes, {spaceAfter: 1})).toThrow('2 MiB');
  let nested = paragraph('a');
  for (let depth = 0; depth < 33; depth++) nested = schema.node('blockquote', null, [nested]);
  expect(() => createParagraphSpacingTransaction(all(doc([nested])), {lineSpacing: 360})).toThrow('文档结构');
});

test('prospective whole-draft byte limit is checked even when most bytes are in an unselected image', () => {
  const imageSchema = getSchema([StarterKit, Image, ParagraphSpacing]);
  const content = imageSchema.node('paragraph', null, [imageSchema.text('small')]);
  const make = payload => imageSchema.node('doc', null, [content, imageSchema.node('image', {src: payload})]);
  const envelopeBytes = Buffer.byteLength(JSON.stringify({version: 1, document: omitDefaultParagraphSpacing(make('').toJSON())}));
  const original = make('A'.repeat(32 * 1024 * 1024 - envelopeBytes - 1));
  const state = EditorState.create({doc: original, selection: TextSelection.create(original, 1), plugins: [history()]});
  expect(() => createParagraphSpacingTransaction(state, {spaceBefore: 20})).toThrow('32 MiB');
  expect(state.doc).toBe(original);
  expect(undo(state)).toBe(false);
});

test('incoming explicit invalid or misplaced attrs fail and snapshots preserve legacy bytes and profile', () => {
  const legacy = {type: 'doc', content: [{type: 'paragraph', attrs: {textAlign: null}, content: [{type: 'text', text: '旧稿'}]},
    {type: 'heading', attrs: {level: 2, textAlign: 'center'}, content: [{type: 'text', text: '标题'}]}]};
  const withNulls = structuredClone(legacy);
  for (const node of withNulls.content) Object.assign(node.attrs, {spaceBefore: null, spaceAfter: null, lineSpacing: null});
  expect(JSON.stringify(omitDefaultParagraphSpacing(withNulls))).toBe(JSON.stringify(legacy));
  const modern = {...withNulls, attrs: {styleProfile: 'standard-v2'}};
  expect(omitDefaultParagraphSpacing(modern).attrs).toEqual({styleProfile: 'standard-v2'});
  expect(withNulls.content[0].attrs).toHaveProperty('spaceBefore', null);
  for (const invalid of [null, -1, 31681, 1.5, '120', true]) {
    expect(() => validateParagraphSpacingDocument({type: 'doc', content: [{type: 'paragraph', attrs: {spaceBefore: invalid}}]})).toThrow('spaceBefore');
  }
  expect(() => validateParagraphSpacingDocument({type: 'doc', attrs: {lineSpacing: 360}, content: []})).toThrow('lineSpacing');
  expect(() => validateParagraphSpacingDocument(legacy)).not.toThrow();
  expect(() => validateParagraphSpacingDocument({type: 'doc', content: [{type: 'heading', attrs: {level: 1, spaceBefore: 0, spaceAfter: 31680, lineSpacing: 2400}}]})).not.toThrow();
});
