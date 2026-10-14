import {Extension} from '@tiptap/core';
import {tableCellMarginsError, validTableCellMarginsHTML} from './word-table-cell-margins';
import {tableVerticalError, validTableVerticalHTML} from './word-table-vertical-layout';
import {tableCellColorsError, validTableCellColorsHTML} from './word-table-cell-colors';
import {type Node, Slice} from '@tiptap/pm/model';
import {Plugin, PluginKey, TextSelection, type Transaction} from '@tiptap/pm/state';
import {isHistoryTransaction, closeHistory} from '@tiptap/pm/history';
import {handlePaste as pasteTableCells, isInTable, selectedRect} from '@tiptap/pm/tables';
import {ReplaceAroundStep, ReplaceStep, type Step} from '@tiptap/pm/transform';
import {isValidTableRowPaginationHTML, omitDefaultTableRowPagination, preflightTableRowPaginationHTML,
  rowCantSplitAttribute, tableRowPaginationError, tableRowPaginationPasteError, validateTableRowPaginationDocument} from './word-table-row-pagination-model';
export * from './word-table-row-pagination-model';

const tableRowPaginationKey = new PluginKey('tableRowPaginationGuard');
const approved = new WeakMap<Transaction, {before: Node; after: Node; steps: Step[]}>();
const explicit = (node: Node) => node.type.name === 'tableRow' && node.attrs[rowCantSplitAttribute] != null;
const encoder = new TextEncoder();
export function validateTableRowPaginationState(document: Node): void {
  let count = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error(tableRowPaginationError);
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error(tableRowPaginationError);
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultTableRowPagination(document.toJSON());
  validateTableRowPaginationDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) throw new Error(tableRowPaginationError);
}
// Only application-owned candidates call this, after their precise operation
// and complete document have been validated. Transaction metadata is no proof.
export function approveTableRowPaginationTransaction(transaction: Transaction): void {
  validateTableRowPaginationState(transaction.doc);
  closeHistory(transaction);
  approved.set(transaction, {before: transaction.before, after: transaction.doc, steps: [...transaction.steps]});
}
function approvalLength(transaction: Transaction): number {
  const proof = approved.get(transaction);
  return proof?.before === transaction.before && proof.steps.every((step, index) => transaction.steps[index] === step)
    && (transaction.docs[proof.steps.length] ?? transaction.doc) === proof.after ? proof.steps.length : 0;
}
// Replay is intentionally one operation. Validate the shared prefix before
// granting approval to its exact new prefix; can() never calls this function.
export function replayTableRowPaginationTransaction(target: Transaction, candidate: Transaction): void {
  if (target.doc !== candidate.before) throw new Error(tableRowPaginationError);
  validateTableRowPaginationContinuity(target);
  validateTableRowPaginationContinuity(candidate);
  for (const step of candidate.steps) target.step(step);
  approveTableRowPaginationTransaction(target);
}
function rows(document: Node) {
  const result: {node: Node; pos: number; table: Node; tablePos: number}[] = [];
  document.forEach((table, tablePos) => {
    if (table.type.name === 'table') table.forEach((node, offset) => result.push({node, pos: tablePos + 1 + offset, table, tablePos}));
  });
  return result;
}
const closed = (step: Step): step is ReplaceStep => step instanceof ReplaceStep && !step.slice.openStart && !step.slice.openEnd;
function wholeTableReplacement(step: Step, document: Node, tablePos: number, table: Node): boolean {
  return closed(step) && step.from <= tablePos && step.to >= tablePos + table.nodeSize
    && document.resolve(step.from).depth === 0 && document.resolve(step.to).depth === 0;
}
function insertedWholeTable(step: Step, after: Node, tablePos: number, table: Node): boolean {
  if (!closed(step)) return false;
  let found = false;
  step.getMap().forEach((_from, _to, start, end) => {
    if (start <= tablePos && end >= tablePos + table.nodeSize && after.resolve(start).depth === 0 && after.resolve(end).depth === 0) found = true;
  });
  return found;
}
export function validateTableRowPaginationContinuity(transaction: Transaction): void {
  if (isHistoryTransaction(transaction)) return;
  for (let index = approvalLength(transaction); index < transaction.steps.length; index++) {
    const step = transaction.steps[index], before = transaction.docs[index], after = transaction.docs[index + 1] ?? transaction.doc;
    const preserved = new Set<number>(), map = step.getMap();
    for (const {node, pos, table, tablePos} of rows(before)) {
      if (!explicit(node)) continue;
      if (wholeTableReplacement(step, before, tablePos, table)) continue;
      // Complete deletion is a removal of ownership, not a transfer. Generic
      // row replacement/recreation needs an application-owned command proof.
      if (closed(step) && !step.slice.size && step.from === pos && step.to === pos + node.nodeSize) continue;
      const end = pos + node.nodeSize, mapped = map.map(pos, 1), next = after.nodeAt(mapped);
      const rowMarkup = step instanceof ReplaceAroundStep && step.from === pos && step.to === end
        && step.gapFrom === pos + 1 && step.gapTo === end - 1 && step.insert === 1 && next?.content.eq(node.content);
      let supported = true;
      map.forEach((from, to) => {
        if (from >= end || to <= pos || rowMarkup) return;
        // A row shell can survive paragraph splits/joins and cell-content
        // paste. Each edit must stay in one cell, not cross cell boundaries.
        let insideCell = false;
        node.forEach((cell, offset) => {
          const cellPos = pos + 1 + offset, cellEnd = cellPos + cell.nodeSize;
          if (from > cellPos && to < cellEnd) insideCell = true;
          // Header cell toggles use a ReplaceAround shell with exact content.
          if (step instanceof ReplaceAroundStep && step.from === cellPos && step.to === cellEnd
            && step.gapFrom === cellPos + 1 && step.gapTo === cellEnd - 1 && step.insert === 1) {
            const mappedCell = after.nodeAt(map.map(cellPos, -1));
            if (mappedCell && ['tableCell', 'tableHeader'].includes(mappedCell.type.name) && mappedCell.content.eq(cell.content)) insideCell = true;
          }
        });
        if (!insideCell) supported = false;
      });
      if (!supported || !next || next.type.name !== 'tableRow' || map.map(end, -1) !== mapped + next.nodeSize
        || next.attrs[rowCantSplitAttribute] !== node.attrs[rowCantSplitAttribute] || next.childCount !== node.childCount) throw new Error(tableRowPaginationError);
      preserved.add(mapped);
    }
    // An AttrStep on an absent row or a copied row shell cannot invent new
    // ownership. A closed complete top-level table is the bounded copy unit.
    for (const {node, pos, table, tablePos} of rows(after)) {
      if (explicit(node) && !preserved.has(pos) && !insertedWholeTable(step, after, tablePos, table)) throw new Error(tableRowPaginationError);
    }
  }
}
function hasExplicitRows(slice: Slice): boolean {
  let found = false; slice.content.descendants(node => {if (explicit(node)) found = true;}); return found;
}
// A copied text selection inside a cell has four fully open single-child
// context shells (table/row/cell/paragraph). Those shells do not transfer row
// ownership. CellSelection and partial row slices have shallower/different
// boundaries and deliberately do not enter this narrow inline exception.
function copiedRowInlineContent(slice: Slice): Slice | null {
  if (slice.openStart !== 4 || slice.openEnd !== 4 || slice.content.childCount !== 1) return null;
  const table = slice.content.firstChild!;
  if (table.type.name !== 'table' || table.childCount !== 1) return null;
  const row = table.firstChild!;
  if (row.type.name !== 'tableRow' || row.childCount !== 1) return null;
  const cell = row.firstChild!;
  if (!['tableCell', 'tableHeader'].includes(cell.type.name) || cell.childCount !== 1) return null;
  const paragraph = cell.firstChild!;
  if (!['paragraph', 'heading'].includes(paragraph.type.name)
    || Array.from({length: paragraph.childCount}, (_, index) => paragraph.child(index)).some(node => !node.isInline)) return null;
  validateTableRowPaginationDocument(omitDefaultTableRowPagination({type: 'doc', content: slice.content.toJSON()}));
  return new Slice(paragraph.content, 0, 0);
}
export function tableRowPaginationGuard(onError: (message: string) => void = () => {}, locked: () => boolean = () => false): Plugin {
  return new Plugin({key: tableRowPaginationKey,
    // App-owned changes are one undo unit even when subsequent typing is
    // adjacent to a newly created row (the upstream last-cell Tab route).
    // This barrier is attached only after filters accepted an exact proof.
    appendTransaction: (transactions, _before, state) => transactions.some(transaction => approvalLength(transaction) > 0)
      ? closeHistory(state.tr) : null,
    filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {if (locked()) return false; validateTableRowPaginationState(transaction.doc); validateTableRowPaginationContinuity(transaction); return true;}
    catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(view, event, slice) {
    try {
      const html = event.clipboardData?.getData('text/html');
      if (html && !isValidTableRowPaginationHTML(html)) throw new Error(tableRowPaginationError);
      // Our handled paste must not skip the older late table guard's raw
      // wrong-node protections when schema parsing already erased evidence.
      if (html && /data-(?:table-cell-margins-twips|row-min-height-twips|cell-vertical-align|cell-fill-rgb|cell-text-color-mode)/i.test(html)) {
        const parsed = new DOMParser().parseFromString(html, 'text/html');
        if (!validTableCellMarginsHTML(parsed)) throw new Error(tableCellMarginsError);
        if (!validTableVerticalHTML(parsed)) throw new Error(tableVerticalError);
        if (!validTableCellColorsHTML(parsed)) throw new Error(tableCellColorsError);
      }
      const inline = hasExplicitRows(slice) ? copiedRowInlineContent(slice) : null;
      if (inline) slice = inline;
      const pasteInline = () => {
        if (locked() || view.editable === false || view.composing) return true;
        const selection = view.state.selection;
        if (!(selection instanceof TextSelection) || !selection.$from.sameParent(selection.$to) || !selection.$from.parent.isTextblock) throw new Error(tableRowPaginationPasteError);
        const transaction = view.state.tr.replaceSelection(slice).setMeta('paste', true).setMeta('uiEvent', 'paste');
        validateTableRowPaginationState(transaction.doc); validateTableRowPaginationContinuity(transaction);
        view.dispatch(closeHistory(transaction)); if (view.state.doc === transaction.doc) view.dispatch(closeHistory(view.state.tr)); return true;
      };
      if (!hasExplicitRows(slice)) {
        if (!isInTable(view.state)) return inline ? pasteInline() : false;
        const target = selectedRect(view.state), tablePos = target.tableStart - 1;
        if (!inline && !Array.from({length: target.table.childCount}, (_, index) => target.table.child(index)).some(explicit)) return false;
        if (locked() || view.editable === false || view.composing) return true;
        // Upstream table paste replaces cell fragments and may grow rows. We
        // invoke that exact route only for unflagged source content, then prove
        // every destination row and all other top-level nodes before dispatch.
        const proxy = new Proxy(view, {get(object, property) {
          if (property !== 'dispatch') return Reflect.get(object, property);
          return (transaction: Transaction) => {
            const before = transaction.before, next = transaction.doc.nodeAt(tablePos);
            if (!next || next.type.name !== 'table' || next.childCount < target.table.childCount
              || before.childCount !== transaction.doc.childCount) throw new Error(tableRowPaginationError);
            let targetIndex = -1;
            before.forEach((node, pos, index) => {if (pos === tablePos) targetIndex = index; else if (!node.eq(transaction.doc.child(index))) throw new Error(tableRowPaginationError);});
            if (targetIndex < 0) throw new Error(tableRowPaginationError);
            next.forEach((node, _offset, index) => {
              if (index < target.table.childCount ? JSON.stringify(node.attrs) !== JSON.stringify(target.table.child(index).attrs)
                : node.attrs[rowCantSplitAttribute] != null) throw new Error(tableRowPaginationError);
            });
            validateTableRowPaginationState(transaction.doc);
            approveTableRowPaginationTransaction(transaction);
            view.dispatch(closeHistory(transaction)); if (view.state.doc === transaction.doc) view.dispatch(closeHistory(view.state.tr));
          };
        }});
        const handled = pasteTableCells(proxy, event, slice);
        return handled || inline ? handled || pasteInline() : false;
      }
      if (locked() || view.editable === false || view.composing) return true;
      const {selection} = view.state;
      const outside = selection.$from.depth === 0 && selection.$to.depth === 0;
      // A caret in an otherwise empty top-level paragraph can be replaced by
      // a complete table. Other fitting/open/nested contexts are refused.
      const emptyParagraph = selection.empty && selection.$from.depth === 1 && selection.$from.parent.type.name === 'paragraph'
        && !selection.$from.parent.content.size;
      if (slice.openStart || slice.openEnd || !slice.content.childCount
        || Array.from({length: slice.content.childCount}, (_, index) => slice.content.child(index)).some(node => node.type.name !== 'table')
        || !outside && !emptyParagraph) throw new Error(tableRowPaginationPasteError);
      validateTableRowPaginationDocument(omitDefaultTableRowPagination({type: 'doc', content: slice.content.toJSON()}));
      const from = emptyParagraph ? selection.$from.before() : selection.from;
      const to = emptyParagraph ? selection.$to.after() : selection.to;
      const transaction = view.state.tr.replaceWith(from, to, slice.content).setMeta('paste', true).setMeta('uiEvent', 'paste');
      validateTableRowPaginationState(transaction.doc); validateTableRowPaginationContinuity(transaction);
      view.dispatch(closeHistory(transaction)); if (view.state.doc === transaction.doc) view.dispatch(closeHistory(view.state.tr)); return true;
    } catch (error) {onError(String(error)); return true;}
  }}});
}
export const TableRowPagination = Extension.create<{onError: (message: string) => void; locked: () => boolean}>({
  name: 'tableRowPagination', priority: 1200,
  addOptions: () => ({onError: () => {}, locked: () => false}),
  transformPastedHTML: preflightTableRowPaginationHTML,
  addProseMirrorPlugins() {return [tableRowPaginationGuard(this.options.onError, () => this.options.locked() || !this.editor.isEditable)];},
});
