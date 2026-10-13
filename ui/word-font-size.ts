import {Mark} from '@tiptap/core';
import type {EditorState, Transaction} from '@tiptap/pm/state';
import {createTextMarkTransaction, selectedTextMarkAttribute, textOnlyMarkPlugin} from './word-text-mark';

export function isFontSizeHalfPoints(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 2 && value <= 3276;
}

// The document stores half points. Do not round pasted CSS or an entered size.
function parseDecimalHalfPoints(value: string, numerator: bigint, denominator: bigint): number | null {
  if (value.length > 64 || !/^(?:\d+(?:\.\d+)?|\.\d+)$/.test(value)) return null;
  const [whole, fraction = ''] = value.split('.');
  const scaled = BigInt(`${whole || '0'}${fraction}`) * numerator;
  const divisor = 10n ** BigInt(fraction.length) * denominator;
  if (scaled % divisor !== 0n) return null;
  const halfPoints = scaled / divisor;
  return halfPoints >= 2n && halfPoints <= 3276n ? Number(halfPoints) : null;
}

export function parseFontSizePoints(value: string): number | null {
  return parseDecimalHalfPoints(value.trim(), 2n, 1n);
}

export function parseFontSizeCss(value: string): number | null {
  const match = /^(\d+(?:\.\d+)?|\.\d+)(pt|px)$/i.exec(value.trim());
  if (!match) return null;
  return match[2].toLowerCase() === 'pt'
    ? parseDecimalHalfPoints(match[1], 2n, 1n)
    : parseDecimalHalfPoints(match[1], 3n, 2n);
}

export const FontSize = Mark.create({
  name: 'fontSize',
  // Size wraps sup/sub, so their relative reduction applies to the chosen size.
  priority: 110,
  addAttributes: () => ({halfPoints: {default: null, rendered: false}}),
  parseHTML: () => [{style: 'font-size', getAttrs: value => {
    const halfPoints = parseFontSizeCss(String(value));
    return halfPoints === null ? false : {halfPoints};
  }}],
  renderHTML: ({mark}) => {
    const halfPoints: unknown = mark.attrs.halfPoints;
    return isFontSizeHalfPoints(halfPoints)
      ? ['span', {'data-font-size': String(halfPoints), style: `font-size: ${halfPoints / 2}pt`}, 0]
      : ['span', 0];
  },
  addProseMirrorPlugins: () => [textOnlyMarkPlugin('fontSize')],
});

export function selectedFontSize(state: EditorState): {halfPoints: number | null; mixed: boolean} {
  const {value, mixed} = selectedTextMarkAttribute(state, 'fontSize', 'halfPoints', isFontSizeHalfPoints);
  return {halfPoints: value, mixed};
}

export function createFontSizeTransaction(state: EditorState, halfPoints: number | null): Transaction | null {
  if (halfPoints !== null && !isFontSizeHalfPoints(halfPoints)) throw new Error('字号须为 1–1638 pt，且以 0.5 pt 递增。');
  return createTextMarkTransaction(state, 'fontSize', halfPoints === null ? null : {halfPoints}, '字号');
}
