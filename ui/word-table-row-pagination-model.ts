import type {JSONContent} from '@tiptap/core';
import {simpleCellMarginsTable} from './word-table-cell-margins';

export const rowCantSplitAttribute = 'rowCantSplit';
export const rowCantSplitDataAttribute = 'data-row-cant-split';
export const tableRowPaginationError = '行内不分页设置只接受默认、开启或关闭；仅支持正文顶层无合并、无嵌套的矩形表格，未证明的行结构修改未应用。';
export const tableRowPaginationPasteError = '此复制内容包含行分页设置，暂不支持粘贴到现有表格；可复制普通文字，或在表格外粘贴完整表格';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);

// External JSON is inspected before schema construction. Only the known
// schema default on tableRow is stripped by snapshot normalization below.
export function validateTableRowPaginationDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; table?: JSONContent; topLevel: boolean; depth: number}[] = [{node: document, depth: 0, topLevel: false}];
  const checked = new Set<JSONContent>();
  while (stack.length) {
    const {node, parent, table, topLevel, depth} = stack.pop()!;
    if (node.marks?.some(mark => mark.attrs && own(mark.attrs, rowCantSplitAttribute))) throw new Error(tableRowPaginationError);
    if (node.attrs && own(node.attrs, rowCantSplitAttribute)) {
      if (node.type !== 'tableRow' || typeof node.attrs[rowCantSplitAttribute] !== 'boolean'
        || !table || parent !== table || !topLevel) throw new Error(tableRowPaginationError);
      if (!checked.has(table)) {if (!simpleCellMarginsTable(table)) throw new Error(tableRowPaginationError); checked.add(table);}
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, depth: depth + 1,
      table: node.type === 'table' ? node : table, topLevel: node.type === 'table' ? depth === 1 : topLevel});
  }
}
export function omitDefaultTableRowPagination(document: JSONContent): JSONContent {
  const result = {...document};
  if (result.type === 'tableRow' && result.attrs?.[rowCantSplitAttribute] === null) {
    result.attrs = {...result.attrs}; delete result.attrs[rowCantSplitAttribute];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultTableRowPagination);
  return result;
}
export function tableRowPaginationAttributes() {
  return [{types: ['tableRow'], attributes: {[rowCantSplitAttribute]: {default: null, keepOnSplit: false,
    parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(rowCantSplitDataAttribute);
      return raw === null ? null : raw === 'true' ? true : raw === 'false' ? false : 'invalid-row-pagination';
    },
    renderHTML: (attrs: Record<string, unknown>) => typeof attrs[rowCantSplitAttribute] === 'boolean'
      ? {[rowCantSplitDataAttribute]: String(attrs[rowCantSplitAttribute])} : {},
  }}}];
}
// This is owned metadata, never an inference from CSS, MSO properties, or
// table/cell styles. Detect malformed/wrong-node evidence before schema loss.
export function validTableRowPaginationHTML(root: ParentNode): boolean {
  const elements = Array.from(root.querySelectorAll(`[${rowCantSplitDataAttribute}]`));
  for (const row of elements) {
    const value = row.getAttribute(rowCantSplitDataAttribute), parent = row.parentElement;
    const table = parent?.tagName === 'TABLE' ? parent : ['TBODY', 'THEAD', 'TFOOT'].includes(parent?.tagName ?? '') ? parent?.parentElement : null;
    if (row.tagName !== 'TR' || value !== 'true' && value !== 'false' || table?.tagName !== 'TABLE'
      || table.parentElement?.closest('table,li,blockquote') || table.querySelector('table')) return false;
    const rows = Array.from(table.querySelectorAll('tr'));
    const width = rows[0]?.children.length ?? 0;
    if (!rows.length || rows.length > 500 || !width || width > 50 || rows.some(item => item.children.length !== width
      || Array.from(item.children).some(cell => !['TD', 'TH'].includes(cell.tagName)
        || ![null, '1'].includes(cell.getAttribute('colspan')) || ![null, '1'].includes(cell.getAttribute('rowspan'))))) return false;
  }
  return true;
}
// Count actual start-tag attributes, not spelling in text, comments, script
// text, or another attribute's quoted value. The browser remains the semantic
// parser; this bounded lexical pass only retains duplicate/orphan evidence it
// would discard. Unowned HTML has no new interpretation or rejection.
export function countRowPaginationHTMLAttributes(html: string, attribute = rowCantSplitDataAttribute): number {
  let at = 0, count = 0;
  const length = html.length, space = (char: string) => /[\t\n\f\r ]/.test(char);
  const rawText = new Set(['script', 'style', 'textarea', 'title', 'xmp', 'iframe', 'noembed', 'noframes']);
  let lower: string | undefined;
  while (at < length) {
    at = html.indexOf('<', at);
    if (at < 0) break;
    if (html.startsWith('<!--', at)) {
      if (html[at + 4] === '>') {at += 5; continue;}
      if (html.startsWith('->', at + 4)) {at += 6; continue;}
      const ordinary = html.indexOf('-->', at + 4), bang = html.indexOf('--!>', at + 4);
      const end = ordinary < 0 ? bang : bang < 0 ? ordinary : Math.min(ordinary, bang);
      at = end < 0 ? length : end + (end === bang ? 4 : 3); continue;
    }
    if (!/[A-Za-z]/.test(html[at + 1] ?? '')) {const end = html.indexOf('>', at + 1); at = end < 0 ? length : end + 1; continue;}
    let end = ++at;
    while (end < length && !space(html[end]) && !['/', '>'].includes(html[end])) end++;
    const tag = html.slice(at, end).toLowerCase(); at = end;
    while (at < length) {
      while (at < length && (space(html[at]) || html[at] === '/')) at++;
      if (at >= length || html[at] === '>') {at++; break;}
      const start = at;
      while (at < length && !space(html[at]) && !['/', '=', '>'].includes(html[at])) at++;
      // An unexpected '=' is not an owned attribute, but must make progress.
      if (start === at) {at++; continue;}
      if (html.slice(start, at).toLowerCase() === attribute) count++;
      while (at < length && space(html[at])) at++;
      if (html[at] !== '=') continue;
      at++; while (at < length && space(html[at])) at++;
      const quote = html[at];
      if (quote === '"' || quote === "'") {const close = html.indexOf(quote, at + 1); at = close < 0 ? length : close + 1;}
      else while (at < length && !space(html[at]) && html[at] !== '>') at++;
    }
    if (tag === 'plaintext') break;
    if (rawText.has(tag)) {
      lower ??= html.toLowerCase();
      let close = lower.indexOf(`</${tag}`, at);
      while (close >= 0 && !/[\t\n\f\r />]/.test(html[close + tag.length + 2] ?? '')) close = lower.indexOf(`</${tag}`, close + 2);
      at = close < 0 ? length : close;
    }
  }
  return count;
}
export function isValidTableRowPaginationHTML(html: string): boolean {
  if (!/data-row-cant-split/i.test(html)) return true;
  const rawCount = countRowPaginationHTMLAttributes(html);
  // Size limits are owned-payload limits, not a new restriction triggered by
  // harmless literal spelling in otherwise ordinary clipboard text.
  if (html.length > 32 * 1024 * 1024) return rawCount === 0;
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  // HTML parsing can discard duplicate attrs and orphan rows. Keep that raw
  // evidence significant instead of accepting the normalized clean fragment.
  const parsedCount = parsed.querySelectorAll(`[${rowCantSplitDataAttribute}]`).length;
  if (rawCount === 0 && parsedCount === 0) return true;
  if (rawCount !== parsedCount || !validTableRowPaginationHTML(parsed)) return false;
  let count = 0;
  const stack = [{element: parsed.body, depth: 0}];
  while (stack.length) {
    const {element, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) return false;
    for (const child of Array.from(element.children)) stack.push({element: child as HTMLElement, depth: depth + 1});
  }
  return true;
}
export function preflightTableRowPaginationHTML(html: string): string {
  return isValidTableRowPaginationHTML(html) ? html
    : `<table><tbody><tr ${rowCantSplitDataAttribute}="invalid"><td><p></p></td></tr></tbody></table>`;
}
