import type { PDFDocumentProxy } from 'pdfjs-dist';
import type { TextItem, TextStyle } from 'pdfjs-dist/types/src/display/api';

export interface PdfTextReport { text: string; pages: number; total: number; failed: number[]; complete: boolean; stopped: boolean; truncated: boolean }

// Only join an unseparated run when its horizontal LTR geometry proves that
// it continues the previous run. PDF.js can omit hasEOL across diagrams and
// backwards jumps. This is not a reading-order or rotated/RTL layout solver.
function contiguous(previous: TextItem, next: TextItem, styles: Record<string, TextStyle>): boolean {
  if (previous.dir !== 'ltr' || next.dir !== 'ltr' ||
      styles[previous.fontName]?.vertical !== false || styles[next.fontName]?.vertical !== false) return false;
  const a = previous.transform, b = next.transform;
  if (![a, b].every(t => Array.isArray(t) && t.length === 6 && t.every(n => typeof n === 'number' && Number.isFinite(n)) &&
      t[0] > 0 && t[1] === 0 && t[2] === 0 && t[3] > 0) ||
      ![previous.width, previous.height, next.width, next.height].every(n => Number.isFinite(n) && n > 0)) return false;
  // Allow only small coordinate roundoff, capped at 0.01 PDF user units and
  // 0.1% of either run's scale. A changed font name does not imply a space.
  const tolerance = Math.min(0.01, a[0] / 1000, a[3] / 1000, b[0] / 1000, b[3] / 1000);
  return Math.abs(b[5] - a[5]) <= tolerance && Math.abs(b[4] - (a[4] + previous.width)) <= tolerance;
}

// Bound the retained text and sequential work. This is an extraction limit,
// not an OCR or reading-order guarantee.
export async function extractPdfText(pdf: PDFDocumentProxy, cancelled: () => boolean, progress: (page: number) => void): Promise<PdfTextReport> {
  const report: PdfTextReport = { text: '', pages: 0, total: pdf.numPages, failed: [], complete: false, stopped: false, truncated: pdf.numPages > 1000 };
  const maxChars = 2 * 1024 * 1024;
  for (let pageNumber = 1; pageNumber <= Math.min(pdf.numPages, 1000); pageNumber++) {
    if (cancelled()) { report.stopped = true; break; }
    progress(pageNumber);
    let page;
    try {
      page = await pdf.getPage(pageNumber);
      const content = await page.getTextContent();
      if (cancelled()) { report.stopped = true; break; }
      let text = '';
      let previous: TextItem | undefined;
      let endsWithWhitespace = false;
      for (const item of content.items) {
        if (!('str' in item)) continue;
        // Keep explicit whitespace and even empty EOL markers. Do not add a
        // space around space items or inside proven contiguous font runs.
        const separator = item.str && text && !endsWithWhitespace && !/^\s/u.test(item.str) &&
          (!previous || !contiguous(previous, item, content.styles ?? {})) ? ' ' : '';
        const remaining = maxChars - report.text.length - text.length - 2;
        const value = separator + item.str + (item.hasEOL ? '\n' : '');
        text += value.slice(0, Math.max(0, remaining));
        if (value.length > remaining) { report.truncated = true; break; }
        if (value) endsWithWhitespace = /\s/u.test(value[value.length - 1]);
        if (item.hasEOL) previous = undefined;
        else if (item.str) previous = item;
      }
      report.text += (report.text ? '\n\n' : '') + text.trimEnd();
      report.pages++;
      if (report.truncated && pageNumber <= 1000 && report.text.length >= maxChars - 2) break;
    } catch { report.failed.push(pageNumber); }
    finally { page?.cleanup(); }
  }
  report.complete = !report.stopped && !report.truncated && report.failed.length === 0 && report.pages === report.total;
  return report;
}

export function textSummary(report: PdfTextReport): string {
  if (report.complete && !report.text.trim()) return '未提取到正文；文件可能是扫描件或空白页，文字识别尚未支持。';
  const prefix = report.complete ? '已提取' : report.stopped ? '已停止，保留部分正文' : '部分正文已提取';
  return `${prefix}（${report.pages}/${report.total} 页${report.failed.length ? `，${report.failed.length} 页失败` : ''}${report.truncated ? '，达到提取上限' : ''}）。文字顺序和字体映射仍需核对。`;
}
