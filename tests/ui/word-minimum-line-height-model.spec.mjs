import {getSchema} from '@tiptap/core';
import {EditorState} from '@tiptap/pm/state';
import {extensions} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
import {test, expect} from '@playwright/test';
import {AllSelection, TextSelection} from '@tiptap/pm/state';
import {Fragment, Slice} from '@tiptap/pm/model';
import {closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {spacing, minimum, pagination, block, controls, schema, p, h, doc, snapshot, stateFor, fullEditor, key} from './word-minimum-line-height-harness.mjs';
const attr = {minimumLineHeightTwips: 276};
const raw = (value = 276, extra = {}) => ({type: 'doc', content: [{type: 'paragraph', attrs: {minimumLineHeightTwips: value, ...extra}, content: [{type: 'text', text: 'x'}]}]});
const apply = (state, patch) => state.applyTransaction(closeHistory(spacing.createParagraphSpacingTransaction(state, patch))).state;

test('minimum raw model accepts exact positive product bounds and rejects invalid/null/coexisting modes', () => {
  for (const value of [1, 276, 31680]) expect(() => spacing.validateParagraphSpacingDocument(raw(value))).not.toThrow();
  for (const value of [0, -1, 31681, 1.5, NaN, Infinity, null, true, '276', {}, []]) expect(() => spacing.validateParagraphSpacingDocument(raw(value))).toThrow();
  for (const value of [null, 120, 276, 2400, '240']) expect(() => spacing.validateParagraphSpacingDocument(raw(276, {lineSpacing: value}))).toThrow();
});
test('minimum raw model is local to direct paragraph or heading1–3 and rejects marks/images/nested contexts', () => {
  for (const level of [1, 2, 3]) {const model = raw(); Object.assign(model.content[0], {type: 'heading'}); model.content[0].attrs.level = level; expect(() => spacing.validateParagraphSpacingDocument(model)).not.toThrow();}
  for (const type of ['doc', 'image', 'pageBreak', 'table', 'text']) {const model = raw(); model.content[0].type = type; expect(() => spacing.validateParagraphSpacingDocument(model)).toThrow();}
  for (const level of [0, 4, null, '1']) {const model = raw(); model.content[0].type = 'heading'; model.content[0].attrs.level = level; expect(() => spacing.validateParagraphSpacingDocument(model)).toThrow();}
  for (const type of ['bulletList', 'tableCell', 'blockquote']) {const model = raw(); model.content = [{type, content: model.content}]; expect(() => spacing.validateParagraphSpacingDocument(model)).toThrow();}
  const image = raw(); image.content[0].content.push({type: 'image'}); expect(() => spacing.validateParagraphSpacingDocument(image)).toThrow();
  const mark = raw(); delete mark.content[0].attrs; mark.content[0].content[0].marks = [{type: 'bold', attrs: attr}]; expect(() => spacing.validateParagraphSpacingDocument(mark)).toThrow();
  const legacy = raw(); legacy.attrs = {splitPageBreakAndParagraphMark: true}; expect(() => spacing.validateParagraphSpacingDocument(legacy)).toThrow();
  legacy.attrs.splitPageBreakAndParagraphMark = false; expect(() => spacing.validateParagraphSpacingDocument(legacy)).not.toThrow();
  const unrelated = raw(); unrelated.content.push({type: 'table'}, {type: 'pageBreak'}); expect(() => spacing.validateParagraphSpacingDocument(unrelated)).not.toThrow();
});
test('minimum schema omission preserves exact old snapshots and emits metadata without fixed CSS', () => {
  const old = doc([p('old', {lineSpacing: 241, spaceBefore: 0}), h('head')]);
  expect(JSON.stringify(snapshot(old))).toBe(JSON.stringify({type: 'doc', content: [{type: 'paragraph', attrs: {textAlign: null, spaceBefore: 0, lineSpacing: 241}, content: [{type: 'text', text: 'old'}]}, {type: 'heading', attrs: {textAlign: null, level: 2}, content: [{type: 'text', text: 'head'}]}]}));
  const element = schema.nodes.paragraph.spec.toDOM(p('large\ntext', {...attr, spaceAfter: 120}));
  expect(element[1]).toEqual({textAlign: null, 'data-space-after': '120', style: 'margin-bottom: 6pt', 'data-word-minimum-line-height-twips': '276'});
  expect(schema.nodes.paragraph.spec.toDOM(p('', attr))[1]).toEqual({textAlign: null, 'data-word-minimum-line-height-twips': '276'});
  expect(schema.nodeFromJSON(snapshot(doc([p('x', attr)]))).firstChild.attrs.minimumLineHeightTwips).toBe(276);
});
test('minimum exact pt input and whole schema metadata reject coercion and never infer CSS minimum', () => {
  for (const [text, value] of [['0.05', 1], ['13.8', 276], ['1584', 31680]]) expect(spacing.parseParagraphSpacePoints(text)).toBe(value);
  for (const text of ['.051', '-1', '1e2', '1584.05']) expect(spacing.parseParagraphSpacePoints(text)).toBeNull();
  const definition = spacing.ParagraphSpacing.config.addGlobalAttributes.call(spacing.ParagraphSpacing)[0].attributes.minimumLineHeightTwips;
  expect(definition.keepOnSplit).toBe(true);
  expect(definition.parseHTML({getAttribute: () => null, style: {lineHeight: '13.8pt'}})).toBeNull();
  for (const value of ['', '0', '0276', '276.0', '31681', 'null']) expect(definition.parseHTML({getAttribute: () => value})).toBe('invalid-minimum-line-height');
});
test('minimum control transactions switch modes and reset line independently of paragraph gaps', () => {
  let state = stateFor(doc([p('x', {spaceBefore: 20, spaceAfter: 40, lineSpacing: 241})]));
  state = apply(state, {minimumLineHeightTwips: 276}); expect(snapshot(state.doc).content[0].attrs).toEqual({textAlign: null, spaceBefore: 20, spaceAfter: 40, minimumLineHeightTwips: 276});
  state = apply(state, {spaceBefore: 60}); expect(state.doc.firstChild.attrs.minimumLineHeightTwips).toBe(276);
  state = apply(state, {lineSpacing: 360}); expect(snapshot(state.doc).content[0].attrs).toEqual({textAlign: null, spaceBefore: 60, spaceAfter: 40, lineSpacing: 360});
  state = apply(state, {minimumLineHeightTwips: 1}); state = apply(state, {lineSpacing: null});
  expect(snapshot(state.doc).content[0].attrs).toEqual({textAlign: null, spaceBefore: 60, spaceAfter: 40});
});
test('minimum selection modes distinguish default multiple minimum mixed and precise endpoints', () => {
  for (const [attrs, mode] of [[{}, 'default'], [{lineSpacing: 276}, 'multiple'], [attr, 'minimum']]) expect(controls.selectedLineHeight(stateFor(doc([p('x', attrs)]))).mode).toBe(mode);
  const original = doc([p('a', attr), p('b', {lineSpacing: 276})]);
  expect(controls.selectedLineHeight(stateFor(original, new AllSelection(original))).mode).toBe('mixed');
  const transaction = spacing.createParagraphSpacingTransaction(stateFor(original, TextSelection.create(original, 1, 4)), {minimumLineHeightTwips: 300});
  expect(transaction.doc.firstChild.attrs.minimumLineHeightTwips).toBe(300); expect(transaction.doc.lastChild.attrs.lineSpacing).toBe(276);
});
test('minimum rejected mixed-context patches do not mutate shared transactions or history', () => {
  const list = schema.node('bulletList', null, schema.node('listItem', null, p('nested'))), original = doc([p('top'), list]);
  const state = stateFor(original, new AllSelection(original)), shared = state.tr, chain = Object.create(state); Object.defineProperty(chain, 'tr', {get: () => shared});
  expect(() => spacing.createParagraphSpacingTransaction(chain, attr)).toThrow(); expect(shared.steps).toEqual([]); expect(undoDepth(state)).toBe(0);
  const withAtom = doc([p('top'), schema.node('pageBreak')]); expect(() => spacing.createParagraphSpacingTransaction(stateFor(withAtom, new AllSelection(withAtom)), attr)).toThrow();
  expect(() => spacing.createParagraphSpacingTransaction(stateFor(doc([p('x')])), {...attr, lineSpacing: 360})).toThrow();
});
test('minimum edits retain one undo/redo group and stored marks without contaminating later typing', () => {
  const original = doc([p('one'), h('two')]); let state = stateFor(original, new AllSelection(original)); state = state.apply(state.tr.setStoredMarks([schema.mark('bold')]));
  const marks = state.storedMarks;
  state = apply(state, attr); state = state.apply(closeHistory(state.tr)); const changed = state.doc;
  expect(state.storedMarks).toEqual(marks); expect(undoDepth(state)).toBe(1);
  let cursor = stateFor(doc([p('cursor')])); cursor = cursor.apply(cursor.tr.setStoredMarks([schema.mark('bold')]));
  const cursorChange = spacing.createParagraphSpacingTransaction(cursor, attr); expect(cursorChange.storedMarks).toEqual([schema.mark('bold')]);
  expect(cursor.apply(cursorChange).storedMarks).toEqual([schema.mark('bold')]);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(original)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(changed)).toBe(true);
});
test('minimum inline typing deletion and mark changes work at both paragraph edges', () => {
  for (const position of [1, 4]) {
    let state = stateFor(doc([p('abc', attr)])); const result = state.applyTransaction(state.tr.insertText('Z', position)); expect(result.transactions).toHaveLength(1); state = result.state;
    state = state.apply(state.tr.addMark(1, 2, schema.mark('bold'))); expect(state.doc.firstChild.attrs.minimumLineHeightTwips).toBe(276);
    expect(state.applyTransaction(state.tr.delete(position, position + 1)).transactions).toHaveLength(1);
  }
});
test('minimum Enter retains value in both top-level shells at start middle end and heading end', () => {
  for (const type of [p, h]) for (const position of [1, 2, 4]) {
    const {editor} = fullEditor(doc([type('abc', attr)])); editor.commands.setTextSelection(position);
    expect(key(editor, 'Enter')).toBe(true);
    const nodes = []; editor.state.doc.forEach(node => {if (node.attrs.minimumLineHeightTwips != null) nodes.push(node);});
    expect(nodes).toHaveLength(2); expect(nodes.map(node => node.textContent).join('')).toBe('abc');
    expect(nodes.every(node => node.attrs.minimumLineHeightTwips === 276)).toBe(true); editor.destroy();
  }
});
test('minimum Enter composes with independent pagination flags without bypassing their rules', () => {
  const {editor} = fullEditor(doc([p('abc', {...attr, keepNext: false, keepLines: true, pageBreakBefore: true})]));
  editor.commands.setTextSelection(2); expect(key(editor, 'Enter')).toBe(true);
  expect(editor.state.doc.child(0).attrs.minimumLineHeightTwips).toBe(276); expect(editor.state.doc.child(1).attrs.minimumLineHeightTwips).toBe(276);
  expect(editor.state.doc.child(0).attrs.pageBreakBefore).toBe(true); expect(editor.state.doc.child(1).attrs.pageBreakBefore).toBeNull(); editor.destroy();
});
test('minimum adjacent join accepts equal line mode with unrelated gaps marks and heading differences', () => {
  const original = doc([p('left', {...attr, spaceAfter: 20}), h('right', {...attr, spaceBefore: 40})]);
  let state = stateFor(original); state = state.apply(state.tr.addMark(7, 8, schema.mark('italic')));
  const result = state.applyTransaction(state.tr.join(original.firstChild.nodeSize));
  expect(result.transactions).toHaveLength(1); expect(result.state.doc.childCount).toBe(1); expect(result.state.doc.textContent).toBe('leftright'); expect(result.state.doc.firstChild.attrs.minimumLineHeightTwips).toBe(276);
});
test('minimum adjacent join rejects different minimum multiple and omitted default modes atomically', () => {
  for (const attrs of [{minimumLineHeightTwips: 300}, {lineSpacing: 276}, {}]) {
    const original = doc([p('a', attr), p('b', attrs)]), errors = [], state = stateFor(original, TextSelection.atStart(original), errors);
    expect(state.applyTransaction(state.tr.join(original.firstChild.nodeSize)).transactions).toEqual([]); expect(state.doc.eq(original)).toBe(true); expect(errors).not.toEqual([]);
  }
  expect(minimum.effectiveLineHeight(p('x'), doc([p()], {styleProfile: 'standard-v2'}))).toEqual({mode: 'multiple', value: 360});
  expect(minimum.effectiveLineHeight(p('x'), doc([p()]))).toEqual({mode: 'multiple', value: 456});
});
test('minimum paragraph heading conversion preserves mode while unsupported list conversion rejects', () => {
  let state = stateFor(doc([p('x', attr)])); state = state.apply(block.createWordBlockTypeTransaction(state, 3)); expect(state.doc.firstChild.attrs.minimumLineHeightTwips).toBe(276);
  state = state.apply(block.createWordBlockTypeTransaction(state, 0)); expect(state.doc.firstChild.type.name).toBe('paragraph');
  const {editor} = fullEditor(state.doc); const before = editor.state.doc; editor.commands.toggleBulletList(); expect(editor.state.doc.eq(before)).toBe(true);
  editor.view.dispatch(spacing.createParagraphSpacingTransaction(editor.state, {lineSpacing: null})); editor.commands.toggleBulletList(); expect(editor.state.doc.firstChild.type.name).toBe('bulletList'); editor.destroy();
});
test('minimum direct metadata erasure and forged transaction approval cannot bypass structural guard', () => {
  const original = doc([p('x', attr)]), state = stateFor(original);
  const forged = state.tr.setNodeMarkup(0, undefined, {...state.doc.firstChild.attrs, minimumLineHeightTwips: null}).setMeta('minimumLineHeightApproved', true);
  expect(state.applyTransaction(forged).transactions).toEqual([]);
  const approved = spacing.createParagraphSpacingTransaction(state, {spaceAfter: 20}); approved.setNodeMarkup(0, undefined, {...approved.doc.firstChild.attrs, minimumLineHeightTwips: null});
  expect(state.applyTransaction(approved).transactions).toEqual([]);
});
test('minimum whole-block deletion and whole-document clear work but partial cross-block deletion rejects', () => {
  const original = doc([p('abc', attr), p('def')]), state = stateFor(original);
  expect(state.applyTransaction(state.tr.delete(0, original.firstChild.nodeSize)).transactions).toHaveLength(1);
  expect(state.applyTransaction(state.tr.delete(2, 7)).transactions).toEqual([]);
  const single = stateFor(doc([p('abc', attr)])); expect(single.applyTransaction(single.tr.replaceWith(0, single.doc.content.size, p())).transactions).toHaveLength(1);
});
test('minimum owned inline paste preserves source marks and destination line mode while closed blocks keep minimum', () => {
  const source = p('paste', attr), errors = [], guard = minimum.minimumLineHeightGuard(message => errors.push(message));
  let state = stateFor(doc([p('target', {lineSpacing: 360})]));
  const view = {get state() {return state;}, editable: true, composing: false, dispatch: tr => {state = state.apply(tr);}};
  expect(guard.props.handlePaste(view, {}, new Slice(Fragment.from(source), 1, 1))).toBe(true);
  expect(state.doc.firstChild.attrs.lineSpacing).toBe(360); expect(state.doc.firstChild.attrs.minimumLineHeightTwips).toBeNull(); expect(state.doc.textContent).toContain('paste');
  expect(guard.props.handlePaste(view, {}, new Slice(Fragment.from(source), 0, 0))).toBe(false); expect(errors).toEqual([]);
});
test('minimum malformed nested and ambiguous open slices are consumed without changing history', () => {
  let state = stateFor(doc([p('target', attr)])); const original = state.doc, errors = [], guard = minimum.minimumLineHeightGuard(message => errors.push(message));
  const view = {get state() {return state;}, editable: true, composing: false, dispatch: tr => {state = state.apply(tr);}};
  for (const slice of [new Slice(Fragment.from(p('bad', {minimumLineHeightTwips: 0})), 1, 1), new Slice(Fragment.fromArray([p('a', attr), p('b', attr)]), 1, 1),
    new Slice(Fragment.from(schema.node('bulletList', null, schema.node('listItem', null, p('nested', attr)))), 0, 0)]) expect(guard.props.handlePaste(view, {}, slice)).toBe(true);
  expect(state.doc.eq(original)).toBe(true); expect(undoDepth(state)).toBe(0); expect(errors).toHaveLength(3);
});

test('minimum handled inline paste does not erase malformed pagination or other owned shell metadata', () => {
  let state = stateFor(doc([p('target')])); const original = state.doc, errors = [], guard = minimum.minimumLineHeightGuard(message => errors.push(message));
  const view = {get state() {return state;}, editable: true, composing: false, dispatch: tr => {state = state.apply(tr);}};
  for (const extra of [{keepNext: 'invalid'}, {emptyParagraphFont: 'invalid'}, {indentLeftTwips: 'invalid'}, {textAlign: 'invalid'}]) {
    expect(guard.props.handlePaste(view, {}, new Slice(Fragment.from(p('bad', {...attr, ...extra})), 1, 1))).toBe(true);
  }
  expect(state.doc.eq(original)).toBe(true); expect(errors).toHaveLength(4); expect(undoDepth(state)).toBe(0);
});

test('minimum schema-null addition does not invalidate old preserved-list editing or loosen raw list validation', async () => {
  const list = await loadTsModule('./ui/word-list-geometry.ts'), page = await loadTsModule('./ui/word-page-geometry.ts');
  const listSchema = getSchema([...extensions, list.PreservedListGeometry]);
  const raw = {type: 'doc', attrs: {styleProfile: 'standard-v2', pageGeometry: page.presetPageGeometry('A4', 'portrait')}, content: [{type: 'orderedList', attrs: {start: 9, markerFont: {fontFamily: 'Calibri', fontSize: 22}}, content: [{type: 'listItem', content: [{type: 'paragraph', attrs: {listGeometry: {leftTwips: 720, rightTwips: 180, hangingTwips: 360}}, content: [{type: 'text', text: 'old list'}]}]}]}]};
  expect(() => list.validateListGeometryDocument(raw)).not.toThrow();
  const node = listSchema.nodeFromJSON(raw), state = EditorState.create({doc: node, plugins: [list.listGeometryGuard(), minimum.minimumLineHeightGuard()]});
  expect(node.firstChild.firstChild.firstChild.attrs.minimumLineHeightTwips).toBeNull();
  expect(state.applyTransaction(state.tr.insertText('Z', 3)).transactions).toHaveLength(1);
  for (const value of [null, 276]) {
    const invalid = structuredClone(raw); invalid.content[0].content[0].content[0].attrs.minimumLineHeightTwips = value;
    expect(() => list.validateListGeometryDocument(invalid)).toThrow();
    expect(() => spacing.validateParagraphSpacingDocument(invalid)).toThrow();
  }
});
