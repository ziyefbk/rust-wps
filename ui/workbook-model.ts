export type Content = { type: 'empty' } | { type: 'text' | 'formula'; value: string } | { type: 'number'; value: number } | { type: 'boolean'; value: boolean };
import { updateWorksheetReferences } from './worksheet-references';
import { translateFormula } from './formula-references';
export interface EditableCell { row: number; column: number; content: Content; number_format?: string | null }
export interface EditableWorkbook { version: number; date1904?: boolean; sheets: { name: string; hidden?: boolean; cells: EditableCell[] }[] }
interface Change { sheet: number; key: string; before?: EditableCell; after?: EditableCell; beforeState: number; afterState: number }
interface Transaction { changes: Change[]; bytes: number; workbook?: { before: EditableWorkbook; after: EditableWorkbook; beforeState: number; afterState: number } }
export interface CellInput { row: number; column: number; type: Content['type']; input: string }
const key = (row: number, column: number) => `${row}:${column}`;
const copy = <T>(value: T): T => structuredClone(value);
const cellBytes = (cell: EditableCell | undefined) => cell ? new TextEncoder().encode((cell.content.type === 'text' || cell.content.type === 'formula' ? cell.content.value : '') + (cell.number_format ?? '')).length : 0;

export function parseCellInput(type: Content['type'], input: string): Content {
  if (type === 'empty') return { type };
  if (type === 'number') {
    if (!input.trim() || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(input.trim()) || !Number.isFinite(Number(input))) throw new Error('请输入有限数值');
    return { type, value: Number(input) };
  }
  if (type === 'boolean') {
    if (!/^(true|false)$/i.test(input.trim())) throw new Error('布尔值请输入 true 或 false');
    return { type, value: input.trim().toLowerCase() === 'true' };
  }
  const value = type === 'formula' && input.startsWith('=') ? input.slice(1) : input;
  if (/[^\t\n\r\u0020-\ud7ff\ue000-\ufffd\u{10000}-\u{10ffff}]/u.test(value)) throw new Error('内容含无法保存的字符');
  if (type === 'formula' && (!value.trim() || value.startsWith('=') || value.length > 8192)) throw new Error('公式为空或超过长度限制');
  if (type === 'text' && value.length > 32767) throw new Error('文本超过单元格长度限制');
  return { type, value };
}

/** Cell history is sparse; worksheet operations retain bounded workbook snapshots. */
export class WorkbookEditor {
  private book: EditableWorkbook;
  private cells: Map<string, EditableCell>[];
  private past: Transaction[] = [];
  private future: Transaction[] = [];
  private state = 0;
  private nextState = 0;
  private savedState = 0;
  private textBytes = 0;
  constructor(model: EditableWorkbook) {
    if (model.version !== 1 || !model.sheets.length || model.sheets.length > 100) throw new Error('不支持的工作簿模型');
    if (model.date1904 !== undefined && typeof model.date1904 !== 'boolean') throw new Error('日期系统标记无效');
    this.book = copy(model);
    this.cells = this.book.sheets.map(sheet => new Map(sheet.cells.map(cell => [key(cell.row, cell.column), cell])));
    this.textBytes = this.book.sheets.reduce((n, sheet) => n + new TextEncoder().encode(sheet.name).length, 0);
    for (const sheet of this.cells) for (const cell of sheet.values()) this.textBytes += cellBytes(cell);
  }
  get dirty() { return this.state !== this.savedState; }
  get sheetCount() { return this.book.sheets.length; }
  sheetHidden(index: number) { return this.book.sheets[index]?.hidden ?? false; }
  get canUndo() { return this.past.length > 0; }
  get canRedo() { return this.future.length > 0; }
  get revision() { return this.state; }
  markSaved(revision: number) { this.savedState = revision; }
  markUnsaved() { this.savedState = -1; }
  getCell(sheet: number, row: number, column: number) { return copy(this.cells[sheet]?.get(key(row, column))); }
  snapshot(): EditableWorkbook {
    return { version: 1, ...(this.book.date1904 ? { date1904: true } : {}), sheets: this.book.sheets.map((sheet, index) => ({ name: sheet.name, hidden: sheet.hidden ?? false,
      cells: [...this.cells[index].values()].sort((a, b) => a.row - b.row || a.column - b.column).map(copy) })) };
  }
  private replace(model: EditableWorkbook) {
    this.book = copy(model);
    this.cells = this.book.sheets.map(sheet => new Map(sheet.cells.map(cell => [key(cell.row, cell.column), cell])));
    this.textBytes = this.book.sheets.reduce((n, sheet) => n + new TextEncoder().encode(sheet.name).length, 0);
    for (const sheet of this.cells) for (const cell of sheet.values()) this.textBytes += cellBytes(cell);
  }
  private editSheets(change: (book: EditableWorkbook) => void) {
    const before = this.snapshot(), after = copy(before); change(after);
    if (!after.sheets.length || after.sheets.length > 100 || !after.sheets.some(sheet => !sheet.hidden)) throw new Error('须保留至少一张可见工作表，最多 100 张');
    const names = new Set<string>(); let bytes = 0;
    for (const sheet of after.sheets) {
      if (!sheet.name.trim() || sheet.name.length > 31 || /[\[\]:*?/\\]/.test(sheet.name) || sheet.name.startsWith("'") || sheet.name.endsWith("'")) throw new Error('工作表名称无效');
      parseCellInput('text', sheet.name);
      if (names.has(sheet.name.toLowerCase())) throw new Error('工作表名称重复'); names.add(sheet.name.toLowerCase());
      bytes += new TextEncoder().encode(sheet.name).length;
      for (const cell of sheet.cells) { bytes += cellBytes(cell); if (cell.content.type === 'formula') parseCellInput('formula', cell.content.value); }
    }
    if (bytes > 4 * 1024 * 1024) throw new Error('工作簿文字和公式超过 4 MiB');
    if (JSON.stringify(before) === JSON.stringify(after)) return false;
    const workbook = { before, after, beforeState: this.state, afterState: this.nextState + 1 };
    const transaction: Transaction = { changes: [], workbook, bytes: new TextEncoder().encode(JSON.stringify(workbook)).length };
    if (transaction.bytes > 32 * 1024 * 1024) throw new Error('工作表操作超过撤销历史大小限制');
    this.nextState++; this.replace(after); this.state = workbook.afterState;
    this.past.push(transaction); this.prune(); this.future = []; return true;
  }
  addSheet(name: string) { return this.editSheets(book => { book.sheets.push({ name, hidden: false, cells: [] }); }); }
  setSheetHidden(index: number, hidden: boolean) {
    if (!Number.isInteger(index) || !this.book.sheets[index]) throw new Error('工作表不存在');
    return this.editSheets(book => { book.sheets[index].hidden = hidden; });
  }
  moveSheet(index: number, destination: number) {
    if (!Number.isInteger(index) || !Number.isInteger(destination) || !this.book.sheets[index] || !this.book.sheets[destination]) throw new Error('工作表位置无效');
    if (index === destination) return false;
    return this.editSheets(book => {
      const names = book.sheets.map(sheet => sheet.name);
      for (const sheet of book.sheets) for (const cell of sheet.cells) if (cell.content.type === 'formula') cell.content.value = updateWorksheetReferences(cell.content.value, names, index, undefined, destination);
      const [sheet] = book.sheets.splice(index, 1); book.sheets.splice(destination, 0, sheet);
    });
  }
  renameSheet(index: number, name: string) {
    if (!this.book.sheets[index]) throw new Error('工作表不存在');
    if (this.book.sheets[index].name === name) return false;
    return this.editSheets(book => {
      const names = book.sheets.map(sheet => sheet.name);
      for (const sheet of book.sheets) for (const cell of sheet.cells) if (cell.content.type === 'formula') cell.content.value = updateWorksheetReferences(cell.content.value, names, index, name);
      book.sheets[index].name = name;
    });
  }
  deleteSheet(index: number) {
    if (!this.book.sheets[index]) throw new Error('工作表不存在');
    return this.editSheets(book => {
      const names = book.sheets.map(sheet => sheet.name); book.sheets.splice(index, 1);
      for (const sheet of book.sheets) for (const cell of sheet.cells) if (cell.content.type === 'formula') cell.content.value = updateWorksheetReferences(cell.content.value, names, index);
    });
  }
  private prune() { while (this.past.length > 100 || this.past.reduce((n, item) => n + item.bytes, 0) > 32 * 1024 * 1024) this.past.shift(); }
  setCell(sheet: number, row: number, column: number, type: Content['type'], input: string) {
    return this.setCells(sheet, [{ row, column, type, input }]);
  }
  sortRange(sheet: number, range: { top: number; left: number; bottom: number; right: number }, column: number, descending = false, header = false, formulaValues = new Map<number, Content>()) {
    const { top, left, bottom, right } = range;
    if (!this.cells[sheet] || ![top, left, bottom, right, column].every(Number.isInteger) || top < 0 || left < 0 || bottom < top || right < left || bottom >= 1048576 || right >= 16384 || column < left || column > right || (bottom - top + 1) * (right - left + 1) > 10000) throw new Error('排序选区无效或超过 10000 个单元格');
    const start = top + Number(header);
    const rows = Array.from({ length: Math.max(0, bottom - start + 1) }, (_, index) => {
      const row = start + index;
      let content = this.cells[sheet].get(key(row, column))?.content ?? { type: 'empty' } as Content;
      if (content.type === 'formula') {
        const result = formulaValues.get(row);
        if (!result || result.type === 'formula') throw new Error('排序列含未计算或失败公式，请先计算公式');
        content = parseCellInput(result.type, result.type === 'empty' ? '' : String(result.value));
      }
      return { row, content };
    });
    const rank = (value: Content) => value.type === 'number' ? 0 : value.type === 'text' ? 1 : value.type === 'boolean' ? 2 : 3;
    const collator = new Intl.Collator('zh-CN', { numeric: false, sensitivity: 'variant' });
    rows.sort((a, b) => {
      const x = a.content, y = b.content, rx = rank(x), ry = rank(y);
      // Empty positions remain last in either direction; equal keys keep source order.
      if (rx === 3 || ry === 3) return rx === ry ? a.row - b.row : rx === 3 ? 1 : -1;
      let order = rx - ry;
      if (!order && x.type === 'number' && y.type === 'number') order = x.value < y.value ? -1 : x.value > y.value ? 1 : 0;
      if (!order && x.type === 'text' && y.type === 'text') order = collator.compare(x.value, y.value);
      if (!order && x.type === 'boolean' && y.type === 'boolean') order = Number(x.value) - Number(y.value);
      return order ? order * (descending ? -1 : 1) : a.row - b.row;
    });
    const changes: Change[] = [];
    rows.forEach((source, index) => {
      const row = start + index;
      for (let column = left; column <= right; column++) {
        const address = key(row, column), before = this.cells[sheet].get(address);
        const after = copy(this.cells[sheet].get(key(source.row, column)));
        if (after) {
          after.row = row;
          if (after.content.type === 'formula') after.content = parseCellInput('formula', translateFormula(after.content.value, row - source.row, 0));
        }
        if (JSON.stringify(before) !== JSON.stringify(after)) changes.push({ sheet, key: address, before: copy(before), after, beforeState: this.state, afterState: this.nextState + 1 });
      }
    });
    return this.commitCells(changes);
  }
  setNumberFormat(sheet: number, range: { top: number; left: number; bottom: number; right: number }, format: string | null) {
    const { top, left, bottom, right } = range;
    if (!this.cells[sheet] || ![top, left, bottom, right].every(Number.isInteger) || top < 0 || left < 0 || bottom < top || right < left || bottom >= 1048576 || right >= 16384 || (bottom - top + 1) * (right - left + 1) > 10000) throw new Error('格式选区无效或超过 10000 个单元格');
    if (format !== null) {
      if (!format.length || new TextEncoder().encode(format).length > 1024) throw new Error('格式代码须为 1 至 1024 字节');
      parseCellInput('text', format);
    }
    const changes: Change[] = [];
    for (let row = top; row <= bottom; row++) for (let column = left; column <= right; column++) {
      const address = key(row, column), before = this.cells[sheet].get(address);
      if ((before?.number_format ?? null) === format) continue;
      const after: EditableCell | undefined = format === null && (!before || before.content.type === 'empty') ? undefined : {
        row, column, content: copy(before?.content ?? { type: 'empty' }), number_format: format,
      };
      changes.push({ sheet, key: address, before: copy(before), after, beforeState: this.state, afterState: this.nextState + 1 });
    }
    return this.commitCells(changes);
  }
  setCells(sheet: number, inputs: CellInput[]) {
    if (!this.cells[sheet] || inputs.length > 10000) throw new Error('批量操作最多 10000 个单元格');
    const changes: Change[] = [], addresses = new Set<string>();
    for (const { row, column, type, input } of inputs) {
    if (!Number.isInteger(row) || !Number.isInteger(column) || row < 0 || column < 0 || row >= 1048576 || column >= 16384) throw new Error('地址超出工作表范围');
    const content = parseCellInput(type, input), address = key(row, column), before = this.cells[sheet].get(address);
    if (addresses.has(address)) throw new Error('批量操作含重复地址'); addresses.add(address);
    const after: EditableCell | undefined = content.type === 'empty' && !before?.number_format ? undefined : { row, column, content, number_format: before?.number_format ?? null };
    if (JSON.stringify(before) === JSON.stringify(after)) continue;
    changes.push({ sheet, key: address, before: copy(before), after: copy(after), beforeState: this.state, afterState: this.nextState + 1 });
    }
    return this.commitCells(changes);
  }
  private commitCells(changes: Change[]) {
    if (!changes.length) return false;
    const count = this.cells.reduce((n, cells) => n + cells.size, 0) + changes.reduce((n, change) => n + Number(!!change.after) - Number(!!change.before), 0);
    if (count > 100000) throw new Error('工作簿超过单元格数量限制');
    let textBytes = this.textBytes;
    for (const change of changes) textBytes += cellBytes(change.after) - cellBytes(change.before);
    if (textBytes > 4 * 1024 * 1024) throw new Error('工作簿文字和公式超过 4 MiB');
    const transaction = { changes, bytes: new TextEncoder().encode(JSON.stringify(changes)).length };
    if (transaction.bytes > 32 * 1024 * 1024) throw new Error('单元格操作超过撤销历史大小限制');
    this.nextState++;
    for (const change of changes) this.apply(change, true);
    this.past.push(transaction);
    this.prune();
    this.future = [];
    return true;
  }
  private apply(change: Change, forward: boolean) {
    this.textBytes += (cellBytes(change.after) - cellBytes(change.before)) * (forward ? 1 : -1);
    const value = forward ? change.after : change.before;
    if (value) this.cells[change.sheet].set(change.key, copy(value)); else this.cells[change.sheet].delete(change.key);
    this.state = forward ? change.afterState : change.beforeState;
  }
  undo() { const transaction = this.past.pop(); if (!transaction) return false; if (transaction.workbook) { this.replace(transaction.workbook.before); this.state = transaction.workbook.beforeState; } else for (const change of transaction.changes) this.apply(change, false); this.future.push(transaction); return true; }
  redo() { const transaction = this.future.pop(); if (!transaction) return false; if (transaction.workbook) { this.replace(transaction.workbook.after); this.state = transaction.workbook.afterState; } else for (const change of transaction.changes) this.apply(change, true); this.past.push(transaction); return true; }
}
