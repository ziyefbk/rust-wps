import type { EditableWorkbook } from './workbook-model';
export interface CalculationValue { type: 'number' | 'text' | 'boolean' | 'empty'; value?: number | string | boolean }
export interface CalculationCell { sheet: number; row: number; column: number; formula: string; value?: CalculationValue; failure?: { kind: 'error' | 'unsupported' | 'cycle' | 'limit'; code: string } }
export interface CalculationReport { engine: string; complete: boolean; operations: number; cells: CalculationCell[] }
export const calculationKey = (sheet: number, row: number, column: number) => `${sheet}:${row}:${column}`;
export function validateCalculation(input: unknown, model: EditableWorkbook): CalculationReport {
  const report = input as CalculationReport;
  if (!report || report.engine !== 'bounded-numeric-v1' || typeof report.complete !== 'boolean' || !Number.isInteger(report.operations) || report.operations < 0 || report.operations > 1000000 || !Array.isArray(report.cells)) throw new Error('计算报告无效');
  const expected = new Map<string, string>();
  model.sheets.forEach((sheet, i) => sheet.cells.forEach(cell => { if (cell.content.type === 'formula') expected.set(calculationKey(i, cell.row, cell.column), cell.content.value); }));
  if (report.cells.length !== expected.size) throw new Error('计算报告缺少公式结果');
  for (const cell of report.cells) {
    if (!cell || ![cell.sheet, cell.row, cell.column].every(Number.isInteger)) throw new Error('计算坐标无效');
    const key = calculationKey(cell.sheet, cell.row, cell.column);
    if (!expected.has(key) || expected.get(key) !== cell.formula) throw new Error('计算报告与工作簿版本不符');
    expected.delete(key);
    if (Boolean(cell.value) === Boolean(cell.failure)) throw new Error('计算结果状态无效');
    if (cell.value) {
      const { type, value } = cell.value;
      if (!(type === 'empty' && value === undefined || type === 'number' && typeof value === 'number' && Number.isFinite(value) || type === 'text' && typeof value === 'string' || type === 'boolean' && typeof value === 'boolean')) throw new Error('计算值无效');
    } else {
      const failure = cell.failure!;
      const valid = failure.kind === 'error' ? ['#DIV/0!', '#NUM!', '#REF!', '#VALUE!', '#N/A', '#NAME?', '#NULL!'].includes(failure.code)
        : failure.kind === 'unsupported' ? failure.code === 'UNSUPPORTED_FORMULA'
        : failure.kind === 'cycle' ? failure.code === 'CIRCULAR_REFERENCE'
        : failure.kind === 'limit' && failure.code === 'CALCULATION_LIMIT';
      if (!valid) throw new Error('计算失败状态无效');
    }
  }
  if (report.complete !== report.cells.every(cell => !cell.failure || cell.failure.kind === 'error')) throw new Error('计算完成状态不一致');
  return report;
}
export function calculationText(cell: CalculationCell) {
  if (cell.value) return cell.value.type === 'empty' ? '' : String(cell.value.value);
  return cell.failure!.kind === 'error' ? cell.failure!.code : { unsupported: '未支持', cycle: '循环引用', limit: '超出计算上限' }[cell.failure!.kind];
}
