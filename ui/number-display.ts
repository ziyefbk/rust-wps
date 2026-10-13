export interface NumericDisplay { text: string; fallback?: string }
const formats = new Map<string, Intl.NumberFormat>();
/** Deliberately bounded preset renderer, not an Excel format-code interpreter. */
export function formatNumber(value: number, code: string | null | undefined, date1904 = false): NumericDisplay {
  const raw = String(value), fail = (reason: string): NumericDisplay => ({ text: raw, fallback: reason });
  if (!Number.isFinite(value)) return fail('非有限数值');
  if (!code || code === 'General' || code === '@') return { text: raw };
  if (['0', '0.00', '#,##0.00', '0%', '0.00%'].includes(code)) {
    const percent = code.endsWith('%'), number = percent ? value * 100 : value;
    if (!Number.isFinite(number)) return fail('百分比显示超出数值范围');
    let formatter = formats.get(code);
    if (!formatter) {
      const digits = code.includes('.00') ? 2 : 0;
      formatter = new Intl.NumberFormat('en-US', { useGrouping: code.includes(','), minimumFractionDigits: digits, maximumFractionDigits: digits });
      formats.set(code, formatter);
    }
    return { text: formatter.format(Object.is(number, -0) ? 0 : number) + (percent ? '%' : '') };
  }
  if (code === 'yyyy-mm-dd') {
    const day = Math.floor(value);
    if (value < 0 || (!date1904 && day === 0) || day > 2958465) return fail('日期序列超出受支持范围');
    if (!date1904 && day === 60) return { text: '1900-02-29' };
    const timestamp = date1904 ? Date.UTC(1904, 0, 1) + day * 86400000 : Date.UTC(1899, 11, 31) + (day > 60 ? day - 1 : day) * 86400000;
    const date = new Date(timestamp);
    if (date.getUTCFullYear() > 9999) return fail('日期序列超出受支持范围');
    return { text: date.toISOString().slice(0, 10) };
  }
  if (code === 'hh:mm:ss') {
    if (value < 0 || value > 2958465) return fail('时间序列超出受支持范围');
    const seconds = Math.round((value - Math.floor(value)) * 86400) % 86400;
    return { text: [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map(n => String(n).padStart(2, '0')).join(':') };
  }
  return fail('此格式尚未支持显示，已显示原值');
}
