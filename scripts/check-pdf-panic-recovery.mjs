// Fresh independent checks of ordinary extractor-panic containment. Sources
// remain the six pinned, unchanged Mozilla originals. No Rust builds, PDF
// rewriting, browser rendering, native IPC, or historical evidence updates.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import ts from 'typescript';
import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs';

const scriptPath = fileURLToPath(import.meta.url);
const root = path.resolve(path.dirname(scriptPath), '..');
const options = {};
const allowed = ['output', 'corpus-dir', 'baseline', 'candidate', 'preview-runner', 'preview-source', 'preview-library', 'preview-provenance'];
for (let at = 2; at < process.argv.length; at += 2) {
  const key = process.argv[at], value = process.argv[at + 1];
  assert(key.startsWith('--') && value && !value.startsWith('--'), 'Use --name value options');
  assert(allowed.includes(key.slice(2)), `Unknown option: ${key}`);
  assert(!(key.slice(2) in options), `Duplicate option: ${key}`);
  options[key.slice(2)] = value;
}
assert(options.output, '--output must name a new evidence directory');
const resolve = (key, fallback) => path.resolve(root, options[key] ?? fallback);
const output = resolve('output');
const corpus = resolve('corpus-dir', 'tmp/pdf-gap-investigation');
const baseline = resolve('baseline', 'tmp/pdf-snapshot-final-cli');
const candidate = resolve('candidate', 'tmp/pdf-panic-final-cli');
const preview = resolve('preview-runner', 'tmp/pdf-panic-preview');
const previewSource = resolve('preview-source', 'scripts/pdf-preview-probe.rs');
const previewLibrary = resolve('preview-library', 'target/debug/librust_wps.rlib');
const manifestPath = path.join(root, 'scripts/pdf-snapshot-corpus.json');
const baselineHash = 'aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67';
const helperBaselineRef = '16eb21e93fd8f81a212f2a684f971f169314bfa8';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
const identity = filename => {
  const bytes = readFileSync(filename);
  return { bytes: bytes.length, sha256: sha(bytes), git_blob_sha: createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') };
};
const save = (filename, bytes) => {
  writeFileSync(path.join(output, filename), bytes, { flag: 'wx' });
  return { file: filename, bytes: Buffer.byteLength(bytes), sha256: sha(bytes) };
};
const run = (program, args) => spawnSync(program, args, { cwd: root, timeout: 60000, maxBuffer: 16 * 1024 * 1024 });
const text = bytes => (bytes ?? Buffer.alloc(0)).toString('utf8');
const commandOk = (result, name, status = 0) => {
  assert.equal(result.error, undefined, `${name}: subprocess execution error ${result.error}`);
  assert.equal(result.signal, null, `${name}: terminated by ${result.signal}`);
  assert.equal(result.status, status, `${name}: expected exit ${status}, got ${result.status}: ${text(result.stderr)}`);
};
const recordCommand = (prefix, program, args) => {
  const result = run(program, args);
  const evidence = {
    program, args, status: result.status, signal: result.signal,
    stdout: save(`${prefix}.stdout.txt`, result.stdout ?? Buffer.alloc(0)),
    stderr: save(`${prefix}.stderr.txt`, result.stderr ?? Buffer.alloc(0)),
    ...(result.error ? { execution_error: String(result.error) } : {}),
  };
  return { result, evidence };
};
const withoutElapsed = ({ elapsed_ms, ...rest }) => rest;
const literalCount = (value, needle) => value.split(needle).length - 1;
const inspectShape = (value, fixture) => {
  assert.equal(value.format, 'pdf');
  assert.equal(value.backend, 'pdf-extract 0.12.1');
  assert.equal(value.bytes, fixture.bytes);
  assert(Number.isSafeInteger(value.elapsed_ms) && value.elapsed_ms >= 0, 'Invalid elapsed_ms');
  assert.equal(typeof value.text, 'string');
  assert(Array.isArray(value.warnings) && value.warnings.every(item => typeof item === 'string'));
};

// Refuse existing directories before spawning any extractor, and never replace
// an existing report. A failed run retains its own logs and status.
mkdirSync(output, { recursive: false });
const report = {
  status: 'running', started_utc: new Date().toISOString(), baseline_sha256: baselineHash,
  helper_baseline_ref: helperBaselineRef,
  runtime: { node: process.version, pdfjs: pdfjs.version, adapter: 'legacy/build/pdf.mjs' },
  limits: [
    'Containment of ordinary unwinding Rust extractor panics is not CMap support or successful Rust text extraction.',
    'No protection against panic=abort, OOM, aborting/double panics, stack overflow, hangs, or excessive resource use is established here.',
    'The default Rust panic hook may still write stderr. Worker deadline, response limit and Windows quota remain separate protections.',
    'The public-preview runner and actual main.ts functions use headless transport; no Tauri IPC, browser, WebView2, page rendering or physical UI acceptance.',
    'PDF.js uses installed Node legacy adapter assets. Rendering precedes automatic recovery in the actual UI and is not exercised here.',
    'Only explicit text anchors are checked across independent readers; footer order, ligatures and glyph mapping differences remain visible.',
    'Any whitespace folding is restricted to manifest evidence anchors, never CLI parity or product search. No source rewriting or OCR.',
    'The later read_current_pdf pathname reopen and concurrent in-place writes remain separate source-identity limitations.',
  ],
  fixtures: [], sources_before: {}, protected_before: {},
};
let failure;
try {
  const manifestBytes = readFileSync(manifestPath), manifest = JSON.parse(manifestBytes);
  assert.equal(manifest.fixtures.length, 6);
  assert.equal(manifest.fixtures.filter(item => item.expected_cli.status === 'success').length, 4);
  assert.equal(manifest.fixtures.filter(item => item.expected_cli.status === 'known-unsupported-encoding-panic').length, 2);
  report.manifest = save('corpus-manifest.json', manifestBytes);
  report.checker = save('check-pdf-panic-recovery.mjs', readFileSync(scriptPath));
  const protectedPaths = [
    baseline, candidate, preview, previewSource, previewLibrary, scriptPath, manifestPath,
    ...['src/lib.rs', 'src/pdf_snapshot.rs', 'Cargo.toml', 'Cargo.lock', 'src-tauri/src/worker.rs', 'src-tauri/src/main.rs',
      'ui/main.ts', 'ui/pdf-text.ts', 'package.json', 'package-lock.json', 'scripts/check-pdf-snapshot.py',
      'scripts/check-pdf-phrase-search.mjs'].map(name => path.join(root, name)),
  ];
  if (options['preview-provenance']) protectedPaths.push(resolve('preview-provenance'));
  for (const filename of protectedPaths) report.protected_before[filename] = identity(filename);
  assert.equal(report.protected_before[baseline].sha256, baselineHash, 'Frozen baseline CLI changed');
  assert.notEqual(report.protected_before[candidate].sha256, baselineHash, 'Candidate is the baseline binary');
  report.preview_runner_source = save('preview-runner.rs', readFileSync(previewSource));
  report.preview_link_library = { path: previewLibrary, identity: report.protected_before[previewLibrary] };
  if (options['preview-provenance']) {
    const provenanceBytes = readFileSync(resolve('preview-provenance'));
    const provenance = JSON.parse(provenanceBytes);
    for (const filename of [candidate, preview, previewSource, previewLibrary]) {
      assert(provenance.identities?.[path.relative(root, filename)], `Build provenance omits ${filename}`);
    }
    for (const [name, expected] of Object.entries(provenance.identities)) {
      const filename = path.resolve(root, name);
      assert(filename.startsWith(root + path.sep), 'Build provenance paths must remain in this checkout');
      const observed = identity(filename);
      assert.equal(observed.bytes, expected.bytes, `${name}: provenance size mismatch`);
      assert.equal(observed.sha256, expected.sha256, `${name}: provenance hash mismatch`);
      if (report.protected_before[filename]) assert.deepEqual(observed, report.protected_before[filename]);
      else report.protected_before[filename] = observed;
    }
    report.preview_build_provenance = save('preview-build-provenance.json', provenanceBytes);
    report.preview_build_provenance_identities_verified = true;
    report.limits.push('Build provenance supplied by the implementation lead is checked against current source, library, dependency, binary and log identities; this checker does not rebuild Rust.');
  } else report.limits.push('Preview runner source and binary hashes recorded; binary-to-library build provenance must be reviewed separately.');
  report.binary_identity = { baseline: report.protected_before[baseline], candidate: report.protected_before[candidate], preview: report.protected_before[preview] };
  for (const fixture of manifest.fixtures) {
    assert.equal(path.basename(fixture.name), fixture.name, 'Fixture name must be a basename');
    const before = identity(path.join(corpus, fixture.name));
    assert.deepEqual(before, Object.fromEntries(['bytes', 'sha256', 'git_blob_sha'].map(key => [key, fixture[key]])), `${fixture.name}: source pin mismatch`);
    report.sources_before[fixture.name] = before;
  }

  const helperSource = readFileSync(path.join(root, 'ui/pdf-text.ts'), 'utf8');
  const mainSource = readFileSync(path.join(root, 'ui/main.ts'), 'utf8');
  for (const [name, source] of [['ui/pdf-text.ts', helperSource], ['ui/main.ts', mainSource]]) {
    const frozen = run('git', ['show', `${helperBaselineRef}:${name}`]);
    commandOk(frozen, `Frozen ${name}`);
    assert.equal(source, text(frozen.stdout), `${name}: changed since phrase fix; review before reusing this checker`);
  }
  save('actual-helper.ts', helperSource);
  const helperJs = ts.transpileModule(helperSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  const helper = await import(`data:text/javascript;base64,${Buffer.from(helperJs).toString('base64')}`);
  const ast = ts.createSourceFile('ui/main.ts', mainSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
  const actualFunctions = ['recoverPdfText', 'renderText'].map(name => {
    const node = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert(node, `Missing application function ${name}`); return node.getText(ast);
  }).join('\n');
  report.actual_main_functions = save('actual-main-functions.ts', actualFunctions);
  const integrationJs = ts.transpileModule(actualFunctions, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  function integration(pdf, inspection, extractor = helper.extractPdfText) {
    const elements = new Map(), notices = [], reports = [];
    const el = id => {
      if (!elements.has(id)) elements.set(id, { value: '', textContent: '', hidden: false, nodes: [], replaceChildren() { this.nodes = []; this.textContent = ''; }, append(node) { this.nodes.push(node); } });
      return elements.get(id);
    };
    const document = { createTextNode: value => ({ textContent: value }), createElement: tag => ({ tag, textContent: '' }) };
    const current = { inspection: structuredClone(inspection) };
    const trackedExtract = async (...args) => { const result = await extractor(...args); reports.push(structuredClone(result)); return result; };
    const build = new Function('extractPdfText', 'textSummary', 'pdf', 'current', 'document', 'el', 'notice',
      `let stopText = false, pdfTextSummary = null; ${integrationJs}; return { recoverPdfText, renderText, summary: () => pdfTextSummary, stop: () => { stopText = true; } };`);
    return { ...build(trackedExtract, helper.textSummary, pdf, current, document, el, message => notices.push(message)), el, current, notices, reports };
  }
  const search = (app, phrase) => {
    app.el('search').value = phrase; app.renderText();
    return app.el('text-content').nodes.filter(node => node.tag === 'mark').length;
  };
  async function verifyRecovery(pdf, previewReport, expectedText, queries) {
    const app = integration(pdf, previewReport);
    app.renderText();
    assert.equal(app.current.inspection.text, '');
    assert.equal(app.current.inspection.text_error, previewReport.text_error);
    assert(app.el('text-content').textContent.includes('PDF 文字提取未成功'));
    const result = { transport: 'Actual main.ts recovery/search functions, actual helper and real PDF.js document, minimal DOM double', repetitions: [] };
    for (let attempt = 1; attempt <= 2; attempt++) {
      await app.recoverPdfText();
      assert.equal(app.current.inspection.text, expectedText);
      assert(!('text_error' in app.current.inspection));
      assert.equal(app.el('stop-pdf-text').hidden, true);
      assert.equal(app.reports.at(-1).complete, true);
      const matches = {};
      for (const [phrase, expected] of queries) { matches[phrase] = search(app, phrase); assert.equal(matches[phrase], expected, `Literal search: ${phrase}`); }
      result.repetitions.push({ attempt, matches, report: app.reports.at(-1), summary: app.summary(), text_error_present: false });
    }
    // Inject only page transport failures, not generated source PDF bytes. The
    // successful page in the partial case is the original's actual first page.
    const retained = { ...previewReport, text: 'Earlier retained extraction' };
    const failedPdf = { numPages: 2, getPage: async () => { throw new Error('Injected page acquisition failure'); } };
    result.incomplete_controls = [];
    for (const initial of [previewReport, retained]) {
      const failed = integration(failedPdf, initial);
      await failed.recoverPdfText();
      assert.equal(failed.current.inspection.text, initial.text);
      assert.equal(failed.current.inspection.text_error, previewReport.text_error);
      assert.equal(failed.reports[0].complete, false);
      assert.equal(failed.reports[0].text, '');
      assert.deepEqual(failed.reports[0].failed, [1, 2]);
      assert(!failed.summary().startsWith('已提取'));
      assert.equal(failed.el('stop-pdf-text').hidden, true);
      result.incomplete_controls.push({ case: initial.text ? 'empty-failure-keeps-earlier-text' : 'empty-failure-keeps-preview-error', report: failed.reports[0], summary: failed.summary(), text_preserved: true, error_preserved: true });
    }
    const partialPdf = { numPages: 2, getPage: async number => { if (number === 2) throw new Error('Injected second-page acquisition failure'); return pdf.getPage(1); } };
    const partial = integration(partialPdf, retained);
    await partial.recoverPdfText();
    assert.equal(partial.current.inspection.text, expectedText);
    assert.equal(partial.current.inspection.text_error, previewReport.text_error);
    assert.equal(partial.reports[0].complete, false);
    assert.deepEqual(partial.reports[0].failed, [2]);
    assert.equal(partial.reports[0].pages, 1);
    assert(!partial.summary().startsWith('已提取'));
    assert.equal(partial.el('stop-pdf-text').hidden, true);
    result.incomplete_controls.push({ case: 'partial-real-page-replaces-text-but-keeps-error', report: partial.reports[0], summary: partial.summary(), error_preserved: true });
    // The actual helper checks cancellation after in-flight text loading and
    // before assembling that page. Cancel at page one to retain earlier text;
    // cancel at transport page two to retain the already assembled real page.
    for (const cancelAt of [1, 2]) {
      let cancelled, cleanups = 0;
      const cancelledPdf = { numPages: 2, getPage: async number => {
        const page = await pdf.getPage(1);
        return {
          getTextContent: async () => { const content = await page.getTextContent(); if (number === cancelAt) cancelled.stop(); return content; },
          cleanup: () => { cleanups++; page.cleanup(); },
        };
      } };
      cancelled = integration(cancelledPdf, retained);
      await cancelled.recoverPdfText();
      assert.equal(cancelled.reports[0].stopped, true);
      assert.equal(cancelled.reports[0].complete, false);
      assert.deepEqual(cancelled.reports[0].failed, []);
      assert.equal(cancelled.reports[0].pages, cancelAt - 1);
      assert.equal(cancelled.reports[0].text, cancelAt === 1 ? '' : expectedText);
      assert.equal(cancelled.current.inspection.text, cancelAt === 1 ? retained.text : expectedText);
      assert.equal(cancelled.current.inspection.text_error, previewReport.text_error);
      assert(cancelled.summary().startsWith('已停止，保留部分正文'));
      assert.equal(cancelled.el('stop-pdf-text').hidden, true);
      assert.equal(cleanups, cancelAt);
      result.incomplete_controls.push({ case: cancelAt === 1 ? 'cancel-in-flight-first-page-keeps-earlier-text' : 'cancel-in-flight-second-page-keeps-completed-real-page', report: cancelled.reports[0], summary: cancelled.summary(), error_preserved: true, cleanups });
    }
    const rejected = integration(pdf, retained, async () => { throw new Error('Injected extraction transport rejection'); });
    await assert.rejects(rejected.recoverPdfText(), /Injected extraction transport rejection/);
    assert.deepEqual(rejected.current.inspection, retained);
    assert.equal(rejected.summary(), null);
    assert.deepEqual(rejected.notices, []);
    assert.equal(rejected.el('stop-pdf-text').hidden, true);
    result.incomplete_controls.push({ case: 'pre-report-rejection-keeps-state', text_preserved: true, error_preserved: true, no_success_notice: true });
    return result;
  }

  for (const fixture of manifest.fixtures) {
    const source = path.join(corpus, fixture.name);
    const entry = { name: fixture.name, source_url: fixture.source_url, revision: fixture.revision, classification: fixture.classification, identity: report.sources_before[fixture.name] };
    report.fixtures.push(entry);
    const old = recordCommand(`${fixture.name}.baseline`, baseline, ['inspect', source]); entry.baseline = old.evidence;
    const next = recordCommand(`${fixture.name}.candidate`, candidate, ['inspect', source]); entry.candidate = next.evidence;
    const fallback = recordCommand(`${fixture.name}.preview`, preview, [source]); entry.preview = fallback.evidence;
    commandOk(fallback.result, `${fixture.name} public preview`);
    const previewReport = JSON.parse(text(fallback.result.stdout)); inspectShape(previewReport, fixture);
    const texts = {};
    if (fixture.expected_cli.status === 'success') {
      commandOk(old.result, `${fixture.name} baseline`); commandOk(next.result, `${fixture.name} candidate`);
      const oldReport = JSON.parse(text(old.result.stdout)), nextReport = JSON.parse(text(next.result.stdout));
      inspectShape(oldReport, fixture); inspectShape(nextReport, fixture);
      assert.deepEqual(withoutElapsed(nextReport), withoutElapsed(oldReport));
      assert.deepEqual(withoutElapsed(previewReport), withoutElapsed(nextReport));
      assert.equal(nextReport.text_error, undefined); assert.equal(previewReport.text_error, undefined);
      texts.baseline = oldReport.text; texts.candidate = nextReport.text;
      entry.exact_json_and_text_parity_excluding_elapsed = true;
      entry.preview_equals_successful_strict_excluding_elapsed = true;
    } else {
      commandOk(old.result, `${fixture.name} baseline panic`, 101);
      commandOk(next.result, `${fixture.name} candidate strict error`, 1);
      assert.equal(text(old.result.stdout), ''); assert.equal(text(next.result.stdout), '');
      const encodingError = fixture.expected_cli.stderr_contains;
      assert(text(old.result.stderr).includes(encodingError));
      const diagnostic = `PDF text extractor panicked: ${encodingError}`;
      assert(text(next.result.stderr).includes(diagnostic));
      assert(/^Error:/m.test(text(next.result.stderr)), 'Candidate must return an ordinary strict CLI error');
      assert.equal(previewReport.text, '');
      assert.equal(previewReport.text_error, diagnostic);
      assert(previewReport.warnings.includes('No text extracted; this does not prove the document is empty'));
      entry.containment = { baseline_exit: 101, candidate_strict_exit: 1, diagnostic, preview_candidate_only: true, rust_text_support: false };
    }
    const independent = recordCommand(`${fixture.name}.pypdf`, process.env.PYTHON ?? 'python3', ['-c',
      'import json,sys,pypdf; r=pypdf.PdfReader(sys.argv[1]); print(json.dumps({"version":pypdf.__version__,"pages":[p.extract_text() or "" for p in r.pages]},ensure_ascii=True))', source]);
    commandOk(independent.result, `${fixture.name} pypdf`);
    const parsed = JSON.parse(text(independent.result.stdout));
    assert.equal(parsed.pages.length, fixture.pages);
    texts.pypdf = parsed.pages.join('\n');
    entry.pypdf = { ...independent.evidence, version: parsed.version, pages: parsed.pages.length, text: save(`${fixture.name}.pypdf.text.txt`, texts.pypdf) };
    const poppler = recordCommand(`${fixture.name}.poppler`, 'pdftotext', ['-enc', 'UTF-8', source, '-']);
    commandOk(poppler.result, `${fixture.name} Poppler`); texts.poppler = text(poppler.result.stdout); entry.poppler = poppler.evidence;
    entry.anchors = fixture.anchors.map(anchor => {
      const counts = {}, fold = anchor.whitespace_fold ?? true;
      for (const consumer of anchor.consumers) {
        const value = fold ? texts[consumer].trim().split(/\s+/u).join(' ') : texts[consumer];
        counts[consumer] = literalCount(value, anchor.text);
        assert(counts[consumer] >= 1, `${fixture.name}: missing ${consumer} anchor ${anchor.text}`);
      }
      return { text: anchor.text, evidence_only_whitespace_fold: fold, counts };
    });
    if (fixture.expected_cli.status !== 'success') {
      const loading = pdfjs.getDocument({ data: new Uint8Array(readFileSync(source)), isEvalSupported: false, useSystemFonts: true,
        cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps/'), cMapPacked: true,
        standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts/'),
        wasmUrl: path.join(root, 'node_modules/pdfjs-dist/wasm/'), iccUrl: path.join(root, 'node_modules/pdfjs-dist/iccs/'),
      });
      try {
        const pdf = await loading.promise;
        assert.equal(pdf.numPages, fixture.pages);
        const pages = [];
        const recording = { numPages: pdf.numPages, getPage: async number => {
          const page = await pdf.getPage(number);
          return { getTextContent: async () => { const content = await page.getTextContent(); pages.push({ page: number, ...content }); return content; }, cleanup: () => page.cleanup() };
        } };
        const extraction = await helper.extractPdfText(recording, () => false, () => {});
        assert.equal(extraction.complete, true); assert.deepEqual(extraction.failed, []);
        assert.equal(extraction.pages, fixture.pages); assert.equal(extraction.total, fixture.pages);
        assert.equal(extraction.stopped, false); assert.equal(extraction.truncated, false);
        entry.pdfjs = { text: save(`${fixture.name}.pdfjs.txt`, extraction.text), text_items: save(`${fixture.name}.pdfjs-items.json`, json(pages)), report: { ...extraction, text: undefined } };
        let queries;
        if (fixture.name === 'noembed-sjis.pdf') {
          assert.equal(extraction.text, 'あいうえお');
          assert.equal(texts.poppler, 'あいうえお\n\n\f');
          assert(!texts.pypdf.includes('あいうえお'));
          assert(text(independent.result.stderr).includes('90ms-RKSJ-H'));
          queries = [['あいうえお', 1], ['あいうえおか', 0]];
          entry.independent_mapping_scope = 'Exact five-character Japanese anchor agrees with Poppler; pypdf unsupported-encoding warning/misdecode retained. No general Japanese/Chinese font or visual claim.';
        } else {
          assert.equal(fixture.name, 'mixedfonts.pdf');
          const cid = '[CID-0 font] : abcdefghijklmnopqrstuvwxyzABCDEFGHI\nJKLMNOPQRSTUVWXYZ0123456789';
          assert(extraction.text.includes(cid)); assert(texts.poppler.includes(cid));
          assert(!texts.pypdf.includes(cid)); assert(texts.pypdf.includes('\0'));
          assert(text(independent.result.stderr).includes('UniJIS-UTF16-H'));
          const credit = 'Powered by TCPDF (www.tcpdf.org)', page = 'page 1 / 1';
          for (const value of [extraction.text, texts.poppler]) { assert(value.includes(credit)); assert(value.includes(page)); }
          assert(extraction.text.indexOf(credit) < extraction.text.indexOf(page));
          assert(texts.poppler.indexOf(page) < texts.poppler.indexOf(credit));
          queries = [['Font Types', 1], ['[CID-0 font] : abcdefghijklmnopqrstuvwxyzABCDEFGHI', 1], ['font types', 0]];
          entry.independent_mapping_scope = 'CID-0 Latin/alphanumeric content agrees with Poppler; pypdf NUL-corrupts it. PDF.js credit-before-page footer order differs from Poppler. No full-text order or visual equivalence.';
        }
        entry.recovery_integration = await verifyRecovery(pdf, previewReport, extraction.text, queries);
      } finally { await loading.destroy(); }
    }
    assert.deepEqual(identity(source), report.sources_before[fixture.name], `${fixture.name}: source changed`);
    entry.source_unchanged = true;
  }
  report.summary = {
    originals_checked: report.fixtures.length,
    exact_strict_and_preview_success_parity: report.fixtures.filter(item => item.exact_json_and_text_parity_excluding_elapsed).length,
    contained_known_panics_without_rust_text_support: report.fixtures.filter(item => item.containment).length,
    real_pdfjs_recovery_documents: report.fixtures.filter(item => item.recovery_integration).length,
    complete_recovery_repetitions: report.fixtures.reduce((sum, item) => sum + (item.recovery_integration?.repetitions.length ?? 0), 0),
    incomplete_or_rejected_recovery_controls: report.fixtures.reduce((sum, item) => sum + (item.recovery_integration?.incomplete_controls.length ?? 0), 0),
    cancelled_recovery_controls: report.fixtures.reduce((sum, item) => sum + (item.recovery_integration?.incomplete_controls.filter(control => control.report?.stopped).length ?? 0), 0),
    actual_browser_native_acceptance: false,
  };
  report.status = 'passed';
} catch (error) {
  failure = error;
  report.status = 'failed'; report.error = String(error.stack ?? error);
} finally {
  try {
    report.sources_after = {};
    for (const [name, before] of Object.entries(report.sources_before)) {
      report.sources_after[name] = identity(path.join(corpus, name));
      assert.deepEqual(report.sources_after[name], before, `${name}: source identity changed`);
    }
    report.protected_after = {};
    for (const [filename, before] of Object.entries(report.protected_before)) {
      report.protected_after[filename] = identity(filename);
      assert.deepEqual(report.protected_after[filename], before, `${filename}: protected code/binary identity changed`);
    }
    report.identities_unchanged = true;
  } catch (error) {
    failure ??= error;
    report.status = 'failed'; report.final_identity_error = String(error.stack ?? error);
  }
  report.finished_utc = new Date().toISOString();
  save('report.json', json(report));
}
console.log(json({ status: report.status, report: path.join(output, 'report.json'), report_sha256: sha(readFileSync(path.join(output, 'report.json'))), summary: report.summary, error: report.error, final_identity_error: report.final_identity_error }));
if (failure) process.exitCode = 1;
