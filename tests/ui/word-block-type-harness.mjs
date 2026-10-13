import {getSchema, Extension, Mark, Node as TiptapNode} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';
export const {createWordBlockTypeTransaction} = await loadTsModule('./ui/word-block-type.ts');
const spacing = await loadTsModule('./ui/word-paragraph-spacing.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const tables = await loadTsModule('./ui/word-table-widths.ts');
const images = await loadTsModule('./ui/word-image-dimensions.ts');
const {snapshotDocument} = await loadTsModule('./ui/word-style-profile.ts');
const {FontFamily} = await loadTsModule('./ui/word-font-family.ts');
const {FontSize} = await loadTsModule('./ui/word-font-size.ts');
const {TextColor} = await loadTsModule('./ui/word-text-color.ts');
const WordDoc = TiptapNode.create({name: 'doc', topNode: true, content: 'block+', addAttributes: () => ({styleProfile: {default: null}})});
const Alignment = Extension.create({name: 'paragraphAlignment', addGlobalAttributes: () => [{types: ['paragraph', 'heading'], attributes: {textAlign: {default: null}}}]});
const Superscript = Mark.create({name: 'superscript', excludes: 'subscript'});
const Subscript = Mark.create({name: 'subscript', excludes: 'superscript'});
const PageBreak = TiptapNode.create({name: 'pageBreak', group: 'block', atom: true, selectable: true});
// Mirror the application's supported code + explicit font/size/color marks.
const WordStarterKit = StarterKit.extend({addExtensions() {
  const base = this.parent?.() ?? [];
  const excluded = [...base, Superscript, Subscript].filter(extension => extension.type === 'mark'
    && !['fontSize', 'textColor', 'fontFamily'].includes(extension.name)).map(extension => extension.name).join(' ');
  return base.map(extension => extension.name === 'code' && extension instanceof Mark ? extension.extend({excludes: excluded}) : extension);
}});
export const extensions = [WordStarterKit.configure({document: false, heading: {levels: [1, 2, 3]}}), WordDoc, Alignment,
  Superscript, Subscript, PageBreak, TableKit.configure({table: false}), tables.WordTable, images.WordImage,
  FontFamily, FontSize, TextColor, spacing.ParagraphSpacing, indent.ParagraphIndent, page.WordPageGeometry];
export const schema = getSchema(extensions);
export const snapshot = document => indent.omitDefaultParagraphIndent(images.omitDefaultImageDimensions(tables.omitDefaultTableWidths(
  spacing.omitDefaultParagraphSpacing(snapshotDocument({getJSON: () => document.toJSON()})))));
export const stateFor = (document, selection = TextSelection.atStart(document), storedMarks = null) => EditorState.create({doc: document, selection, storedMarks, plugins: [history()]});
export const paragraph = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
export const heading = (text = '', attrs = {}) => schema.node('heading', {level: 2, ...attrs}, text ? schema.text(text) : null);
export const doc = (content, attrs = {}) => schema.node('doc', attrs, content);
export const blocks = document => {const result = []; document.descendants((node, pos) => {if (node.isTextblock) result.push({node, pos});}); return result;};
