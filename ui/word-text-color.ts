import {Mark} from '@tiptap/core';
import type {EditorState, Transaction} from '@tiptap/pm/state';
import {createTextMarkTransaction, selectedTextMarkAttribute, textOnlyMarkPlugin} from './word-text-mark';

export function isTextColorRgb(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9A-F]{6}$/.test(value);
}

export function parseTextColor(value: string): string | null {
  const match = /^#?([0-9a-f]{6})$/i.exec(value.trim());
  return match ? match[1].toUpperCase() : null;
}

// Opaque legacy RGB syntax only. Never preserve arbitrary CSS in the document.
export function parseTextColorCss(value: string): string | null {
  const css = value.trim();
  if (css.length > 128) return null;
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(css);
  if (hex) return (hex[1].length === 3 ? [...hex[1]].map(char => char + char).join('') : hex[1]).toUpperCase();
  const rgb = /^rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)$/i.exec(css)
    ?? /^rgba\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*1(?:\.0+)?\s*\)$/i.exec(css);
  if (!rgb) return null;
  const channels = rgb.slice(1, 4).map(Number);
  return channels.every(channel => channel <= 255)
    ? channels.map(channel => channel.toString(16).padStart(2, '0')).join('').toUpperCase()
    : null;
}

export const TextColor = Mark.create({
  name: 'textColor',
  // Link has priority 1000: the explicit color stays inside its default blue style.
  priority: 105,
  addAttributes: () => ({rgb: {default: null, rendered: false}}),
  parseHTML: () => [{style: 'color', getAttrs: value => {
    const rgb = parseTextColorCss(String(value));
    return rgb === null ? false : {rgb};
  }}],
  renderHTML: ({mark}) => {
    const rgb: unknown = mark.attrs.rgb;
    return isTextColorRgb(rgb)
      ? ['span', {'data-text-color': rgb, style: `color: #${rgb}`}, 0]
      : ['span', 0];
  },
  addProseMirrorPlugins: () => [textOnlyMarkPlugin('textColor')],
});

export function selectedTextColor(state: EditorState): {rgb: string | null; mixed: boolean} {
  const {value, mixed} = selectedTextMarkAttribute(state, 'textColor', 'rgb', isTextColorRgb);
  return {rgb: value, mixed};
}

export function createTextColorTransaction(state: EditorState, rgb: string | null): Transaction | null {
  if (rgb !== null && !isTextColorRgb(rgb)) throw new Error('文字颜色须为六位十六进制 RGB，例如 #C03030。');
  return createTextMarkTransaction(state, 'textColor', rgb === null ? null : {rgb}, '文字颜色');
}
