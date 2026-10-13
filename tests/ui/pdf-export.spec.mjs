import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';

for (const outcome of ['success', 'cancel', 'error']) {
  test(`PDF direct export ${outcome} retains editor and DOCX dirty state`, async ({ page }) => {
    await nativeBridge(page, async command => {
      if (command === 'save_draft') return;
      if (command === 'export_current_pdf') {
        if (outcome === 'error') throw new Error('目标文件已存在');
        return outcome === 'success' ? { path: 'copy.pdf', bytes: 1234 } : null;
      }
      throw new Error(`Unexpected ${command}`);
    });
    await page.goto('/'); await page.locator('#new-document').click();
    await page.locator('#editor-content .tiptap').fill('未保存的 DOCX 内容');
    await page.locator('#edit-export-pdf').click();
    await expect(page.getByRole('status')).toContainText(outcome === 'success' ? '已另存 PDF' : outcome === 'cancel' ? '已取消 PDF 导出' : '目标文件已存在');
    await expect(page.locator('#print-document')).toBeEmpty();
    await expect(page.locator('#draft-state')).toContainText('未导出');
    await expect(page.locator('#edit-export-pdf')).toBeEnabled();
    await page.locator('#editor-content .tiptap').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 继续编辑');
    await expect(page.locator('#editor-content')).toContainText('继续编辑');
  });
}
