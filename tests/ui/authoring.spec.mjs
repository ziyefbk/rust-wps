import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

test('table rows and columns edit, undo and export a rectangular DOCX', async ({ page }) => {
  const directory = mkdtempSync(path.resolve('tmp/corpus/table-'));
  const output = path.join(directory, 'table.docx');
  const cli = path.resolve('target/debug/rust-wps.exe');
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const model = path.join(directory, 'model.json'); writeFileSync(model, JSON.stringify(args.doc));
      return { path: output, report: JSON.parse(execFileSync(cli, ['create-docx', model, output], { encoding: 'utf8' })) };
    }
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.locator('#new-document').click();
  await expect(page.locator('#edit-column')).toBeDisabled();
  await page.locator('#edit-table').click();
  const table = page.locator('#editor-content table');
  await table.locator('th').first().click(); await page.keyboard.insertText('保留内容');
  await page.locator('#edit-column').click();
  await expect(table.locator('tr').first().locator('th,td')).toHaveCount(4);
  await page.locator('#edit-row').click();
  await expect(table.locator('tr')).toHaveCount(4);
  await table.locator('tr').last().locator('th,td').last().click();
  await page.locator('#edit-delete-column').click();
  await expect(table.locator('tr').first().locator('th,td')).toHaveCount(3);
  await page.locator('#edit-delete-row').click();
  await expect(table.locator('tr')).toHaveCount(3);
  await page.locator('#edit-undo').click(); await expect(table.locator('tr')).toHaveCount(4);
  await page.locator('#edit-redo').click(); await expect(table.locator('tr')).toHaveCount(3);
  await expect(table).toContainText('保留内容');
  await expect(page.locator('#draft-state')).toHaveText('草稿已保存在本机');
  const savedTable = draft.content.find(n => n.type === 'table');
  expect(savedTable.content).toHaveLength(3);
  expect(savedTable.content.every(row => row.content.length === 3)).toBe(true);
  await page.locator('#edit-save').click(); await expect(page.getByRole('status')).toContainText('已导出 DOCX');
  const imported = JSON.parse(execFileSync(cli, ['import-docx', output], { encoding: 'utf8' }));
  const importedTable = imported.document.content.find(n => n.type === 'table');
  expect(importedTable.content).toHaveLength(3);
  expect(importedTable.content.every(row => row.content.length === 3)).toBe(true);
  expect(JSON.stringify(importedTable)).toContain('保留内容');
  for (let remaining = 3; remaining > 0; remaining--) {
    await table.locator('tr').first().locator('th,td').last().click();
    await page.locator('#edit-delete-column').click();
  }
  await expect(table).toHaveCount(0);
  await expect(page.locator('#edit-delete-column')).toBeDisabled();
  await page.locator('#edit-undo').click();
  await expect(table).toHaveCount(1);
  await expect(table).toContainText('保留内容');
  const rows = await table.locator('tr').count();
  for (let remaining = rows; remaining > 0; remaining--) {
    await table.locator('tr').last().locator('th,td').first().click();
    await page.locator('#edit-delete-row').click();
  }
  await expect(table).toHaveCount(0);
  await page.locator('#edit-undo').click();
  await expect(table).toContainText('保留内容');
});

test('links edit, autosave and survive real Rust DOCX import', async ({ page }) => {
  const directory = mkdtempSync(path.resolve('tmp/corpus/links-'));
  const output = path.join(directory, 'links.docx');
  const cli = path.resolve('target/debug/rust-wps.exe');
  let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'save_new_document') {
      const model = path.join(directory, 'model.json'); writeFileSync(model, JSON.stringify(args.doc));
      const report = JSON.parse(execFileSync(cli, ['create-docx', model, output], { encoding: 'utf8' }));
      return { path: output, report };
    }
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.getByRole('button', { name: '新建文档', exact: true }).click();
  const editable = page.locator('.tiptap');
  await editable.click(); await page.keyboard.insertText('中文链接'); await page.keyboard.press('Control+a');
  page.once('dialog', dialog => dialog.accept('javascript:alert(1)'));
  await page.getByRole('button', { name: '编辑链接', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('链接须使用');
  await expect(editable.locator('a')).toHaveCount(0);
  const href = 'https://example.com/中文?q=1&v=2';
  page.once('dialog', dialog => dialog.accept(href));
  await page.getByRole('button', { name: '编辑链接', exact: true }).click();
  await expect(editable.locator('a')).toHaveAttribute('href', href);
  await expect(page.locator('#draft-state')).toHaveText('草稿已保存在本机');
  expect(draft.content[0].content[0].marks.some(m => m.type === 'link' && m.attrs.href === href)).toBe(true);
  await editable.locator('a').click(); expect(page.url()).toBe('http://127.0.0.1:1421/');
  await page.getByRole('button', { name: '另存 DOCX', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已导出 DOCX');
  const imported = JSON.parse(execFileSync(cli, ['import-docx', output], { encoding: 'utf8' }));
  expect(imported.document.content[0].content[0].marks.some(m => m.type === 'link' && m.attrs.href === href)).toBe(true);
  page.once('dialog', dialog => dialog.accept(''));
  await page.getByRole('button', { name: '编辑链接', exact: true }).click();
  await expect(editable.locator('a')).toHaveCount(0);
  await expect(editable).toHaveText('中文链接');
});

test('new document edits Chinese, undo, table, autosave and Rust DOCX export', async ({ page }) => {
  const directory = mkdtempSync(path.resolve('tmp/corpus/authoring-'));
  const cli = path.resolve('target/debug/rust-wps.exe');
  const output = path.join(directory, 'edited.docx');
  let draft = null;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_draft') { draft = args.doc; return; }
    if (command === 'load_draft') return draft;
    if (command === 'save_new_document') {
      const model = path.join(directory, 'model.json'); writeFileSync(model, JSON.stringify(args.doc));
      const report = JSON.parse(execFileSync(cli, ['create-docx', model, output], { encoding: 'utf8' }));
      return { path: output, report };
    }
    if (command === 'open_document') return null;
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.getByRole('button', { name: '新建文档', exact: true }).click();
  // ProseMirror sets contenteditable; use its explicit accessible label.
  const editable = page.locator('.tiptap');
  await editable.click(); await page.keyboard.insertText('中文办公测试');
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: '加粗', exact: true }).click();
  await page.keyboard.insertText('加粗内容');
  await expect(editable.locator('strong')).toHaveText('加粗内容');
  await page.getByRole('button', { name: '撤销', exact: true }).click();
  await expect(editable).not.toContainText('加粗内容');
  await page.getByRole('button', { name: '重做', exact: true }).click();
  await expect(editable).toContainText('加粗内容');
  await page.keyboard.press('End'); await page.keyboard.press('Enter');
  await page.getByRole('button', { name: '插入表格', exact: true }).click();
  await expect(editable.locator('table')).toHaveCount(1);
  await editable.locator('th').first().click(); await page.keyboard.insertText('表格内容');
  await expect(page.locator('#draft-state')).toHaveText('草稿已保存在本机');
  expect(draft.content.some(n => n.type === 'table')).toBe(true);
  await page.screenshot({ path: 'tmp/ui-authoring.png' });
  await page.getByRole('button', { name: '另存 DOCX', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已导出 DOCX');
  const inspected = JSON.parse(execFileSync(cli, ['inspect', output], { encoding: 'utf8' }));
  expect(inspected.text).toContain('中文办公测试'); expect(inspected.text).toContain('表格内容');
  // Saved document leaves no dirty confirmation; a new document clears the editor.
  await page.getByRole('button', { name: '新建文档', exact: true }).click();
  await expect(page.locator('.tiptap')).not.toContainText('中文办公测试');
  await page.getByRole('button', { name: '恢复本地草稿', exact: true }).click();
  await expect(page.locator('.tiptap')).toContainText('中文办公测试');
  page.on('dialog', dialog => dialog.dismiss());
  await page.getByRole('button', { name: '新建文档', exact: true }).click();
  await expect(page.locator('.tiptap')).toContainText('中文办公测试');
});

test('opens an authored DOCX in the editor rather than rebuilding imported text', async ({ page }) => {
  const doc = JSON.parse(readFileSync('tests/fixtures/authoring.json', 'utf8'));
  await nativeBridge(page, async command => {
    if (command === 'open_document') return { name: '可编辑.docx', path: 'local.docx', authored: doc, inspection: { format: 'docx', text: 'Rust 办公文档', bytes: 1000, warnings: [] } };
    if (command === 'save_draft') return;
  });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator('#title')).toHaveText('可编辑.docx');
  await expect(page.locator('.tiptap h1')).toHaveText('Rust 办公文档');
  await expect(page.locator('#text-content')).toBeHidden();
  await expect(page.locator('#draft-state')).toHaveText('文档已打开');
  await page.locator('.tiptap h1').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 已编辑');
  await expect(page.locator('.tiptap h1')).toContainText('已编辑');
});

test('alignment consumes pending DOM selection and restores focus before another edit', async ({page})=>{
  const doc=JSON.parse(readFileSync('tests/fixtures/authoring.json','utf8'));
  await nativeBridge(page,async command=>{if(command==='load_draft')return doc;if(command==='save_draft')return;});
  await page.goto('/');await page.getByRole('button',{name:'恢复本地草稿',exact:true}).click();await page.locator('.tiptap h1').click();
  expect(await page.evaluate(()=>{const select=document.getElementById('edit-align');select.focus();select.value='center';select.dispatchEvent(new Event('change',{bubbles:true}));return document.activeElement===document.querySelector('.tiptap');})).toBe(true);
  // Change DOM selection and format in one task, before the browser delivers selectionchange.
  expect(await page.evaluate(()=>{const text=document.querySelector('.tiptap th p').firstChild;window.getSelection().setBaseAndExtent(text,0,text,1);const select=document.getElementById('edit-align');select.value='right';select.dispatchEvent(new Event('change',{bubbles:true}));return document.activeElement===document.querySelector('.tiptap');})).toBe(true);
  await expect(page.locator('.tiptap th p').first()).toHaveCSS('text-align','right');await expect(page.locator('.tiptap h1')).toHaveCSS('text-align','center');
});
test('restore rich fixture and export through the real Rust model validator', async ({ page }) => {
  const directory = mkdtempSync(path.resolve('tmp/corpus/rich-'));
  const doc = JSON.parse(readFileSync('tests/fixtures/authoring.json', 'utf8'));
  const cli = path.resolve('target/debug/rust-wps.exe');
  const output = path.join(directory, 'rich.docx');
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_draft') return doc;
    if (command === 'save_draft') return;
    if (command === 'save_new_document') {
      const model = path.join(directory, 'model.json'); writeFileSync(model, JSON.stringify(args.doc));
      return { path: output, report: JSON.parse(execFileSync(cli, ['create-docx', model, output], { encoding: 'utf8' })) };
    }
  });
  await page.goto('/'); await page.getByRole('button', { name: '恢复本地草稿', exact: true }).click();
  await expect(page.locator('.tiptap h1')).toHaveText('Rust 办公文档');
  await expect(page.locator('.tiptap ol')).toHaveAttribute('start', '3');
  await expect(page.locator('.tiptap table')).toHaveCount(1);
  await page.locator('.tiptap h1').click();
  await page.getByLabel('段落对齐').selectOption('center');
  await expect(page.locator('.tiptap h1')).toHaveCSS('text-align', 'center');
  await page.locator('.tiptap th p').first().click();
  await page.getByLabel('段落对齐').selectOption('right');
  await expect(page.locator('.tiptap th p').first()).toHaveCSS('text-align', 'right');
  await page.getByRole('button', { name: '撤销', exact: true }).click();
  await expect(page.locator('.tiptap th p').first()).not.toHaveCSS('text-align', 'right');
  await page.getByRole('button', { name: '重做', exact: true }).click();
  await expect(page.locator('.tiptap th p').first()).toHaveCSS('text-align', 'right');
  await page.getByRole('button', { name: '另存 DOCX', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已导出 DOCX');
  const inspected = JSON.parse(execFileSync(cli, ['inspect', output], { encoding: 'utf8' }));
  expect(inspected.text).toContain('嵌套列表'); expect(inspected.text).toContain('100');
  const imported = JSON.parse(execFileSync(cli, ['import-docx', output], { encoding: 'utf8' }));
  expect(imported.document.content[0].attrs.textAlign).toBe('center');
  const table = imported.document.content.find(n => n.type === 'table');
  expect(table.content[0].content[0].content[0].attrs.textAlign).toBe('right');
});
