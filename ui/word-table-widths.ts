import {autoPreferredAttribute, autoPreferredDataAttribute, autoPreferredError, isAutoPreferredWidths, parseAutoPreferredMetadata, validateAutoPreferredDocument, validAutoPreferredHTML, isValidAutoPreferredHTML, preflightAutoPreferredHTML, validateAutoPreferredPaste} from './word-table-auto-preferred';
import {approveTableRowPaginationTransaction, replayTableRowPaginationTransaction, rowCantSplitAttribute, tableRowPaginationAttributes, tableRowPaginationError, validateTableRowPaginationDocument} from './word-table-row-pagination';
import {cellFillAttribute, cellFillModeAttribute, cellTextColorModeAttribute, isCellFillMode, isCellFillRgb, isCellTextColorMode, tableCellColorFields, tableCellColorsAttributes, tableCellColorsError, validateTableCellColorsDocument, validTableCellColorsHTML, isValidTableCellColorsHTML, preflightTableCellColorsHTML, type TableCellColorsPatch} from './word-table-cell-colors';
import {cellVerticalAlignAttribute, isCellVerticalAlignment, isRowMinHeight, rowMinHeightAttribute, simpleVerticalTable, tableVerticalAttributes, tableVerticalError, validateTableVerticalDocument, validTableVerticalHTML, type TableVerticalPatch} from './word-table-vertical-layout';
import {isTableCellMargins, parseTableCellMarginsMetadata, simpleCellMarginsTable, tableCellMarginsAttribute, tableCellMarginsDataAttribute, tableCellMarginsDOM, tableCellMarginsError, validateTableCellMarginsDocument, validTableCellMarginsHTML, type TableCellMargins} from './word-table-cell-margins';
import {omitDefaultParagraphIndent, validateParagraphIndentDocument} from './word-paragraph-indent-model';
import {isPageGeometry, omitDefaultPageGeometry, pageTableWidthTwips, validatePageGeometryDocument} from './word-page-geometry';
import {mergeAttributes, type CommandProps, type JSONContent} from '@tiptap/core';
import {Table, TableView, type TableOptions} from '@tiptap/extension-table';
import type {DOMOutputSpec, Node} from '@tiptap/pm/model';
import {EditorState, NodeSelection, Plugin, Selection, TextSelection, type Transaction} from '@tiptap/pm/state';
import {CellSelection, TableMap, addColumnAfter, addColumnBefore, addRowAfter, addRowBefore,
  deleteColumn, deleteRow, deleteTable, isInTable, selectedRect} from '@tiptap/pm/tables';
import {parseParagraphSpacePoints} from './word-paragraph-spacing';
import {isTableAlignment, isTableIndent, parseTableAlignMetadata, parseTableIndentMetadata, selectedPositionTables,
  selectedTablePosition, tablePositionDOM, tablePositionError, tablePositionFields, tablePositionMargins,
  validateTablePositionAttributes, type TablePositionPatch} from './word-table-position';

export const MAX_TABLE_WIDTH_TWIPS = 31680;
export const DEFAULT_TABLE_WIDTH_TWIPS = 9000;
const widthAttribute = 'columnWidthsTwips';
const widthDataAttribute = 'data-column-widths-twips';
const widthError = '固定列宽须为 0.05–1584 pt（0.05 pt 递增），整表总宽不超过 1584 pt；只支持无合并、无嵌套的矩形表格。';
const cellTypes = new Set(['tableCell', 'tableHeader']);

export function isTableColumnWidths(value: unknown): value is number[] {
  return Array.isArray(value) && value.length > 0 && value.length <= 50
    && Array.from(value).every(item => typeof item === 'number' && Number.isInteger(item) && item >= 1 && item <= MAX_TABLE_WIDTH_TWIPS)
    && value.reduce((sum, item) => sum + item, 0) <= MAX_TABLE_WIDTH_TWIPS;
}

export function parseTableColumnWidthPoints(value: string): number | null {
  const result = parseParagraphSpacePoints(value);
  return result !== null && result > 0 ? result : null;
}

// Canonical twips never go into Tiptap's colwidth, which is a CSS-pixel array.
// Only our own exact metadata is accepted from HTML; arbitrary Office/CSS
// geometry must not be silently promoted to a supported fixed-width table.
export function parseTableColumnWidthsMetadata(value: string | null): number[] | null {
  if (value === null || value.length > 300 || !/^\d+(?:,\d+)*$/.test(value)) return null;
  const widths = value.split(',').map(Number);
  return isTableColumnWidths(widths) ? widths : null;
}

export function validateTableWidthsDocument(document: JSONContent): void {
  validateAutoPreferredDocument(document);
  const visit = (node: JSONContent, tableDepth: number, geometryAncestor: string | null) => {
    if (cellTypes.has(node.type ?? '') && node.attrs?.colwidth != null) throw new Error('不支持单元格像素 colwidth；请使用整表 columnWidthsTwips 固定列宽。');
    const hasWidth = !!node.attrs && Object.prototype.hasOwnProperty.call(node.attrs, widthAttribute);
    if (hasWidth && (node.type !== 'table' || !isTableColumnWidths(node.attrs![widthAttribute]))) throw new Error(widthError);
    const positioned = validateTablePositionAttributes(node);
    const fixed = node.type === 'table' && hasWidth;
    const geometry = fixed || positioned;
    const error = positioned ? tablePositionError : widthError;
    if (node.type === 'table' && (geometry && tableDepth > 0 || geometryAncestor)) throw new Error(geometryAncestor ?? error);
    if (geometry) {
      const widths = fixed ? node.attrs![widthAttribute] as number[] : null;
      const columns = widths?.length ?? node.content?.[0]?.content?.length ?? 0;
      if (!node.content?.length || node.content.length > 500 || !columns || columns > 50) throw new Error(error);
      if (positioned && (widths?.reduce((sum, width) => sum + width, 0) ?? DEFAULT_TABLE_WIDTH_TWIPS)
        + (node.attrs!.tableIndentTwips ?? 0) > MAX_TABLE_WIDTH_TWIPS) throw new Error(tablePositionError);
      for (const row of node.content) {
        if (row.type !== 'tableRow' || row.content?.length !== columns) throw new Error(error);
        for (const cell of row.content) {
          if (!cellTypes.has(cell.type ?? '') || (cell.attrs?.colspan ?? 1) !== 1 || (cell.attrs?.rowspan ?? 1) !== 1) throw new Error(error);
        }
      }
    }
    for (const child of node.content ?? []) visit(child, tableDepth + Number(node.type === 'table'), geometryAncestor ?? (geometry ? error : null));
  };
  visit(document, 0, null);
  validateTableCellMarginsDocument(document);
  validateTableVerticalDocument(document);
  validateTableCellColorsDocument(document);
  validateTableRowPaginationDocument(document);
}

// Default null is an editor-only attribute. Fixed cells carry no second width
// source in saved drafts/models, even the builtin's default colwidth:null.
export function omitDefaultTableWidths(document: JSONContent, inFixedTable = false): JSONContent {
  const result = {...document};
  const fixed = result.type === 'table' ? result.attrs?.[widthAttribute] != null || result.attrs?.[autoPreferredAttribute] != null : inFixedTable;
  if (result.attrs) {
    result.attrs = {...result.attrs};
    if (cellTypes.has(result.type ?? '')) for (const field of tableCellColorFields) if (result.attrs[field] === null) delete result.attrs[field];
    if (result.type === 'tableRow' && result.attrs[rowCantSplitAttribute] === null) delete result.attrs[rowCantSplitAttribute];
    if (result.type === 'tableRow' && result.attrs[rowMinHeightAttribute] === null) delete result.attrs[rowMinHeightAttribute];
    if (cellTypes.has(result.type ?? '') && result.attrs[cellVerticalAlignAttribute] === null) delete result.attrs[cellVerticalAlignAttribute];
    if (result.type === 'table' && result.attrs[autoPreferredAttribute] === null) delete result.attrs[autoPreferredAttribute];
    if (result.type === 'table' && result.attrs[widthAttribute] === null) delete result.attrs[widthAttribute];
    if (result.type === 'table' && result.attrs[tableCellMarginsAttribute] === null) delete result.attrs[tableCellMarginsAttribute];
    if (result.type === 'table') for (const field of tablePositionFields) if (result.attrs[field] === null) delete result.attrs[field];
    if (fixed && cellTypes.has(result.type ?? '') && result.attrs.colwidth === null) delete result.attrs.colwidth;
    if (Object.keys(result.attrs).length === 0) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(child => omitDefaultTableWidths(child, fixed));
  return result;
}

export function fixedTableDOM(widths: number[], attributes: Record<string, unknown> = {}): DOMOutputSpec {
  if (!isTableColumnWidths(widths)) throw new Error(widthError);
  return ['table', mergeAttributes(attributes, {
    [widthDataAttribute]: widths.join(','),
    style: `table-layout: fixed; width: ${widths.reduce((sum, width) => sum + width, 0) / 15}px; min-width: 0`,
  }), ['colgroup', {}, ...widths.map(width => ['col', {style: `width: ${width / 15}px`}] as DOMOutputSpec)], ['tbody', 0]];
}

export function autoPreferredTableDOM(widths: number[], attributes: Record<string, unknown> = {}): DOMOutputSpec {
  if (!isAutoPreferredWidths(widths)) throw new Error(autoPreferredError);
  return ['table', mergeAttributes(attributes, {[autoPreferredDataAttribute]: widths.join(','), style: 'table-layout: auto; width: auto; min-width: 0'}),
    ['colgroup', {}, ...widths.map(width => ['col', {style: `width: ${width / 15}px`}] as DOMOutputSpec)], ['tbody', 0]];
}

// The stock non-resizable TableView still derives its colgroup from the first
// row's pixel colwidth attributes. Override both mount and update for canonical
// tables, and retain the original renderer for tables without fixed geometry.
export class FixedTableView extends TableView {
  constructor(...args: ConstructorParameters<typeof TableView>) { super(...args); this.syncFixedWidths(); this.syncPosition(); this.syncCellMargins(); }
  private syncFixedWidths() {
    const preferred: unknown = this.node.attrs[autoPreferredAttribute];
    this.table.removeAttribute(autoPreferredDataAttribute);
    if (isAutoPreferredWidths(preferred)) {
      this.table.removeAttribute(widthDataAttribute);
      this.table.setAttribute(autoPreferredDataAttribute, preferred.join(','));
      this.table.style.tableLayout = 'auto'; this.table.style.width = 'auto'; this.table.style.minWidth = '0';
      this.colgroup.replaceChildren(...preferred.map(width => {const col = this.table.ownerDocument.createElement('col'); col.style.width = `${width / 15}px`; return col;}));
      return;
    }
    const widths: unknown = this.node.attrs[widthAttribute];
    if (!isTableColumnWidths(widths)) { this.table.removeAttribute(widthDataAttribute); return; }
    this.table.setAttribute(widthDataAttribute, widths.join(','));
    this.table.style.tableLayout = 'fixed';
    this.table.style.width = `${widths.reduce((sum, width) => sum + width, 0) / 15}px`;
    this.table.style.minWidth = '0';
    this.colgroup.replaceChildren(...widths.map(width => {
      const col = this.table.ownerDocument.createElement('col');
      col.style.width = `${width / 15}px`;
      return col;
    }));
  }
  private syncPosition() {
    const attrs = this.node.attrs, margins = tablePositionMargins(attrs);
    for (const name of ['data-table-position', 'data-table-align', 'data-table-indent-twips']) this.table.removeAttribute(name);
    this.table.style.marginLeft = margins?.left ?? '';
    this.table.style.marginRight = margins?.right ?? '';
    if (!margins) {if (!isTableColumnWidths(attrs[widthAttribute]) && !isAutoPreferredWidths(attrs[autoPreferredAttribute])) this.table.style.tableLayout = ''; return;}
    this.table.setAttribute('data-table-position', '');
    if (isTableAlignment(attrs.tableAlign)) this.table.setAttribute('data-table-align', attrs.tableAlign);
    if (isTableIndent(attrs.tableIndentTwips)) this.table.setAttribute('data-table-indent-twips', String(attrs.tableIndentTwips));
    if (!isTableColumnWidths(attrs[widthAttribute]) && !isAutoPreferredWidths(attrs[autoPreferredAttribute])) {
      this.table.style.tableLayout = 'auto';
      this.table.style.width = `${DEFAULT_TABLE_WIDTH_TWIPS / 15}px`;
      this.table.style.minWidth = '0';
    }
  }
  private syncCellMargins() {
    const margins = this.node.attrs[tableCellMarginsAttribute];
    this.table.removeAttribute(tableCellMarginsDataAttribute);
    if (isTableCellMargins(margins)) {
      this.table.setAttribute(tableCellMarginsDataAttribute, `${margins.left},${margins.right}`);
      if (!isTableColumnWidths(this.node.attrs[widthAttribute]) && !isAutoPreferredWidths(this.node.attrs[autoPreferredAttribute])) {
        this.table.style.tableLayout = 'auto'; this.table.style.width = `${DEFAULT_TABLE_WIDTH_TWIPS / 15}px`; this.table.style.minWidth = '0';
      }
    }
    // Empty values remove owned variables in the real DOM, avoiding stale
    // padding after reset and preserving independently cloned print snapshots.
    this.table.style.setProperty('--word-cell-margin-left', isTableCellMargins(margins) ? `${margins.left / 20}pt` : '');
    this.table.style.setProperty('--word-cell-margin-right', isTableCellMargins(margins) ? `${margins.right / 20}pt` : '');
  }
  update(node: Node) {
    if (node.type !== this.node.type) return false;
    if ((isTableColumnWidths(this.node.attrs[widthAttribute]) || isAutoPreferredWidths(this.node.attrs[autoPreferredAttribute]))
      && (this.node.attrs[widthAttribute] !== node.attrs[widthAttribute] || this.node.attrs[autoPreferredAttribute] !== node.attrs[autoPreferredAttribute])) {
      // The stock renderer writes min-width without clearing width. Discard
      // our owned cols before restoring auto so reset/print cannot keep stale
      // widths that are absent from the model and exported DOCX.
      this.colgroup.replaceChildren();
      this.table.style.tableLayout = ''; this.table.style.width = ''; this.table.style.minWidth = '';
    }
    if (!super.update(node)) return false;
    this.syncFixedWidths();
    this.syncPosition();
    this.syncCellMargins();
    return true;
  }
}

export interface SelectedTableColumn {table: Node; pos: number; column: number; row: number; widths: number[] | null; preferredWidths: number[] | null}

export function selectedTableColumn(state: EditorState): SelectedTableColumn | null {
  if (!isInTable(state)) return null;
  const selection = state.selection;
  const selectedCellNode = selection instanceof NodeSelection && cellTypes.has(selection.node.type.name);
  const $head = selection instanceof CellSelection ? selection.$headCell : selectedCellNode ? selection.$anchor : selection.$head;
  for (let depth = $head.depth; depth > 0; depth--) {
    const table = $head.node(depth);
    if (table.type.name !== 'table') continue;
    const pos = $head.before(depth), start = $head.start(depth);
    if (selection.from < start || selection.to > pos + table.nodeSize - 1) return null;
    if (!(selection instanceof CellSelection) && !selectedCellNode && $head.depth < depth + 2) return null;
    const cellPos = selection instanceof CellSelection || selectedCellNode ? $head.pos : $head.before(depth + 2);
    const rect = TableMap.get(table).findCell(cellPos - start);
    return {table, pos, column: rect.left, row: rect.top, widths: table.attrs[widthAttribute] ?? null, preferredWidths: table.attrs[autoPreferredAttribute] ?? null};
  }
  return null;
}

const encoder = new TextEncoder();
function validateProspectiveDocument(document: Node): void {
  let nodes = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++nodes > 10000 || depth > 32) throw new Error('表格修改后文档结构超出保存上限，未修改正文。');
    if (node.isText) {
      bytes += encoder.encode(node.text!).length;
      if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未修改表格。');
    }
    for (let index = node.childCount - 1; index >= 0; index--) stack.push({node: node.child(index), depth: depth + 1});
  }
  const snapshot = omitDefaultParagraphIndent(omitDefaultTableWidths(document.toJSON()));
  validateTableWidthsDocument(snapshot);
  validatePageGeometryDocument(omitDefaultPageGeometry(snapshot));
  validateParagraphIndentDocument(omitDefaultParagraphIndent(omitDefaultPageGeometry(snapshot)));
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) {
    throw new Error('表格修改后草稿超过 32 MiB 保存上限，未修改正文。');
  }
}

export function createTableColumnWidthTransaction(state: EditorState, width: number | null): Transaction | null {
  if (width !== null && !isTableColumnWidths([width])) throw new Error(widthError);
  const selected = selectedTableColumn(state);
  if (!selected) return null;
  const {table, pos, column} = selected;
  const columns = table.firstChild?.childCount ?? 0;
  const page = state.doc.attrs.pageGeometry;
  const defaultWidth = pageTableWidthTwips(table.attrs, isPageGeometry(page) ? page : null);
  const defaults = Array.from({length: columns}, (_, index) => Math.floor(defaultWidth / columns)
    + Number(index < defaultWidth % columns));
  const widths = width === null ? null : [...(selected.widths ?? selected.preferredWidths ?? defaults)];
  if (widths) widths[column] = width!;
  if (!selected.preferredWidths && JSON.stringify(widths) === JSON.stringify(selected.widths)) return null;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr.setNodeMarkup(pos, undefined, {...table.attrs, [widthAttribute]: widths, [autoPreferredAttribute]: null}, table.marks);
  validateProspectiveDocument(transaction.doc);
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}

// Position changes affect entire selected tables. Construct an isolated
// transaction so rejected multi-table edits cannot leak into a Tiptap chain.
export function createTablePositionTransaction(state: EditorState, patch: TablePositionPatch): Transaction | null {
  const fields = Object.keys(patch) as (keyof TablePositionPatch)[];
  for (const field of fields) {
    if (!tablePositionFields.includes(field) || patch[field] !== null
      && !(field === 'tableAlign' ? isTableAlignment(patch[field]) : isTableIndent(patch[field]))) throw new Error(tablePositionError);
  }
  const selected = selectedTablePosition(state);
  if (!selected.count || !fields.length) return null;
  if (!selected.supported) throw new Error(tablePositionError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const {table, pos} of selectedPositionTables(state)) {
    const attrs = {...table.attrs, ...patch};
    // Selecting center/right discards only incompatible positive indentation.
    // Preserve explicit zero as distinct metadata from omitted indentation.
    if ((patch.tableAlign === 'center' || patch.tableAlign === 'right') && (attrs.tableIndentTwips ?? 0) > 0) {
      if (Object.prototype.hasOwnProperty.call(patch, 'tableIndentTwips')) throw new Error(tablePositionError);
      attrs.tableIndentTwips = null;
    }
    if (tablePositionFields.some(field => table.attrs[field] !== attrs[field])) transaction.setNodeMarkup(pos, undefined, attrs, table.marks);
  }
  if (!transaction.docChanged) return null;
  validateProspectiveDocument(transaction.doc);
  // setNodeMarkup replaces the table node and maps a table NodeSelection to
  // text by default. Position edits never change sizes, so keep its exact kind.
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}

export interface TableCellMarginsToken {doc: Node; selection: Selection}
export const captureTableCellMargins = (state: EditorState): TableCellMarginsToken => ({doc: state.doc, selection: state.selection});
export const matchesTableCellMargins = (state: EditorState, token: TableCellMarginsToken): boolean => state.doc === token.doc && state.selection.eq(token.selection);
export function selectedTableCellMargins(state: EditorState) {
  const selection = state.selection;
  const selected = selection instanceof NodeSelection && selection.node.type.name === 'table'
    ? {table: selection.node, pos: selection.from} : selectedTableColumn(state);
  if (!selected) return null;
  const {table, pos} = selected;
  return {table, pos, supported: state.doc.resolve(pos).depth === 0 && simpleCellMarginsTable(table.toJSON()),
    value: isTableCellMargins(table.attrs[tableCellMarginsAttribute]) ? table.attrs[tableCellMarginsAttribute] as TableCellMargins : null};
}
export function createTableCellMarginsTransaction(state: EditorState, margins: TableCellMargins | null, token?: TableCellMarginsToken): Transaction | null {
  if (margins !== null && !isTableCellMargins(margins)) throw new Error(tableCellMarginsError);
  if (token && !matchesTableCellMargins(state, token)) return null;
  const selected = selectedTableCellMargins(state);
  if (!selected) return null;
  if (!selected.supported) throw new Error(tableCellMarginsError);
  const previous = selected.value;
  if (margins === null && previous === null || margins && previous && margins.left === previous.left && margins.right === previous.right) return null;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr
    .setNodeMarkup(selected.pos, undefined, {...selected.table.attrs, [tableCellMarginsAttribute]: margins ? {...margins} : null}, selected.table.marks);
  validateProspectiveDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}


export interface SelectedVerticalNode {node: Node; pos: number}
export function selectedTableVerticalLayout(state: EditorState) {
  const selection = state.selection;
  const selected = selection instanceof NodeSelection && selection.node.type.name === 'table'
    ? {table: selection.node, pos: selection.from}
    : selection instanceof NodeSelection && selection.node.type.name === 'tableRow' && selection.$from.parent.type.name === 'table'
      ? {table: selection.$from.parent, pos: selection.$from.before()} : selectedTableColumn(state);
  if (!selected) return null;
  const {table, pos} = selected, rows: SelectedVerticalNode[] = [], cells: SelectedVerticalNode[] = [];
  const selectedCells = new Set<number>();
  if (selection instanceof CellSelection) selection.forEachCell((_cell, at) => selectedCells.add(at));
  table.forEach((row, rowOffset) => {
    const rowPos = pos + 1 + rowOffset;
    row.forEach((cell, cellOffset) => {
      const cellPos = rowPos + 1 + cellOffset;
      let active = selection instanceof CellSelection ? selectedCells.has(cellPos)
        : selection instanceof NodeSelection ? selection.from <= cellPos && selection.to >= cellPos + cell.nodeSize
        : selection.empty ? selection.from > cellPos && selection.from < cellPos + cell.nodeSize : false;
      if (!(selection instanceof CellSelection) && !(selection instanceof NodeSelection) && !selection.empty) {
        cell.descendants((child, offset) => {
          if (child.isTextblock && selection.ranges.some(({$from, $to}) => {
            const start = cellPos + 1 + offset;
            return start + 1 < $to.pos && start + child.nodeSize > $from.pos;
          })) active = true;
        });
      }
      if (active) {cells.push({node: cell, pos: cellPos}); if (!rows.some(item => item.pos === rowPos)) rows.push({node: row, pos: rowPos});}
    });
  });
  const fieldState = (nodes: SelectedVerticalNode[], field: string) => {
    const values = nodes.map(({node}) => node.attrs[field] ?? null), mixed = values.some(value => value !== values[0]);
    return {value: mixed ? null : values[0] ?? null, mixed, hasExplicit: values.some(value => value !== null)};
  };
  return {table, pos, rows, cells, supported: cells.length > 0 && state.doc.resolve(pos).depth === 0 && simpleVerticalTable(table.toJSON()),
    rowMinHeightTwips: fieldState(rows, rowMinHeightAttribute), cellVerticalAlign: fieldState(cells, cellVerticalAlignAttribute)};
}
export function createTableVerticalTransaction(state: EditorState, patch: TableVerticalPatch, token?: TableCellMarginsToken): Transaction | null {
  const fields = Object.keys(patch) as (keyof TableVerticalPatch)[];
  for (const field of fields) if (![rowMinHeightAttribute, cellVerticalAlignAttribute].includes(field)
    || patch[field] !== null && !(field === rowMinHeightAttribute ? isRowMinHeight(patch[field]) : isCellVerticalAlignment(patch[field]))) throw new Error(tableVerticalError);
  if (token && !matchesTableCellMargins(state, token)) return null;
  const selected = selectedTableVerticalLayout(state);
  if (!selected || !fields.length) return null;
  if (!selected.supported) throw new Error(tableVerticalError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const field of fields) for (const {node, pos} of field === rowMinHeightAttribute ? selected.rows : selected.cells) {
    if ((node.attrs[field] ?? null) !== patch[field]) transaction.setNodeMarkup(pos, undefined, {...node.attrs, [field]: patch[field]}, node.marks);
  }
  if (!transaction.docChanged) return null;
  validateProspectiveDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}

// Pagination shares the complete caret/range/cell/row/table selection logic,
// but remains independent from minimum height and header-cell representation.
export function selectedTableRowPagination(state: EditorState) {
  const selected = selectedTableVerticalLayout(state);
  if (!selected) return null;
  const values = selected.rows.map(({node}) => node.attrs[rowCantSplitAttribute] ?? null);
  const mixed = values.some(value => value !== values[0]);
  return {...selected, count: selected.rows.length, value: mixed ? null : values[0] ?? null,
    mixed, hasExplicit: values.some(value => value !== null)};
}
export function createTableRowPaginationTransaction(state: EditorState, value: boolean | null, token?: TableCellMarginsToken): Transaction | null {
  if (value !== null && typeof value !== 'boolean') throw new Error(tableRowPaginationError);
  if (token && !matchesTableCellMargins(state, token)) return null;
  const selected = selectedTableRowPagination(state);
  if (!selected) return null;
  if (!selected.supported) throw new Error(tableRowPaginationError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const {node, pos} of selected.rows) if ((node.attrs[rowCantSplitAttribute] ?? null) !== value) {
    transaction.setNodeMarkup(pos, undefined, {...node.attrs, [rowCantSplitAttribute]: value}, node.marks);
  }
  if (!transaction.docChanged) return null;
  validateProspectiveDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON())).setStoredMarks(state.storedMarks);
  approveTableRowPaginationTransaction(transaction);
  return transaction;
}

// Reuse the exact established cell/range selection semantics of vertical
// layout. Fill and automatic text color remain independent field states.
export function selectedTableCellColors(state: EditorState) {
  const selected = selectedTableVerticalLayout(state);
  if (!selected) return null;
  const fieldState = (field: string) => {
    const values = selected.cells.map(({node}) => node.attrs[field] ?? null), mixed = values.some(value => value !== values[0]);
    return {value: mixed ? null : values[0] ?? null, mixed, hasExplicit: values.some(value => value !== null)};
  };
  const fills = selected.cells.map(({node}) => node.attrs[cellFillModeAttribute] === 'none' ? 'none' : node.attrs[cellFillAttribute] ?? null);
  const mixed = fills.some(value => value !== fills[0]);
  const cellFill = {value: mixed ? null : fills[0] ?? null, mixed, hasExplicit: fills.some(value => value !== null)};
  return {...selected, cellFill, cellFillMode: fieldState(cellFillModeAttribute), cellFillRgb: fieldState(cellFillAttribute), cellTextColorMode: fieldState(cellTextColorModeAttribute)};
}
export function createTableCellColorsTransaction(state: EditorState, patch: TableCellColorsPatch, token?: TableCellMarginsToken): Transaction | null {
  let fields = Object.keys(patch) as (keyof TableCellColorsPatch)[];
  for (const field of fields) if (!tableCellColorFields.includes(field)
    || patch[field] !== null && !(field === cellFillAttribute ? isCellFillRgb(patch[field]) : field === cellFillModeAttribute ? isCellFillMode(patch[field]) : isCellTextColorMode(patch[field]))) throw new Error(tableCellColorsError);
  if (patch.cellFillRgb != null && patch.cellFillMode != null) throw new Error(tableCellColorsError);
  // Any fill-only null reset clears both declarations; a nonnull fill replaces
  // the other declaration atomically. Text-only patches never touch fill.
  if (fields.includes(cellFillAttribute) || fields.includes(cellFillModeAttribute)) {
    patch = {...patch, cellFillRgb: patch.cellFillRgb ?? null, cellFillMode: patch.cellFillMode ?? null};
    fields = Object.keys(patch) as (keyof TableCellColorsPatch)[];
  }
  if (token && !matchesTableCellMargins(state, token)) return null;
  const selected = selectedTableCellColors(state);
  if (!selected || !fields.length) return null;
  if (!selected.supported) throw new Error(tableCellColorsError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  // One markup step per cell preserves all fields in a combined patch.
  for (const {node, pos} of selected.cells) if (fields.some(field => (node.attrs[field] ?? null) !== patch[field])) {
    transaction.setNodeMarkup(pos, undefined, {...node.attrs, ...patch}, node.marks);
  }
  if (!transaction.docChanged) return null;
  validateProspectiveDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}

const structuralCommands = {addColumnBefore, addColumnAfter, deleteColumn, addRowBefore, addRowAfter, deleteRow, deleteTable};
export type TableStructureAction = keyof typeof structuralCommands;

export function createTableStructureTransaction(state: EditorState, action: TableStructureAction): Transaction | null {
  if (!(action in structuralCommands)) throw new Error('不支持的表格操作。');
  const selected = selectedTableColumn(state);
  if (!selected) return null;
  const rect = selectedRect(state), {table, pos} = selected;
  const vectorField = selected.preferredWidths ? autoPreferredAttribute : widthAttribute;
  let widths = selected.preferredWidths ? [...selected.preferredWidths] : selected.widths ? [...selected.widths] : null;
  // Check additions before constructing structural steps, including the total
  // width budget. The newly inserted column inherits its adjacent column.
  if (action === 'addColumnBefore' || action === 'addColumnAfter') {
    if (rect.map.width >= 50) throw new Error('表格最多 50 列，未增加列。');
    if (widths) {
      const at = action === 'addColumnBefore' ? rect.left : rect.right;
      widths.splice(at, 0, widths[action === 'addColumnBefore' ? rect.left : rect.right - 1]);
      if (!isTableColumnWidths(widths)) throw new Error(widthError);
    }
  }
  if ((action === 'addRowBefore' || action === 'addRowAfter') && table.childCount >= 500) throw new Error('表格最多 500 行，未增加行。');
  if (action === 'deleteColumn') {
    if (rect.left === 0 && rect.right === rect.map.width) action = 'deleteTable';
    else if (widths) widths.splice(rect.left, rect.right - rect.left);
  }
  if (action === 'deleteRow' && rect.top === 0 && rect.bottom === rect.map.height) action = 'deleteTable';
  let transaction: Transaction | null = null;
  // Tiptap chainable state.tr is mutable shared state. Build off an isolated
  // state so a failed prospective check cannot leak steps into that chain.
  const detached = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks});
  structuralCommands[action](detached, tr => {
    if (action !== 'deleteTable') {
      const nextPos = tr.mapping.map(pos), next = tr.doc.nodeAt(nextPos)!;
      if (widths) tr.setNodeMarkup(nextPos, undefined, {...next.attrs, [vectorField]: widths}, next.marks);
      // Deleting a terminal row/column otherwise leaves the caret in a later
      // table or after this table. Pick the nearest surviving cell explicitly.
      if (action === 'deleteColumn' || action === 'deleteRow') {
        const map = TableMap.get(tr.doc.nodeAt(nextPos)!);
        const row = Math.min(selected.row, map.height - 1), column = Math.min(selected.column, map.width - 1);
        tr.setSelection(TextSelection.near(tr.doc.resolve(nextPos + 1 + map.map[row * map.width + column] + 1)));
      }
    }
    validateProspectiveDocument(tr.doc);
    approveTableRowPaginationTransaction(tr);
    transaction = tr;
  });
  return transaction;
}

// Also protects keyboard/paste and third-party commands: invalid fixed tables
// never enter history or autosave. Our structural commands pass one complete,
// already validated transaction rather than temporarily inconsistent columns.
export function tableWidthGuard(onRejected: (message: string) => void = () => {}): Plugin {
  return new Plugin({props: {handlePaste: (view, event, slice) => {
    try {validateAutoPreferredPaste(view.state, slice);} catch (error) {onRejected(String(error)); return true;}
    const html = event.clipboardData?.getData('text/html');
    if (!html || !/data-(?:auto-preferred-column-widths-twips|table-cell-margins-twips|row-min-height-twips|cell-vertical-align|cell-fill-rgb|cell-fill-mode|cell-text-color-mode)/i.test(html)) return false;
    if (!isValidAutoPreferredHTML(html)) {onRejected(autoPreferredError); return true;}
    if (!isValidTableCellColorsHTML(html)) {onRejected(tableCellColorsError); return true;}
    const parsed = new DOMParser().parseFromString(html, 'text/html');
    if (!validAutoPreferredHTML(parsed)) {onRejected(autoPreferredError); return true;}
    if (!validTableCellMarginsHTML(parsed)) {onRejected(tableCellMarginsError); return true;}
    if (!validTableVerticalHTML(parsed)) {onRejected(tableVerticalError); return true;}
    if (!validTableCellColorsHTML(parsed)) {onRejected(tableCellColorsError); return true;}
    return false;
  }}, filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try { validateProspectiveDocument(transaction.doc); return true; }
    catch (error) { onRejected(String(error)); return false; }
  }});
}

export const WordTable = Table.extend<TableOptions & {onWidthError: (message: string) => void; locked: () => boolean}>({
  addOptions() { return {...this.parent!(), onWidthError: () => {}, locked: () => false}; },
  transformPastedHTML: html => preflightAutoPreferredHTML(preflightTableCellColorsHTML(html)),
  addGlobalAttributes() {return [...(this.parent?.() ?? []), ...tableVerticalAttributes(), ...tableCellColorsAttributes(), ...tableRowPaginationAttributes()];},
  addAttributes() {
    return {...this.parent?.(), [autoPreferredAttribute]: {default: null, rendered: false, parseHTML: element => {
      const raw = element.getAttribute(autoPreferredDataAttribute);
      return raw === null ? null : parseAutoPreferredMetadata(raw) ?? 'invalid-auto-preferred-widths';
    }}, [widthAttribute]: {
      default: null, rendered: false,
      parseHTML: element => parseTableColumnWidthsMetadata(element.getAttribute(widthDataAttribute)),
    }, [tableCellMarginsAttribute]: {default: null, rendered: false, parseHTML: element => {
      const raw = element.getAttribute(tableCellMarginsDataAttribute);
      return raw === null ? null : parseTableCellMarginsMetadata(raw) ?? 'invalid-table-cell-margins';
    }}, tableAlign: {default: null, rendered: false, parseHTML: element => parseTableAlignMetadata(element.getAttribute('data-table-align'))},
    tableIndentTwips: {default: null, rendered: false, parseHTML: element => parseTableIndentMetadata(element.getAttribute('data-table-indent-twips'))}};
  },
  renderHTML({node, HTMLAttributes}) {
    const widths: unknown = node.attrs[widthAttribute];
    const preferred = node.attrs[autoPreferredAttribute];
    if (isAutoPreferredWidths(preferred)) return autoPreferredTableDOM(preferred, mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, tablePositionDOM(node.attrs, false)));
    const attributes = mergeAttributes(HTMLAttributes, tablePositionDOM(node.attrs, !isTableColumnWidths(widths)), tableCellMarginsDOM(node.attrs, !isTableColumnWidths(widths)));
    return isTableColumnWidths(widths) ? fixedTableDOM(widths, mergeAttributes(this.options.HTMLAttributes, attributes))
      : this.parent!({node, HTMLAttributes: attributes});
  },
  addCommands() {
    const rowPaginationCommand = (value: boolean | null) => ({state, dispatch, tr}: CommandProps) => {
      try {
        if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
        const transaction = createTableRowPaginationTransaction(state, value);
        if (!transaction) return false;
        if (dispatch) {
          replayTableRowPaginationTransaction(tr, transaction);
          tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
          if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
        }
        return true;
      } catch (error) {if (dispatch) this.options.onWidthError(String(error)); return false;}
    };
    const cellColorsCommand = (patch: TableCellColorsPatch) => ({state, dispatch, tr}: CommandProps) => {
      try {
        if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
        const transaction = createTableCellColorsTransaction(state, patch);
        if (!transaction) return false;
        if (dispatch) {
          replayTableRowPaginationTransaction(tr, transaction);
          tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
          if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
        }
        return true;
      } catch (error) {if (dispatch) this.options.onWidthError(String(error)); return false;}
    };
    const verticalCommand = (patch: TableVerticalPatch) => ({state, dispatch, tr}: CommandProps) => {
      try {
        if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
        const transaction = createTableVerticalTransaction(state, patch);
        if (!transaction) return false;
        if (dispatch) {
          replayTableRowPaginationTransaction(tr, transaction);
          tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
          if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
        }
        return true;
      } catch (error) {if (dispatch) this.options.onWidthError(String(error)); return false;}
    };
    const cellMarginsCommand = (margins: TableCellMargins | null) => ({state, dispatch, tr}: CommandProps) => {
      try {
        if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
        const transaction = createTableCellMarginsTransaction(state, margins);
        if (!transaction) return false;
        if (dispatch) {
          replayTableRowPaginationTransaction(tr, transaction);
          tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
          if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
        }
        return true;
      } catch (error) {if (dispatch) this.options.onWidthError(String(error)); return false;}
    };
    const positionCommand = (patch: TablePositionPatch) => ({state, dispatch, tr}: CommandProps) => {
      try {
        if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
        const transaction = createTablePositionTransaction(state, patch);
        if (!transaction) return false;
        if (dispatch) {
          replayTableRowPaginationTransaction(tr, transaction);
          tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
          if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
        }
        return true;
      } catch (error) {if (dispatch) this.options.onWidthError(String(error)); return false;}
    };
    return {...this.parent?.(), setTableRowCantSplit: rowPaginationCommand, setTableCellColors: cellColorsCommand, resetTableCellColors: () => cellColorsCommand({cellFillRgb: null, cellFillMode: null, cellTextColorMode: null}), setTableVerticalLayout: verticalCommand, resetTableVerticalLayout: () => verticalCommand({rowMinHeightTwips: null, cellVerticalAlign: null}), setTableCellMargins: cellMarginsCommand, resetTableCellMargins: () => cellMarginsCommand(null), setTablePosition: positionCommand, resetTablePosition: () => positionCommand({tableAlign: null, tableIndentTwips: null}),
      ...Object.fromEntries(Object.keys(structuralCommands).map(action => [action, () =>
      ({state, dispatch, tr}: CommandProps) => {
        try {
          if (this.options.locked?.() || this.editor && (!this.editor.isEditable || this.editor.view.composing)) return false;
          const transaction = createTableStructureTransaction(state, action as TableStructureAction);
          if (!transaction) return false;
          if (dispatch) {
            replayTableRowPaginationTransaction(tr, transaction);
            tr.setSelection(Selection.fromJSON(tr.doc, transaction.selection.toJSON()));
            if (transaction.storedMarksSet) tr.setStoredMarks(transaction.storedMarks);
          }
          return true;
        } catch (error) { if (dispatch) this.options.onWidthError(String(error)); return false; }
      }]))};
  },
  addProseMirrorPlugins() { return [...(this.parent?.() ?? []), tableWidthGuard(this.options.onWidthError)]; },
}).configure({resizable: false, View: FixedTableView});
