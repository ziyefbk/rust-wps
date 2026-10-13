import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {isCellVerticalAlignment, parseRowMinHeightPoints, tableVerticalError, type TableVerticalPatch} from './word-table-vertical-layout';
import {captureTableCellMargins, createTableVerticalTransaction, matchesTableCellMargins, selectedTableVerticalLayout, type TableCellMarginsToken} from './word-table-widths';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
export class WordTableVerticalControls {
  private owner: Editor | null = null;
  private token: TableCellMarginsToken | null = null;
  private editing = false;
  private changed = false;
  constructor(private readonly options: Options) {
    el('table-vertical-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('table-vertical-controls').contains(event.relatedTarget)) return;
      this.cancelEditing();
    });
    const input = el<HTMLInputElement>('edit-row-min-height');
    input.addEventListener('pointerdown', options.syncSelection);
    input.addEventListener('focus', () => {this.editing = true;});
    input.addEventListener('input', () => {this.editing = true; this.changed = true;});
    input.addEventListener('keydown', event => {
      if (event.isComposing || event.keyCode === 229) return;
      if (event.key === 'Enter') {event.preventDefault(); this.applyHeight();}
      if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}
    });
    const align = el<HTMLSelectElement>('edit-cell-vertical-align');
    align.addEventListener('pointerdown', options.syncSelection);
    align.addEventListener('change', () => {
      if (align.value === '' || isCellVerticalAlignment(align.value)) this.commit({cellVerticalAlign: align.value || null});
    });
    for (const action of ['apply', 'reset']) {
      const button = el(`edit-row-min-height-${action}`);
      button.addEventListener('pointerdown', options.syncSelection);
      button.addEventListener('click', () => action === 'apply' ? this.applyHeight() : this.commit({rowMinHeightTwips: null}));
    }
    el('edit-cell-vertical-align-reset').addEventListener('pointerdown', options.syncSelection);
    el('edit-cell-vertical-align-reset').addEventListener('click', () => this.commit({cellVerticalAlign: null}));
  }
  reset() {this.owner = null; this.token = null; this.editing = false; this.changed = false;}
  cancelEditing() {this.editing = false; this.changed = false; this.update();}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token && matchesTableCellMargins(editor.state, this.token) ? editor : null;
  }
  private applyHeight() {
    const editor = this.current(); if (!editor) return;
    const input = el<HTMLInputElement>('edit-row-min-height'), value = input.value.trim();
    if (input.validity.badInput) {this.options.notice(tableVerticalError, true); return;}
    // An untouched mixed display is not an implicit destructive reset.
    if (!this.changed && selectedTableVerticalLayout(editor.state)?.rowMinHeightTwips.mixed) return;
    const height = value ? parseRowMinHeightPoints(value) : null;
    if (value && height === null) {this.options.notice(tableVerticalError, true); return;}
    this.commit({rowMinHeightTwips: height});
  }
  private commit(patch: TableVerticalPatch) {
    const editor = this.current(); if (!editor) return;
    try {
      const transaction = createTableVerticalTransaction(editor.state, patch, this.token!);
      if (transaction) {editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));}
      this.editing = false; this.changed = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor();
    if (editor !== this.owner || !editor || !this.token || !matchesTableCellMargins(editor.state, this.token)) {
      this.owner = editor; this.token = editor ? captureTableCellMargins(editor.state) : null; this.editing = false; this.changed = false;
    }
    const selected = editor ? selectedTableVerticalLayout(editor.state) : null;
    const disabled = !selected?.supported || this.options.locked();
    const height = selected?.rowMinHeightTwips, align = selected?.cellVerticalAlign;
    const input = el<HTMLInputElement>('edit-row-min-height'); input.disabled = disabled;
    if (!this.editing) {
      input.value = height?.value == null ? '' : String(height.value / 20);
      input.placeholder = !selected ? '先选表格' : height?.mixed ? '混合' : '默认';
      input.dataset.heightState = height?.mixed ? 'mixed' : height?.value == null ? 'default' : 'explicit';
    }
    const select = el<HTMLSelectElement>('edit-cell-vertical-align'); select.disabled = disabled;
    select.value = align?.mixed ? 'mixed' : align?.value ?? '';
    select.dataset.alignState = align?.mixed ? 'mixed' : align?.value == null ? 'default' : 'explicit';
    el<HTMLButtonElement>('edit-row-min-height-apply').disabled = disabled;
    el<HTMLButtonElement>('edit-row-min-height-reset').disabled = disabled || !height?.hasExplicit;
    el<HTMLButtonElement>('edit-cell-vertical-align-reset').disabled = disabled || !align?.hasExplicit;
    el('edit-table-vertical-state').textContent = !selected ? '' : !selected.supported ? '仅限正文顶层无合并、无嵌套的矩形表格'
      : `当前 ${selected.rows.length} 行 / ${selected.cells.length} 格；最小高度允许内容自然增高，默认垂直对齐为顶部`;
  }
}
