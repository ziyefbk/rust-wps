import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { readFileSync, mkdtempSync } from 'node:fs';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

const cli = path.resolve('target/debug/rust-wps.exe');
const source = path.resolve('tmp/corpus/SampleDoc.docx');
const inspect = file => JSON.parse(execFileSync(cli, ['inspect', file], { encoding: 'utf8' }));

// The UI uses a mocked IPC transport backed by the real Rust CLI. These tests
// verify frontend workflows and core results, not native dialog/WebView IPC.
async function bridge(page, opened, handler) {
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return opened;
    return handler?.(command, args);
  });
}

test('browser preview clearly requires desktop and disables file actions', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: '选择 DOC、DOCX 或 PDF' })).toBeDisabled();
  await expect(page.getByRole('status')).toContainText('npm run desktop');
  await page.screenshot({ path: 'tmp/ui-empty.png' });
});

test('opens real DOCX, searches text, replaces via Rust and keeps the source view', async ({ page }) => {
  const inspection = inspect(source);
  const outputDir = mkdtempSync(path.resolve('tmp/corpus/ui-'));
  const output = path.join(outputDir, 'edited.docx');
  await bridge(page, { name: 'SampleDoc.docx', path: source, inspection }, (command, args) => {
    if (command !== 'replace_current') throw new Error(`Unexpected ${command}`);
    const report = JSON.parse(execFileSync(cli, ['replace', source, output, args.find, args.replacement], { encoding: 'utf8' }));
    return { path: output, report };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '选择 DOC、DOCX 或 PDF' }).click();
  await expect(page.locator('#text-content')).toContainText('I am a test document');
  await page.getByRole('searchbox', { name: '搜索正文' }).fill('page');
  await expect(page.locator('#text-content mark')).toHaveCount(2);
  await page.getByRole('button', { name: '查找替换', exact: true }).click();
  await page.locator('#find').fill('test document');
  await page.locator('#replacement').fill('Rust 办公测试文档');
  await page.getByRole('button', { name: '替换并另存' }).click();
  await expect(page.getByRole('status')).toContainText('已替换 1 处');
  expect(inspect(output).text).toContain('Rust 办公测试文档');
  await expect(page.locator('#text-content')).toContainText('I am a test document');
  await page.screenshot({ path: 'tmp/ui-docx.png' });
});

test('document text is inert and errors keep the document open', async ({ page }) => {
  await bridge(page, { name: '安全.docx', path: source, inspection: { ...inspect(source), text: '<img src=x onerror=alert(1)> 中文正文' } }, () => { throw new Error('磁盘空间不足'); });
  await page.goto('/');
  await page.locator('#open').click();
  await expect(page.locator('#text-content img')).toHaveCount(0);
  await expect(page.locator('#text-content')).toContainText('<img');
  await page.locator('#replace-toggle').click();
  await page.locator('#find').fill('中文');
  await page.locator('#save-replacement').click();
  await expect(page.getByRole('status')).toContainText('磁盘空间不足');
  await expect(page.locator('#text-content')).toContainText('中文正文');
  await expect(page.locator('#open')).toBeEnabled();
});

test('PDF page is rendered locally and zoom updates canvas', async ({ page }) => {
  // Use a compact valid PDF fixture assembled with explicit xref offsets.
  const stream = 'BT /F1 24 Tf 72 720 Td (Rust PDF preview) Tj ET';
  const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>', `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`];
  let bytes = '%PDF-1.4\n'; const offsets = [];
  objects.forEach((o, i) => { offsets.push(bytes.length); bytes += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const xref = bytes.length; bytes += 'xref\n0 6\n0000000000 65535 f \n';
  offsets.forEach(o => { bytes += `${String(o).padStart(10, '0')} 00000 n \n`; });
  bytes += `trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  await bridge(page, { session_token: '1', name: 'preview.pdf', path: 'preview.pdf', inspection: { format: 'pdf', text: 'Rust PDF preview', bytes: bytes.length, warnings: [] } }, () => [...Buffer.from(bytes)]);
  const external = []; page.on('request', r => { if (!r.url().startsWith('http://127.0.0.1:1421') && !r.url().startsWith('blob:') && !r.url().startsWith('data:')) external.push(r.url()); });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator('#page-position')).toHaveText('1 / 1');
  await expect(page.locator('#status')).toHaveText('就绪');
  await expect(page.locator('canvas')).toHaveAttribute('width', '612');
  await page.locator('#zoom').selectOption('1.25');
  await expect(page.locator('canvas')).toHaveAttribute('width', '765');
  await expect(page.locator('#replace-toggle')).toBeDisabled();
  expect(external).toEqual([]);
  await page.screenshot({ path: 'tmp/ui-pdf.png' });
  await page.setViewportSize({ width: 760, height: 600 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(760);
});
