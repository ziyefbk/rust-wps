import type {JSONContent} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import type {Transaction} from '@tiptap/pm/state';
import type {Step} from '@tiptap/pm/transform';

export const minimumLineHeightField = 'minimumLineHeightTwips' as const;
export const minimumLineHeightData = 'data-word-minimum-line-height-twips';
export const minimumLineHeightError = '最小行高仅支持顶层正文和标题 1–3，须为 0.05–1584 pt（0.05 pt 递增），不能与倍数行距并存。请先重置行距再移入列表或表格；不同有效行距的段落不能合并，未应用修改。';
export const minimumLineHeightPreview = '最小行高按原值保存到 DOCX；编辑区和打印采用自然行框，不模拟 Word 最小行高或分页。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export const isMinimumLineHeight = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= 31680;
export function validateMinimumLineHeightDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; depth: number}[] = [{node: document, depth: 0}];
  let count = 0;
  while (stack.length) {
    const {node, parent, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('最小行高文档结构超出保存上限。');
    if (node.marks?.some(mark => mark.attrs && own(mark.attrs, minimumLineHeightField))) throw new Error(minimumLineHeightError);
    if (node.attrs && own(node.attrs, minimumLineHeightField)) {
      if (document.attrs?.splitPageBreakAndParagraphMark === true || !isMinimumLineHeight(node.attrs[minimumLineHeightField]) || own(node.attrs, 'lineSpacing')
        || document.type !== 'doc' || parent !== document || !['paragraph', 'heading'].includes(node.type ?? '')
        || node.type === 'heading' && ![1, 2, 3].includes(node.attrs.level)
        || node.content?.some(child => !['text', 'hardBreak'].includes(child.type ?? ''))) throw new Error(minimumLineHeightError);
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, depth: depth + 1});
  }
}
// Only editor schema defaults are stripped. Raw authored input is validated
// unchanged, so an explicit external null remains an error.
export function omitDefaultMinimumLineHeight(document: JSONContent): JSONContent {
  const result = {...document};
  if (['paragraph', 'heading'].includes(result.type ?? '') && result.attrs?.minimumLineHeightTwips === null) {
    result.attrs = {...result.attrs}; delete result.attrs.minimumLineHeightTwips;
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultMinimumLineHeight);
  return result;
}
export function effectiveLineHeight(node: Node, document: Node): {mode: 'multiple' | 'minimum'; value: number} {
  if (isMinimumLineHeight(node.attrs[minimumLineHeightField])) return {mode: 'minimum', value: node.attrs[minimumLineHeightField]};
  return {mode: 'multiple', value: node.attrs.lineSpacing ?? (document.attrs.styleProfile === 'standard-v2' || node.type.name === 'heading' ? 360 : 456)};
}
// Deliberate control edits are approved by object/step identity, not by public
// transaction metadata. Subsequent appended steps still undergo continuity checks.
const approved = new WeakMap<Transaction, {before: Node; after: Node; steps: Step[]}>();
export function approveMinimumLineHeightChange(transaction: Transaction): void {
  approved.set(transaction, {before: transaction.before, after: transaction.doc, steps: [...transaction.steps]});
}
export function minimumLineHeightApprovedPrefix(transaction: Transaction): number {
  const proof = approved.get(transaction);
  return proof?.before === transaction.before && proof.steps.every((step, index) => transaction.steps[index] === step)
    && (transaction.docs[proof.steps.length] ?? transaction.doc) === proof.after ? proof.steps.length : 0;
}
