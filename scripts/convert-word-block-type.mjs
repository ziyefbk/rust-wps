// Browser-free verification driver for the same transaction used by the toolbar.
// Usage: node scripts/convert-word-block-type.mjs input.json output.json 0|1|2|3
import {readFileSync, writeFileSync} from 'node:fs';
import {AllSelection} from '@tiptap/pm/state';
import {closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {schema, snapshot, stateFor, createWordBlockTypeTransaction} from '../tests/ui/word-block-type-harness.mjs';
const [input, output, value] = process.argv.slice(2);
if (!input || !output || !/^[0-3]$/.test(value ?? '')) throw new Error('Expected input.json output.json 0|1|2|3');
const original = schema.nodeFromJSON(JSON.parse(readFileSync(input, 'utf8')));
let state = stateFor(original, new AllSelection(original));
const transaction = createWordBlockTypeTransaction(state, Number(value));
if (transaction) {state = state.apply(closeHistory(transaction)); state = state.apply(closeHistory(state.tr));}
const converted = state.doc, captured = snapshot(converted), depth = undoDepth(state);
if (transaction) {
  if (!undo(state, tr => {state = state.apply(tr);}) || !state.doc.eq(original)) throw new Error('Undo failed');
  if (!redo(state, tr => {state = state.apply(tr);}) || !state.doc.eq(converted)) throw new Error('Redo failed');
}
writeFileSync(output, `${JSON.stringify(captured, null, 2)}\n`, {flag: 'wx'});
console.log(JSON.stringify({changed: !!transaction, steps: transaction?.steps.length ?? 0, undoDepth: depth, selection: state.selection.toJSON()}));
