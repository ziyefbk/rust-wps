import {Mark} from '@tiptap/core';
import type {EditorState, Transaction} from '@tiptap/pm/state';
import {createTextMarkTransaction, selectedTextMarkAttribute, textOnlyMarkPlugin} from './word-text-mark';

export function isFontFamilyName(value: unknown): value is string {
  if (typeof value !== 'string' || !value || value !== value.trim() || value.length > 31
    || /[\x00-\x1f\x7f-\x9f\ufffe\uffff]/.test(value)) return false;
  // JS length matches Word's UTF-16 limit; require paired surrogate code units.
  for (let index = 0; index < value.length; index++) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(++index);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}

export function fontFamilyCss(name: string): string {
  // Font names are literal CSS strings, never declarations or identifiers.
  return `"${name.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

const genericFamilies = new Set(['serif', 'sans-serif', 'monospace', 'cursive', 'fantasy', 'system-ui',
  'ui-serif', 'ui-sans-serif', 'ui-monospace', 'ui-rounded', 'emoji', 'math', 'fangsong',
  'inherit', 'initial', 'unset', 'revert', 'revert-layer']);
const cssSpace = (character: string) => /^[\t\n\f\r ]$/.test(character);

function cssEscape(value: string, start: number): {text: string; next: number} | null {
  if (start + 1 >= value.length || /[\n\r\f]/.test(value[start + 1])) return null;
  const hex = /^[0-9a-f]{1,6}/i.exec(value.slice(start + 1));
  if (!hex) return {text: value[start + 1], next: start + 2};
  const scalar = Number.parseInt(hex[0], 16);
  if (!scalar || scalar > 0x10ffff || scalar >= 0xd800 && scalar <= 0xdfff) return null;
  let next = start + 1 + hex[0].length;
  if (cssSpace(value[next] ?? '')) {
    if (value[next] === '\r' && value[next + 1] === '\n') next++;
    next++;
  }
  return {text: String.fromCodePoint(scalar), next};
}

export function parseFontFamilyCss(value: string): string | null {
  if (!value || value.length > 4096) return null;
  let index = 0;
  const families: {name: string; quoted: boolean}[] = [];
  while (index < value.length) {
    while (cssSpace(value[index] ?? '')) index++;
    const quote = value[index] === '"' || value[index] === "'" ? value[index++] : null;
    let name = '';
    if (quote) {
      let closed = false;
      while (index < value.length) {
        if (value[index] === quote) { index++; closed = true; break; }
        if (value[index] === '\\') {
          const escaped = cssEscape(value, index); if (!escaped) return null;
          name += escaped.text; index = escaped.next;
        } else {
          if (/[\n\r\f]/.test(value[index])) return null;
          name += value[index++];
        }
      }
      if (!closed) return null;
      while (cssSpace(value[index] ?? '')) index++;
    } else {
      // Bare families contain CSS identifiers. Escaped punctuation stays literal;
      // functions, comments and declaration punctuation are never document data.
      const words: string[] = [];
      while (index < value.length && value[index] !== ',') {
        let word = '', rawStart = true;
        while (index < value.length && value[index] !== ',' && !cssSpace(value[index])) {
          const character = value[index];
          if (character === '\\') {
            const escaped = cssEscape(value, index); if (!escaped) return null;
            word += escaped.text; index = escaped.next;
          } else {
            if (!/[A-Za-z0-9_\-\u0080-\uffff]/.test(character)
              || rawStart && /[0-9]/.test(character)
              || rawStart && character === '-' && /[0-9]/.test(value[index + 1] ?? '')) return null;
            word += character; index++;
          }
          rawStart = false;
        }
        if (!word || word === '-') return null;
        words.push(word);
        while (cssSpace(value[index] ?? '')) index++;
      }
      name = words.join(' ');
    }
    if (!name || index < value.length && value[index] !== ',') return null;
    families.push({name, quoted: quote !== null});
    if (index < value.length && ++index === value.length) return null;
  }
  const first = families.find(family => family.quoted || !genericFamilies.has(family.name.toLowerCase()));
  return first && isFontFamilyName(first.name) ? first.name : null;
}

export const FontFamily = Mark.create({
  name: 'fontFamily',
  // Inside code (100), whose default Consolas yields to an explicit family.
  priority: 90,
  addAttributes: () => ({name: {default: null, rendered: false}}),
  parseHTML: () => [{style: 'font-family', getAttrs: value => {
    const name = parseFontFamilyCss(String(value));
    return name === null ? false : {name};
  }}],
  renderHTML: ({mark}) => {
    const name: unknown = mark.attrs.name;
    return isFontFamilyName(name)
      ? ['span', {'data-font-family': name, style: `font-family: ${fontFamilyCss(name)}`}, 0]
      : ['span', 0];
  },
  addProseMirrorPlugins: () => [textOnlyMarkPlugin('fontFamily')],
});

export function selectedFontFamily(state: EditorState): {name: string | null; mixed: boolean} {
  const {value, mixed} = selectedTextMarkAttribute(state, 'fontFamily', 'name', isFontFamilyName);
  return {name: value, mixed};
}

export function createFontFamilyTransaction(state: EditorState, name: string | null): Transaction | null {
  if (name !== null && !isFontFamilyName(name)) throw new Error('字体名须为 1–31 个 UTF-16 字符，不含首尾空白、控制字符或无效 Unicode。');
  return createTextMarkTransaction(state, 'fontFamily', name === null ? null : {name}, '字体');
}
