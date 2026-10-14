import {Extension, type JSONContent} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {Plugin} from '@tiptap/pm/state';
import {isFontFamilyName} from './word-font-family';

export interface FontFamilyMetadataEntry {name: string; family: 'roman' | 'swiss'}
export const FONT_FAMILY_METADATA = 'data-font-family-metadata';
export const fontFamilyMetadataError = '字体类别仅限文档根属性中的 1–256 个完整 name / family 项；字面字体名须有效且 ASCII 大小写不重复，类别仅支持 roman 或 swiss。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
const asciiFold = (name: string) => name.replace(/[A-Z]/g, letter => letter.toLowerCase());

export function isFontFamilyMetadata(value: unknown): value is FontFamilyMetadataEntry[] {
  if (!Array.isArray(value) || !value.length || value.length > 256) return false;
  const names = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)
      || Object.keys(entry).length !== 2 || !own(entry, 'name') || !own(entry, 'family')
      || !isFontFamilyName(entry.name) || !['roman', 'swiss'].includes(entry.family)) return false;
    const name = asciiFold(entry.name);
    if (names.has(name)) return false;
    names.add(name);
  }
  return true;
}

// This is authored document intent, not a cache of currently used run fonts.
// Unused entries and exact source spelling/order survive every content edit.
export function validateFontFamilyMetadataDocument(document: JSONContent): void {
  if (document.type !== 'doc') throw new Error('文档根节点必须为 doc。');
  const stack = [{node: document, depth: 0}]; let count = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('字体类别文档结构超出保存上限。');
    if (node.attrs && own(node.attrs, 'fontFamilyMetadata')
      && (node !== document || !isFontFamilyMetadata(node.attrs.fontFamilyMetadata))) throw new Error(fontFamilyMetadataError);
    if (node.marks?.some(mark => mark.attrs && own(mark.attrs, 'fontFamilyMetadata'))) throw new Error(fontFamilyMetadataError);
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}

export function copyFontFamilyMetadata(value: unknown): FontFamilyMetadataEntry[] {
  if (!isFontFamilyMetadata(value)) throw new Error(fontFamilyMetadataError);
  return value.map(({name, family}) => ({name, family}));
}

// Copy before the application's first await/schema load. Freeze only our new
// internal copies, never the caller's model; snapshots get detached copies.
export function prepareFontFamilyMetadataDocument(document: JSONContent): JSONContent {
  validateFontFamilyMetadataDocument(document);
  if (!document.attrs || !own(document.attrs, 'fontFamilyMetadata')) return document;
  const entries = copyFontFamilyMetadata(document.attrs.fontFamilyMetadata);
  for (const entry of entries) Object.freeze(entry);
  return {...document, attrs: {...document.attrs, fontFamilyMetadata: Object.freeze(entries)}};
}

// Explicit external null is invalid. This helper is only for schema snapshots,
// where the root null is an internal default; nested nulls are never stripped.
export function omitDefaultFontFamilyMetadata(document: JSONContent): JSONContent {
  if (document.type !== 'doc' || !document.attrs || !own(document.attrs, 'fontFamilyMetadata')) return document;
  const attrs = {...document.attrs};
  if (attrs.fontFamilyMetadata === null) delete attrs.fontFamilyMetadata;
  else attrs.fontFamilyMetadata = copyFontFamilyMetadata(attrs.fontFamilyMetadata);
  const result: JSONContent = {...document, attrs};
  if (!Object.keys(attrs).length) delete result.attrs;
  return result;
}

export function validateFontFamilyMetadataState(document: Node): void {
  validateFontFamilyMetadataDocument(omitDefaultFontFamilyMetadata(document.toJSON()));
}

export function fontFamilyMetadataGuard(onError: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction(transaction, state) {
    if (!transaction.docChanged) return true;
    try {
      // ProseMirror replaces an explicit undefined docAttr value with the
      // schema default. Check supplied root values before that erasure; null
      // alone is the intentional internal unset value.
      for (const step of transaction.steps) {
        const supplied = step.toJSON();
        if (supplied.stepType === 'docAttr' && supplied.attr === 'fontFamilyMetadata'
          && supplied.value !== null && !isFontFamilyMetadata(supplied.value)) throw new Error(fontFamilyMetadataError);
      }
      validateFontFamilyMetadataState(transaction.doc);
      // HTML/clipboard content may have different metadata, but cannot replace
      // the destination document's authored root through a paste transaction.
      if ((transaction.getMeta('paste') || transaction.getMeta('uiEvent') === 'paste')
        && JSON.stringify(transaction.doc.attrs.fontFamilyMetadata) !== JSON.stringify(state.doc.attrs.fontFamilyMetadata)) {
        throw new Error('粘贴不能更改目标文档的字体类别。');
      }
      return true;
    } catch (error) {onError(String(error)); return false;}
  }});
}

export const WordFontFamilyMetadata = Extension.create<{onError: (message: string) => void}>({
  name: 'wordFontFamilyMetadata',
  addOptions: () => ({onError: () => {}}),
  addGlobalAttributes: () => [{types: ['doc'], attributes: {fontFamilyMetadata: {
    default: null, rendered: false, parseHTML: () => null,
  }}}],
  addProseMirrorPlugins() {return [fontFamilyMetadataGuard(this.options.onError)];},
});

// Deliberately inert DOM metadata: neither font names nor CSS fallback stacks
// are rewritten. Clipboard parsing never reads this root attribute.
export function fontFamilyMetadataDocumentDOM(attrs: Record<string, unknown>): Record<string, string> {
  return isFontFamilyMetadata(attrs.fontFamilyMetadata) ? {[FONT_FAMILY_METADATA]: JSON.stringify(attrs.fontFamilyMetadata)} : {};
}

export function fontFamilyMetadataPreviewWarning(document: JSONContent): string {
  return isFontFamilyMetadata(document.attrs?.fontFamilyMetadata)
    ? 'DOCX 导出会保留字体类别；编辑预览、打印和 PDF 尚未据此调整缺失字体替代，可能与导出的 DOCX 不同。' : '';
}
