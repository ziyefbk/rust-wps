import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {captureParagraphIndent, createParagraphIndentTransaction, matchesParagraphIndent, paragraphIndentError, paragraphIndentFields,
  parseParagraphIndentPoints, selectedParagraphIndent, type ParagraphIndentField, type ParagraphIndentPatch, type ParagraphIndentToken} from './word-paragraph-indent';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const ids = {indentLeftTwips: 'edit-paragraph-indent-left', indentRightTwips: 'edit-paragraph-indent-right', firstLineIndentTwips: 'edit-paragraph-indent-first'};
export class WordParagraphIndentControls {
  private owner: Editor | null = null;
  private token: ParagraphIndentToken | null = null;
  private editing = false;
  private changed = new Set<ParagraphIndentField>();
  constructor(private readonly options: Options) {
    el('paragraph-indent-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('paragraph-indent-controls').contains(event.relatedTarget)) return;
      this.editing = false; this.changed.clear(); this.update();
    });
    for (const field of paragraphIndentFields) {
      const input = el<HTMLInputElement>(ids[field]);
      input.addEventListener('pointerdown', options.syncSelection);
      input.addEventListener('focus', () => {this.editing = true;});
      input.addEventListener('input', () => {this.editing = true; this.changed.add(field);});
      input.addEventListener('keydown', event => {
        if (event.isComposing || event.keyCode === 229) return;
        if (event.key === 'Enter') {event.preventDefault(); this.apply();}
        if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}
      });
      el(`${ids[field]}-reset`).addEventListener('click', () => this.commit({[field]: null}));
    }
    el('edit-paragraph-indent-apply').addEventListener('click', () => this.apply());
    el('edit-paragraph-indent-reset').addEventListener('click', () => this.commit({indentLeftTwips: null, indentRightTwips: null, firstLineIndentTwips: null}));
  }
  reset() {this.owner = null; this.token = null; this.editing = false; this.changed.clear();}
  cancelEditing() {this.editing = false; this.changed.clear(); this.update();}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token && matchesParagraphIndent(editor.state, this.token) ? editor : null;
  }
  private apply() {
    if (!this.current()) return;
    const patch: ParagraphIndentPatch = {};
    for (const field of paragraphIndentFields) {
      const input = el<HTMLInputElement>(ids[field]), text = input.value.trim();
      if (!text && input.dataset.indentState === 'mixed' && !this.changed.has(field)) continue;
      const value = text ? parseParagraphIndentPoints(field, text) : null;
      if (input.validity.badInput || text && value === null) {this.options.notice(paragraphIndentError, true); return;}
      patch[field] = value;
    }
    this.commit(patch);
  }
  private commit(patch: ParagraphIndentPatch) {
    const editor = this.current(); if (!editor) return;
    try {
      const transaction = createParagraphIndentTransaction(editor.state, patch, this.token!);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
      this.editing = false; this.changed.clear(); editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor();
    if (editor !== this.owner || !editor || !this.token || !matchesParagraphIndent(editor.state, this.token)) {
      this.owner = editor; this.token = editor ? captureParagraphIndent(editor.state) : null; this.editing = false; this.changed.clear();
    }
    const selected = editor ? selectedParagraphIndent(editor.state) : null;
    const disabled = !selected?.supported || this.options.locked();
    for (const field of paragraphIndentFields) {
      const input = el<HTMLInputElement>(ids[field]), value = selected?.fields[field];
      input.disabled = disabled;
      el<HTMLButtonElement>(`${ids[field]}-reset`).disabled = disabled || !value?.explicit;
      if (!this.editing) {
        input.value = value?.value == null ? '' : String(value.value / 20);
        input.placeholder = value?.mixed ? '混合' : '默认';
        input.dataset.indentState = value?.mixed ? 'mixed' : value?.value == null ? 'default' : 'explicit';
      }
    }
    el<HTMLButtonElement>('edit-paragraph-indent-apply').disabled = disabled;
    el<HTMLButtonElement>('edit-paragraph-indent-reset').disabled = disabled || !selected || !paragraphIndentFields.some(field => selected.fields[field].explicit);
    el('edit-paragraph-indent-state').textContent = !selected?.count ? '' : !selected.supported ? '仅限正文、标题和简单顶层表格；列表不支持'
      : selected.count > 1 ? `${selected.count} 个段落；首行负数表示悬挂` : '首行负数表示悬挂';
  }
}
