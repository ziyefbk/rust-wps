import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';

const js = ts.transpileModule(readFileSync('ui/reading-search.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { findReadingMatches } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
const compatible = (text, query) => findReadingMatches(text, query, 'pdf-ligatures');
const pairs = [['ﬀ', 'ff'], ['ﬁ', 'fi'], ['ﬂ', 'fl'], ['ﬃ', 'ffi'], ['ﬄ', 'ffl'], ['ﬅ', 'st'], ['ﬆ', 'st'], ['ſ', 's']];

for (const [source, expanded] of pairs) test(`PDF compatible search projects ${source} symmetrically and preserves source spans`, () => {
  expect(compatible(source, expanded)).toEqual([{ start: 0, end: 1 }]);
  expect(compatible(expanded, source)).toEqual([{ start: 0, end: expanded.length }]);
  expect(compatible(source, source)).toEqual([{ start: 0, end: 1 }]);
  expect(findReadingMatches(source, expanded)).toEqual([]);
  expect(findReadingMatches(expanded, source)).toEqual([]);
});

test('Reading search defaults to exact case-sensitive nonoverlapping literal matches', () => {
  expect(findReadingMatches('aaa AAA ﬃ ffi', 'aa')).toEqual([{ start: 0, end: 2 }]);
  expect(findReadingMatches('aaa AAA ﬃ ffi', 'ffi')).toEqual([{ start: 10, end: 13 }]);
  expect(findReadingMatches('ﬃﬃ', 'ﬃ')).toEqual([{ start: 0, end: 1 }, { start: 1, end: 2 }]);
});

test('PDF compatible search rejects every proper prefix and suffix of an expansion', () => {
  for (const [source, expanded] of pairs) {
    for (let at = 1; at < expanded.length; at++) {
      expect(compatible(source, expanded.slice(0, at))).toEqual([]);
      expect(compatible(source, expanded.slice(at))).toEqual([]);
    }
  }
  expect(compatible('ﬃ', 'f')).toEqual([]);
  expect(compatible('ﬃ', 'fi')).toEqual([]);
});

test('PDF compatible search does not skip later valid hits after invalid partial candidates', () => {
  expect(compatible('ﬀf', 'f')).toEqual([{ start: 1, end: 2 }]);
  expect(compatible('fﬀ', 'ff')).toEqual([{ start: 1, end: 2 }]);
  expect(compatible('ﬃfiﬃ', 'fi')).toEqual([{ start: 1, end: 3 }]);
  expect(compatible('ﬀff', 'ff')).toEqual([{ start: 0, end: 1 }, { start: 1, end: 3 }]);
});

test('PDF compatible search matches across whole expansions with no duplicate or overlapping source spans', () => {
  expect(compatible('ﬀﬁﬃ', 'fiffi')).toEqual([{ start: 1, end: 3 }]);
  expect(compatible('ﬀﬀﬀ', 'ffff')).toEqual([{ start: 0, end: 2 }]);
  expect(compatible('ﬅﬆſt', 'st')).toEqual([{ start: 0, end: 1 }, { start: 1, end: 2 }, { start: 2, end: 4 }]);
  expect(compatible('office oﬃce', 'oﬃce')).toEqual([{ start: 0, end: 6 }, { start: 7, end: 11 }]);
});

test('PDF compatible search preserves astral UTF-16 offsets and rejects surrogate interiors', () => {
  expect(compatible('😀oﬃce😀ffi', 'ffi')).toEqual([{ start: 3, end: 4 }, { start: 8, end: 11 }]);
  expect(compatible('😀ﬃ😀', '😀ffi😀')).toEqual([{ start: 0, end: 5 }]);
  for (const query of ['\ud83d', '\ude00', '\ude00ffi', 'ffi\ud83d']) expect(compatible('😀ﬃ😀', query)).toEqual([]);
  // Existing literal mode deliberately remains a UTF-16 indexOf search.
  expect(findReadingMatches('😀', '\ude00')).toEqual([{ start: 1, end: 2 }]);
  expect(compatible('\ud83d x', '\ud83d')).toEqual([{ start: 0, end: 1 }]);
});

test('PDF compatible search does not add case, whitespace, hyphenation or other Unicode normalization', () => {
  for (const [source, query] of [['FI', 'fi'], ['ſ', 'S'], ['Ａ', 'A'], ['é', 'e\u0301'], ['a  b', 'a b'], ['a\nb', 'a b'], ['hyphen-\nation', 'hyphenation'], ['①', '1'], ['\u00a0', ' ']]) {
    expect(compatible(source, query)).toEqual([]);
  }
});

test('Reading search returns no empty-query matches in either mode', () => {
  for (const mode of ['literal', 'pdf-ligatures']) {
    expect(findReadingMatches('ﬀ x', '', mode)).toEqual([]);
    expect(findReadingMatches('', 'ff', mode)).toEqual([]);
    expect(findReadingMatches('ﬀ', 'ffff', mode)).toEqual([]);
  }
});

test('Reading search caps accepted nonoverlapping original matches at 2000', () => {
  for (const size of [1999, 2000, 2001]) {
    const literal = findReadingMatches('a'.repeat(size), 'a');
    const expanded = compatible('ﬀ'.repeat(size), 'ff');
    expect(literal).toHaveLength(Math.min(size, 2000));
    expect(expanded).toEqual(literal);
  }
  const hits = compatible('ﬀ'.repeat(3000) + 'f'.repeat(2001), 'f');
  expect(hits).toHaveLength(2000);
  expect(hits[0]).toEqual({ start: 3000, end: 3001 });
  expect(hits[1999]).toEqual({ start: 4999, end: 5000 });
});

test('Reading search preserves original HTML-like text and expansion spelling for safe highlighting', () => {
  const text = '<img onerror="ﬃ">😀&ﬀ</img>';
  const matches = compatible(text, 'ffi');
  expect(matches).toEqual([{ start: 14, end: 15 }]);
  expect(matches.map(({ start, end }) => text.slice(start, end))).toEqual(['ﬃ']);
  let offset = 0, reconstructed = '';
  for (const { start, end } of matches) { reconstructed += text.slice(offset, start) + text.slice(start, end); offset = end; }
  expect(reconstructed + text.slice(offset)).toBe(text);
});

const whitespace = (text, query, ligatures = false) => findReadingMatches(text, query, ligatures ? 'pdf-ligatures' : 'literal', true);
for (const run of [' ', '\t', ' \t  ', '\n', '\r', '\r\n', ' \t\r\n\t ']) test(`PDF whitespace compatibility preserves complete ${JSON.stringify(run)} source span`, () => {
  expect(whitespace(`a${run}b`, 'a b')).toEqual([{ start: 0, end: run.length + 2 }]);
  expect(whitespace('a b', `a${run}b`)).toEqual([{ start: 0, end: 3 }]);
  expect(whitespace(`a${run}b`, ' b')).toEqual([{ start: 1, end: run.length + 2 }]);
  expect(whitespace(`a${run}b`, 'a ')).toEqual([{ start: 0, end: run.length + 1 }]);
});

test('PDF whitespace mode rejects blank-line and form-feed barriers symmetrically', () => {
  for (const run of ['\n\n', '\r\r', '\r\n\r\n', '\n \t\r', '\n\r', '\f', ' \f ', '\n\f\n']) {
    expect(whitespace(`a${run}b`, 'a b')).toEqual([]);
    expect(whitespace('a b', `a${run}b`)).toEqual([]);
    expect(whitespace(`a${run}b`, `a${run}b`)).toEqual([]);
    expect(whitespace(`a${run}b`, 'b')).toEqual([{ start: run.length + 1, end: run.length + 2 }]);
  }
});

test('PDF whitespace mode rejects empty and Unicode whitespace-only queries without trimming words', () => {
  for (const query of ['', ' ', '\t', '\r\n', ' \n ', '\f', '\u00a0', '\u2003', '\u2028']) expect(whitespace(`a${query}b`, query)).toEqual([]);
  expect(whitespace('a b', ' a b')).toEqual([]);
  expect(whitespace('a b ', 'a b ')).toEqual([{ start: 0, end: 4 }]);
  expect(findReadingMatches('a  b', ' ')).toHaveLength(2);
});

test('PDF whitespace and ligature options compose only when explicitly enabled', () => {
  const text = '😀oﬃce\r\n\twork';
  expect(whitespace(text, 'office work')).toEqual([]);
  expect(compatible(text, 'office work')).toEqual([]);
  expect(whitespace(text, 'office work', true)).toEqual([{ start: 2, end: text.length }]);
  expect(whitespace(text, 'fi', true)).toEqual([]);
  expect(whitespace('ﬀf\nword', 'f word', true)).toEqual([{ start: 1, end: 7 }]);
});

test('PDF whitespace mode retains astral boundaries and literal Unicode distinctions', () => {
  expect(whitespace('😀\n😀', '😀 😀')).toEqual([{ start: 0, end: 5 }]);
  expect(whitespace('😀\n😀', '\ude00')).toEqual([]);
  for (const [source, query] of [['a\u00a0b', 'a b'], ['a\u2003b', 'a b'], ['a\u2028b', 'a b'], ['a\vb', 'a b'], ['A\nb', 'a b'], ['hyphen-\nation', 'hyphenation'], ['é\nb', 'e\u0301 b']]) expect(whitespace(source, query)).toEqual([]);
});

test('PDF whitespace mode preserves nonoverlap and the 2000-result cap', () => {
  expect(whitespace('a\na\na', 'a a')).toEqual([{ start: 0, end: 3 }]);
  const hits = whitespace('a\t\nb!'.repeat(2001), 'a b');
  expect(hits).toHaveLength(2000); expect(hits[1999]).toEqual({ start: 9995, end: 9999 });
});

test('PDF whitespace mode preserves the genuine tracemonkey title line-wrap text and offsets', () => {
  const title = 'Trace-based Just-in-Time Type Specialization for Dynamic\nLanguages';
  const query = 'Type Specialization for Dynamic Languages';
  expect(findReadingMatches(title, query)).toEqual([]); expect(compatible(title, query)).toEqual([]);
  expect(whitespace(title, query)).toEqual([{ start: 25, end: 66 }]);
  expect(whitespace('\n\n' + title, query)).toEqual([{ start: 27, end: 68 }]);
});

test('PDF whitespace search remains bounded at the extraction text limit', () => {
  const prefix = 'x'.repeat(2 * 1024 * 1024 - 4), text = prefix + 'a\nb!';
  expect(whitespace(text, 'a b')).toEqual([{ start: prefix.length, end: prefix.length + 3 }]);
  expect(whitespace(' \t'.repeat(1024 * 1024), 'word')).toEqual([]);
  expect(whitespace('\r\n'.repeat(1024 * 1024), 'word')).toEqual([]);
});
