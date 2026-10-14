import {Extension} from '@tiptap/core';
import {Slice, type Node} from '@tiptap/pm/model';
import {EditorState, Plugin, Selection, TextSelection, type Transaction} from '@tiptap/pm/state';
import {closeHistory, isHistoryTransaction} from '@tiptap/pm/history';
import {canJoin, canSplit, ReplaceAroundStep, ReplaceStep, type Step} from '@tiptap/pm/transform';
import {paragraphIndentFields, validParagraphIndent} from './word-paragraph-indent-model';
import {omitDefaultParagraphSpacing, validateParagraphSpacingDocument} from './word-paragraph-spacing';
import {omitDefaultParagraphPagination, paragraphPaginationData, paragraphPaginationError, paragraphPaginationFields,
  paragraphPaginationTypes, validateParagraphPaginationDocument, type ParagraphPaginationPatch} from './word-paragraph-pagination-model';
export * from './word-paragraph-pagination-model';
const approved = new WeakMap<Transaction, {before: Node; after: Node; steps: Step[]}>();
const encoder = new TextEncoder();
const explicit = (node: Node) => paragraphPaginationFields.some(field => node.attrs[field] != null);
function selectedParagraphs(state: EditorState) {
  const nodes = new Map<number, Node>(); let supported = true;
  const record = (node: Node, pos: number) => {
    if (!paragraphPaginationTypes.has(node.type.name)) {supported = false; return;}
    nodes.set(pos, node);
    if (state.doc.resolve(pos).depth !== 0 || Array.from({length: node.childCount}, (_, index) => node.child(index)).some(child => !child.isText && child.type.name !== 'hardBreak')) supported = false;
  };
  if (state.selection.empty) {
    const {$from} = state.selection;
    if ($from.depth) record($from.parent, $from.before());
  } else for (const {$from, $to} of state.selection.ranges) state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
    if (node.isTextblock || node.isAtom) {
      if (pos + (node.isTextblock ? 1 : 0) < $to.pos && pos + node.nodeSize > $from.pos) record(node, pos);
      return false;
    }
    // A selected unsupported wrapper is unsupported even when empty.
    if (!paragraphPaginationTypes.has(node.type.name)) supported = false;
  });
  return {nodes, supported: supported && nodes.size > 0};
}
export function selectedParagraphPagination(state: EditorState) {
  const {nodes, supported} = selectedParagraphs(state);
  const fields = Object.fromEntries(paragraphPaginationFields.map(field => {
    const values = [...nodes.values()].map(node => typeof node.attrs[field] === 'boolean' ? node.attrs[field] as boolean : null);
    const first = values[0] ?? null, mixed = values.some(value => value !== first);
    return [field, {value: mixed ? null : first, mixed, explicit: values.some(value => value !== null)}];
  })) as Record<typeof paragraphPaginationFields[number], {value: boolean | null; mixed: boolean; explicit: boolean}>;
  return {count: nodes.size, supported, fields};
}
export function validateParagraphPaginationState(document: Node): void {
  let count = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('段落分页文档结构超出保存上限。');
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未应用段落分页。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultParagraphPagination(document.toJSON());
  validateParagraphPaginationDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 2, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('段落分页草稿超过 32 MiB 保存上限。');
}
export interface ParagraphPaginationToken {doc: Node; selection: Selection}
export const captureParagraphPagination = (state: EditorState): ParagraphPaginationToken => ({doc: state.doc, selection: state.selection});
export const matchesParagraphPagination = (state: EditorState, token: ParagraphPaginationToken) => state.doc === token.doc && state.selection.eq(token.selection);
export function createParagraphPaginationTransaction(state: EditorState, patch: ParagraphPaginationPatch, token?: ParagraphPaginationToken): Transaction | null {
  const fields = Object.keys(patch) as typeof paragraphPaginationFields[number][];
  if (fields.some(field => !paragraphPaginationFields.includes(field) || patch[field] !== null && typeof patch[field] !== 'boolean')) throw new Error(paragraphPaginationError);
  if (token && !matchesParagraphPagination(state, token)) return null;
  if (!fields.length) return null;
  const {nodes, supported} = selectedParagraphs(state);
  if (!supported) throw new Error(paragraphPaginationError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const [pos, node] of nodes) if (fields.some(field => (node.attrs[field] ?? null) !== patch[field])) transaction.setNodeMarkup(pos, undefined, {...node.attrs, ...patch}, node.marks);
  if (!transaction.docChanged) return null;
  validateParagraphPaginationState(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON())).setStoredMarks(state.storedMarks);
  approved.set(transaction, {before: state.doc, after: transaction.doc, steps: [...transaction.steps]});
  return transaction;
}
// Explicit bounded Enter: retain leading p/h and its break-before. The trailing
// node has the same type (heading at its end becomes paragraph), copies keep
// flags including false, and never inherits pageBreakBefore or empty-line font.
export function createParagraphPaginationSplitTransaction(state: EditorState): Transaction | null {
  const {$from, $to} = state.selection;
  if (!($from.depth === 1 && paragraphPaginationTypes.has($from.parent.type.name) && explicit($from.parent))) return null;
  if (!(state.selection instanceof TextSelection) || !$from.sameParent($to)) throw new Error(paragraphPaginationError);
  validateParagraphPaginationState(state.doc);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  transaction.delete($from.pos, $to.pos);
  const type = $to.parentOffset === $to.parent.content.size && $from.parent.type.name === 'heading' ? state.schema.nodes.paragraph : $from.parent.type;
  const attrs: Record<string, unknown> = {...$from.parent.attrs, pageBreakBefore: null, emptyParagraphFont: null};
  if (type.name === 'paragraph') delete attrs.level;
  const types = [{type, attrs}];
  if (!canSplit(transaction.doc, $from.pos, 1, types)) throw new Error(paragraphPaginationError);
  transaction.split($from.pos, 1, types).setSelection(TextSelection.create(transaction.doc, $from.pos + 2));
  transaction.setStoredMarks(state.storedMarks ?? $from.marks());
  validateParagraphPaginationState(transaction.doc);
  approved.set(transaction, {before: state.doc, after: transaction.doc, steps: [...transaction.steps]});
  return closeHistory(transaction).scrollIntoView();
}
// Absent keepNext inherits the historical heading default. The other two
// flags are false by default. Explicit false never uses a truthiness fallback.
export function effectiveParagraphPagination(node: Node) {
  return Object.fromEntries(paragraphPaginationFields.map(field => [field,
    typeof node.attrs[field] === 'boolean' ? node.attrs[field] : field === 'keepNext' && node.type.name === 'heading'])) as Record<typeof paragraphPaginationFields[number], boolean>;
}
export function createParagraphPaginationMergeTransaction(state: EditorState, direction: -1 | 1): Transaction | null {
  const cursor = state.selection instanceof TextSelection ? state.selection.$cursor : null;
  if (!cursor || cursor.depth !== 1 || !paragraphPaginationTypes.has(cursor.parent.type.name)
    || cursor.parentOffset !== (direction < 0 ? 0 : cursor.parent.content.size)) return null;
  // Empty heading Backspace belongs to the existing heading normalization /
  // input-rule undo path, before any paragraph joining.
  if (direction < 0 && cursor.parent.type.name === 'heading' && cursor.parent.content.size === 0) return null;
  const index = cursor.index(0), leadingIndex = direction < 0 ? index - 1 : index;
  if (leadingIndex < 0 || leadingIndex + 1 >= state.doc.childCount) return null;
  const leading = state.doc.child(leadingIndex), trailing = state.doc.child(leadingIndex + 1);
  if (!explicit(leading) && !explicit(trailing)) return null;
  if (!paragraphPaginationTypes.has(leading.type.name) || !paragraphPaginationTypes.has(trailing.type.name)) return null;
  validateParagraphPaginationState(state.doc);
  const first = effectiveParagraphPagination(leading), second = effectiveParagraphPagination(trailing);
  const otherKeys = new Set([...Object.keys(leading.attrs), ...Object.keys(trailing.attrs)]);
  for (const field of paragraphPaginationFields) otherKeys.delete(field);
  if (leading.type !== trailing.type || trailing.attrs.emptyParagraphFont != null || leading.attrs.emptyParagraphFont != null
    || second.pageBreakBefore || paragraphPaginationFields.some(field => first[field] !== second[field])
    || [...otherKeys].some(key => JSON.stringify(leading.attrs[key]) !== JSON.stringify(trailing.attrs[key]))
    || JSON.stringify(leading.marks) !== JSON.stringify(trailing.marks)) throw new Error(paragraphPaginationError);
  const boundary = direction < 0 ? cursor.before(1) : cursor.after(1), leadingPos = boundary - leading.nodeSize;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  if (!canJoin(transaction.doc, boundary)) throw new Error(paragraphPaginationError);
  transaction.join(boundary);
  const attrs = {...leading.attrs};
  for (const field of paragraphPaginationFields) attrs[field] = leading.attrs[field] ?? trailing.attrs[field] ?? null;
  const joined = transaction.doc.nodeAt(leadingPos)!;
  if (paragraphPaginationFields.some(field => joined.attrs[field] !== attrs[field])) transaction.setNodeMarkup(leadingPos, undefined, attrs, joined.marks);
  transaction.setSelection(TextSelection.create(transaction.doc, leadingPos + leading.nodeSize - 1)).setStoredMarks(state.storedMarks);
  validateParagraphPaginationState(transaction.doc);
  approved.set(transaction, {before: state.doc, after: transaction.doc, steps: [...transaction.steps]});
  return closeHistory(transaction).scrollIntoView();
}
// One open p/h shell denotes a copied inline selection, not a whole paragraph.
// Its paragraph attrs do not transfer. Every inline mark/hardBreak and all
// destination paragraph attrs survive; malformed metadata is rejected first.
export function createParagraphPaginationInlinePasteTransaction(state: EditorState, slice: Slice): Transaction | null {
  if (slice.openStart !== 1 || slice.openEnd !== 1 || slice.content.childCount !== 1) return null;
  const source = slice.content.firstChild!;
  if (!paragraphPaginationTypes.has(source.type.name) || !explicit(source)) return null;
  const sourceDocument = omitDefaultParagraphPagination({type: 'doc', content: slice.content.toJSON()});
  validateParagraphPaginationDocument(sourceDocument);
  validateParagraphSpacingDocument(omitDefaultParagraphSpacing(sourceDocument));
  if (source.attrs.emptyParagraphFont != null || source.attrs.listGeometry != null
    || paragraphIndentFields.some(field => source.attrs[field] != null && !validParagraphIndent(field, source.attrs[field]))
    || source.attrs.textAlign != null && !['left', 'center', 'right', 'justify'].includes(source.attrs.textAlign)) throw new Error(paragraphPaginationError);
  const {$from, $to} = state.selection;
  if (!(state.selection instanceof TextSelection) || $from.depth !== 1 || !$from.sameParent($to)
    || !paragraphPaginationTypes.has($from.parent.type.name)) throw new Error(paragraphPaginationError);
  validateParagraphPaginationState(state.doc);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  transaction.replaceSelection(new Slice(source.content, 0, 0)).setMeta('paste', true).setMeta('uiEvent', 'paste');
  validateParagraphPaginationState(transaction.doc);
  return closeHistory(transaction).scrollIntoView();
}
function validateContinuity(transaction: Transaction): void {
  const proof = approved.get(transaction);
  if (isHistoryTransaction(transaction)) return;
  const count = proof?.before === transaction.before && proof.steps.every((step, index) => transaction.steps[index] === step)
    && (transaction.docs[proof.steps.length] ?? transaction.doc) === proof.after ? proof.steps.length : 0;
  for (let index = count; index < transaction.steps.length; index++) {
    const step = transaction.steps[index], before = transaction.docs[index], after = transaction.docs[index + 1] ?? transaction.doc;
    before.forEach((node, pos) => {
      if (!explicit(node)) return;
      const end = pos + node.nodeSize;
      if (step instanceof ReplaceAroundStep && step.from === pos && step.to === end && step.gapFrom === pos + 1 && step.gapTo === end - 1 && step.insert === 1) {
        const next = after.nodeAt(pos);
        if (!next || !paragraphPaginationTypes.has(next.type.name) || !next.content.eq(node.content)
          || paragraphPaginationFields.some(field => next.attrs[field] !== node.attrs[field])) throw new Error(paragraphPaginationError);
        return;
      }
      let mappedRange = false;
      step.getMap().forEach((from, to, newFrom, newTo) => {
        mappedRange = true;
        if (from >= end || to <= pos) return;
        // Inline changes wholly inside a shell retain its identity. Splits are
        // explicitly approved above; a generic structural insertion is refused.
        if (from > pos && to < end && step instanceof ReplaceStep && step.slice.openStart === 0 && step.slice.openEnd === 0
          && Array.from({length: step.slice.content.childCount}, (_, i) => step.slice.content.child(i)).every(child => child.isInline)) return;
        // Complete block removal/replacement is allowed; a join or partial
        // cross-block deletion must never silently discard paragraph ownership.
        if (step instanceof ReplaceStep && from <= pos && to >= end && step.slice.openStart === 0 && step.slice.openEnd === 0) return;
        if (newFrom === newTo && from === to) return;
        throw new Error(paragraphPaginationError);
      });
      // AttrStep and mark operations have empty maps. Mark edits are harmless,
      // but a direct attr step must not erase pagination before a later wrap.
      if (!mappedRange) {
        const next = after.nodeAt(pos);
        if (!next || paragraphPaginationFields.some(field => next.attrs[field] !== node.attrs[field])) throw new Error(paragraphPaginationError);
      }
    });
  }
}
export function paragraphPaginationGuard(onError: (message: string) => void = () => {}, locked: () => boolean = () => false): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateParagraphPaginationState(transaction.doc); validateContinuity(transaction); return true;}
    catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(_view, _event, slice) {
    try {
      const inline = createParagraphPaginationInlinePasteTransaction(_view.state, slice);
      if (inline) {
        if (_view.editable !== false && !locked() && !_view.composing) {_view.dispatch(inline); _view.dispatch(closeHistory(_view.state.tr));}
        return true;
      }
      const snapshot = omitDefaultParagraphPagination({type: 'doc', content: slice.content.toJSON()});
      validateParagraphPaginationDocument(snapshot);
      // Other open slices can own multiple paragraph boundaries. Their flag
      // ownership is ambiguous, so do not allow fitting to discard it.
      if ((slice.openStart || slice.openEnd) && snapshot.content?.some(node => paragraphPaginationFields.some(field => typeof node.attrs?.[field] === 'boolean'))) throw new Error(paragraphPaginationError);
      return false;
    } catch (error) {onError(String(error)); return true;}
  }}});
}
export function preflightParagraphPaginationHTML(html: string): string {
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  try {
    parsed.querySelectorAll<HTMLElement>(paragraphPaginationFields.map(field => `[${paragraphPaginationData[field]}]`).join(',')).forEach(element => {
      if (!['P', 'H1', 'H2', 'H3'].includes(element.tagName) || element.closest('li,td,th,table,ol,ul,blockquote')
        || element.querySelector('img,table,ol,ul,div,[data-page-break]')) throw new Error(paragraphPaginationError);
      for (const field of paragraphPaginationFields) {
        const raw = element.getAttribute(paragraphPaginationData[field]);
        if (raw !== null && raw !== 'true' && raw !== 'false') throw new Error(paragraphPaginationError);
      }
    });
    return html;
  } catch {return `<p ${paragraphPaginationData.keepNext}="invalid"></p>`;}
}
export const ParagraphPagination = Extension.create<{onError: (message: string) => void; locked: () => boolean}>({
  name: 'paragraphPagination', priority: 1150,
  addOptions: () => ({onError: () => {}, locked: () => false}),
  addGlobalAttributes: () => [{types: [...paragraphPaginationTypes], attributes: Object.fromEntries(paragraphPaginationFields.map(field => [field, {
    default: null, keepOnSplit: field !== 'pageBreakBefore',
    parseHTML: (element: HTMLElement) => {const raw = element.getAttribute(paragraphPaginationData[field]); return raw === null ? null : raw === 'true' ? true : raw === 'false' ? false : 'invalid-paragraph-pagination';},
    renderHTML: (attrs: Record<string, unknown>) => typeof attrs[field] === 'boolean' ? {[paragraphPaginationData[field]]: String(attrs[field])} : {},
  }]))}],
  addKeyboardShortcuts() {
    const apply = (candidate: () => Transaction | null) => {
      try {
        const transaction = candidate(); if (!transaction) return false;
        if (this.editor.isEditable && !this.options.locked() && !this.editor.view.composing) {
          this.editor.view.dispatch(transaction); this.editor.view.dispatch(closeHistory(this.editor.state.tr));
        }
        return true;
      } catch (error) {this.options.onError(String(error)); return true;}
    };
    return {Enter: () => apply(() => createParagraphPaginationSplitTransaction(this.editor.state)),
      Backspace: () => {
        // Match core ordering: a just-applied Markdown rule must undo before
        // testing a paragraph join, including second and later headings.
        if (this.editor.isEditable && !this.options.locked() && !this.editor.view.composing && this.editor.commands.undoInputRule()) return true;
        return apply(() => createParagraphPaginationMergeTransaction(this.editor.state, -1));
      },
      Delete: () => apply(() => createParagraphPaginationMergeTransaction(this.editor.state, 1))};
  },
  transformPastedHTML: preflightParagraphPaginationHTML,
  addProseMirrorPlugins() {return [paragraphPaginationGuard(this.options.onError, () => this.options.locked() || !this.editor.isEditable)];},
});
