import { test, expect } from '@playwright/test';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

const cli = path.resolve(process.env.RUST_WPS_CLI || `target/debug/rust-wps${process.platform === 'win32' ? '.exe' : ''}`);
const python = process.env.PYTHON || (process.platform === 'win32'
  ? path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe')
  : 'python3');

test('hidden-text rejection preserves reading view, previous draft and source after repeated attempts', async ({ page }, info) => {
  const reportPath = execFileSync(python, ['scripts/check-hidden-text.py', '--root', info.outputPath('fixtures')], { encoding: 'utf8' }).trim();
  const report = JSON.parse(readFileSync(reportPath, 'utf8'));
  const source = report.sources.direct.path;
  const original = readFileSync(source);
  const inspection = JSON.parse(execFileSync(cli, ['inspect', source], { encoding: 'utf8' }));
  expect(inspection.text).toContain('VisibleProbe');
  expect(inspection.text).not.toContain('HiddenProbe');
  let importCalls = 0, draftWrites = 0;
  const previousDraft = { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'PreviousDraftProbe' }] }] };
  await nativeBridge(page, async (command) => {
    if (command === 'open_document') return { name: 'hidden.docx', path: source, inspection, authored: null };
    if (command === 'import_current') {
      importCalls++;
      const result = spawnSync(cli, ['import-docx', source], { encoding: 'utf8' });
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('Hidden text cannot be preserved in an editable copy');
      throw new Error(result.stderr);
    }
    if (command === 'load_draft') return previousDraft;
    if (command === 'save_draft') { draftWrites++; return; }
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.locator('#open').click();
  for (let attempt = 1; attempt <= 2; attempt++) {
    await page.locator('#import-document').click();
    await expect(page.locator('#notice')).toContainText('文档含隐藏文字声明');
    await expect(page.locator('#notice')).toContainText('只读正文可能不显示隐藏文字');
    await expect(page.locator('#import-document')).toBeEnabled();
    await expect(page.locator('#title')).toHaveText('hidden.docx');
    await expect(page.locator('#text-content')).toContainText('VisibleProbe');
    await expect(page.locator('#editor-content .tiptap')).toHaveCount(0);
    await expect(page.getByRole('dialog')).not.toBeVisible();
    expect(importCalls).toBe(attempt); expect(draftWrites).toBe(0);
    expect(readFileSync(source).equals(original)).toBe(true);
  }
  await page.locator('#restore-draft').click();
  await expect(page.locator('#editor-content .tiptap')).toContainText('PreviousDraftProbe');
});
