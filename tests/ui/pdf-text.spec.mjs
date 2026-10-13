import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';

const js = ts.transpileModule(readFileSync('ui/pdf-text.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { extractPdfText, textSummary } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);

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
