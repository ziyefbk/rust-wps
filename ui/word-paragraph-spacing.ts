import {approveMinimumLineHeightChange, isMinimumLineHeight, minimumLineHeightData, minimumLineHeightError, minimumLineHeightField, validateMinimumLineHeightDocument} from './word-minimum-line-height-model';
import {Extension, type JSONContent} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {EditorState, type Transaction} from '@tiptap/pm/state';

export const paragraphSpacingFields = ['spaceBefore', 'spaceAfter', 'lineSpacing'] as const;
export type ParagraphSpacingField = typeof paragraphSpacingFields[number];
export type ParagraphSpacingPatch = Partial<Record<ParagraphSpacingField | typeof minimumLineHeightField, number | null>>;
const paragraphTypes = new Set(['paragraph', 'heading']);

export function isParagraphSpace(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 31680;
}

export function isLineSpacing(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 120 && value <= 2400;
}

function validSpacing(field: ParagraphSpacingField, value: unknown): value is number {
  return field === 'lineSpacing' ? isLineSpacing(value) : isParagraphSpace(value);
}

// Exact decimal arithmetic: pasted CSS and typed values must not silently round.
function scaledDecimal(value: string, numerator: bigint, denominator = 1n): number | null {
  if (value.length > 64 || !/^(?:\d+(?:\.\d+)?|\.\d+)$/.test(value)) return null;
  const [whole, fraction = ''] = value.split('.');
  const scaled = BigInt(`${whole || '0'}${fraction}`) * numerator;
  const divisor = 10n ** BigInt(fraction.length) * denominator;
  if (scaled % divisor !== 0n || scaled / divisor > BigInt(Number.MAX_SAFE_INTEGER)) return null;
  return Number(scaled / divisor);
}

export function parseParagraphSpacePoints(value: string): number | null {
  const twips = scaledDecimal(value.trim(), 20n);
  return isParagraphSpace(twips) ? twips : null;
}

export function parseParagraphSpaceCss(value: string): number | null {
  const match = /^(\d+(?:\.\d+)?|\.\d+)(pt|px)$/i.exec(value.trim());
  const twips = match ? scaledDecimal(match[1], match[2].toLowerCase() === 'pt' ? 20n : 15n)
    : value.trim() === '0' ? 0 : null;
  return isParagraphSpace(twips) ? twips : null;
}

export function parseLineSpacingLines(value: string): number | null {
  const normalized = value.trim();
  const fraction = /^(\d{1,4})\s*\/\s*240$/.exec(normalized);
  const units = fraction ? Number(fraction[1]) : scaledDecimal(normalized, 240n);
  return isLineSpacing(units) ? units : null;
}

export function formatLineSpacingLines(units: number): string {
  if (!isLineSpacing(units)) throw new Error('行距须为 120–2400 个 1/240 行单位。');
  // A factor of 3 in the reduced denominator is repeating. Keep the canonical
  // integer in the displayed fraction so even Apply without editing is exact.
  return units % 3 === 0 ? String(units / 240) : `${units}/240`;
}

export function parseLineSpacingCss(value: string): number | null {
  const normalized = value.trim();
  const units = normalized.endsWith('%') ? scaledDecimal(normalized.slice(0, -1), 12n, 5n)
    : scaledDecimal(normalized, 240n);
  return isLineSpacing(units) ? units : null;
}

const dataNames: Record<ParagraphSpacingField, string> = {
  spaceBefore: 'data-space-before', spaceAfter: 'data-space-after', lineSpacing: 'data-line-spacing',
};

export const ParagraphSpacing = Extension.create({
  name: 'paragraphSpacing',
  addGlobalAttributes() {
    return [{types: ['paragraph', 'heading'], attributes: {...Object.fromEntries(paragraphSpacingFields.map(field => [field, {
      default: null,
      parseHTML: (element: HTMLElement) => {
        const exact = element.getAttribute(dataNames[field]);
        if (exact !== null && /^\d{1,5}$/.test(exact) && validSpacing(field, Number(exact))) return Number(exact);
        return field === 'lineSpacing' ? parseLineSpacingCss(element.style.lineHeight)
          : parseParagraphSpaceCss(field === 'spaceBefore' ? element.style.marginTop : element.style.marginBottom);
      },
      renderHTML: (attrs: Record<string, unknown>) => {
        const value = attrs[field];
        if (!validSpacing(field, value)) return {};
        const property = field === 'spaceBefore' ? 'margin-top' : field === 'spaceAfter' ? 'margin-bottom' : 'line-height';
        const css = field === 'lineSpacing' ? String(value / 240) : `${value / 20}pt`;
        // Inline properties survive the immutable print clone and override both
        // legacy and standard profile defaults without changing those defaults.
        return {[dataNames[field]]: String(value), style: `${property}: ${css}`};
      },
    }])), [minimumLineHeightField]: {
      default: null, keepOnSplit: true,
      parseHTML: (element: HTMLElement) => {
        const raw = element.getAttribute(minimumLineHeightData);
        return raw === null ? null : /^[1-9][0-9]{0,4}$/.test(raw) && isMinimumLineHeight(Number(raw)) ? Number(raw) : 'invalid-minimum-line-height';
      },
      // Never emit a fixed CSS line-height for Word's minimum line-height.
      renderHTML: (attrs: Record<string, unknown>) => isMinimumLineHeight(attrs[minimumLineHeightField])
        ? {[minimumLineHeightData]: String(attrs[minimumLineHeightField])} : {},
    }}}];
  },
});

export function validateParagraphSpacingDocument(document: JSONContent): void {
  validateMinimumLineHeightDocument(document);
  const stack = [document];
  while (stack.length) {
    const node = stack.pop()!;
    for (const field of paragraphSpacingFields) {
      if (node.attrs && Object.prototype.hasOwnProperty.call(node.attrs, field)
        && (!paragraphTypes.has(node.type ?? '') || !validSpacing(field, node.attrs[field]))) {
        throw new Error(`不支持的段落间距属性 ${field}；段前/段后须为 0–31680 twip，行距须为 120–2400 个 1/240 行单位。`);
      }
    }
    for (const child of node.content ?? []) stack.push(child);
  }
}

// Null exists only inside the editor schema. Never introduce it into old drafts
// or exported models, and never remove an unrelated pre-existing attribute.
export function omitDefaultParagraphSpacing(document: JSONContent): JSONContent {
  const result = {...document};
  if (paragraphTypes.has(result.type ?? '') && result.attrs) {
    result.attrs = {...result.attrs};
    for (const field of [...paragraphSpacingFields, minimumLineHeightField]) if (result.attrs[field] === null) delete result.attrs[field];
    if (Object.keys(result.attrs).length === 0) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultParagraphSpacing);
  return result;
}

export function selectedSpacingParagraphs(state: EditorState): Map<number, Node> {
  const result = new Map<number, Node>();
  if (state.selection.empty) {
    const {$from} = state.selection;
    for (let depth = $from.depth; depth > 0; depth--) {
      const node = $from.node(depth);
      if (paragraphTypes.has(node.type.name)) { result.set($from.before(depth), node); break; }
    }
    return result;
  }
  for (const {$from, $to} of state.selection.ranges) {
    state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
      if (!paragraphTypes.has(node.type.name)) return;
      // A range ending at the next paragraph's text start does not select it.
      if (pos + 1 < $to.pos && pos + node.nodeSize > $from.pos) result.set(pos, node);
      return false;
    });
  }
  return result;
}

export function minimumLineHeightSelectionSupported(state: EditorState): boolean {
  const selected = selectedSpacingParagraphs(state);
  let supported = !!selected.size && [...selected].every(([pos, node]) => state.doc.resolve(pos).depth === 0
    && (node.type.name !== 'heading' || [1, 2, 3].includes(node.attrs.level))
    && Array.from({length: node.childCount}, (_, index) => node.child(index)).every(child => child.isText || child.type.name === 'hardBreak'));
  if (!state.selection.empty) for (const {$from, $to} of state.selection.ranges) state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
    if (node.isTextblock || node.isAtom) {
      if (pos + (node.isTextblock ? 1 : 0) < $to.pos && pos + node.nodeSize > $from.pos && !paragraphTypes.has(node.type.name)) supported = false;
      return false;
    }
    if (!paragraphTypes.has(node.type.name)) supported = false;
  });
  return supported;
}

export function selectedParagraphSpacing(state: EditorState): Record<ParagraphSpacingField, {value: number | null; mixed: boolean}> {
  const nodes = [...selectedSpacingParagraphs(state).values()];
  return Object.fromEntries(paragraphSpacingFields.map(field => {
    const values = nodes.map(node => validSpacing(field, node.attrs[field]) ? node.attrs[field] as number : null);
    const first = values[0] ?? null, mixed = values.some(value => value !== first);
    return [field, {value: mixed ? null : first, mixed}];
  })) as Record<ParagraphSpacingField, {value: number | null; mixed: boolean}>;
}

const encoder = new TextEncoder();
function checkSpacingBudget(document: Node): void {
  let nodes = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++nodes > 10000 || depth > 32) throw new Error('段落间距修改后文档结构超出保存上限，未修改正文。');
    if (node.isText) {
      bytes += encoder.encode(node.text!).length;
      if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未应用段落间距。');
    }
    for (let index = node.childCount - 1; index >= 0; index--) stack.push({node: node.child(index), depth: depth + 1});
  }
  const snapshot = omitDefaultParagraphSpacing(document.toJSON());
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) {
    throw new Error('段落间距修改后草稿超过 32 MiB 保存上限，未修改正文。');
  }
}

export function createParagraphSpacingTransaction(state: EditorState, patch: ParagraphSpacingPatch): Transaction | null {
  const fields = Object.keys(patch) as (ParagraphSpacingField | typeof minimumLineHeightField)[];
  for (const field of fields) {
    if (!(paragraphSpacingFields as readonly string[]).includes(field) && field !== minimumLineHeightField || patch[field] !== null && !(field === minimumLineHeightField ? isMinimumLineHeight(patch[field]) : validSpacing(field, patch[field]))) {
      throw new Error('段前/段后须为 0–1584 pt（0.05 pt 递增），行距须为 0.5–10 行（1/240 行递增）。');
    }
  }
  if (patch.lineSpacing != null && patch.minimumLineHeightTwips != null) throw new Error(minimumLineHeightError);
  const normalized = {...patch};
  if (Object.prototype.hasOwnProperty.call(patch, 'lineSpacing')) normalized.minimumLineHeightTwips = null;
  if (Object.prototype.hasOwnProperty.call(patch, minimumLineHeightField)) normalized.lineSpacing = null;
  // A numeric minimum wins over an explicit clearing of the old multiple.
  if (patch.minimumLineHeightTwips != null) normalized.minimumLineHeightTwips = patch.minimumLineHeightTwips;
  if (patch.lineSpacing != null) normalized.lineSpacing = patch.lineSpacing;
  const changedFields = Object.keys(normalized) as (keyof ParagraphSpacingPatch)[];
  const selected = selectedSpacingParagraphs(state);
  if (normalized.minimumLineHeightTwips != null && !minimumLineHeightSelectionSupported(state)) throw new Error(minimumLineHeightError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const [pos, node] of selected) {
    if (!changedFields.some(field => (node.attrs[field] ?? null) !== normalized[field])) continue;
    transaction.setNodeMarkup(pos, undefined, {...node.attrs, ...normalized}, node.marks);
  }
  if (!transaction.docChanged) return null;
  // All limits are checked on a prospective document before the caller dispatches.
  validateParagraphSpacingDocument(omitDefaultParagraphSpacing(transaction.doc.toJSON()));
  checkSpacingBudget(transaction.doc);
  approveMinimumLineHeightChange(transaction);
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}
