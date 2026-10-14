import {Extension, Node as TiptapNode} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {AllSelection, EditorState, NodeSelection, Plugin, Selection, TextSelection, type Transaction} from '@tiptap/pm/state';
import {closeHistory, isHistoryTransaction} from '@tiptap/pm/history';
import {canSplit} from '@tiptap/pm/transform';
import {isListGeometry, isListMarkerFont, listGeometryError, listGeometryStyle, listMarkerFontStyle,
  LIST_GEOMETRY_METADATA, LIST_MARKER_FONT_METADATA, listGeometryStateSnapshot, parseListMetadata, validateListGeometryDocument} from './word-list-geometry-model';
export * from './word-list-geometry-model';
const safeSplits = new WeakMap<Transaction, Node>();
const safeDeletions = new WeakMap<Transaction, {before: Node; after: Node}>();
const encoder = new TextEncoder();
export function validateListGeometryState(document: Node): void {
  let count = 0, bytes = 0; const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('编号布局文档结构超出保存上限。');
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = listGeometryStateSnapshot(document.toJSON());
  validateListGeometryDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 2, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('编号布局草稿超过 32 MiB 保存上限。');
}
function preservedLists(document: Node): {node: Node; pos: number}[] {
  const lists: {node: Node; pos: number}[] = [];
  document.forEach((node, pos) => {if (node.type.name === 'orderedList' && node.attrs.markerFont != null) lists.push({node, pos});});
  return lists;
}
// Mapping must retain each old list and item shell in place. This rejects even
// structurally valid joins/reorders that otherwise silently change numbering.
function validateContinuity(transaction: Transaction): void {
  const deletion = safeDeletions.get(transaction);
  if (isHistoryTransaction(transaction) || safeSplits.get(transaction) === transaction.doc
    || deletion?.before === transaction.before && deletion.after === transaction.doc) return;
  const next = preservedLists(transaction.doc), used = new Set<number>();
  for (const {node, pos} of preservedLists(transaction.before)) {
    const mapped = transaction.mapping.mapResult(pos, 1), after = next.find(item => item.pos === mapped.pos);
    if (mapped.deleted || !after || used.has(after.pos) || after.node.childCount !== node.childCount
      || JSON.stringify(after.node.attrs) !== JSON.stringify(node.attrs)) throw new Error(listGeometryError);
    used.add(after.pos);
    let expected = after.pos + 1;
    node.forEach((item, offset, index) => {
      const location = transaction.mapping.mapResult(pos + 1 + offset, 1);
      const end = transaction.mapping.mapResult(pos + offset + item.nodeSize, 1);
      if (location.deleted || location.pos !== expected || end.deleted || end.pos !== expected + after.node.child(index).nodeSize - 1
        || JSON.stringify(item.firstChild?.attrs.listGeometry)
        !== JSON.stringify(after.node.child(index).firstChild?.attrs.listGeometry)) throw new Error(listGeometryError);
      expected += after.node.child(index).nodeSize;
    });
  }
}
// A text selection ending at the next paragraph's start includes a complete
// paragraph boundary. Delete the selected item shells rather than letting PM's
// fitting merge differently formatted paragraphs. Selecting only an item's
// text still means clearing its text, not silently removing its number.
export function createPreservedListDeleteTransaction(state: EditorState): Transaction | null {
  const {selection, doc} = state, {$from, $to} = selection;
  if (selection.empty) return null;
  const lists = preservedLists(doc);
  if (!lists.length) return null;
  let from: number, to: number;
  if (selection instanceof AllSelection) {
    from = 0; to = doc.content.size;
  } else if (selection instanceof NodeSelection) {
    if (selection.node.type.name === 'orderedList' && $from.depth === 0 && isListMarkerFont(selection.node.attrs.markerFont)) {
      from = selection.from; to = selection.to;
    } else if (selection.node.type.name === 'listItem' && $from.depth === 1 && $from.parent.type.name === 'orderedList'
      && isListMarkerFont($from.parent.attrs.markerFont)) {
      from = selection.from; to = selection.to;
      if ($from.parent.childCount === 1) {from = $from.before(1); to = $from.after(1);}
    } else return null;
  } else if (selection instanceof TextSelection && $from.depth === 3 && $from.node(1).type.name === 'orderedList'
    && isListMarkerFont($from.node(1).attrs.markerFont)) {
    if ($from.sameParent($to)) return null;
    if ($from.parentOffset !== 0) throw new Error(listGeometryError);
    from = $from.before(2);
    if ($to.depth === 3 && $to.before(1) === $from.before(1) && $to.parentOffset === 0) {
      to = $to.before(2);
    } else if ($to.depth === 1 && $to.parent.type.name === 'paragraph' && $to.parentOffset === 0
      && $to.before(1) === $from.after(1)) {
      to = $from.end(1);
      if ($from.index(1) === 0) {from = $from.before(1); to = $from.after(1);}
    } else throw new Error(listGeometryError);
  } else return null;
  validateListGeometryState(doc);
  // Never mutate a Tiptap chain's shared transaction during candidate creation.
  const transaction = EditorState.create({doc, selection, storedMarks: state.storedMarks}).tr;
  if (from === 0 && to === doc.content.size) {
    const paragraph = state.schema.nodes.paragraph?.createAndFill();
    if (!paragraph) throw new Error(listGeometryError);
    transaction.replaceWith(from, to, paragraph);
  } else transaction.delete(from, to);
  transaction.setSelection(Selection.near(transaction.doc.resolve(Math.min(from, transaction.doc.content.size)), 1));
  transaction.setStoredMarks(null);
  validateListGeometryState(transaction.doc);
  safeDeletions.set(transaction, {before: doc, after: transaction.doc});
  return closeHistory(transaction).scrollIntoView();
}
export function createPreservedListSplitTransaction(state: EditorState): Transaction | null {
  const {$from, $to} = state.selection;
  if (!($from.depth === 3 && $from.node(1).type.name === 'orderedList' && isListMarkerFont($from.node(1).attrs.markerFont))) return null;
  if (!$from.sameParent($to) || $from.parent.type.name !== 'paragraph' || !isListGeometry($from.parent.attrs.listGeometry)) throw new Error(listGeometryError);
  validateListGeometryState(state.doc);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  transaction.delete($from.pos, $to.pos);
  const types = [{type: $from.node(2).type, attrs: {...$from.node(2).attrs}}, {type: $from.parent.type, attrs: {...$from.parent.attrs}}];
  if (!canSplit(transaction.doc, $from.pos, 2, types)) throw new Error(listGeometryError);
  transaction.split($from.pos, 2, types);
  transaction.setSelection(TextSelection.create(transaction.doc, $from.pos + 4));
  transaction.setStoredMarks(state.storedMarks ?? $from.marks());
  validateListGeometryState(transaction.doc); safeSplits.set(transaction, transaction.doc);
  return transaction.scrollIntoView();
}
export function listGeometryGuard(onError: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateListGeometryState(transaction.doc); validateContinuity(transaction); return true;}
    catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(_view, _event, slice) {
    try {
      // Fitting can discard open wrapper metadata. Clipboard list metadata is
      // accepted only in complete, independently valid list blocks.
      const root = {type: 'doc', attrs: {pageGeometry: _view.state.doc.attrs.pageGeometry}, content: slice.content.toJSON()};
      const snapshot = listGeometryStateSnapshot(root);
      validateListGeometryDocument(snapshot);
      if ((slice.openStart || slice.openEnd) && snapshot.content?.some(node => isListMarkerFont(node.attrs?.markerFont))) throw new Error(listGeometryError);
      return false;
    } catch (error) {onError(String(error)); return true;}
  }}});
}
// Raw metadata on unsupported tags must not disappear during HTML parsing.
export function preflightListGeometryHTML(html: string): string {
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  try {
    parsed.querySelectorAll<HTMLElement>(`[${LIST_MARKER_FONT_METADATA}], [${LIST_GEOMETRY_METADATA}]`).forEach(element => {
      if (element.hasAttribute(LIST_MARKER_FONT_METADATA)) {
        if (element.tagName !== 'OL' || element.closest('li,td,th,ol ol,ul ol')) throw new Error(listGeometryError);
        parseListMetadata(element.getAttribute(LIST_MARKER_FONT_METADATA), 'font');
        const scoped = [element, ...Array.from(element.querySelectorAll<HTMLElement>('*'))];
        for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) scoped.push(ancestor);
        for (const child of scoped) if (child.getAttribute('dir') && child.getAttribute('dir') !== 'ltr'
          || child.style.direction && child.style.direction !== 'ltr' || child.style.writingMode && child.style.writingMode !== 'horizontal-tb'
          || child.style.unicodeBidi && child.style.unicodeBidi !== 'normal'
          || child.style.listStylePosition && child.style.listStylePosition !== 'outside') throw new Error(listGeometryError);
        if (!element.children.length || Array.from(element.childNodes).some(child => child.nodeType === 3 && child.textContent?.trim())) throw new Error(listGeometryError);
        if (element.hasAttribute('type') || element.hasAttribute('reversed') || element.style.listStyleType && element.style.listStyleType !== 'decimal') throw new Error(listGeometryError);
        const start = element.getAttribute('start');
        if (start !== null && (!/^\d{1,7}$/.test(start) || Number(start) < 1 || Number(start) > 1000000)) throw new Error(listGeometryError);
        for (const item of Array.from(element.children)) if (item.tagName !== 'LI' || item.children.length !== 1 || item.firstElementChild?.tagName !== 'P'
          || !item.firstElementChild.hasAttribute(LIST_GEOMETRY_METADATA) || item.hasAttribute('value')
          || Array.from(item.childNodes).some(child => child.nodeType === 3 && child.textContent?.trim())) throw new Error(listGeometryError);
      }
      if (element.hasAttribute(LIST_GEOMETRY_METADATA)) {
        if (element.tagName !== 'P' || element.parentElement?.tagName !== 'LI' || element.parentElement.children.length !== 1
          || element.parentElement.parentElement?.tagName !== 'OL' || !element.parentElement.parentElement.hasAttribute(LIST_MARKER_FONT_METADATA)
          || element.querySelector('img,table,ol,ul,div,[data-page-break]')) throw new Error(listGeometryError);
        if (/[\t\r]/.test(element.textContent ?? '') || Array.from(element.querySelectorAll('*')).some(child =>
          !['SPAN', 'B', 'STRONG', 'I', 'EM', 'S', 'STRIKE', 'U', 'SUB', 'SUP', 'CODE', 'A', 'BR'].includes(child.tagName))) throw new Error(listGeometryError);
        parseListMetadata(element.getAttribute(LIST_GEOMETRY_METADATA), 'geometry');
      }
    });
    return html;
  } catch {return `<p ${LIST_GEOMETRY_METADATA}="invalid"></p>`;}
}
export function withPreservedListRendering(node: TiptapNode): TiptapNode {
  if (!['orderedList', 'paragraph'].includes(node.name)) return node;
  return node.extend({renderHTML(props) {
    const output = this.parent!(props), geometry = props.node.attrs.listGeometry, font = props.node.attrs.markerFont;
    const isFont = node.name === 'orderedList' && isListMarkerFont(font), isGeometry = node.name === 'paragraph' && isListGeometry(geometry);
    if ((!isFont && !isGeometry) || !Array.isArray(output)) return output;
    const attrs = output[1] && typeof output[1] === 'object' ? output[1] : {};
    // Append after Tiptap's non-escape-aware style merger, preserving font names.
    return [output[0], {...attrs, [isFont ? LIST_MARKER_FONT_METADATA : LIST_GEOMETRY_METADATA]: JSON.stringify(isFont ? font : geometry),
      style: `${attrs.style ? `${attrs.style}; ` : ''}${isFont ? listMarkerFontStyle(font, props.node.attrs.start) : listGeometryStyle(geometry)}`}, ...output.slice(2)] as typeof output;
  }});
}
export const PreservedListGeometry = Extension.create<{onError: (message: string) => void; locked: () => boolean}>({
  name: 'preservedListGeometry', priority: 1200,
  addOptions: () => ({onError: () => {}, locked: () => false}),
  addGlobalAttributes: () => [
    {types: ['orderedList'], attributes: {markerFont: {default: null, rendered: false, parseHTML: (element: HTMLElement) => {
      try {return parseListMetadata(element.getAttribute(LIST_MARKER_FONT_METADATA), 'font');} catch {return 'invalid-list-marker-font';}
    }}}},
    {types: ['paragraph'], attributes: {listGeometry: {default: null, rendered: false, keepOnSplit: true, parseHTML: (element: HTMLElement) => {
      try {return parseListMetadata(element.getAttribute(LIST_GEOMETRY_METADATA), 'geometry');} catch {return 'invalid-list-geometry';}
    }}}},
  ],
  addKeyboardShortcuts() {
    const deleteSelection = () => {
      try {
        const transaction = createPreservedListDeleteTransaction(this.editor.state);
        if (!transaction) return false;
        if (this.editor.isEditable && !this.options.locked()) {
          this.editor.view.dispatch(transaction);
          this.editor.view.dispatch(closeHistory(this.editor.state.tr));
        }
        return true;
      } catch (error) {this.options.onError(String(error)); return true;}
    };
    return {Backspace: deleteSelection, Delete: deleteSelection, Enter: () => {
    try {
      const transaction = createPreservedListSplitTransaction(this.editor.state);
      if (!transaction) return false;
      if (this.editor.isEditable && !this.options.locked()) this.editor.view.dispatch(transaction);
      return true;
    } catch (error) {this.options.onError(String(error)); return true;}
  }};},
  transformPastedHTML: preflightListGeometryHTML,
  addProseMirrorPlugins() {return [listGeometryGuard(this.options.onError)];},
});
