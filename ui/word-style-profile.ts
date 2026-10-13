import {documentPageGeometry, omitDefaultPageGeometry} from './word-page-geometry';
import type {Editor, JSONContent} from '@tiptap/core';

export const STANDARD_WORD_STYLE_PROFILE = 'standard-v2' as const;
export type WordStyleProfile = typeof STANDARD_WORD_STYLE_PROFILE | null;

// External models may omit the profile; null is reserved for the editor schema.
export function documentStyleProfile(document: JSONContent): WordStyleProfile {
  if (document.type !== 'doc') throw new Error('文档根节点必须为 doc。');
  const attrs = document.attrs;
  if (attrs === undefined) return null;
  if (!attrs || typeof attrs !== 'object' || Array.isArray(attrs) || Object.keys(attrs).some(key => key !== 'styleProfile' && key !== 'pageGeometry')) {
    throw new Error('文档根属性不受支持。');
  }
  documentPageGeometry(document);
  if (!Object.prototype.hasOwnProperty.call(attrs, 'styleProfile')) return null;
  if (attrs.styleProfile !== STANDARD_WORD_STYLE_PROFILE) throw new Error('不支持的文档默认样式版本。');
  return STANDARD_WORD_STYLE_PROFILE;
}

export function withStandardWordStyle(document: JSONContent): JSONContent {
  documentStyleProfile(document);
  return {...document, attrs: {...document.attrs, styleProfile: STANDARD_WORD_STYLE_PROFILE}};
}

export function editorStyleProfile(owner: Pick<Editor, 'state'>): WordStyleProfile {
  const profile: unknown = owner.state.doc.attrs.styleProfile;
  if (profile === null || profile === undefined) return null;
  if (profile !== STANDARD_WORD_STYLE_PROFILE) throw new Error('不支持的文档默认样式版本。');
  return STANDARD_WORD_STYLE_PROFILE;
}

export function snapshotDocument(owner: Pick<Editor, 'getJSON'>): JSONContent {
  const document = {...omitDefaultPageGeometry(owner.getJSON())};
  if (document.attrs) {
    document.attrs = {...document.attrs};
    if (document.attrs.styleProfile === null) delete document.attrs.styleProfile;
    if (Object.keys(document.attrs).length === 0) delete document.attrs;
  }
  documentStyleProfile(document);
  return document;
}
