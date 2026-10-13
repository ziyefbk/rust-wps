import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {captureImageSelection, createImageParagraphTransaction, matchesImageSelection, selectedWordImage, type ImageSelectionToken} from './word-image-dimensions';
import {defaultImageParagraph, imageParagraphError, isImageParagraph, type ImageParagraph} from './word-image-paragraph';
import {formatLineSpacingLines, parseLineSpacingLines, parseParagraphSpacePoints} from './word-paragraph-spacing';

interface ImageParagraphControlsOptions {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const inputs = ['edit-image-space-before', 'edit-image-space-after', 'edit-image-line-spacing', 'edit-image-alignment'];
export class WordImageParagraphControls {
  private token: ImageSelectionToken | null = null;
  private owner: Editor | null = null;
  private editing = false;
  constructor(private readonly options: ImageParagraphControlsOptions) {
    el('image-paragraph-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('image-paragraph-controls').contains(event.relatedTarget)) return;
      this.editing = false; this.update();
    });
    for (const id of inputs) {
      el(id).addEventListener('focus', () => {this.editing = true;});
      el(id).addEventListener('input', () => {this.editing = true;});
      el(id).addEventListener('keydown', event => {
        if (event.isComposing || event.keyCode === 229) return;
        if (event.key === 'Enter') {event.preventDefault(); this.apply();}
        if (event.key === 'Escape') {event.preventDefault(); this.editing = false; this.options.editor()?.view.focus(); this.update();}
      });
    }
    el('edit-image-paragraph-apply').addEventListener('click', () => this.apply());
    el('edit-image-paragraph-reset').addEventListener('click', () => this.commit(null));
  }
  reset() {this.token = null; this.owner = null; this.editing = false;}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token
      && matchesImageSelection(editor.state, this.token) ? editor : null;
  }
  private apply() {
    if (!this.current()) return;
    const before = parseParagraphSpacePoints(el<HTMLInputElement>(inputs[0]).value);
    const after = parseParagraphSpacePoints(el<HTMLInputElement>(inputs[1]).value);
    const line = parseLineSpacingLines(el<HTMLInputElement>(inputs[2]).value);
    // Preserve the complete imported font pair. These controls change layout;
    // only an explicit reset discards captured source baseline font metadata.
    const previous: unknown = this.token!.image.node.attrs.imageParagraph;
    const paragraph = {...(isImageParagraph(previous) ? previous : {}), spaceBefore: before, spaceAfter: after,
      lineSpacing: line, textAlign: el<HTMLSelectElement>(inputs[3]).value};
    if (!isImageParagraph(paragraph)) {
      this.options.notice(`${imageParagraphError} 段前/段后为 0–1584 pt（0.05 pt 递增），行距为 0.5–10 行，也可输入 241/240。`, true); return;
    }
    this.commit(paragraph);
  }
  private commit(paragraph: ImageParagraph | null) {
    const editor = this.current();
    if (!editor) return;
    try {
      const transaction = createImageParagraphTransaction(editor.state, paragraph, this.token!);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
        if (paragraph === null) this.options.notice('已移除图片段落格式和基线字体，恢复文档默认图片布局；位置和后续正文间距可能改变，可撤销。');
      }
      this.editing = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor(), selected = editor ? selectedWordImage(editor.state) : null;
    if (editor !== this.owner || !this.token || !editor || !matchesImageSelection(editor.state, this.token)) {
      this.owner = editor; this.token = editor ? captureImageSelection(editor.state) : null; this.editing = false;
    }
    const disabled = this.options.locked() || !selected?.supported;
    const stored: unknown = selected?.node.attrs.imageParagraph;
    const paragraph = isImageParagraph(stored) ? stored : null;
    for (const id of [...inputs, 'edit-image-paragraph-apply']) el<HTMLInputElement | HTMLButtonElement>(id).disabled = disabled;
    el<HTMLButtonElement>('edit-image-paragraph-reset').disabled = disabled || !paragraph;
    if (!this.editing) {
      const value = selected?.supported ? paragraph ?? defaultImageParagraph(editor?.state.doc.attrs.styleProfile) : null;
      el<HTMLInputElement>(inputs[0]).value = value ? String(value.spaceBefore / 20) : '';
      el<HTMLInputElement>(inputs[1]).value = value ? String(value.spaceAfter / 20) : '';
      el<HTMLInputElement>(inputs[2]).value = value ? formatLineSpacingLines(value.lineSpacing) : '';
      el<HTMLSelectElement>(inputs[3]).value = value?.textAlign ?? 'left';
      el('edit-image-paragraph-state').textContent = !selected ? '' : !selected.supported ? '仅支持正文顶层单图段落'
        : !paragraph ? '默认图片布局；应用后启用图片段落行框'
        : paragraph.paragraphFont ? '已保留图片段落和基线字体；缺失字体可能替代'
        : '已设置图片段落；基线字体使用文档默认值';
    }
  }
}
