import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { nativeBridge } from './bridge.mjs';

async function setup(page, invalid = false, text = false) {
  const bytes = invalid ? Buffer.from('%PDF-1.7\ninvalid') : readFileSync(`tests/fixtures/${text ? 'encrypted-text' : 'encrypted'}.pdf`);
  const commands = [];
  await nativeBridge(page, async (command, args) => {
    commands.push({ command, args });
    if (command === 'open_document') return { session_token: '1', name: 'encrypted.pdf', path: 'encrypted.pdf', inspection: { format: 'pdf', text: '', text_error: 'Encrypted PDF', bytes: bytes.length, warnings: [] } };
    if (command === 'read_current_pdf') return [...bytes];
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.locator('#open').click();
  return commands;
}

test('real AES encrypted PDF retries wrong password and renders locally', async ({ page }) => {
  const commands = await setup(page);
  await expect(page.locator('#pdf-password')).toBeVisible();
  await page.getByLabel('PDF 密码', { exact: true }).fill('wrong-password');
  await page.getByRole('button', { name: '打开 PDF', exact: true }).click();
  await expect(page.locator('#pdf-password-hint')).toContainText('密码不正确');
  await expect(page.getByLabel('PDF 密码', { exact: true })).toHaveValue('');
  await page.getByLabel('PDF 密码', { exact: true }).fill('test-password');
  await page.getByLabel('PDF 密码', { exact: true }).press('Enter');
  await expect(page.locator('#page-position')).toHaveText('1 / 1');
  await expect(page.locator('canvas')).toHaveAttribute('width', '300');
  await expect(page.getByRole('status')).toContainText('未提取到正文');
  await page.locator('#text-view').click();
  await expect(page.locator('#text-content')).toContainText('未提取到正文');
  await expect(page.locator('#search')).toBeDisabled();
  expect(JSON.stringify(commands)).not.toContain('test-password');
  expect(JSON.stringify(commands)).not.toContain('wrong-password');
});

test('unlocked PDF text becomes searchable across pages without password IPC', async ({ page }) => {
  const commands = await setup(page, false, true);
  await page.getByLabel('PDF 密码', { exact: true }).fill('test-password');
  await page.getByLabel('PDF 密码', { exact: true }).press('Enter');
  await expect(page.getByRole('status')).toContainText('已提取（2/2 页');
  await page.locator('#text-view').click();
  await expect(page.locator('#text-content')).toContainText('First page searchable secret');
  await expect(page.locator('#text-content')).toContainText('Second page searchable text');
  await page.locator('#search').fill('searchable');
  await expect(page.locator('#text-content mark')).toHaveCount(2);
  await page.locator('#page-view').click();
  await page.locator('#next-page').click();
  await expect(page.locator('#page-position')).toHaveText('2 / 2');
  expect(JSON.stringify(commands)).not.toContain('test-password');
});

test('password cancellation leaves file open and allows a fresh retry', async ({ page }) => {
  await setup(page);
  await page.locator('#pdf-password').getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已取消解锁');
  await expect(page.locator('#text-content')).toBeVisible();
  await expect(page.locator('#open')).toBeEnabled();
  await page.locator('#page-view').click();
  await expect(page.locator('#pdf-password')).toBeVisible();
  await page.getByLabel('PDF 密码', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: '打开 PDF', exact: true }).click();
  await expect(page.locator('#page-position')).toHaveText('1 / 1');
});

test('invalid PDF render failure retains text fallback and open controls', async ({ page }) => {
  await setup(page, true);
  await expect(page.getByRole('status')).toContainText('PDF 页面无法打开');
  await expect(page.locator('#text-content')).toContainText('文字提取未成功');
  await expect(page.locator('#open')).toBeEnabled();
  await expect(page.locator('#title')).toHaveText('encrypted.pdf');
});
