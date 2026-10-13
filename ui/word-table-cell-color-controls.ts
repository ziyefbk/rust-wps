import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {cellAutoColorPreviewWarning, isCellTextColorMode, parseCellFill, tableCellColorsError, type TableCellColorsPatch} from './word-table-cell-colors';
import {captureTableCellMargins, createTableCellColorsTransaction, matchesTableCellMargins, selectedTableCellColors, type TableCellMarginsToken} from './word-table-widths';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
export class WordTableCellColorControls {
  private owner: Editor | null = null;
  private token: TableCellMarginsToken | null = null;
  private pickerOwner: Editor | null = null;
  private pickerToken: TableCellMarginsToken | null = null;
  private editing = false;
  private changed = false;
  constructor(private readonly options: Options) {
    el('table-cell-color-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('table-cell-color-controls').contains(event.relatedTarget)) return;
      this.cancelEditing();
    });
    const input = el<HTMLInputElement>('edit-cell-fill');
    input.addEventListener('pointerdown', options.syncSelection);
    input.addEventListener('focus', () => {this.editing = true;});
    input.addEventListener('input', () => {this.editing = true; this.changed = true;});
    input.addEventListener('keydown', event => {
      if (event.isComposing || event.keyCode === 229) return;
      if (event.key === 'Enter') {event.preventDefault(); this.applyFill();}
      if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}
    });
    const picker = el<HTMLInputElement>('edit-cell-fill-picker');
    const capturePicker = () => {const editor = this.current(); this.pickerOwner = editor; this.pickerToken = editor ? captureTableCellMargins(editor.state) : null;};
    picker.addEventListener('pointerdown', () => {options.syncSelection(); capturePicker();});
    picker.addEventListener('focus', capturePicker);
    picker.addEventListener('input', () => {if (this.currentPicker()) {input.value = picker.value.toUpperCase(); this.editing = true; this.changed = true;}});
    picker.addEventListener('change', () => {
      if (!this.currentPicker()) return;
      input.value = picker.value.toUpperCase(); this.changed = true; this.applyFill(); this.pickerOwner = null; this.pickerToken = null;
    });
    picker.addEventListener('keydown', event => {if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}});
    const mode = el<HTMLSelectElement>('edit-cell-text-color-mode');
    mode.addEventListener('pointerdown', options.syncSelection);
    mode.addEventListener('change', () => {
      if (mode.value === '' || isCellTextColorMode(mode.value)) this.commit({cellTextColorMode: mode.value || null});
    });
    for (const action of ['apply', 'reset']) {
      const button = el(`edit-cell-fill-${action}`);
      button.addEventListener('pointerdown', options.syncSelection);
      button.addEventListener('click', () => action === 'apply' ? this.applyFill() : this.commit({cellFillRgb: null}));
    }
    el('edit-cell-text-color-mode-reset').addEventListener('pointerdown', options.syncSelection);
    el('edit-cell-text-color-mode-reset').addEventListener('click', () => this.commit({cellTextColorMode: null}));
  }
  reset() {this.owner = null; this.token = null; this.pickerOwner = null; this.pickerToken = null; this.editing = false; this.changed = false;}
  cancelEditing() {this.pickerOwner = null; this.pickerToken = null; this.editing = false; this.changed = false; this.update();}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token && matchesTableCellMargins(editor.state, this.token) ? editor : null;
  }
  private currentPicker(): Editor | null {
    const editor = this.current();
    return editor && editor === this.pickerOwner && this.pickerToken && matchesTableCellMargins(editor.state, this.pickerToken) ? editor : null;
  }
  private applyFill() {
    const editor = this.current(); if (!editor) return;
    if (!this.changed && selectedTableCellColors(editor.state)?.cellFillRgb.mixed) return;
    const value = el<HTMLInputElement>('edit-cell-fill').value.trim(), rgb = value ? parseCellFill(value) : null;
    if (value && rgb === null) {this.options.notice(tableCellColorsError, true); return;}
    this.commit({cellFillRgb: rgb});
  }
  private commit(patch: TableCellColorsPatch) {
    const editor = this.current(); if (!editor) return;
    try {
      const transaction = createTableCellColorsTransaction(editor.state, patch, this.token!);
      if (transaction) {editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));}
      this.editing = false; this.changed = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor();
    if (editor !== this.owner || !editor || !this.token || !matchesTableCellMargins(editor.state, this.token)) {
      this.pickerOwner = null; this.pickerToken = null;
      this.owner = editor; this.token = editor ? captureTableCellMargins(editor.state) : null; this.editing = false; this.changed = false;
    }
    const selected = editor ? selectedTableCellColors(editor.state) : null;
    const disabled = !selected?.supported || this.options.locked(), fill = selected?.cellFillRgb, mode = selected?.cellTextColorMode;
    const input = el<HTMLInputElement>('edit-cell-fill'), picker = el<HTMLInputElement>('edit-cell-fill-picker');
    input.disabled = disabled; picker.disabled = disabled;
    if (!this.editing) {
      input.value = fill?.value == null ? '' : `#${fill.value}`;
      input.placeholder = !selected ? '先选表格' : fill?.mixed ? '混合' : '默认';
      input.dataset.fillState = fill?.mixed ? 'mixed' : fill?.value == null ? 'default' : 'explicit';
      picker.value = `#${fill?.value ?? 'FFFFFF'}`;
      picker.title = fill?.mixed ? '选区底色混合；色盘以白色开始' : fill?.value == null ? '当前使用默认底色；色盘以白色开始' : `当前底色 #${fill.value}`;
    }
    const select = el<HTMLSelectElement>('edit-cell-text-color-mode'); select.disabled = disabled;
    select.value = mode?.mixed ? 'mixed' : mode?.value ?? '';
    select.dataset.modeState = mode?.mixed ? 'mixed' : mode?.value == null ? 'default' : 'explicit';
    el<HTMLButtonElement>('edit-cell-fill-apply').disabled = disabled;
    el<HTMLButtonElement>('edit-cell-fill-reset').disabled = disabled || !fill?.hasExplicit;
    el<HTMLButtonElement>('edit-cell-text-color-mode-reset').disabled = disabled || !mode?.hasExplicit;
    el('edit-cell-color-state').textContent = !selected ? '' : !selected.supported ? '仅限正文顶层无合并、无嵌套的矩形表格'
      : `当前 ${selected.cells.length} 格；底色与自动文字色分别设置。${cellAutoColorPreviewWarning}`;
  }
}
