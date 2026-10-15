import type {Editor} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import type {Selection} from '@tiptap/pm/state';
import {closeHistory} from '@tiptap/pm/history';
import {createParagraphSpacingTransaction, formatLineSpacingLines, parseLineSpacingLines, parseParagraphSpacePoints,
  minimumLineHeightSelectionSupported, selectedParagraphSpacing, selectedSpacingParagraphs, type ParagraphSpacingPatch} from './word-paragraph-spacing';
import {isMinimumLineHeight, minimumLineHeightError, minimumLineHeightPreview} from './word-minimum-line-height-model';
interface Options {editor: () => Editor | null; locked: () => boolean; notice: (message: string, error?: boolean) => void; syncSelection: () => void}
type Mode = 'default' | 'multiple' | 'minimum' | 'mixed';
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const ids = {spaceBefore: 'edit-space-before', spaceAfter: 'edit-space-after', lineSpacing: 'edit-line-spacing', minimumLineHeightTwips: 'edit-minimum-line-height'} as const;
type Field = keyof typeof ids;
export function selectedLineHeight(state: Editor['state']) {
  const selected = selectedSpacingParagraphs(state), nodes = [...selected.values()];
  const values = nodes.map(node => isMinimumLineHeight(node.attrs.minimumLineHeightTwips) ? {mode: 'minimum' as Mode, value: node.attrs.minimumLineHeightTwips as number}
    : node.attrs.lineSpacing != null ? {mode: 'multiple' as Mode, value: node.attrs.lineSpacing as number} : {mode: 'default' as Mode, value: null});
  const first = values[0] ?? {mode: 'default' as Mode, value: null};
  const mixed = values.some(value => value.mode !== first.mode || value.value !== first.value);
  return {mode: mixed ? 'mixed' as Mode : first.mode, value: mixed ? null : first.value, count: nodes.length,
    minimumSupported: minimumLineHeightSelectionSupported(state)};
}
export class WordParagraphSpacingControls {
  private owner: Editor | null = null;
  private token: {doc: Node; selection: Selection} | null = null;
  private editing = false;
  private changed = new Set<Field | 'mode'>();
  constructor(private readonly options: Options) {
    const capture = () => {if (!this.editing) {options.syncSelection(); this.update();} this.editing = true;};
    for (const field of Object.keys(ids) as Field[]) {
      const input = el<HTMLInputElement>(ids[field]);
      input.addEventListener('pointerdown', capture); input.addEventListener('focus', capture);
      input.addEventListener('input', () => {
        this.changed.add(field);
        if (field === 'lineSpacing') el<HTMLSelectElement>('edit-line-mode').value = 'multiple';
        if (field === 'minimumLineHeightTwips') el<HTMLSelectElement>('edit-line-mode').value = 'minimum';
      });
      input.addEventListener('keydown', event => {
        if (event.isComposing || event.keyCode === 229) return;
        if (event.key === 'Enter') {event.preventDefault(); this.apply();}
        if (event.key === 'Escape') {event.preventDefault(); this.cancelEditing(); this.options.editor()?.view.focus();}
      });
    }
    const mode = el<HTMLSelectElement>('edit-line-mode');
    mode.addEventListener('pointerdown', capture); mode.addEventListener('focus', capture);
    mode.addEventListener('change', () => {this.changed.add('mode'); this.displayMode(mode.value as Mode);});
    el('paragraph-spacing-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('paragraph-spacing-controls').contains(event.relatedTarget)) return;
      this.cancelEditing();
    });
    el('edit-paragraph-spacing-apply').addEventListener('click', () => this.apply());
    el('edit-paragraph-spacing-reset').addEventListener('click', () => this.commit({spaceBefore: null, spaceAfter: null, lineSpacing: null}));
    el('edit-line-spacing-reset').addEventListener('click', () => this.commit({lineSpacing: null}));
  }
  reset() {this.owner = null; this.token = null; this.editing = false; this.changed.clear();}
  cancelEditing() {this.editing = false; this.changed.clear(); this.update();}
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && this.token && editor.state.doc === this.token.doc && editor.state.selection.eq(this.token.selection)
      && !this.options.locked() && editor.isEditable && !editor.view.composing ? editor : null;
  }
  private apply() {
    if (!this.current()) {this.cancelEditing(); return;}
    const patch: ParagraphSpacingPatch = {};
    for (const field of ['spaceBefore', 'spaceAfter'] as const) {
      const input = el<HTMLInputElement>(ids[field]), value = input.value.trim();
      if (!value && input.dataset.spacingState === 'mixed' && !this.changed.has(field)) continue;
      const parsed = value ? parseParagraphSpacePoints(value) : null;
      if (input.validity.badInput || value && parsed === null) {this.options.notice('段前/段后须为 0–1584 pt（0.05 pt 递增）。', true); return;}
      patch[field] = parsed;
    }
    const mode = el<HTMLSelectElement>('edit-line-mode').value;
    if (this.changed.has('mode') || this.changed.has('lineSpacing') || this.changed.has('minimumLineHeightTwips')) {
      if (mode === 'default') patch.lineSpacing = null;
      else if (mode === 'minimum' || mode === 'multiple') {
        const field = mode === 'minimum' ? 'minimumLineHeightTwips' : 'lineSpacing', input = el<HTMLInputElement>(ids[field]), text = input.value.trim();
        const value = text ? mode === 'minimum' ? parseParagraphSpacePoints(text) : parseLineSpacingLines(text) : null;
        if (input.validity.badInput || text && (value === null || mode === 'minimum' && !isMinimumLineHeight(value))) {this.options.notice(minimumLineHeightError, true); return;}
        patch[field] = value;
      } else if (mode !== 'mixed') {this.options.notice(minimumLineHeightError, true); return;}
    }
    this.commit(patch);
  }
  private commit(patch: ParagraphSpacingPatch) {
    const editor = this.current();
    if (!editor) {this.cancelEditing(); return;}
    try {
      const transaction = createParagraphSpacingTransaction(editor.state, patch);
      if (transaction) {editor.view.dispatch(closeHistory(transaction)); editor.view.dispatch(closeHistory(editor.state.tr));}
      this.editing = false; this.changed.clear(); editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  private displayMode(mode: Mode) {
    el('edit-minimum-line-height-control').hidden = mode !== 'minimum';
    el('edit-multiple-line-spacing-control').hidden = mode === 'minimum';
  }
  update() {
    const editor = this.options.editor(), locked = !editor || this.options.locked() || !editor.isEditable;
    const selected = editor ? selectedParagraphSpacing(editor.state) : null, line = editor ? selectedLineHeight(editor.state) : null;
    for (const field of Object.keys(ids) as Field[]) el<HTMLInputElement>(ids[field]).disabled = locked;
    for (const id of ['edit-line-mode', 'edit-line-spacing-reset', 'edit-paragraph-spacing-apply', 'edit-paragraph-spacing-reset']) el<HTMLButtonElement>(id).disabled = locked;
    const minimumOption = el<HTMLSelectElement>('edit-line-mode').querySelector<HTMLOptionElement>('option[value="minimum"]');
    if (minimumOption) minimumOption.disabled = !line?.minimumSupported;
    if (!this.editing) {
      this.owner = editor; this.token = editor ? {doc: editor.state.doc, selection: editor.state.selection} : null;
      this.changed.clear();
      for (const field of ['spaceBefore', 'spaceAfter', 'lineSpacing'] as const) {
        const input = el<HTMLInputElement>(ids[field]), state = selected?.[field] ?? {value: null, mixed: false};
        input.value = state.value == null ? '' : field === 'lineSpacing' ? formatLineSpacingLines(state.value) : String(state.value / 20);
        input.placeholder = state.mixed ? '混合' : '默认'; input.dataset.spacingState = state.mixed ? 'mixed' : state.value == null ? 'default' : 'explicit';
      }
      const minimum = el<HTMLInputElement>(ids.minimumLineHeightTwips);
      minimum.value = line?.mode === 'minimum' && line.value != null ? String(line.value / 20) : '';
      minimum.placeholder = line?.mode === 'mixed' ? '混合' : '默认';
      el<HTMLSelectElement>('edit-line-mode').value = line?.mode ?? 'default';
      this.displayMode(line?.mode ?? 'default');
    }
    el('edit-line-spacing-state').textContent = line?.mode === 'minimum' ? `最小行高 ${line.value! / 20} pt。${minimumLineHeightPreview}`
      : line?.mode === 'mixed' ? `行距混合；未修改的行距保持原值。${minimumLineHeightPreview}` : minimumLineHeightPreview;
  }
}
