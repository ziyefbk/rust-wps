import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {captureEmptyParagraphFont, createEmptyParagraphFontReset, matchesEmptyParagraphFont, selectedEmptyParagraphFont, type EmptyParagraphFontToken} from './word-empty-paragraph-font';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
export class WordEmptyParagraphFontControls {
  private owner: Editor | null = null;
  private token: EmptyParagraphFontToken | null = null;
  private pending: {owner: Editor | null; token: EmptyParagraphFontToken | null} | null = null;
  constructor(private readonly options: Options) {
    const reset = el<HTMLButtonElement>('edit-empty-paragraph-font-reset');
    reset.addEventListener('pointerdown', () => {
      options.syncSelection(); this.update(); this.pending = {owner: this.owner, token: this.token};
    });
    reset.addEventListener('pointercancel', () => {this.pending = null;});
    reset.addEventListener('keydown', event => {if (event.key === 'Enter' || event.key === ' ') this.pending = {owner: this.owner, token: this.token};});
    reset.addEventListener('click', () => this.commit());
  }
  reset() {this.owner = null; this.token = null; this.pending = null;}
  private commit() {
    const expected = this.pending ?? {owner: this.owner, token: this.token}; this.pending = null;
    const editor = this.options.editor();
    if (!editor || editor !== expected.owner || !expected.token || this.options.locked() || !editor.isEditable
      || editor.view.composing || !matchesEmptyParagraphFont(editor.state, expected.token)) return;
    try {
      const transaction = createEmptyParagraphFontReset(editor.state, expected.token);
      if (!transaction) return;
      editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));
      // A different guard can reject the reset; never report an unapplied edit.
      if (!editor.state.doc.eq(transaction.doc)) return;
      editor.view.focus(); this.update();
      this.options.notice('已恢复当前空段的文档默认字体，可一次撤销；段落间距保持不变。');
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor();
    this.owner = editor; this.token = editor ? captureEmptyParagraphFont(editor.state) : null;
    const selected = editor ? selectedEmptyParagraphFont(editor.state) : null;
    el('empty-paragraph-font-controls').hidden = !selected;
    el<HTMLButtonElement>('edit-empty-paragraph-font-reset').disabled = !selected || !editor?.isEditable || this.options.locked() || !!editor?.view.composing;
    const font = selected?.font;
    el('edit-empty-paragraph-font-state').textContent = font
      ? `导入空段：${font.family} · ${font.halfPoints / 2} pt${font.bold ? ' · 粗体' : ''}${font.italic ? ' · 斜体' : ''}。首次输入或粘贴内容后结束；上方字体控件仍设置后续文字。` : '';
  }
}
