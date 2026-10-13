import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';

const js = ts.transpileModule(readFileSync('ui/pdf-text.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { extractPdfText, textSummary } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);

const run = (str, x = 0, extra = {}) => ({ str, dir: 'ltr', width: 10, height: 12, transform: [12, 0, 0, 12, x, 100], fontName: 'regular', hasEOL: false, ...extra });
const styles = { regular: { vertical: false }, alternate: { vertical: false }, vertical: { vertical: true } };
async function extractItems(items, options = {}) {
  return extractPdfText({ numPages: 1, getPage: async () => ({ getTextContent: async () => ({ items, styles, ...options }), cleanup() {} }) }, () => false, () => {});
}

test('PDF text assembly keeps explicit space items and leading/trailing run spaces exactly once', async () => {
  const report = await extractItems([run('Keywords'), run(' ', 10), run('JavaScript', 20), run('  next', 30), run('\t\u00a0', 40), run('end', 50)]);
  expect(report.text).toBe('Keywords JavaScript  next\t\u00a0end');
  expect(report.complete).toBe(true);
});

test('PDF text assembly joins proven contiguous different-font runs without Unicode normalization', async () => {
  const report = await extractItems([run('abcdef'), run('fffiflffifflſtst', 10, { fontName: 'alternate' }), run('ghijklmno', 20)]);
  expect(report.text).toBe('abcdeffffiflffifflſtstghijklmno');
  expect(report.text).not.toContain('stst');
});

test('PDF text assembly keeps literal case, punctuation, combining text, and explicit hyphenation', async () => {
  const report = await extractItems([run('Java'), run('Script,e\u0301', 10), run('hyphen-', 20, { hasEOL: true }), run('ation', 0)]);
  expect(report.text).toBe('JavaScript,e\u0301hyphen-\nation');
  expect(report.text.indexOf('javascript')).toBe(-1);
  expect(report.text.indexOf('é')).toBe(-1);
  expect(report.text.indexOf('hyphenation')).toBe(-1);
});

test('PDF text assembly tolerates only bounded coordinate roundoff at a contiguous baseline', async () => {
  expect((await extractItems([run('A'), run('B', 10.005, { transform: [12, 0, 0, 12, 10.005, 100.005] })])).text).toBe('AB');
  expect((await extractItems([run('A'), run('B', 10.02)])).text).toBe('A B');
  expect((await extractItems([run('A', 0, { transform: [0.1, 0, 0, 0.1, 0, 100] }), run('B', 10.001)])).text).toBe('A B');
});

test('PDF text assembly separates forward gaps, backwards jumps, overlapping runs and changed baselines', async () => {
  for (const next of [run('B', 40), run('B', -77.71), run('B', 5), run('B', 10, { transform: [12, 0, 0, 12, 10, 54.06] })]) {
    expect((await extractItems([run('A'), next])).text).toBe('A B');
  }
});

test('PDF text assembly does not infer joins for rotated, skewed, reversed or vertical directions', async () => {
  for (const patch of [
    { dir: 'rtl' }, { dir: 'ttb' }, { dir: '' }, { fontName: 'vertical' }, { fontName: 'missing' },
    { transform: [0, 12, -12, 0, 10, 100] }, { transform: [12, 0.01, 0, 12, 10, 100] },
    { transform: [12, 0, 1, 12, 10, 100] }, { transform: [-12, 0, 0, 12, 10, 100] },
    { transform: [12, 0, 0, 0, 10, 100] }, { transform: [12, 0, 0, -12, 10, 100] },
  ]) expect((await extractItems([run('A'), run('B', 10, patch)])).text).toBe('A B');
  expect((await extractItems([run('A'), run('B', 10)], { styles: undefined })).text).toBe('A B');
});

test('PDF text assembly rejects incomplete, nonnumeric and nonfinite geometry on either run', async () => {
  const patches = [{ transform: [] }, { transform: null }, { transform: [12, 0, 0, 12, 10] }];
  for (let at = 0; at < 6; at++) for (const invalid of [NaN, Infinity, -Infinity, '12']) {
    const transform = [12, 0, 0, 12, 10, 100]; transform[at] = invalid; patches.push({ transform });
  }
  for (const field of ['width', 'height']) for (const invalid of [NaN, Infinity, -1, 0, undefined, '12']) patches.push({ [field]: invalid });
  for (const patch of patches) {
    expect((await extractItems([run('A'), run('B', 10, patch)])).text).toBe('A B');
    expect((await extractItems([run('A', 0, patch), run('B', 10)])).text).toBe('A B');
  }
});

test('PDF text assembly preserves empty EOL markers, repeated line boundaries, and existing page-edge trimming', async () => {
  const report = await extractItems([run('A'), run('', 10, { hasEOL: true }), run('', 10, { hasEOL: true }), run('B', 10), run(' \t', 20, { hasEOL: true })]);
  expect(report.text).toBe('A\n\nB');
});

test('PDF text assembly ignores non-text markers and empty non-EOL runs without losing adjacency', async () => {
  const report = await extractItems([run('A'), { type: 'beginMarkedContent', id: 'section' }, run('', 999), { type: 'endMarkedContent' }, run('B', 10)]);
  expect(report.text).toBe('AB');
  expect((await extractItems([run('A'), run(' '), run('', 999), run('B', 10)])).text).toBe('A B');
  const fragments = Array.from({ length: 20000 }, (_, index) => run('x', index, { width: 1 }));
  expect((await extractItems(fragments)).text).toBe('x'.repeat(fragments.length));
});

test('PDF text assembly never joins runs across page boundaries and keeps failed-page separation', async () => {
  let cleaned = 0;
  const report = await extractPdfText({ numPages: 4, getPage: async n => ({
    getTextContent: async () => { if (n === 3) throw new Error('bad content'); return { items: n === 2 ? [] : [run(String(n))], styles }; },
    cleanup: () => cleaned++,
  }) }, () => false, () => {});
  expect(report.text).toBe('1\n\n\n\n4'); expect(report.failed).toEqual([3]); expect(report.pages).toBe(3);
  expect(cleaned).toBe(4); expect(report.complete).toBe(false);
});

test('PDF text assembly stops before starting or between pages without an extra fetch', async () => {
  let fetched = 0, cleaned = 0, stop = true;
  const pdf = { numPages: 3, getPage: async () => { fetched++; return { getTextContent: async () => ({ items: [run('retained')], styles }), cleanup: () => { cleaned++; stop = true; } }; } };
  const first = await extractPdfText(pdf, () => stop, () => {});
  expect(first.stopped).toBe(true); expect(fetched).toBe(0); expect(first.pages).toBe(0);
  stop = false;
  const second = await extractPdfText(pdf, () => stop, () => {});
  expect(second.text).toBe('retained'); expect(second.stopped).toBe(true); expect(second.pages).toBe(1);
  expect(fetched).toBe(1); expect(cleaned).toBe(1);
});

test('PDF text assembly budget bounds inserted separators and explicit EOL characters', async () => {
  const limit = 2 * 1024 * 1024;
  for (const last of [run('B', 50), run('', 50, { hasEOL: true }), run(' ', 50)]) {
    const report = await extractItems([run('A'.repeat(limit - 2)), last]);
    expect(report.text).toBe('A'.repeat(limit - 2)); expect(report.truncated).toBe(true); expect(report.complete).toBe(false);
  }
});

test('PDF text assembly budget covers page separators and stops further page traversal', async () => {
  let fetched = 0, cleaned = 0;
  const limit = 2 * 1024 * 1024;
  const report = await extractPdfText({ numPages: 3, getPage: async n => { fetched++; return {
    getTextContent: async () => ({ items: [run(n === 1 ? 'A'.repeat(limit - 4) : 'BCDE')], styles }), cleanup: () => cleaned++,
  }; } }, () => false, () => {});
  expect(report.text).toBe('A'.repeat(limit - 4) + '\n\nBC'); expect(report.text.length).toBe(limit);
  expect(report.truncated).toBe(true); expect(report.complete).toBe(false); expect(fetched).toBe(2); expect(cleaned).toBe(2);
});

test('PDF text assembly retains the existing UTF-16 code-unit budget, not a UTF-8 byte claim', async () => {
  const report = await extractItems([run('文'.repeat(2 * 1024 * 1024))]);
  expect(report.text.length).toBe(2 * 1024 * 1024 - 2);
  expect(Buffer.byteLength(report.text, 'utf8')).toBe(3 * report.text.length);
  expect(report.truncated).toBe(true);
});

test('partial page errors retain successful text and clean pages', async () => {
  let cleaned = 0;
  const pdf = { numPages: 3, getPage: async n => {
    if (n === 2) throw new Error('bad page');
    return { getTextContent: async () => ({ items: [{ str: `Page ${n}`, hasEOL: true }] }), cleanup: () => cleaned++ };
  } };
  const report = await extractPdfText(pdf, () => false, () => {});
  expect(report.complete).toBe(false); expect(report.failed).toEqual([2]);
  expect(report.text).toBe('Page 1\n\nPage 3'); expect(cleaned).toBe(2);
  expect(textSummary(report)).toContain('1 页失败');
});

test('stop is honored after an in-flight page finishes without claiming completeness', async () => {
  let stop = false, cleaned = false;
  const pdf = { numPages: 2, getPage: async () => ({ getTextContent: async () => { stop = true; return { items: [{ str: 'pending', hasEOL: true }] }; }, cleanup: () => { cleaned = true; } }) };
  const report = await extractPdfText(pdf, () => stop, () => {});
  expect(report.stopped).toBe(true); expect(report.complete).toBe(false);
  expect(report.text).toBe(''); expect(cleaned).toBe(true);
});

test('oversized last-page text is partial even when every page was visited', async () => {
  const pdf = { numPages: 1, getPage: async () => ({ getTextContent: async () => ({ items: [{ str: 'A'.repeat(3 * 1024 * 1024), hasEOL: true }] }), cleanup: () => {} }) };
  const report = await extractPdfText(pdf, () => false, () => {});
  expect(report.truncated).toBe(true); expect(report.complete).toBe(false);
  expect(report.text.length).toBeLessThanOrEqual(2 * 1024 * 1024);
  expect(textSummary(report)).toContain('达到提取上限');
});

test('page count ceiling reports partial results and bounds traversal', async () => {
  let visited = 0;
  const pdf = { numPages: 1001, getPage: async () => { visited++; return { getTextContent: async () => ({ items: [] }), cleanup: () => {} }; } };
  const report = await extractPdfText(pdf, () => false, () => {});
  expect(visited).toBe(1000); expect(report.complete).toBe(false);
  expect(report.truncated).toBe(true); expect(textSummary(report)).toContain('达到提取上限');
});
