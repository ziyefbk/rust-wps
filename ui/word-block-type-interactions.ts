import {withEmptyParagraphFontRendering} from './word-empty-paragraph-font';
import {omitDefaultEmptyParagraphFont, validateEmptyParagraphFontDocument} from './word-empty-paragraph-font-model';
import {InputRule, Node as TiptapNode, type Editor} from '@tiptap/core';
import {closeHistory} from '@tiptap/pm/history';
import {EditorState, Plugin, PluginKey, TextSelection, type Transaction} from '@tiptap/pm/state';
import {Transform} from '@tiptap/pm/transform';
import {createWordBlockTypeTransaction, validateWordBlockTypeDocument, wordBlockTypeIsActive} from './word-block-type';

export interface WordBlockTypeInteractionOptions {
  locked?: () => boolean;
  syncSelection?: () => void;
  onError?: (message: string) => void;
}

const inputRuleMeta = 'wordBlockTypeInputRule';
const inputBoundaryMeta = 'wordBlockTypeInputBoundary';
const unavailable = (editor: Editor, options: WordBlockTypeInteractionOptions) =>
  options.locked?.() || !editor.isEditable || editor.view.composing;

function shortcut(editor: Editor, options: WordBlockTypeInteractionOptions, level: number): boolean {
  if (unavailable(editor, options)) return false;
  try {
    options.syncSelection?.();
    const target = level && wordBlockTypeIsActive(editor.state, level) ? 0 : level;
    const transaction = createWordBlockTypeTransaction(editor.state, target);
    if (transaction) {
      editor.view.dispatch(closeHistory(transaction));
      editor.view.dispatch(closeHistory(editor.state.tr));
    }
  } catch (error) {options.onError?.(String(error));}
  // Consume a rejected request too: never fall through to an unsafe conversion.
  return true;
}

type InputUndo = {transform: Transform; from: number; to: number; text: string};
type InputHistory = {root: Transaction; source: Plugin; undo: InputUndo; restored: boolean} | null;

function inputHistoryPlugin(): Plugin<InputHistory> {
  return new Plugin<InputHistory>({
    key: new PluginKey('wordBlockTypeInputHistory'),
    state: {
      init: () => null,
      apply(transaction, previous, oldState) {
        if (transaction.getMeta(inputRuleMeta) && transaction.getMeta('inputRule')) {
          const source = oldState.plugins.find(plugin => (plugin.spec as {isInputRules?: boolean}).isInputRules && transaction.getMeta(plugin));
          return source ? {root: transaction, source, undo: transaction.getMeta(source) as InputUndo, restored: false} : null;
        }
        if (!previous) return null;
        if (transaction.getMeta(inputBoundaryMeta) === previous.root) return {...previous, restored: true};
        if (transaction.getMeta('appendedTransaction') === previous.root) {
          if (!transaction.steps.length) return previous;
          // Preserve automatic trailing paragraphs and other changes appended
          // to this conversion as part of the same reversible input rule.
          const transform = new Transform(previous.undo.transform.before);
          for (const step of previous.undo.transform.steps) transform.step(step);
          for (const step of transaction.steps) transform.step(step);
          return {...previous, undo: {...previous.undo, transform}, restored: false};
        }
        // Ordinary typing, selection changes, undo or a different root ends the
        // input-rule window. Never resurrect metadata for an unrelated edit.
        return transaction.docChanged || transaction.selectionSet ? null : previous;
      },
    },
    appendTransaction(_transactions, _oldState, newState) {
      const pending = this.getState(newState);
      if (!pending || pending.restored) return null;
      const transaction = closeHistory(newState.tr).setMeta(pending.source, pending.undo).setMeta(inputBoundaryMeta, pending.root);
      if (pending.root.storedMarks !== null) transaction.setStoredMarks(pending.root.storedMarks);
      return transaction;
    },
  });
}

// Replace only the two nodes' user interactions. Schema, HTML and upstream
// programmatic commands remain unchanged; no duplicate node or keymap fallback.
export function withWordBlockTypeInteractions(node: TiptapNode, options: WordBlockTypeInteractionOptions = {}): TiptapNode {
  node = withEmptyParagraphFontRendering(node);
  if (node.name === 'paragraph') return node.extend({
    addKeyboardShortcuts() {return {'Mod-Alt-0': () => shortcut(this.editor, options, 0)};},
  });
  if (node.name !== 'heading') return node;
  return node.extend({
    addKeyboardShortcuts() {
      return Object.fromEntries([1, 2, 3].map(level => [`Mod-Alt-${level}`, () => shortcut(this.editor, options, level)]));
    },
    addInputRules() {
      return [1, 2, 3].map(level => new InputRule({
        find: new RegExp(`^(#{${level}})\\s$`),
        handler: ({state, range, match}) => {
          if (unavailable(this.editor, options)) return null;
          try {
            validateEmptyParagraphFontDocument(omitDefaultEmptyParagraphFont(state.doc.toJSON()));
            const $from = state.doc.resolve(range.from), $to = state.doc.resolve(range.to);
            if (!$from.sameParent($to) || range.from !== $from.start()
              || !['paragraph', 'heading'].includes($from.parent.type.name)
              || !new RegExp(`^#{${level}}\\s$`).test(match[0])) return null;
            const marker = state.doc.textBetween(range.from, range.to);
            // Tiptap may pass an already inserted trigger (typed text or IME),
            // or just the hashes (Enter and direct registered-handler callers).
            if (marker !== '#'.repeat(level) && marker !== match[0]) return null;
            const marks = state.storedMarks ?? $to.marks();
            const candidate = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
            candidate.delete(range.from, range.to);
            candidate.setSelection(TextSelection.create(candidate.doc, range.from));
            candidate.setStoredMarks(marks);
            const converted = createWordBlockTypeTransaction(EditorState.create({
              doc: candidate.doc, selection: candidate.selection, storedMarks: candidate.storedMarks,
            }), level);
            if (converted) for (const step of converted.steps) candidate.step(step);
            // An existing same-level heading still deletes a marker, so the
            // no-type-change case must pass the same full candidate validation.
            if (!converted) validateWordBlockTypeDocument(candidate.doc);
            candidate.setSelection(TextSelection.create(candidate.doc, range.from));
            candidate.setStoredMarks(marks);
            // Commit only after the complete detached candidate was accepted.
            const transaction = state.tr;
            if (!transaction.doc.eq(state.doc)) return null;
            for (const step of candidate.steps) transaction.step(step);
            transaction.setSelection(TextSelection.create(transaction.doc, range.from)).setStoredMarks(marks).setMeta(inputRuleMeta, true);
          } catch (error) {
            options.onError?.(String(error));
            return null;
          }
        },
      }));
    },
    addProseMirrorPlugins() {
      return [...(this.parent?.() ?? []), inputHistoryPlugin()];
    },
  });
}
