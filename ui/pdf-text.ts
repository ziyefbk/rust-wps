import type { PDFDocumentProxy } from 'pdfjs-dist';

export interface PdfTextReport { text: string; pages: number; total: number; failed: number[]; complete: boolean; stopped: boolean; truncated: boolean }
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
      for (const item of content.items) {
        if (!('str' in item)) continue;
        // PDF.js normalizes text and emits EOL hints. Do not concatenate words
        // from separate positioned spans without a separator.
        const remaining = maxChars - report.text.length - text.length - 2;
        const value = item.str + (item.hasEOL ? '\n' : ' ');
        text += value.slice(0, Math.max(0, remaining));
        if (value.length > remaining) { report.truncated = true; break; }
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
