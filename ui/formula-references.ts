const identifier = (c: string | undefined) => !!c && /[\p{L}\p{N}_.\\$]/u.test(c);
const nextPoint = (text: string, index: number) => index < text.length ? String.fromCodePoint(text.codePointAt(index)!) : undefined;
const previousPoint = (text: string, index: number) => index > 1 && /[\udc00-\udfff]/.test(text[index - 1]) ? text.slice(index - 2, index) : text[index - 1];
const column = (text: string) => [...text.toUpperCase()].reduce((n, c) => n * 26 + c.charCodeAt(0) - 64, 0);
function letters(value: number) { let result = ''; for (; value > 0; value = Math.floor((value - 1) / 26)) result = String.fromCharCode(65 + (value - 1) % 26) + result; return result; }
function shiftColumn(text: string, absolute: string, delta: number) { const value = column(text) + (absolute ? 0 : delta); return value < 1 || value > 16384 ? '#REF!' : absolute + letters(value); }
function shiftRow(text: string, absolute: string, delta: number) { const value = Number(text) + (absolute ? 0 : delta); return value < 1 || value > 1048576 ? '#REF!' : absolute + value; }

/** A1 reference translation only: no evaluation, name resolution or formula rewriting. */
export function translateFormula(formula: string, rowDelta: number, columnDelta: number): string {
  if (formula.length > 8192) throw new Error('公式超过长度限制');
  if (!Number.isInteger(rowDelta) || !Number.isInteger(columnDelta)) throw new Error('公式引用偏移无效');
  if (rowDelta === 0 && columnDelta === 0) return formula;
  let result = '', i = 0;
  while (i < formula.length) {
    const c = formula[i];
    // Copy complete quoted strings/sheet names; doubled delimiters are escapes.
    if (c === '"' || c === "'") {
      const start = i++;
      let closed = false;
      while (i < formula.length) { if (formula[i++] === c) { if (formula[i] === c) i++; else { closed = true; break; } } }
      if (!closed) throw new Error('公式引号未闭合，无法调整引用');
      result += formula.slice(start, i); continue;
    }
    // Structured references and external workbook names are opaque bracket tokens.
    if (c === '[') {
      const start = i; let depth = 0;
      do {
        if (formula[i] === "'" && /[\[\]'#@]/.test(formula[i + 1] ?? '')) { i += 2; continue; }
        if (formula[i] === '[') depth++; else if (formula[i] === ']') depth--; i++;
      } while (i < formula.length && depth);
      if (depth) throw new Error('公式方括号未闭合，无法调整引用');
      result += formula.slice(start, i); continue;
    }
    if (!identifier(previousPoint(formula, i))) {
      const rest = formula.slice(i);
      // Shield unquoted sheet names, including 3-D sheet prefixes looking like A1:B2!.
      const sheet = /^[\p{L}\p{N}_\\$][\p{L}\p{N}_.\\$]*(?::[\p{L}\p{N}_\\$][\p{L}\p{N}_.\\$]*)?!/u.exec(rest);
      if (sheet) { result += sheet[0]; i += sheet[0].length; continue; }
      const cols = /^(\$?)([A-Za-z]{1,3})(\s*:\s*)(\$?)([A-Za-z]{1,3})/.exec(rest);
      if (cols && !identifier(nextPoint(rest, cols[0].length)) && column(cols[2]) <= 16384 && column(cols[5]) <= 16384) {
        const left = shiftColumn(cols[2], cols[1], columnDelta), right = shiftColumn(cols[5], cols[4], columnDelta);
        result += left === '#REF!' || right === '#REF!' ? '#REF!' : left + cols[3] + right;
        i += cols[0].length; continue;
      }
      const rows = /^(\$?)([1-9][0-9]{0,6})(\s*:\s*)(\$?)([1-9][0-9]{0,6})/.exec(rest);
      if (rows && !identifier(nextPoint(rest, rows[0].length)) && Number(rows[2]) <= 1048576 && Number(rows[5]) <= 1048576) {
        const top = shiftRow(rows[2], rows[1], rowDelta), bottom = shiftRow(rows[5], rows[4], rowDelta);
        result += top === '#REF!' || bottom === '#REF!' ? '#REF!' : top + rows[3] + bottom;
        i += rows[0].length; continue;
      }
      const cell = /^(\$?)([A-Za-z]{1,3})(\$?)([1-9][0-9]{0,6})/.exec(rest);
      if (cell && !identifier(nextPoint(rest, cell[0].length)) && rest[cell[0].length] !== '[' && !/^\s*\(/.test(rest.slice(cell[0].length))
          && column(cell[2]) <= 16384 && Number(cell[4]) <= 1048576) {
        const col = shiftColumn(cell[2], cell[1], columnDelta), row = shiftRow(cell[4], cell[3], rowDelta);
        result += col === '#REF!' || row === '#REF!' ? '#REF!' : col + row;
        i += cell[0].length; continue;
      }
    }
    const point = formula.codePointAt(i)!; result += String.fromCodePoint(point); i += point > 0xffff ? 2 : 1;
  }
  return result;
}
