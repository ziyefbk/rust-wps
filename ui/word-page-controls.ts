import type {Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {capturePageGeometry, createPageGeometryTransaction, isPageGeometry, matchesPageGeometry, pageGeometryError,
  pageGeometrySheetStyle, pageMarginFields, pagePreset, parsePageMarginPoints, presetPageGeometry,
  type PageGeometry, type PageGeometryToken, type PageOrientation, type PagePreset} from './word-page-geometry';

interface PageControlsOptions {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void}
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const marginIds = {marginTopTwips: 'edit-page-margin-top', marginRightTwips: 'edit-page-margin-right', marginBottomTwips: 'edit-page-margin-bottom', marginLeftTwips: 'edit-page-margin-left'};
const inputs = ['edit-page-size', 'edit-page-orientation', ...Object.values(marginIds)];
const sheetProperties = ['width', 'min-width', 'max-width', 'min-height', 'padding', 'border'];
export class WordPageControls {
  private token: PageGeometryToken | null = null;
  private owner: Editor | null = null;
  private editing = false;
  constructor(private readonly options: PageControlsOptions) {
    el('page-geometry-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('page-geometry-controls').contains(event.relatedTarget)) return;
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
    el('edit-page-apply').addEventListener('click', () => this.apply());
    el('edit-page-reset').addEventListener('click', () => this.commit(null));
  }
  reset() {this.owner = null; this.token = null; this.editing = false; this.updateSheet(null);}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token && matchesPageGeometry(editor.state, this.token) ? editor : null;
  }
  private apply() {
    const editor = this.current(); if (!editor) return;
    const previous: unknown = editor.state.doc.attrs.pageGeometry;
    const preset = el<HTMLSelectElement>('edit-page-size').value as PagePreset;
    const orientation = el<HTMLSelectElement>('edit-page-orientation').value as PageOrientation;
    let page: PageGeometry;
    try {
      if (!['A4', 'Letter', 'A5'].includes(preset) || !['portrait', 'landscape'].includes(orientation)) throw new Error(pageGeometryError);
      if (isPageGeometry(previous) && preset === pagePreset(previous)) {
        const unchanged = (previous.widthTwips < previous.heightTwips ? 'portrait' : 'landscape') === orientation;
        page = {...previous, widthTwips: unchanged ? previous.widthTwips : previous.heightTwips, heightTwips: unchanged ? previous.heightTwips : previous.widthTwips};
      } else page = presetPageGeometry(preset, orientation);
      for (const field of pageMarginFields) {
        const value = parsePageMarginPoints(el<HTMLInputElement>(marginIds[field]).value);
        if (value === null) throw new Error(pageGeometryError);
        page[field] = value;
      }
      if (!isPageGeometry(page)) throw new Error(pageGeometryError);
      this.commit(page);
    } catch (error) {this.options.notice(String(error), true);}
  }
  private commit(page: PageGeometry | null) {
    const editor = this.current(); if (!editor) return;
    try {
      const transaction = createPageGeometryTransaction(editor.state, page, this.token!);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
      this.editing = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  private updateSheet(page: PageGeometry | null) {
    const sheet = el('authoring-sheet');
    for (const property of sheetProperties) sheet.style.removeProperty(property);
    if (!page) {delete sheet.dataset.pageGeometry; return;}
    sheet.dataset.pageGeometry = JSON.stringify(page);
    Object.assign(sheet.style, pageGeometrySheetStyle(page));
  }
  update() {
    const editor = this.options.editor();
    if (editor !== this.owner || !this.token || !editor || !matchesPageGeometry(editor.state, this.token)) {
      this.owner = editor; this.token = editor ? capturePageGeometry(editor.state) : null; this.editing = false;
    }
    const stored: unknown = editor?.state.doc.attrs.pageGeometry;
    const page = isPageGeometry(stored) ? stored : null;
    this.updateSheet(page);
    const disabled = !editor || this.options.locked();
    for (const id of [...inputs, 'edit-page-apply']) el<HTMLInputElement | HTMLButtonElement>(id).disabled = disabled;
    el<HTMLButtonElement>('edit-page-reset').disabled = disabled || !page;
    if (!this.editing) {
      const value = page ?? presetPageGeometry('A4', 'portrait');
      el<HTMLSelectElement>('edit-page-size').value = pagePreset(value)!;
      el<HTMLSelectElement>('edit-page-orientation').value = value.widthTwips < value.heightTwips ? 'portrait' : 'landscape';
      for (const field of pageMarginFields) el<HTMLInputElement>(marginIds[field]).value = String(value[field] / 20);
      el('edit-page-state').textContent = page
        ? `正文 ${(page.widthTwips - page.marginLeftTwips - page.marginRightTwips) / 20} × ${(page.heightTwips - page.marginTopTwips - page.marginBottomTwips) / 20} pt；自动表格随正文宽度重排；固定宽度/定位表格及图片不会自动缩放`
        : editor?.state.doc.attrs.styleProfile === 'standard-v2' ? '默认 A4；尚未设置显式页面' : '旧版默认；打印仍用 20 mm 页边距，应用后采用所填数值';
    }
  }
}
