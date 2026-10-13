import {Extension, Node as TiptapNode, type Editor} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {EditorState, Plugin, Selection, type Transaction} from '@tiptap/pm/state';
import {isHistoryTransaction} from '@tiptap/pm/history';
import {ReplaceAroundStep, type Step} from '@tiptap/pm/transform';
import {emptyParagraphFontError, emptyParagraphFontStyle, emptyParagraphTypes, EMPTY_PARAGRAPH_FONT_METADATA,
  isEmptyParagraphFont, omitDefaultEmptyParagraphFont, parseEmptyParagraphFontMetadata, validateEmptyParagraphFontDocument} from './word-empty-paragraph-font-model';
export * from './word-empty-paragraph-font-model';
const encoder = new TextEncoder();

export function validateEmptyParagraphFontState(document: Node): void {
  let count = 0, bytes = 0; const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('空段字体文档结构超出保存上限。');
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultEmptyParagraphFont(document.toJSON());
  validateEmptyParagraphFontDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 2, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('空段字体草稿超过 32 MiB 保存上限。');
}
// setNodeMarkup wraps the same content in a new paragraph/heading shell. This
// is the only structural replacement that retains an untouched empty baseline.
function changesOnlyShell(step: Step, node: Node, pos: number, next: Node): boolean {
  if (!(step instanceof ReplaceAroundStep) || step.from !== pos || step.to !== pos + node.nodeSize
    || step.gapFrom !== pos + 1 || step.gapTo !== pos + node.nodeSize - 1 || step.insert !== 1) return false;
  const replacement = next.nodeAt(pos);
  return !!replacement && emptyParagraphTypes.has(replacement.type.name) && replacement.content.eq(node.content);
}

// Called before dispatch, so insertion + metadata removal enter strict guards,
// observers, drafts and history as one transaction. Never create text marks.
export function normalizeEmptyParagraphFontTransaction(transaction: Transaction): Transaction {
  if (!transaction.docChanged) return transaction;
  if (isHistoryTransaction(transaction)) {validateEmptyParagraphFontState(transaction.doc); return transaction;}
  const ranges: {from: number; to: number}[] = [], pasted = new Set<Node>();
  const isPaste = transaction.getMeta('paste') || transaction.getMeta('uiEvent') === 'paste';
  for (let index = 0; index < transaction.steps.length; index++) {
    const step = transaction.steps[index], before = transaction.docs[index];
    const after = transaction.docs[index + 1] ?? transaction.doc, mapping = transaction.mapping.slice(index), map = step.getMap();
    // Malformed new metadata must never be "repaired" by the cleanup below.
    const slice = (step as Step & {slice?: {content: {forEach: (f: (node: Node) => void) => void}}}).slice;
    slice?.content.forEach(node => {
      validateEmptyParagraphFontDocument(omitDefaultEmptyParagraphFont(node.toJSON()));
      if (isPaste) {if (node.attrs.emptyParagraphFont != null) pasted.add(node); node.descendants(child => {if (child.attrs.emptyParagraphFont != null) pasted.add(child);});}
    });
    before.descendants((node, pos) => {
      if (node.attrs.emptyParagraphFont == null) return;
      if (!isEmptyParagraphFont(node.attrs.emptyParagraphFont) || !emptyParagraphTypes.has(node.type.name)) throw new Error(emptyParagraphFontError);
      if (changesOnlyShell(step, node, pos, after)) return;
      let touched = false;
      map.forEach((from, to) => {
        if (from === to ? from > pos && from < pos + node.nodeSize : from < pos + node.nodeSize && to > pos) touched = true;
      });
      if (touched) ranges.push({from: mapping.map(pos, 1), to: mapping.map(pos + node.nodeSize, -1)});
    });
  }
  const remove: number[] = [];
  transaction.doc.descendants((node, pos) => {
    if (node.attrs.emptyParagraphFont == null) return;
    if (!isEmptyParagraphFont(node.attrs.emptyParagraphFont) || !emptyParagraphTypes.has(node.type.name)) throw new Error(emptyParagraphFontError);
    if (ranges.some(range => range.from < pos + node.nodeSize && range.to > pos)
      && !(node.content.size === 0 && pasted.has(node))) remove.push(pos);
  });
  const marks = transaction.storedMarks, selection = transaction.selection;
  for (const pos of remove) {
    const node = transaction.doc.nodeAt(pos)!;
    transaction.setNodeMarkup(pos, undefined, {...node.attrs, emptyParagraphFont: null}, node.marks);
  }
  if (remove.length) {transaction.setSelection(Selection.fromJSON(transaction.doc, selection.toJSON())); transaction.setStoredMarks(marks);}
  validateEmptyParagraphFontState(transaction.doc);
  return transaction;
}

// Upstream input rules prepare a detached state before dispatching their
// trigger. For the first insertion only, commit the normalized text first,
// then let the same registered rules inspect that already-inserted text.
// This prevents a stale `transaction.before` and never inserts twice on reject.
function firstEmptyParagraphInput(editor: Editor): Plugin {
  return new Plugin({props: {handleTextInput(view, from, to, text) {
    if (!text || view.composing || !editor.isEditable) return false;
    const node = view.state.doc.resolve(from).parent;
    if (!emptyParagraphTypes.has(node.type.name) || node.content.size !== 0 || !isEmptyParagraphFont(node.attrs.emptyParagraphFont)) return false;
    const transaction = view.state.tr.insertText(text, from, to);
    view.dispatch(transaction);
    if (!view.state.doc.eq(transaction.doc)) return true;
    const end = view.state.selection.from;
    for (const plugin of view.state.plugins) {
      if ((plugin.spec as {isInputRules?: boolean}).isInputRules
        && plugin.props.handleTextInput?.call(plugin, view, end, end, '', () => view.state.tr)) break;
    }
    return true;
  }}});
}

export function emptyParagraphFontGuard(onError: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateEmptyParagraphFontState(transaction.doc); return true;} catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(_view, _event, slice) {
    try {
      // An open empty slice can disappear during paste fitting. Reject its raw
      // metadata first, before that fitting can discard the invalid evidence.
      slice.content.forEach(node => validateEmptyParagraphFontDocument(omitDefaultEmptyParagraphFont(node.toJSON())));
      return false;
    } catch (error) {onError(String(error)); return true;}
  }}});
}
// Append the literal CSS only after upstream attribute merging. Its CSS parser
// is not escape-aware; a valid quoted family must never become declarations.
export function withEmptyParagraphFontRendering(node: TiptapNode): TiptapNode {
  if (!emptyParagraphTypes.has(node.name)) return node;
  return node.extend({renderHTML(props) {
    const output = this.parent!(props), font = props.node.attrs.emptyParagraphFont;
    if (props.node.content.size || !isEmptyParagraphFont(font) || !Array.isArray(output)) return output;
    const attrs = output[1] && typeof output[1] === 'object' ? output[1] : {};
    return [output[0], {...attrs, [EMPTY_PARAGRAPH_FONT_METADATA]: JSON.stringify(font),
      style: `${attrs.style ? `${attrs.style}; ` : ''}${emptyParagraphFontStyle(font)}`}, ...output.slice(2)] as typeof output;
  }});
}
export function preflightEmptyParagraphFontHTML(html: string): string {
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  try {
    parsed.querySelectorAll(`[${EMPTY_PARAGRAPH_FONT_METADATA}]`).forEach(element => {
      if (!['P', 'H1', 'H2', 'H3'].includes(element.tagName) || element.childNodes.length !== 0) throw new Error(emptyParagraphFontError);
      parseEmptyParagraphFontMetadata(element.getAttribute(EMPTY_PARAGRAPH_FONT_METADATA));
    });
    return html;
  } catch {
    // Preserve the rejection evidence through schema parsing, including
    // metadata on unsupported tags that the schema would otherwise drop.
    return `<p ${EMPTY_PARAGRAPH_FONT_METADATA}="invalid"></p>`;
  }
}
export const EmptyParagraphFont = Extension.create<{onError: (message: string) => void}>({
  name: 'emptyParagraphFont', priority: 1100,
  addOptions: () => ({onError: () => {}}),
  addGlobalAttributes() {return [{types: [...emptyParagraphTypes], attributes: {emptyParagraphFont: {
    default: null, rendered: false, keepOnSplit: false,
    parseHTML: (element: HTMLElement) => {
      try {
        const raw = element.getAttribute(EMPTY_PARAGRAPH_FONT_METADATA);
        if (raw !== null && element.childNodes.length !== 0) throw new Error(emptyParagraphFontError);
        return parseEmptyParagraphFontMetadata(raw);
      } catch {return 'invalid-empty-paragraph-font';}
    },
  }}}];},
  dispatchTransaction({transaction, next}) {
    try {normalizeEmptyParagraphFontTransaction(transaction);} catch (error) {this.options.onError(String(error)); return;}
    next(transaction);
  },
  transformPastedHTML: preflightEmptyParagraphFontHTML,
  addProseMirrorPlugins() {return [firstEmptyParagraphInput(this.editor), emptyParagraphFontGuard(this.options.onError)];},
});
export interface EmptyParagraphFontToken {doc: Node; selection: Selection}
export const captureEmptyParagraphFont = (state: EditorState): EmptyParagraphFontToken => ({doc: state.doc, selection: state.selection});
export const matchesEmptyParagraphFont = (state: EditorState, token: EmptyParagraphFontToken): boolean => state.doc === token.doc && state.selection.eq(token.selection);
export function selectedEmptyParagraphFont(state: EditorState) {
  if (!state.selection.empty) return null;
  const {$from} = state.selection, node = $from.parent;
  return emptyParagraphTypes.has(node.type.name) && node.content.size === 0 && isEmptyParagraphFont(node.attrs.emptyParagraphFont)
    ? {node, pos: $from.before(), font: node.attrs.emptyParagraphFont} : null;
}
export function createEmptyParagraphFontReset(state: EditorState, token?: EmptyParagraphFontToken): Transaction | null {
  if (token && !matchesEmptyParagraphFont(state, token)) return null;
  const selected = selectedEmptyParagraphFont(state); if (!selected) return null;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  transaction.setNodeMarkup(selected.pos, undefined, {...selected.node.attrs, emptyParagraphFont: null}, selected.node.marks);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON())).setStoredMarks(state.storedMarks);
  validateEmptyParagraphFontState(transaction.doc);
  return transaction;
}
