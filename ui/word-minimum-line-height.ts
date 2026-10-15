import {Extension} from '@tiptap/core';
import {omitDefaultParagraphPagination, validateParagraphPaginationDocument} from './word-paragraph-pagination-model';
import {paragraphIndentFields, validParagraphIndent} from './word-paragraph-indent-model';
import {countRowPaginationHTMLAttributes} from './word-table-row-pagination-model';
import {Slice, type Node} from '@tiptap/pm/model';
import {EditorState, Plugin, TextSelection, type Transaction} from '@tiptap/pm/state';
import {closeHistory, isHistoryTransaction} from '@tiptap/pm/history';
import {canSplit, ReplaceAroundStep, ReplaceStep} from '@tiptap/pm/transform';
import {omitDefaultParagraphSpacing, validateParagraphSpacingDocument} from './word-paragraph-spacing';
import {effectiveLineHeight, isMinimumLineHeight, minimumLineHeightApprovedPrefix, minimumLineHeightData,
  minimumLineHeightError, minimumLineHeightField} from './word-minimum-line-height-model';
export * from './word-minimum-line-height-model';
const types = new Set(['paragraph', 'heading']);
const explicit = (node: Node) => node.attrs[minimumLineHeightField] != null;
const encoder = new TextEncoder();
export function validateMinimumLineHeightState(document: Node): void {
  let count = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('最小行高文档结构超出保存上限。');
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultParagraphSpacing(document.toJSON());
  validateParagraphSpacingDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 2, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('最小行高草稿超过 32 MiB 保存上限。');
}
function sameMinimum(before: Node, after: Node | null | undefined): boolean {
  return !!after && types.has(after.type.name) && after.attrs[minimumLineHeightField] === before.attrs[minimumLineHeightField];
}
function validJoin(step: ReplaceStep, before: Node, after: Node): boolean {
  if (step.slice.size || step.to - step.from !== 2) return false;
  const left = before.resolve(step.from), right = before.resolve(step.to);
  if (left.depth !== 1 || right.depth !== 1 || left.parentOffset !== left.parent.content.size || right.parentOffset !== 0
    || left.index(0) + 1 !== right.index(0) || !types.has(left.parent.type.name) || !types.has(right.parent.type.name)) return false;
  const first = effectiveLineHeight(left.parent, before), second = effectiveLineHeight(right.parent, before);
  const joined = after.nodeAt(left.before());
  return first.mode === second.mode && first.value === second.value && !!joined && types.has(joined.type.name)
    && effectiveLineHeight(joined, after).mode === first.mode && effectiveLineHeight(joined, after).value === first.value;
}
function validSplit(step: ReplaceStep, before: Node, after: Node, node: Node, pos: number): boolean {
  if (step.from !== step.to || step.from <= pos || step.from >= pos + node.nodeSize
    || step.slice.openStart !== 1 || step.slice.openEnd !== 1 || step.slice.content.childCount !== 2) return false;
  const first = after.nodeAt(pos);
  if (!sameMinimum(node, first)) return false;
  const nextPos = pos + first!.nodeSize, second = after.nodeAt(nextPos);
  return after.resolve(nextPos).depth === 0 && sameMinimum(node, second)
    && first!.content.append(second!.content).eq(node.content);
}
function validateContinuity(transaction: Transaction): void {
  if (isHistoryTransaction(transaction)) return;
  const start = minimumLineHeightApprovedPrefix(transaction);
  for (let index = start; index < transaction.steps.length; index++) {
    const step = transaction.steps[index], before = transaction.docs[index], after = transaction.docs[index + 1] ?? transaction.doc;
    before.forEach((node, pos) => {
      if (!explicit(node)) return;
      const end = pos + node.nodeSize;
      if (step instanceof ReplaceAroundStep && step.from === pos && step.to === end && step.gapFrom === pos + 1 && step.gapTo === end - 1 && step.insert === 1) {
        if (!sameMinimum(node, after.nodeAt(pos))) throw new Error(minimumLineHeightError);
        return;
      }
      let mapped = false;
      step.getMap().forEach((from, to) => {
        mapped = true;
        if (from >= end || to <= pos) return;
        if (step instanceof ReplaceStep) {
          if (from > pos && to < end && step.slice.openStart === 0 && step.slice.openEnd === 0
            && Array.from({length: step.slice.content.childCount}, (_, i) => step.slice.content.child(i)).every(child => child.isInline)) {
            if (!sameMinimum(node, after.nodeAt(pos))) throw new Error(minimumLineHeightError);
            return;
          }
          if (validSplit(step, before, after, node, pos) || validJoin(step, before, after)) return;
          // A whole shell can be deleted, never silently converted to a wrapper
          // lacking its minimum. Whole-document clear may insert the empty shell.
          if (from <= pos && to >= end && !step.slice.openStart && !step.slice.openEnd && (!step.slice.size
            || from === 0 && to === before.content.size && after.childCount === 1 && after.firstChild!.type.name === 'paragraph' && !after.firstChild!.content.size && !explicit(after.firstChild!))) return;
        }
        throw new Error(minimumLineHeightError);
      });
      if (!mapped && !sameMinimum(node, after.nodeAt(pos))) throw new Error(minimumLineHeightError);
    });
  }
}
export function createMinimumLineHeightSplitTransaction(state: EditorState): Transaction | null {
  const {$from, $to} = state.selection;
  if ($from.depth !== 1 || !types.has($from.parent.type.name) || !explicit($from.parent)) return null;
  // The existing higher-level pagination policy remains authoritative when its
  // flags are present; its bounded split already copies this schema attribute.
  if (['keepNext', 'keepLines', 'pageBreakBefore'].some(field => $from.parent.attrs[field] != null)) return null;
  if (!(state.selection instanceof TextSelection) || !$from.sameParent($to)) throw new Error(minimumLineHeightError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  transaction.delete($from.pos, $to.pos);
  const type = $to.parentOffset === $to.parent.content.size && $from.parent.type.name === 'heading' ? state.schema.nodes.paragraph : $from.parent.type;
  const attrs: Record<string, unknown> = {...$from.parent.attrs, emptyParagraphFont: null};
  if (type.name === 'paragraph') delete attrs.level;
  const trailing = [{type, attrs}];
  if (!canSplit(transaction.doc, $from.pos, 1, trailing)) throw new Error(minimumLineHeightError);
  transaction.split($from.pos, 1, trailing).setSelection(TextSelection.create(transaction.doc, $from.pos + 2));
  transaction.setStoredMarks(state.storedMarks ?? $from.marks());
  validateMinimumLineHeightState(transaction.doc);
  return closeHistory(transaction).scrollIntoView();
}
export function preflightMinimumLineHeightHTML(html: string): string {
  if (!/data-word-minimum-line-height-twips/i.test(html)) return html;
  const rawCount = countRowPaginationHTMLAttributes(html, minimumLineHeightData);
  if (html.length > 32 * 1024 * 1024) return rawCount ? `<p ${minimumLineHeightData}="invalid"></p>` : html;
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  try {
    const elements = parsed.querySelectorAll<HTMLElement>(`[${minimumLineHeightData}]`);
    if (rawCount !== elements.length) throw new Error(minimumLineHeightError);
    elements.forEach(element => {
      const raw = element.getAttribute(minimumLineHeightData)!;
      if (!['P', 'H1', 'H2', 'H3'].includes(element.tagName) || !/^[1-9][0-9]{0,4}$/.test(raw) || !isMinimumLineHeight(Number(raw))
        || element.hasAttribute('data-line-spacing') || element.style.lineHeight.trim()
        || element.closest('li,td,th,table,ol,ul,blockquote,pre') || element.querySelector('img,table,ol,ul,div,[data-page-break]')) throw new Error(minimumLineHeightError);
    });
    return html;
  } catch {return `<p ${minimumLineHeightData}="invalid"></p>`;}
}
export function minimumLineHeightGuard(onError: (message: string) => void = () => {}, locked: () => boolean = () => false): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateMinimumLineHeightState(transaction.doc); validateContinuity(transaction); return true;}
    catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(view, _event, slice) {
    try {
      const snapshot = omitDefaultParagraphSpacing({type: 'doc', content: slice.content.toJSON()});
      validateParagraphSpacingDocument(snapshot);
      const hasMinimum = snapshot.content?.some(node => node.attrs?.minimumLineHeightTwips != null);
      if (!hasMinimum) return false;
      if (slice.openStart === 1 && slice.openEnd === 1 && slice.content.childCount === 1) {
        const source = slice.content.firstChild!, {$from, $to} = view.state.selection;
        // A handled inline paste must not discard another guard's malformed
        // source evidence together with the intentionally uncopied shell.
        validateParagraphPaginationDocument(omitDefaultParagraphPagination(snapshot));
        if (source.attrs.emptyParagraphFont != null || source.attrs.listGeometry != null
          || paragraphIndentFields.some(field => source.attrs[field] != null && !validParagraphIndent(field, source.attrs[field]))
          || source.attrs.textAlign != null && !['left', 'center', 'right', 'justify'].includes(source.attrs.textAlign)) throw new Error(minimumLineHeightError);
        if (!(view.state.selection instanceof TextSelection) || !$from.sameParent($to) || !types.has($from.parent.type.name)) throw new Error(minimumLineHeightError);
        if (view.editable !== false && !locked() && !view.composing) {
          const transaction = view.state.tr.replaceSelection(new Slice(source.content, 0, 0)).setMeta('paste', true).setMeta('uiEvent', 'paste');
          validateMinimumLineHeightState(transaction.doc);
          view.dispatch(closeHistory(transaction)); view.dispatch(closeHistory(view.state.tr));
        }
        return true;
      }
      if (slice.openStart || slice.openEnd) throw new Error(minimumLineHeightError);
      return false;
    } catch (error) {onError(String(error)); return true;}
  }}});
}
export const MinimumLineHeight = Extension.create<{onError: (message: string) => void; locked: () => boolean}>({
  name: 'minimumLineHeight', priority: 1160,
  addOptions: () => ({onError: () => {}, locked: () => false}),
  addKeyboardShortcuts() {return {Enter: () => {
    try {
      const transaction = createMinimumLineHeightSplitTransaction(this.editor.state);
      if (!transaction) return false;
      if (this.editor.isEditable && !this.options.locked() && !this.editor.view.composing) {
        this.editor.view.dispatch(transaction); this.editor.view.dispatch(closeHistory(this.editor.state.tr));
      }
      return true;
    } catch (error) {this.options.onError(String(error)); return true;}
  }};},
  transformPastedHTML: preflightMinimumLineHeightHTML,
  addProseMirrorPlugins() {return [minimumLineHeightGuard(this.options.onError, () => this.options.locked() || !this.editor.isEditable)];},
});
