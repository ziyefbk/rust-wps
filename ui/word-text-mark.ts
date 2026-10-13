import type {Mark as DocumentMark, Node} from '@tiptap/pm/model';
import {Plugin, type EditorState, type Transaction} from '@tiptap/pm/state';

export function textOnlyMarkPlugin(name: string): Plugin {
  return new Plugin({
    appendTransaction: (transactions, _oldState, state) => {
      if (!transactions.some(transaction => transaction.docChanged)) return null;
      const type = state.schema.marks[name], transaction = state.tr;
      // Pasted styles can mark <br>; the persisted document permits text marks only.
      state.doc.descendants((node, pos) => {
        if (!node.isText && type.isInSet(node.marks)) transaction.removeNodeMark(pos, type);
      });
      return transaction.docChanged ? transaction.setStoredMarks(state.storedMarks) : null;
    },
  });
}

export function selectedTextMarkAttribute<T>(
  state: EditorState, name: string, attribute: string, valid: (value: unknown) => value is T,
): {value: T | null; mixed: boolean} {
  const read = (marks: readonly DocumentMark[]): T | null => {
    const value: unknown = marks.find(mark => mark.type.name === name)?.attrs[attribute];
    return valid(value) ? value : null;
  };
  if (state.selection.empty) return {value: read(state.storedMarks ?? state.selection.$from.marks()), mixed: false};
  let first: T | null | undefined;
  let mixed = false;
  for (const {$from, $to} of state.selection.ranges) {
    state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
      if (mixed) return false;
      if (!node.isText || pos >= $to.pos || pos + node.nodeSize <= $from.pos) return;
      const value = read(node.marks);
      if (first === undefined) first = value;
      else if (value !== first) mixed = true;
    });
  }
  return {value: mixed ? null : first ?? null, mixed};
}

const encoder = new TextEncoder();
function checkTextMarkBudget(doc: Node, label: string) {
  let nodes = 0, bytes = 0;
  const stack = [{node: doc, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++nodes > 10000 || depth > 32) throw new Error(`${label}修改后文档结构超出保存上限，未修改正文。`);
    if (node.isText) {
      bytes += encoder.encode(node.text!).length;
      if (bytes > 2 * 1024 * 1024) throw new Error(`正文超过 2 MiB 保存上限，未应用${label}。`);
    }
    for (let index = node.childCount - 1; index >= 0; index--) stack.push({node: node.child(index), depth: depth + 1});
  }
  if (encoder.encode(JSON.stringify({version: 1, document: doc.toJSON()})).length > 32 * 1024 * 1024) {
    throw new Error(`${label}修改后草稿超过 32 MiB 保存上限，未修改正文。`);
  }
}

export function createTextMarkTransaction(
  state: EditorState, name: string, attributes: Record<string, unknown> | null, label: string,
): Transaction | null {
  const type = state.schema.marks[name], transaction = state.tr;
  const mark = attributes === null ? null : type.create(attributes);
  if (state.selection.empty) {
    if (!state.selection.$from.parent.inlineContent || !state.selection.$from.parent.type.allowsMarkType(type)) return null;
    const current = state.storedMarks ?? state.selection.$from.marks();
    const next = mark ? mark.addToSet(current) : type.removeFromSet(current);
    if (current.length === next.length && current.every((item, index) => item.eq(next[index]))) return null;
    return transaction.setStoredMarks(next);
  }
  for (const {$from, $to} of state.selection.ranges) {
    state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
      if (!node.isText) return;
      const from = Math.max(pos, $from.pos), to = Math.min(pos + node.nodeSize, $to.pos);
      if (mark) transaction.addMark(from, to, mark);
      else transaction.removeMark(from, to, type);
    });
  }
  if (!transaction.docChanged) return null;
  checkTextMarkBudget(transaction.doc, label);
  return transaction;
}
