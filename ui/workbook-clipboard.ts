const MAX = 10000;
import type { Content } from './workbook-model';
export const CELL_MIME = 'application/x-rust-wps-cells+json';
export function decodeTypedClipboard(text: string): { rows: Content[][]; origin?: { row: number; column: number } } {
  if (new TextEncoder().encode(text).length > 4 * 1024 * 1024) throw new Error('剪贴板超过 4 MiB');
  const payload = JSON.parse(text);
  if (![1, 2].includes(payload?.version) || !Array.isArray(payload.rows) || !payload.rows.length || payload.rows.length > MAX) throw new Error('剪贴板模型版本或行数无效');
  const width = payload.rows[0]?.length;
  if (!Number.isInteger(width) || width < 1 || width * payload.rows.length > MAX) throw new Error('剪贴板矩形超过限制');
  if (payload.version === 2 && (!Number.isInteger(payload.origin?.row) || !Number.isInteger(payload.origin?.column)
    || payload.origin.row < 0 || payload.origin.column < 0 || payload.origin.row + payload.rows.length > 1048576 || payload.origin.column + width > 16384)) throw new Error('剪贴板源地址无效');
  for (const row of payload.rows) {
    if (!Array.isArray(row) || row.length !== width) throw new Error('剪贴板不是矩形');
    for (const cell of row) {
      if (!cell || typeof cell !== 'object') throw new Error('剪贴板单元格无效');
      switch (cell.type) {
        case 'empty': break;
        case 'number': if (typeof cell.value !== 'number' || !Number.isFinite(cell.value)) throw new Error('剪贴板数值无效'); break;
        case 'boolean': if (typeof cell.value !== 'boolean') throw new Error('剪贴板布尔值无效'); break;
        case 'text': case 'formula': if (typeof cell.value !== 'string') throw new Error('剪贴板文字无效'); break;
        default: throw new Error('剪贴板单元格类型无效');
      }
    }
  }
  return { rows: payload.rows, ...(payload.version === 2 ? { origin: payload.origin } : {}) };
}
export function decodeTypedCells(text: string): Content[][] { return decodeTypedClipboard(text).rows; }
export function encodeTsv(rows: string[][]): string {
  return rows.map(row => row.map(text => /[\t\r\n"]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text).join('\t')).join('\r\n');
}
/** Quoted tabs/newlines and doubled quotes; a final line terminator adds no row. */
export function decodeTsv(text: string): string[][] {
  if (new TextEncoder().encode(text).length > 4 * 1024 * 1024) throw new Error('剪贴板超过 4 MiB');
  const rows: string[][] = []; let row: string[] = [], field = '', quoted = false, endedQuote = false, count = 0;
  const pushField = () => { row.push(field); field = ''; endedQuote = false; if (++count > MAX) throw new Error('粘贴最多 10000 个单元格'); };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') { if (text[i + 1] === '"') { field += '"'; i++; } else { quoted = false; endedQuote = true; } }
      else field += c;
    } else if (c === '\t' || c === '\r' || c === '\n') {
      pushField();
      if (c !== '\t') { rows.push(row); row = []; if (c === '\r' && text[i + 1] === '\n') i++; }
    } else if (c === '"' && !field && !endedQuote) quoted = true;
    else { if (endedQuote) throw new Error('引号结束后含非法字符'); field += c; }
  }
  if (quoted) throw new Error('剪贴板引号未闭合');
  if (row.length || field || endedQuote || !rows.length || text.endsWith('\t')) { pushField(); rows.push(row); }
  const width = Math.max(...rows.map(row => row.length));
  if (width * rows.length > MAX) throw new Error('粘贴最多 10000 个单元格');
  return rows.map(row => [...row, ...Array<string>(width - row.length).fill('')]);
}
