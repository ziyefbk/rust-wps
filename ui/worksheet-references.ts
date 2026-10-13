const part = "(?:'(?:[^']|'')*'|[\\p{L}\\p{N}_\\\\$][\\p{L}\\p{N}_.\\\\$]*)";
const prefix = new RegExp(`^(${part})(?:\\s*:\\s*(${part}))?!`, 'u');
const decode = (name: string) => name.startsWith("'") ? name.slice(1, -1).replaceAll("''", "'") : name;
const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** Rewrite worksheet prefixes, excluding string literals and external workbooks. */
export function updateWorksheetReferences(formula: string, names: string[], index: number, replacement?: string, movedTo?: number): string {
  let result = '', i = 0;
  while (i < formula.length) {
    const previous = i > 1 && /[\udc00-\udfff]/.test(formula[i - 1]) ? formula.slice(i - 2, i) : formula[i - 1];
    if (!previous || !/[\p{L}\p{N}_.\\$]/u.test(previous)) {
      const match = prefix.exec(formula.slice(i));
      if (match) {
        const decoded = match[2] ? [decode(match[1]), decode(match[2])] : decode(match[1]).split(':');
        let updated = [...decoded], changed = false, invalid = false;
        if (previous !== ']' && !decoded.some(name => name.includes('['))) {
          for (let n = 0; n < decoded.length; n++) if (same(decoded[n], names[index])) {
            if (movedTo !== undefined) {
              if (decoded.length !== 2 || same(decoded[0], decoded[1])) continue;
              const other = names.findIndex(name => same(name, decoded[1 - n]));
              if (other < 0) throw new Error('无法维护未知工作表的三维引用');
              if (index < other ? movedTo >= other : movedTo <= other) {
                updated[n] = names[index + Math.sign(other - index)]; changed = true;
              }
              continue;
            }
            changed = true;
            if (replacement !== undefined) updated[n] = replacement;
            else if (decoded.length === 2 && !same(decoded[0], decoded[1])) {
              const other = names.findIndex(name => same(name, decoded[1 - n]));
              if (other < 0) throw new Error('无法维护未知工作表的三维引用');
              updated[n] = names[index + Math.sign(other - index)];
            } else invalid = true;
          }
        }
        if (changed) {
          if (updated.length === 2 && same(updated[0], updated[1])) updated = [updated[0]];
          result += invalid ? '#REF!' : `'${updated.join(':').replaceAll("'", "''")}'!`;
        } else result += match[0];
        i += match[0].length; continue;
      }
    }
    if (formula[i] === '"' || formula[i] === "'") {
      const start = i, quote = formula[i++]; let closed = false;
      while (i < formula.length) if (formula[i++] === quote) { if (formula[i] === quote) i++; else { closed = true; break; } }
      if (!closed) throw new Error('公式引号未闭合，无法维护工作表引用');
      result += formula.slice(start, i); continue;
    }
    if (formula[i] === '[') {
      const start = i; let depth = 0;
      do {
        if (formula[i] === "'" && /[\[\]'#@]/.test(formula[i + 1] ?? '')) { i += 2; continue; }
        if (formula[i] === '[') depth++; else if (formula[i] === ']') depth--; i++;
      } while (i < formula.length && depth);
      if (depth) throw new Error('公式方括号未闭合，无法维护工作表引用');
      result += formula.slice(start, i); continue;
    }
    const point = formula.codePointAt(i)!; result += String.fromCodePoint(point); i += point > 0xffff ? 2 : 1;
  }
  return result;
}
