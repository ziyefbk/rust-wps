import type {JSONContent} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import type {EditorState} from '@tiptap/pm/state';
import {tableCellMargins} from './word-table-cell-margins';
import {parseParagraphSpacePoints} from './word-paragraph-spacing';

export const tablePositionFields = ['tableAlign', 'tableIndentTwips'] as const;
export type TableAlignment = 'left' | 'center' | 'right';
export interface TablePositionPatch {tableAlign?: TableAlignment | null; tableIndentTwips?: number | null}
export const tablePositionError = '表格位置只支持无合并、无嵌套的矩形表格；左缩进须为 0–1584 pt（0.05 pt 递增），表宽加缩进不超过 1584 pt（自动表宽按 450 pt 计）。正缩进须使用左对齐。';

export function isTableAlignment(value: unknown): value is TableAlignment {
  return value === 'left' || value === 'center' || value === 'right';
}
export function isTableIndent(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 31680;
}
export const parseTableIndentPoints = parseParagraphSpacePoints;
export function parseTableAlignMetadata(value: string | null): TableAlignment | null {
  return isTableAlignment(value) ? value : null;
}
export function parseTableIndentMetadata(value: string | null): number | null {
  return value !== null && /^\d{1,5}$/.test(value) && isTableIndent(Number(value)) ? Number(value) : null;
}

// This runs against incoming JSON before ProseMirror inserts its null defaults.
// Explicit null is invalid externally; omitted properties retain legacy bytes.
export function validateTablePositionAttributes(node: JSONContent): boolean {
  const attrs = node.attrs;
  let explicit = false;
  for (const field of tablePositionFields) {
    if (!attrs || !Object.prototype.hasOwnProperty.call(attrs, field)) continue;
    explicit = true;
    if (node.type !== 'table' || !(field === 'tableAlign' ? isTableAlignment(attrs[field]) : isTableIndent(attrs[field]))) {
      throw new Error(tablePositionError);
    }
  }
  if (explicit && attrs!.tableIndentTwips > 0 && attrs!.tableAlign != null && attrs!.tableAlign !== 'left') throw new Error(tablePositionError);
  return explicit;
}

// DOCX table indentation is the first cell's text edge, not a raw border
// translation. Tables without explicit cell margins retain 108-twip sides.
// Absence and explicit zero have different left-border origins; center/right
// alignment ignores zero indentation. Inline margins survive print cloning.
export function tablePositionMargins(attrs: Record<string, unknown>): {left: string; right: string} | null {
  if (attrs.tableAlign == null && attrs.tableIndentTwips == null) return null;
  if (attrs.tableAlign === 'center') return {left: 'auto', right: 'auto'};
  if (attrs.tableAlign === 'right') return {left: 'auto', right: '0pt'};
  return {left: isTableIndent(attrs.tableIndentTwips) ? `${(attrs.tableIndentTwips - tableCellMargins(attrs).left) / 20}pt` : '0pt', right: '0pt'};
}
export function tablePositionDOM(attrs: Record<string, unknown>, automatic: boolean): Record<string, string> {
  const margins = tablePositionMargins(attrs);
  if (!margins) return {};
  return {
    'data-table-position': '',
    ...(isTableAlignment(attrs.tableAlign) ? {'data-table-align': attrs.tableAlign} : {}),
    ...(isTableIndent(attrs.tableIndentTwips) ? {'data-table-indent-twips': String(attrs.tableIndentTwips)} : {}),
    style: `${automatic ? 'table-layout: auto; width: 600px; min-width: 0; ' : ''}margin-left: ${margins.left}; margin-right: ${margins.right}`,
  };
}

export interface SelectedPositionTable {table: Node; pos: number}
export function selectedPositionTables(state: EditorState): SelectedPositionTable[] {
  const tables = new Map<number, Node>();
  if (state.selection.empty) {
    const {$from} = state.selection;
    for (let depth = $from.depth; depth > 0; depth--) {
      if ($from.node(depth).type.name === 'table') {tables.set($from.before(depth), $from.node(depth)); break;}
    }
  } else {
    for (const {$from, $to} of state.selection.ranges) {
      state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
        if (node.type.name !== 'table') return;
        if (pos + 1 < $to.pos && pos + node.nodeSize > $from.pos) tables.set(pos, node);
        return false;
      });
    }
  }
  return [...tables].map(([pos, table]) => ({pos, table}));
}

function supportsTablePosition(state: EditorState, {table, pos}: SelectedPositionTable): boolean {
  const $pos = state.doc.resolve(pos);
  for (let depth = $pos.depth; depth > 0; depth--) if ($pos.node(depth).type.name === 'table') return false;
  const columns = table.firstChild?.childCount ?? 0;
  if (!columns || columns > 50 || !table.childCount || table.childCount > 500) return false;
  let valid = true;
  table.forEach(row => {
    if (row.type.name !== 'tableRow' || row.childCount !== columns) valid = false;
    row.forEach(cell => {
      if (!['tableCell', 'tableHeader'].includes(cell.type.name) || cell.attrs.colspan !== 1 || cell.attrs.rowspan !== 1) valid = false;
      cell.descendants(node => {if (node.type.name === 'table') valid = false;});
    });
  });
  return valid;
}

export function selectedTablePosition(state: EditorState) {
  const tables = selectedPositionTables(state);
  const alignments = tables.map(({table}) => table.attrs.tableAlign as TableAlignment | null);
  const indents = tables.map(({table}) => table.attrs.tableIndentTwips as number | null);
  const alignMixed = alignments.some(value => value !== alignments[0]);
  const indentMixed = indents.some(value => value !== indents[0]);
  return {
    count: tables.length,
    supported: tables.length > 0 && tables.every(table => supportsTablePosition(state, table)),
    tableAlign: {value: alignMixed ? null : alignments[0] ?? null, mixed: alignMixed},
    tableIndentTwips: {value: indentMixed ? null : indents[0] ?? null, mixed: indentMixed},
    hasExplicit: tables.some(({table}) => tablePositionFields.some(field => table.attrs[field] != null)),
  };
}
