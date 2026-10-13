import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {parseTableCellMarginPoints, tableCellMarginsError, type TableCellMargins} from './word-table-cell-margins';
import {captureTableCellMargins, createTableCellMarginsTransaction, matchesTableCellMargins, selectedTableCellMargins, type TableCellMarginsToken} from './word-table-widths';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const sides = ['left', 'right'] as const;
export class WordTableCellMarginsControls {
  private owner: Editor | null = null;
  private token: TableCellMarginsToken | null = null;
  private editing = false;
  constructor(private readonly options: Options) {
    el('table-cell-margins-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('table-cell-margins-controls').contains(event.relatedTarget)) return;
      this.cancelEditing();
    });
    for (const side of sides) {
      const input = el<HTMLInputElement>(`edit-table-cell-margin-${side}`);
      input.addEventListener('pointerdown', options.syncSelection);
      input.addEventListener('focus', () => {this.editing = true;});
      input.addEventListener('input', () => {this.editing = true;});
      input.addEventListener('keydown', event => {
        if (event.isComposing || event.keyCode === 229) return;
        if (event.key === 'Enter') {event.preventDefault(); this.apply();}
        if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}
      });
    }
    for (const action of ['apply', 'reset']) {
      const button = el(`edit-table-cell-margins-${action}`);
      button.addEventListener('pointerdown', options.syncSelection);
      button.addEventListener('click', () => action === 'apply' ? this.apply() : this.commit(null));
    }
  }
  reset() {this.owner = null; this.token = null; this.editing = false;}
  cancelEditing() {this.editing = false; this.update();}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token && matchesTableCellMargins(editor.state, this.token) ? editor : null;
  }
  private apply() {
    if (!this.current()) return;
    const inputs = sides.map(side => el<HTMLInputElement>(`edit-table-cell-margin-${side}`));
    const values = inputs.map(input => input.value.trim());
    if (inputs.some(input => input.validity.badInput)) {this.options.notice(tableCellMarginsError, true); return;}
    if (values.every(value => !value)) {this.commit(null); return;}
    const [left, right] = values.map(parseTableCellMarginPoints);
    if (left === null || right === null) {this.options.notice(tableCellMarginsError, true); return;}
    this.commit({left, right});
  }
  private commit(margins: TableCellMargins | null) {
    const editor = this.current(); if (!editor) return;
    try {
      const transaction = createTableCellMarginsTransaction(editor.state, margins, this.token!);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
      this.editing = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor();
    if (editor !== this.owner || !editor || !this.token || !matchesTableCellMargins(editor.state, this.token)) {
      this.owner = editor; this.token = editor ? captureTableCellMargins(editor.state) : null; this.editing = false;
    }
    const selected = editor ? selectedTableCellMargins(editor.state) : null;
    const disabled = !selected?.supported || this.options.locked();
    for (const side of sides) {
      const input = el<HTMLInputElement>(`edit-table-cell-margin-${side}`);
      input.disabled = disabled;
      if (!this.editing) {
        input.value = selected?.value ? String(selected.value[side] / 20) : '';
        input.placeholder = selected ? '默认' : '先选表格';
        input.dataset.marginState = selected?.value ? 'explicit' : 'default';
      }
    }
    el<HTMLButtonElement>('edit-table-cell-margins-apply').disabled = disabled;
    el<HTMLButtonElement>('edit-table-cell-margins-reset').disabled = disabled || !selected?.value;
    const legacyIndent = selected?.table.attrs.tableIndentTwips != null && !['center', 'right'].includes(selected.table.attrs.tableAlign);
    el('edit-table-cell-margins-state').textContent = !selected ? '' : !selected.supported ? '仅限正文顶层无合并、无嵌套的矩形表格'
      : `${selected.value ? '' : '默认沿用原显示；DOCX 左右各 5.4 pt。'}当前整表：${legacyIndent ? '改左边距会移动左边框，首列文字起点按原缩进保持' : '改左边距会移动文字起点，左边框位置不变'}；每列须保留正文宽度`;

  }
}
