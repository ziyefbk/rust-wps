import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {captureTableCellMargins, createTableRowPaginationTransaction, matchesTableCellMargins,
  selectedTableRowPagination, type TableCellMarginsToken} from './word-table-widths';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
export class WordTableRowPaginationControls {
  private owner: Editor | null = null;
  private token: TableCellMarginsToken | null = null;
  private pending: {owner: Editor | null; token: TableCellMarginsToken | null} | null = null;
  constructor(private readonly options: Options) {
    const select = el<HTMLSelectElement>('edit-row-cant-split');
    select.addEventListener('pointerdown', () => {
      options.syncSelection(); this.update(); this.pending = {owner: this.owner, token: this.token};
    });
    select.addEventListener('focus', () => {this.pending ??= {owner: this.owner, token: this.token};});
    select.addEventListener('blur', () => {this.pending = null;});
    select.addEventListener('change', () => {
      const value = select.value;
      if (['default', 'true', 'false'].includes(value)) this.commit(value === 'default' ? null : value === 'true');
    });
  }
  reset() {this.owner = null; this.token = null; if (this.pending) this.pending = {owner: null, token: null};}
  private commit(value: boolean | null) {
    const editor = this.options.editor(), expected = this.pending ?? {owner: this.owner, token: this.token};
    this.pending = null;
    if (!editor || editor !== expected.owner || !expected.token || this.options.locked() || !editor.isEditable || editor.view.composing
      || !matchesTableCellMargins(editor.state, expected.token)) {this.update(); return;}
    try {
      const transaction = createTableRowPaginationTransaction(editor.state, value, expected.token);
      if (transaction) {editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));}
      editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true); this.update();}
  }
  update() {
    const editor = this.options.editor();
    this.owner = editor; this.token = editor ? captureTableCellMargins(editor.state) : null;
    const selected = editor ? selectedTableRowPagination(editor.state) : null;
    const select = el<HTMLSelectElement>('edit-row-cant-split');
    select.disabled = !selected?.supported || this.options.locked() || !editor?.isEditable || !!editor?.view.composing;
    select.value = selected?.mixed ? 'mixed' : selected?.value == null ? 'default' : String(selected.value);
    select.dataset.rowPaginationState = selected?.mixed ? 'mixed' : selected?.value == null ? 'default' : 'explicit';
    el('edit-row-pagination-state').textContent = !selected ? '' : !selected.supported ? '仅限正文顶层无合并、无嵌套的矩形表格'
      : `当前 ${selected.count} 行；DOCX 保留行内不分页设置，打印仅作提示`;
  }
}
