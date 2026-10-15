// Fresh, unchanged-source checks of the actual application helper. Node uses
// PDF.js's installed legacy adapter; this is not browser/native acceptance.
// No PDF is created, rewritten, normalized, or redistributed by this checker.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import ts from 'typescript';
import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs';

const searchSource = readFileSync(new URL('../ui/reading-search.ts', import.meta.url), 'utf8');
const searchJs = ts.transpileModule(searchSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { findReadingMatches } = await import(`data:text/javascript;base64,${Buffer.from(searchJs).toString('base64')}`);

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const options = Object.fromEntries(process.argv.slice(2).reduce((pairs, value, at, args) => {
  if (at % 2 === 0) { assert(value.startsWith('--') && args[at + 1], 'Use --name value options'); pairs.push([value.slice(2), args[at + 1]]); }
  return pairs;
}, []));
for (const key of Object.keys(options)) assert(['output', 'corpus-dir', 'rust-cli'].includes(key), `Unknown option: ${key}`);
assert(options.output, '--output must name a new evidence directory');
const output = path.resolve(root, options.output);
const corpus = path.resolve(root, options['corpus-dir'] ?? 'tmp/pdf-gap-investigation');
const cli = path.resolve(root, options['rust-cli'] ?? 'tmp/pdf-snapshot-final-cli');
const manifestPath = path.join(root, 'scripts/pdf-snapshot-corpus.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const baselineRef = 'c4d792784930fb004ee869752b2501bf4957ba63';
const cliHash = 'aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67';
const sha = data => createHash('sha256').update(data).digest('hex');
const identity = filename => { const bytes = readFileSync(filename); return { bytes: bytes.length, sha256: sha(bytes), git_blob_sha: createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') }; };
const save = (filename, data) => { writeFileSync(path.join(output, filename), data); return { file: filename, sha256: sha(data), bytes: Buffer.byteLength(data) }; };
const json = value => JSON.stringify(value, null, 2) + '\n';
const command = (program, args) => spawnSync(program, args, { cwd: root, encoding: 'utf8', timeout: 60000, maxBuffer: 16 * 1024 * 1024 });
const baseline = command('git', ['show', `${baselineRef}:ui/pdf-text.ts`]);
assert.equal(baseline.status, 0, baseline.stderr);
const currentSource = readFileSync(path.join(root, 'ui/pdf-text.ts'), 'utf8');
assert.notEqual(sha(currentSource), sha(baseline.stdout), 'No changed application helper to validate');
assert.equal(identity(cli).sha256, cliHash, 'Frozen source-snapshot CLI changed');
const loadHelper = async source => {
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
};
const oldHelper = await loadHelper(baseline.stdout), newHelper = await loadHelper(currentSource);
const mainSource = readFileSync(path.join(root, 'ui/main.ts'), 'utf8');
// Exercise the actual recovery/search functions with a small DOM transport
// double. This checks state replacement and literal marks, not browser layout.
const mainAst = ts.createSourceFile('ui/main.ts', mainSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const functionSources = ['recoverPdfText', 'renderText'].map(name => {
  const node = mainAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert(node, `Application function ${name} missing`); return node.getText(mainAst);
}).join('\n');
const integrationJs = ts.transpileModule(functionSources, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
function integration(pdf, initialText) {
  const elements = new Map();
  const el = id => { if (!elements.has(id)) elements.set(id, { value: '', textContent: '', hidden: false, nodes: [], replaceChildren() { this.nodes = []; }, append(node) { this.nodes.push(node); } }); return elements.get(id); };
  const document = { createTextNode: text => ({ textContent: text }), createElement: tag => ({ tag, textContent: '' }) };
  const current = { inspection: { format: 'pdf', text: initialText } };
  const build = new Function('findReadingMatches', 'extractPdfText', 'textSummary', 'pdf', 'current', 'document', 'el', 'notice', `let stopText = false, pdfTextSummary = null; ${integrationJs}; return { recoverPdfText, renderText };`);
  return { ...build(findReadingMatches, newHelper.extractPdfText, newHelper.textSummary, pdf, current, document, el, () => {}), el, current };
}
mkdirSync(output, { recursive: false });
const report = {
  status: 'running', started_utc: new Date().toISOString(), baseline_ref: baselineRef,
  script_sha256: sha(readFileSync(fileURLToPath(import.meta.url))), manifest_sha256: sha(readFileSync(manifestPath)),
  helper_before_sha256: sha(baseline.stdout), helper_after_sha256: sha(currentSource), rust_cli_sha256: cliHash,
  main_ts_sha256: sha(mainSource),
  runtime: { node: process.version, pdfjs: pdfjs.version, adapter: 'legacy/build/pdf.mjs' },
  limits: [
    'Actual application helper with installed PDF.js legacy adapter in Node; no browser, WebView2, IPC, or mouse/keyboard acceptance.',
    'Only literal anchors and separator semantics are compared across readers, not full text order, glyph normalization, or visual fidelity.',
    'No whitespace folding, Unicode normalization, case folding, dehyphenation, OCR, geometry sorting, or source PDF rewriting.',
    'Rust encoding failures in two pre-existing corpus cases remain failures; this frontend fix does not change Rust extraction.',
  ], fixtures: [],
};
const count = (text, query) => text.split(query).length - 1;
try {
  save('helper-before.ts', baseline.stdout); save('helper-after.ts', currentSource);
  for (const fixture of manifest.fixtures) {
    const source = path.join(corpus, fixture.name), before = identity(source);
    for (const field of ['bytes', 'sha256', 'git_blob_sha']) assert.equal(before[field], fixture[field], `${fixture.name}: ${field} mismatch`);
    const entry = { name: fixture.name, source_url: fixture.source_url, revision: fixture.revision, classification: fixture.classification, identity: before };
    report.fixtures.push(entry);
    const loading = pdfjs.getDocument({ data: new Uint8Array(readFileSync(source)), isEvalSupported: false, useSystemFonts: true,
      cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps/'), cMapPacked: true,
      standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts/'),
      wasmUrl: path.join(root, 'node_modules/pdfjs-dist/wasm/'), iccUrl: path.join(root, 'node_modules/pdfjs-dist/iccs/'),
    });
    let pdf;
    try {
      pdf = await loading.promise;
      assert.equal(pdf.numPages, fixture.pages);
      const oldText = await oldHelper.extractPdfText(pdf, () => false, () => {});
      const pages = [];
      const recording = { numPages: pdf.numPages, getPage: async number => {
        const page = await pdf.getPage(number);
        return { getTextContent: async () => { const content = await page.getTextContent(); pages.push({ page: number, rotate: page.rotate, ...content }); return content; }, cleanup: () => page.cleanup() };
      } };
      const newText = await newHelper.extractPdfText(recording, () => false, () => {});
      assert(oldText.complete && newText.complete, `${fixture.name}: incomplete PDF.js extraction`);
      const { text: candidateText, ...summary } = newText;
      entry.pdfjs = { before: save(`${fixture.name}.before.txt`, oldText.text), after: save(`${fixture.name}.after.txt`, candidateText), report: summary };
      entry.text_items = save(`${fixture.name}.items.json`, json(pages));
      const independent = command(process.env.PYTHON ?? 'python3', ['-c', 'import json,sys,pypdf; r=pypdf.PdfReader(sys.argv[1]); print(json.dumps({"version":pypdf.__version__,"pages":[p.extract_text() or "" for p in r.pages]},ensure_ascii=True))', source]);
      save(`${fixture.name}.pypdf.stderr.txt`, independent.stderr ?? '');
      assert.equal(independent.status, 0, independent.stderr);
      const parsed = JSON.parse(independent.stdout);
      assert.equal(parsed.pages.length, fixture.pages);
      const independentText = parsed.pages.join('\n');
      entry.pypdf = { version: parsed.version, pages: parsed.pages.length, text: save(`${fixture.name}.pypdf.txt`, independentText), raw: save(`${fixture.name}.pypdf.json`, independent.stdout) };
      const poppler = command('pdftotext', ['-enc', 'UTF-8', source, '-']);
      assert.equal(poppler.status, 0, poppler.stderr);
      entry.poppler = { text: save(`${fixture.name}.poppler.txt`, poppler.stdout), stderr: save(`${fixture.name}.poppler.stderr.txt`, poppler.stderr) };
      const rust = command(cli, ['inspect', source]);
      entry.rust = { status: rust.status, stdout: save(`${fixture.name}.rust.json`, rust.stdout ?? ''), stderr: save(`${fixture.name}.rust.stderr.txt`, rust.stderr ?? '') };
      let rustText;
      if (fixture.expected_cli.status === 'success') {
        assert.equal(rust.status, 0, rust.stderr); rustText = JSON.parse(rust.stdout).text;
      } else {
        assert.equal(rust.status, fixture.expected_cli.returncode);
        assert.equal(rust.stdout, ''); assert(rust.stderr.includes(fixture.expected_cli.stderr_contains));
        entry.rust.known_unsupported_encoding = fixture.expected_cli.stderr_contains;
      }
      // Whitespace deletion here proves only the character stream did not
      // change. Literal phrase assertions below do not normalize their inputs.
      assert.equal(candidateText.replace(/\s/gu, ''), oldText.text.replace(/\s/gu, ''), 'Non-whitespace text changed');
      entry.non_whitespace_pdfjs_stream_unchanged = true;
      if (fixture.name === 'tracemonkey.pdf') {
        entry.phrase_counts = {};
        const phrases = ['Keywords JavaScript', 'compiling a trace speculates', 'records a trace tree', 'contains all the guards'];
        for (const phrase of phrases) {
          const counts = { before: count(oldText.text, phrase), after: count(candidateText, phrase), rust: count(rustText, phrase), poppler: count(poppler.stdout, phrase), pypdf: count(independentText, phrase) };
          assert.deepEqual(counts, { before: 0, after: 1, rust: 1, poppler: 1, pypdf: 1 }); entry.phrase_counts[phrase] = counts;
        }
        assert(!candidateText.includes('keywords JavaScript'), 'Search unexpectedly changed case');
        const app = integration(pdf, rustText);
        entry.reextract_search_integration = { transport: 'actual main.ts functions with DOM double, actual helper and PDF.js document', repetitions: [] };
        for (let attempt = 0; attempt < 3; attempt++) {
          if (attempt) await app.recoverPdfText();
          const matches = {};
          for (const phrase of [...phrases, 'keywords JavaScript']) {
            app.el('search').value = phrase; app.renderText();
            matches[phrase] = app.el('text-content').nodes.filter(node => node.tag === 'mark').length;
            assert.equal(matches[phrase], phrase === 'keywords JavaScript' ? 0 : 1);
          }
          if (attempt) { assert.equal(app.current.inspection.text, candidateText); assert.equal(app.el('stop-pdf-text').hidden, true); }
          entry.reextract_search_integration.repetitions.push({ phase: attempt ? `reextract-${attempt}` : 'initial-rust', matches });
        }
        entry.geometry_controls = [];
        for (const [pageNumber, left, right] of [[2, 'snippet.', 'Interpret'], [6, 'Trace 2', 'Trace 1']]) {
          const page = pages.find(p => p.page === pageNumber), at = page.items.findIndex((item, index) => item.str === left && page.items[index + 1]?.str === right);
          assert(at >= 0, 'Original geometric negative control missing');
          const first = page.items[at], second = page.items[at + 1];
          assert.equal(first.hasEOL, false); assert.equal(second.hasEOL, false);
          assert(candidateText.includes(left + ' ' + right)); assert(!candidateText.includes(left + right));
          entry.geometry_controls.push({ page: pageNumber, first, second, x_gap: second.transform[4] - first.transform[4] - first.width, y_gap: second.transform[5] - first.transform[5], joined: false });
        }
      } else if (fixture.name === 'copy_paste_ligatures.pdf') {
        assert.equal(candidateText, 'abcdeffffiflffifflſtstghijklmno');
        assert.equal(oldText.text, 'abcdef fffiflffifflſtst ghijklmno');
        const raw = 'abcdefﬀﬁﬂﬃﬄﬅﬆghijklmno';
        assert(independentText.includes(raw)); assert(poppler.stdout.includes(raw));
        assert(!/\s/u.test(candidateText));
        entry.ligature_control = { raw_independent: raw, pdfjs_literal: candidateText, normalization_equivalence_claimed: false, inserted_spaces_before: 2, inserted_spaces_after: 0 };
      }
    } finally { await loading.destroy(); }
    assert.deepEqual(identity(source), before, `${fixture.name}: source changed`);
    entry.source_unchanged = true;
  }
  assert.equal(identity(cli).sha256, cliHash, 'Frozen CLI changed during validation');
  assert.equal(sha(readFileSync(path.join(root, 'ui/pdf-text.ts'))), report.helper_after_sha256, 'Helper changed during validation');
  assert.equal(sha(readFileSync(path.join(root, 'ui/main.ts'))), report.main_ts_sha256, 'Integration source changed during validation');
  report.status = 'passed';
} catch (error) { report.status = 'failed'; report.error = String(error.stack ?? error); throw error; }
finally { report.finished_utc = new Date().toISOString(); save('report.json', json(report)); }
console.log(json({ status: report.status, report: path.join(output, 'report.json'), fixtures: report.fixtures.length, browser_native: false }));
