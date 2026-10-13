import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
const js = ts.transpileModule(readFileSync('ui/formula-references.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { translateFormula } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);

test('A1 translation preserves anchors and Unicode literals, names and structured references', () => {
  expect(translateFormula('A1+$B2+C$3+$D$4+SUM(A1:B2)', 2, 1)).toBe('B3+$B4+D$3+$D$4+SUM(B3:C4)');
  expect(translateFormula('IF(A1="中文😀 A1 ""引号""",\'中文 A1\'!B2,LOG10 (C3)+1.5E5)', 1, 1)).toBe('IF(B2="中文😀 A1 ""引号""",\'中文 A1\'!C3,LOG10 (D4)+1.5E5)');
  expect(translateFormula("'O''Brien A1'!A1+A1!B2+A1:B2!C3", 1, 1)).toBe("'O''Brien A1'!B2+A1!C3+A1:B2!D4");
  expect(translateFormula('SUM(T1[A1],Table1[[#Headers],[B2]])+中文A1+A1中文+𐐀A1+A1𐐀+A1', 1, 1)).toBe('SUM(T1[A1],Table1[[#Headers],[B2]])+中文A1+A1中文+𐐀A1+A1𐐀+B2');
  expect(translateFormula('[book.xlsx]Sheet1!A1+@B2+C3#', 1, 1)).toBe('[book.xlsx]Sheet1!B2+@C3+D4#');
  expect(translateFormula('SUM(A:$B,$1:3)', 2, 1)).toBe('SUM(B:$B,$1:5)');
  expect(translateFormula("T1[Col']A1]+A1", 1, 1)).toBe("T1[Col']A1]+B2");
  expect(translateFormula('$A1!B2', 1, 1)).toBe('$A1!C3');
});

test('off-grid references become REF errors; malformed tokens reject translation', () => {
  expect(translateFormula('A1+$A1+A$1+$A$1', -1, -1)).toBe('#REF!+#REF!+#REF!+$A$1');
  expect(translateFormula('XFD1048576', 1, 1)).toBe('#REF!');
  expect(() => translateFormula('"unterminated', 1, 1)).toThrow();
  expect(() => translateFormula('Table1[unterminated', 1, 1)).toThrow();
  expect(() => translateFormula('x'.repeat(8193), 1, 1)).toThrow();
});

test('independent openpyxl translator agrees on supported valid reference cases', () => {
  const formulas = ['A1+$B2+C$3+$D$4', 'SUM(A1:B10)', 'LOG10(A1)+1.5E5', 'IF(A1="中文 A1",\'中文 A1\'!B2,C3)', "'O''Brien A1'!A1", '[book.xlsx]Sheet1!A1', 'SUM(A:$B,$1:3)'];
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const input = JSON.stringify(formulas);
  const report = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from openpyxl.formula.translate import Translator; raw=sys.stdin.buffer.read(); values=json.loads(raw.decode("utf-8")); print(json.dumps({"input":values,"hex":raw.hex(),"output":[Translator("="+f,origin="B2").translate_formula("D5")[1:] for f in values]}))'], { input, encoding: 'utf8', windowsHide: true }));
  expect(report.hex).toBe(Buffer.from(input).toString('hex'));
  expect(report.input).toEqual(formulas);
  expect(formulas.map(formula => translateFormula(formula, 3, 2))).toEqual(report.output);
});
