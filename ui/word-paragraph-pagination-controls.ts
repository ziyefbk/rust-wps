import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {captureParagraphPagination, createParagraphPaginationTransaction, matchesParagraphPagination, paragraphPaginationFields,
  selectedParagraphPagination, type ParagraphPaginationPatch, type ParagraphPaginationToken} from './word-paragraph-pagination';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const ids = {keepNext: 'edit-keep-next', keepLines: 'edit-keep-lines', pageBreakBefore: 'edit-page-break-before'};
export class WordParagraphPaginationControls {
  private owner: Editor | null = null;
  private token: ParagraphPaginationToken | null = null;
  private pending: {owner: Editor | null; token: ParagraphPaginationToken | null} | null = null;
  constructor(private readonly options: Options) {
    const capture = () => {options.syncSelection(); this.update(); this.pending = {owner: this.owner, token: this.token};};
    for (const field of paragraphPaginationFields) {
      const select = el<HTMLSelectElement>(ids[field]);
      select.addEventListener('pointerdown', capture);
      select.addEventListener('focus', () => {this.pending ??= {owner: this.owner, token: this.token};});
      select.addEventListener('blur', () => {this.pending = null;});
      select.addEventListener('change', () => {
        const value = select.value;
        if (!['default', 'true', 'false'].includes(value)) return;
        this.commit({[field]: value === 'default' ? null : value === 'true'});
      });
    }
    el('edit-pagination-reset').addEventListener('pointerdown', capture);
    el('edit-pagination-reset').addEventListener('click', () => this.commit({keepNext: null, keepLines: null, pageBreakBefore: null}));
  }
  reset() {this.owner = null; this.token = null; this.pending = null;}
  private commit(patch: ParagraphPaginationPatch) {
    const editor = this.options.editor(), expected = this.pending ?? {owner: this.owner, token: this.token};
    this.pending = null;
    if (!editor || editor !== expected.owner || !expected.token || this.options.locked() || !editor.isEditable
      || editor.view.composing || !matchesParagraphPagination(editor.state, expected.token)) {this.update(); return;}
    try {
      const transaction = createParagraphPaginationTransaction(editor.state, patch, expected.token);
      if (transaction) {editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));}
      editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true); this.update();}
  }
  update() {
    const editor = this.options.editor();
    this.owner = editor; this.token = editor ? captureParagraphPagination(editor.state) : null;
    const selected = editor ? selectedParagraphPagination(editor.state) : null;
    const disabled = !selected?.supported || this.options.locked() || !editor?.isEditable;
    for (const field of paragraphPaginationFields) {
      const select = el<HTMLSelectElement>(ids[field]), state = selected?.fields[field];
      select.disabled = disabled;
      select.value = state?.mixed ? 'mixed' : state?.value == null ? 'default' : String(state.value);
      select.dataset.paginationState = state?.mixed ? 'mixed' : state?.value == null ? 'default' : 'explicit';
    }
    el<HTMLButtonElement>('edit-pagination-reset').disabled = disabled || !selected || !paragraphPaginationFields.some(field => selected.fields[field].explicit);
    el('edit-pagination-state').textContent = !selected?.count ? '' : !selected.supported ? '仅限顶层正文和标题'
      : `${selected.count > 1 ? `${selected.count} 个段落；` : ''}打印分页提示；编辑区不分页`;
  }
}
