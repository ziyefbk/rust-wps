// Successor verification for PDF source identity. Historical checkers and their
// guards remain unchanged. Only isolated copies under a new evidence directory
// are replaced; pinned originals are read-only. No Rust builds or native IPC.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { closeSync, fstatSync, mkdirSync, openSync, readFileSync, readSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import ts from 'typescript';
import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs';

const searchSource = readFileSync(new URL('../ui/reading-search.ts', import.meta.url), 'utf8');
const searchJs = ts.transpileModule(searchSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { findReadingMatches } = await import(`data:text/javascript;base64,${Buffer.from(searchJs).toString('base64')}`);

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scriptPath = fileURLToPath(import.meta.url);
const options = {};
const allowed = ['output', 'candidate', 'probe', 'probe-source', 'library', 'provenance', 'corpus-dir'];
for (let at = 2; at < process.argv.length; at += 2) {
  const key = process.argv[at], value = process.argv[at + 1];
  assert(key.startsWith('--') && value && !value.startsWith('--'), 'Use --name value options');
  assert(allowed.includes(key.slice(2)), `Unknown option: ${key}`);
  assert(!(key.slice(2) in options), `Duplicate option: ${key}`);
  options[key.slice(2)] = value;
}
assert(options.output, '--output must be a new directory');
assert(options.provenance, '--provenance must identify the actual candidate build');
const resolve = (key, fallback) => path.resolve(root, options[key] ?? fallback);
const output = resolve('output');
const corpus = resolve('corpus-dir', 'tmp/pdf-gap-investigation');
const candidate = resolve('candidate', 'tmp/pdf-session-final-cli');
const probe = resolve('probe', 'tmp/pdf-session-probe');
const probeSource = resolve('probe-source', 'scripts/pdf-session-probe.rs');
const library = resolve('library', 'target/debug/librust_wps.rlib');
const provenancePath = resolve('provenance');
const baseline = path.join(root, 'tmp/pdf-panic-final-cli');
const baselinePreview = path.join(root, 'tmp/pdf-panic-preview');
const baselineRef = '9431f8485796cf963cc49f2a935f1c1a6b64ab7d';
const baselineHash = 'c92bdffaeeba9fa823d03ed53c141b284645ac70a20ff002c5110e2918274976';
const baselinePreviewHash = '7ad0cd45ba1098eb104e1816d9f085f86653a3893bc032c843b3f8f4bd36af18';
const manifestPath = path.join(root, 'scripts/pdf-snapshot-corpus.json');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
const text = bytes => (bytes ?? Buffer.alloc(0)).toString('utf8');
const bytesIdentity = bytes => ({ bytes: bytes.length, sha256: sha(bytes), git_blob_sha: createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex') });
// Hash large frozen executables/libraries without allocating another whole-file
// buffer. A changed file length is an evidence failure, never silently ignored.
const identity = filename => {
  const fd = openSync(filename, 'r');
  try {
    const size = fstatSync(fd).size, digest = createHash('sha256');
    const git = createHash('sha1').update(`blob ${size}\0`), chunk = Buffer.alloc(64 * 1024);
    let bytes = 0;
    while (bytes < size) {
      const count = readSync(fd, chunk, 0, Math.min(chunk.length, size - bytes), null);
      assert(count > 0, `${filename}: file shrank during hashing`);
      digest.update(chunk.subarray(0, count)); git.update(chunk.subarray(0, count)); bytes += count;
    }
    assert.equal(readSync(fd, chunk, 0, 1, null), 0, `${filename}: file grew during hashing`);
    return { bytes, sha256: digest.digest('hex'), git_blob_sha: git.digest('hex') };
  } finally { closeSync(fd); }
};
const withoutElapsed = ({ elapsed_ms, ...rest }) => rest;
const save = (filename, bytes) => {
  writeFileSync(path.join(output, filename), bytes, { flag: 'wx' });
  return { file: filename, bytes: Buffer.byteLength(bytes), sha256: sha(bytes) };
};
const run = (program, args, input) => spawnSync(program, args, { cwd: root, input, timeout: 60000, maxBuffer: 16 * 1024 * 1024 });
const commandOk = (result, label, expected = 0) => {
  assert.equal(result.error, undefined, `${label}: subprocess execution error ${result.error}`);
  assert.equal(result.signal, null, `${label}: unexpected signal ${result.signal}`);
  assert.equal(result.status, expected, `${label}: expected exit ${expected}, got ${result.status}: ${text(result.stderr)}`);
};
const command = (prefix, program, args, input) => {
  const result = run(program, args, input);
  const evidence = {
    program, args, status: result.status, signal: result.signal,
    stdout: save(`${prefix}.stdout`, result.stdout ?? Buffer.alloc(0)),
    stderr: save(`${prefix}.stderr.txt`, result.stderr ?? Buffer.alloc(0)),
    ...(input === undefined ? {} : { input: save(`${prefix}.input.json`, input) }),
    ...(result.error ? { execution_error: String(result.error) } : {}),
  };
  return { result, evidence };
};
const successJson = (prefix, program, args, input) => {
  const result = command(prefix, program, args, input);
  commandOk(result.result, prefix);
  return { ...result, value: JSON.parse(text(result.result.stdout)) };
};
const checkIdentity = (value, expected) => {
  assert.deepEqual(Object.keys(value).sort(), ['bytes', 'sha256']);
  assert.equal(value.bytes, expected.bytes);
  assert(Array.isArray(value.sha256) && value.sha256.length === 32);
  assert(value.sha256.every(byte => Number.isInteger(byte) && byte >= 0 && byte <= 255));
  assert.equal(Buffer.from(value.sha256).toString('hex'), expected.sha256);
};
const checkInspection = (value, expected) => {
  assert.equal(value.format, 'pdf');
  assert.equal(value.backend, 'pdf-extract 0.12.1');
  assert.equal(value.bytes, expected.bytes);
  assert(Number.isSafeInteger(value.elapsed_ms) && value.elapsed_ms >= 0);
  assert.equal(typeof value.text, 'string');
  assert(Array.isArray(value.warnings) && value.warnings.every(warning => typeof warning === 'string'));
};
const mismatchMarker = 'PDF 源文件已改变，请重新打开文档后再预览';
const verified = (prefix, source, expected, accepted) => {
  const call = command(prefix, probe, ['verify', source, JSON.stringify(expected)]);
  if (accepted !== undefined) {
    commandOk(call.result, prefix);
    assert.deepEqual(call.result.stdout, accepted, `${prefix}: delivered bytes differ`);
    call.evidence.exact_returned_bytes = bytesIdentity(call.result.stdout);
  } else {
    commandOk(call.result, prefix, 1);
    assert.equal(call.result.stdout.length, 0, `${prefix}: rejected source leaked bytes`);
    assert(text(call.result.stderr).includes(mismatchMarker), `${prefix}: missing specific source-change error`);
    call.evidence.source_change_rejected_without_bytes = true;
  }
  return call;
};

// Refuse before any extractor invocation. Each run, including failures, keeps
// its own immutable evidence rather than overwriting an earlier report.
mkdirSync(output, { recursive: false });
const report = {
  status: 'running', started_utc: new Date().toISOString(), baseline_ref: baselineRef,
  runtime: { node: process.version, pdfjs: pdfjs.version, adapter: 'legacy/build/pdf.mjs' },
  limits: [
    'Core preview APIs and the actual document worker execute in a standalone transport; no Tauri command, IPC, native dialogs, browser, WebView2 or page-rendering acceptance.',
    'Session-token state transitions and concurrency gates belong to the separate production-module test runner; this checker independently proves source identity, raw returned bytes and worker envelope compatibility.',
    'The check hashes the exact returned bounded buffer. It does not establish atomic filesystem snapshots, a history of source changes, decoded-resource limits or second-read deadlines.',
    'A-to-B-to-A restoration may pass because the original byte identity is restored. Sources changed after PDF.js loading do not mutate its already-loaded bytes.',
    'SHA-256 is a content identity check, not authenticity, PDF validity, successful text extraction, or a replacement for parser isolation.',
    'Only isolated source copies under this new evidence directory are changed. The six pinned originals, frozen executables, historical checkers and previous reports are not rewritten.',
    'PDF.js uses the installed Node legacy adapter. Actual main.ts recovery/search functions use a minimal DOM transport double; this is not rendered UI or automatic page-opening acceptance.',
    'Independent-reader anchors do not imply whole-text order, font mapping, visual fidelity, OCR, or whole DOC/DOCX/PDF completion.',
  ],
  fixtures: [], controls: [], non_pdf: [], protected_before: {}, sources_before: {},
};
let failure;
try {
  const manifestBytes = readFileSync(manifestPath), manifest = JSON.parse(manifestBytes);
  assert.equal(manifest.fixtures.length, 6);
  report.manifest = save('corpus-manifest.json', manifestBytes);
  report.checker = save('check-pdf-session-identity.mjs', readFileSync(scriptPath));
  const unchanged = [
    'Cargo.toml', 'Cargo.lock', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'package.json', 'package-lock.json',
    'ui/pdf-text.ts', 'scripts/pdf-snapshot-corpus.json', 'scripts/corpus.json', 'scripts/check-pdf-snapshot.py',
    'scripts/check-pdf-phrase-search.mjs', 'scripts/check-pdf-panic-recovery.mjs', 'scripts/pdf-preview-probe.rs',
    'tests/fixtures/encrypted.pdf', 'tests/fixtures/encrypted-text.pdf',
  ];
  report.unchanged_from_baseline = {};
  for (const name of unchanged) {
    const frozen = run('git', ['show', `${baselineRef}:${name}`]);
    commandOk(frozen, `Read frozen ${name}`);
    const current = readFileSync(path.join(root, name));
    assert.deepEqual(current, frozen.stdout, `${name}: historical contract changed`);
    report.unchanged_from_baseline[name] = bytesIdentity(current);
  }
  const production = ['src/lib.rs', 'src/pdf_snapshot.rs', 'src-tauri/src/main.rs', 'src-tauri/src/worker.rs',
    'src-tauri/src/document_protocol.rs', 'src-tauri/src/document_session.rs', 'ui/main.ts'];
  const historicalReports = ['tmp/pdf-panic-recovery-final/report.json', 'tmp/pdf-snapshot-validation/final-independent/report.json',
    'tmp/pdf-gap-investigation/source-snapshot-race.json'];
  for (const filename of [baseline, baselinePreview, candidate, probe, probeSource, library, scriptPath, provenancePath,
    ...unchanged.map(name => path.join(root, name)), ...production.map(name => path.join(root, name)),
    ...historicalReports.map(name => path.join(root, name))]) {
    report.protected_before[filename] = identity(filename);
  }
  assert.equal(report.protected_before[baseline].sha256, baselineHash, 'Frozen panic CLI changed');
  assert.equal(report.protected_before[baselinePreview].sha256, baselinePreviewHash, 'Frozen panic preview changed');
  assert.notEqual(report.protected_before[candidate].sha256, baselineHash, 'Candidate is the frozen baseline executable');
  const provenanceBytes = readFileSync(provenancePath), provenance = JSON.parse(provenanceBytes);
  assert.equal(provenance.base_commit, baselineRef);
  for (const filename of [candidate, probe, probeSource, library, ...production.map(name => path.join(root, name))]) {
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
  report.build_provenance = save('build-provenance.json', provenanceBytes);
  report.build_provenance_identities_verified = true;
  report.binary_identity = Object.fromEntries([baseline, baselinePreview, candidate, probe].map(filename => [path.relative(root, filename), report.protected_before[filename]]));
  for (const fixture of manifest.fixtures) {
    assert.equal(path.basename(fixture.name), fixture.name);
    const before = identity(path.join(corpus, fixture.name));
    assert.deepEqual(before, Object.fromEntries(['bytes', 'sha256', 'git_blob_sha'].map(key => [key, fixture[key]])), `${fixture.name}: source pin mismatch`);
    report.sources_before[fixture.name] = before;
  }

  const helperSource = readFileSync(path.join(root, 'ui/pdf-text.ts'), 'utf8');
  const helperJs = ts.transpileModule(helperSource, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  const helper = await import(`data:text/javascript;base64,${Buffer.from(helperJs).toString('base64')}`);
  const mainSource = readFileSync(path.join(root, 'ui/main.ts'), 'utf8');
  const ast = ts.createSourceFile('ui/main.ts', mainSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
  const actualFunctions = ['recoverPdfText', 'renderText'].map(name => {
    const node = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert(node, `Missing actual application function ${name}`); return node.getText(ast);
  }).join('\n');
  report.actual_main_functions = save('actual-main-functions.ts', actualFunctions);
  const integrationJs = ts.transpileModule(actualFunctions, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  function integration(sourcePdf, inspection, extractor = helper.extractPdfText) {
    const elements = new Map(), notices = [], reports = [];
    const el = id => {
      if (!elements.has(id)) elements.set(id, { value: '', textContent: '', hidden: false, nodes: [], replaceChildren() { this.nodes = []; this.textContent = ''; }, append(node) { this.nodes.push(node); } });
      return elements.get(id);
    };
    const document = { createTextNode: value => ({ textContent: value }), createElement: tag => ({ tag, textContent: '' }) };
    const current = { inspection: structuredClone(inspection), session_token: '1' };
    const trackedExtract = async (...args) => { const result = await extractor(...args); reports.push(structuredClone(result)); return result; };
    const build = new Function('findReadingMatches', 'extractPdfText', 'textSummary', 'initialPdf', 'initialCurrent', 'document', 'el', 'notice',
      `let pdf = initialPdf, current = initialCurrent, stopText = false, pdfTextSummary = null; ${integrationJs}; return { recoverPdfText, renderText, summary: () => pdfTextSummary, stop: () => { stopText = true; }, replace: (next) => { current = next; } };`);
    return { ...build(findReadingMatches, trackedExtract, helper.textSummary, sourcePdf, current, document, el, message => notices.push(message)), el, current, notices, reports };
  }
  async function verifyRecovery(sourcePdf, inspection, expectedText, queries) {
    const app = integration(sourcePdf, inspection);
    const results = [];
    for (let attempt = 1; attempt <= 2; attempt++) {
      await app.recoverPdfText();
      assert.equal(app.current.inspection.text, expectedText);
      assert(!('text_error' in app.current.inspection));
      assert.equal(app.reports.at(-1).complete, true);
      assert.equal(app.el('stop-pdf-text').hidden, true);
      const matches = {};
      for (const [phrase, expected] of queries) {
        app.el('search').value = phrase; app.renderText();
        matches[phrase] = app.el('text-content').nodes.filter(node => node.tag === 'mark').length;
        assert.equal(matches[phrase], expected, `Literal search ${phrase}`);
      }
      results.push({ attempt, matches, report: app.reports.at(-1), summary: app.summary() });
    }
    const incomplete = [], retained = { ...inspection, text: 'Earlier retained extraction' };
    const failedPdf = { numPages: 2, getPage: async () => { throw new Error('Injected page acquisition failure'); } };
    for (const initial of [inspection, retained]) {
      const failed = integration(failedPdf, initial);
      await failed.recoverPdfText();
      assert.deepEqual(failed.current.inspection, initial);
      assert.equal(failed.reports[0].complete, false); assert.equal(failed.reports[0].text, '');
      assert.deepEqual(failed.reports[0].failed, [1, 2]);
      assert(!failed.summary().startsWith('已提取')); assert.equal(failed.el('stop-pdf-text').hidden, true);
      incomplete.push({ case: initial.text ? 'empty-failure-keeps-earlier-text' : 'empty-failure-keeps-preview-error', report: failed.reports[0], summary: failed.summary() });
    }
    const partialPdf = { numPages: 2, getPage: async number => { if (number === 2) throw new Error('Injected second-page failure'); return sourcePdf.getPage(1); } };
    const partial = integration(partialPdf, retained);
    await partial.recoverPdfText();
    assert.equal(partial.current.inspection.text, expectedText); assert.equal(partial.current.inspection.text_error, inspection.text_error);
    assert.equal(partial.reports[0].complete, false); assert.deepEqual(partial.reports[0].failed, [2]); assert.equal(partial.reports[0].pages, 1);
    assert(!partial.summary().startsWith('已提取'));
    incomplete.push({ case: 'partial-real-page-keeps-error', report: partial.reports[0], summary: partial.summary() });
    for (const cancelAt of [1, 2]) {
      let cancelled, cleanups = 0;
      const cancelledPdf = { numPages: 2, getPage: async number => {
        const page = await sourcePdf.getPage(1);
        return { getTextContent: async () => { const content = await page.getTextContent(); if (number === cancelAt) cancelled.stop(); return content; }, cleanup: () => { cleanups++; page.cleanup(); } };
      } };
      cancelled = integration(cancelledPdf, retained); await cancelled.recoverPdfText();
      assert.equal(cancelled.reports[0].stopped, true); assert.equal(cancelled.reports[0].complete, false);
      assert.deepEqual(cancelled.reports[0].failed, []); assert.equal(cancelled.reports[0].pages, cancelAt - 1);
      assert.equal(cancelled.current.inspection.text, cancelAt === 1 ? retained.text : expectedText);
      assert.equal(cancelled.current.inspection.text_error, inspection.text_error);
      assert(cancelled.summary().startsWith('已停止，保留部分正文')); assert.equal(cleanups, cancelAt);
      assert.equal(cancelled.el('stop-pdf-text').hidden, true);
      incomplete.push({ case: `cancel-in-flight-page-${cancelAt}`, report: cancelled.reports[0], summary: cancelled.summary(), cleanups });
    }
    const rejected = integration(sourcePdf, retained, async () => { throw new Error('Injected extraction transport rejection'); });
    await assert.rejects(rejected.recoverPdfText(), /Injected extraction transport rejection/);
    assert.deepEqual(rejected.current.inspection, retained); assert.equal(rejected.summary(), null); assert.deepEqual(rejected.notices, []);
    assert.equal(rejected.el('stop-pdf-text').hidden, true);
    incomplete.push({ case: 'pre-report-rejection-keeps-state', text_preserved: true, no_success_notice: true });
    let stale;
    const next = { session_token: '2', inspection: { ...inspection, text: 'New current document' } };
    const stalePdf = { numPages: 1, getPage: async () => {
      const page = await sourcePdf.getPage(1);
      return { getTextContent: async () => { const content = await page.getTextContent(); stale.replace(next); return content; }, cleanup: () => page.cleanup() };
    } };
    stale = integration(stalePdf, retained); await stale.recoverPdfText();
    assert.deepEqual(stale.current.inspection, retained); assert.equal(next.inspection.text, 'New current document');
    assert.deepEqual(stale.notices, []); assert.equal(stale.summary(), null);
    assert.equal(stale.reports[0].stopped, true); assert.equal(stale.reports[0].complete, false);
    incomplete.push({ case: 'superseded-owner-never-accepts-late-text', report: stale.reports[0], old_and_new_text_preserved: true, no_success_notice: true });
    return { transport: 'Actual main.ts recovery/search functions, actual helper and real PDF.js document, minimal DOM double', repetitions: results, incomplete_controls: incomplete };
  }

  for (const fixture of manifest.fixtures) {
    const source = path.join(corpus, fixture.name), original = readFileSync(source);
    const entry = { name: fixture.name, source_url: fixture.source_url, revision: fixture.revision, classification: fixture.classification, identity: report.sources_before[fixture.name] };
    report.fixtures.push(entry);
    const old = command(`${fixture.name}.baseline`, baseline, ['inspect', source]); entry.baseline = old.evidence;
    const next = command(`${fixture.name}.candidate`, candidate, ['inspect', source]); entry.candidate = next.evidence;
    const oldPreview = successJson(`${fixture.name}.baseline-preview`, baselinePreview, [source]); entry.baseline_preview = oldPreview.evidence;
    const oldApi = successJson(`${fixture.name}.old-preview-api`, probe, ['inspect-preview', source]); entry.old_preview_api = oldApi.evidence;
    const wrapped = successJson(`${fixture.name}.identity-preview`, probe, ['preview', source]); entry.identity_preview = wrapped.evidence;
    const opened = successJson(`${fixture.name}.worker-open`, probe, ['open', source]); entry.worker_open = opened.evidence;
    assert.equal(opened.value.result, 'opened');
    assert.deepEqual(Object.keys(wrapped.value).sort(), ['inspection', 'pdf_identity']);
    for (const inspection of [oldPreview.value, oldApi.value, wrapped.value.inspection, opened.value.document.inspection]) {
      checkInspection(inspection, fixture);
      assert.deepEqual(withoutElapsed(inspection), withoutElapsed(oldPreview.value), `${fixture.name}: preview compatibility`);
    }
    checkIdentity(wrapped.value.pdf_identity, entry.identity);
    checkIdentity(opened.value.pdf_identity, entry.identity);
    assert.deepEqual(wrapped.value.pdf_identity, opened.value.pdf_identity);
    assert.equal(opened.value.document.path, source);
    assert.equal(opened.value.document.name, fixture.name);
    assert(!('pdf_identity' in opened.value.document.inspection));
    assert(opened.result.stdout.length <= 32 * 1024 * 1024, 'Worker response limit changed');
    const identityMetadataBytes = Buffer.byteLength(JSON.stringify(wrapped.value.pdf_identity));
    assert(identityMetadataBytes <= 160, 'Identity metadata is unexpectedly large');
    entry.worker_response_bytes = opened.result.stdout.length; entry.identity_metadata_bytes = identityMetadataBytes;
    entry.exact_preview_json_parity_excluding_elapsed = true;
    entry.identity_equals_independent_sha256 = true;
    const texts = {};
    if (fixture.expected_cli.status === 'success') {
      commandOk(old.result, `${fixture.name} strict baseline`); commandOk(next.result, `${fixture.name} strict candidate`);
      const previous = JSON.parse(text(old.result.stdout)), current = JSON.parse(text(next.result.stdout));
      assert.deepEqual(withoutElapsed(current), withoutElapsed(previous));
      assert.deepEqual(withoutElapsed(current), withoutElapsed(oldPreview.value));
      texts.baseline = previous.text; texts.candidate = current.text;
      entry.exact_strict_json_and_text_parity_excluding_elapsed = true;
    } else {
      commandOk(old.result, `${fixture.name} contained strict baseline`, 1);
      commandOk(next.result, `${fixture.name} contained strict candidate`, 1);
      assert.equal(old.result.stdout.length, 0); assert.equal(next.result.stdout.length, 0);
      const diagnostic = `PDF text extractor panicked: ${fixture.expected_cli.stderr_contains}`;
      for (const call of [old, next]) {
        assert(text(call.result.stderr).includes(diagnostic));
        assert(/^Error:/m.test(text(call.result.stderr)));
      }
      assert.equal(wrapped.value.inspection.text, '');
      assert.equal(wrapped.value.inspection.text_error, diagnostic);
      entry.contained_panic_policy_preserved = { strict_exit: 1, diagnostic, rust_text_support: false };
    }
    const delivered = verified(`${fixture.name}.verified-bytes`, source, wrapped.value.pdf_identity, original);
    entry.verified_delivery = delivered.evidence;
    const independent = successJson(`${fixture.name}.pypdf`, process.env.PYTHON ?? 'python3', ['-c',
      'import json,sys,pypdf; r=pypdf.PdfReader(sys.argv[1]); print(json.dumps({"version":pypdf.__version__,"pages":[p.extract_text() or "" for p in r.pages]},ensure_ascii=True))', source]);
    assert.equal(independent.value.pages.length, fixture.pages);
    texts.pypdf = independent.value.pages.join('\n'); entry.pypdf = { ...independent.evidence, version: independent.value.version };
    const poppler = command(`${fixture.name}.poppler`, 'pdftotext', ['-enc', 'UTF-8', source, '-']);
    commandOk(poppler.result, `${fixture.name} Poppler`); texts.poppler = text(poppler.result.stdout); entry.poppler = poppler.evidence;
    entry.anchors = fixture.anchors.map(anchor => {
      const counts = {}, fold = anchor.whitespace_fold ?? true;
      for (const consumer of anchor.consumers) {
        const value = fold ? texts[consumer].trim().split(/\s+/u).join(' ') : texts[consumer];
        counts[consumer] = value.split(anchor.text).length - 1;
        assert(counts[consumer] >= 1, `${fixture.name}: missing ${consumer} anchor ${anchor.text}`);
      }
      return { text: anchor.text, evidence_only_whitespace_fold: fold, counts };
    });
    const loading = pdfjs.getDocument({ data: new Uint8Array(delivered.result.stdout), isEvalSupported: false, useSystemFonts: true,
      cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps/'), cMapPacked: true,
      standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts/'),
      wasmUrl: path.join(root, 'node_modules/pdfjs-dist/wasm/'), iccUrl: path.join(root, 'node_modules/pdfjs-dist/iccs/'),
    });
    try {
      const pdf = await loading.promise;
      assert.equal(pdf.numPages, fixture.pages);
      const extraction = await helper.extractPdfText(pdf, () => false, () => {});
      assert.equal(extraction.complete, true); assert.deepEqual(extraction.failed, []);
      assert.equal(extraction.pages, fixture.pages); assert.equal(extraction.stopped, false); assert.equal(extraction.truncated, false);
      entry.pdfjs_from_verified_bytes = { text: save(`${fixture.name}.pdfjs.txt`, extraction.text), report: { ...extraction, text: undefined } };
      if (fixture.expected_cli.status !== 'success') {
        const queries = fixture.name === 'noembed-sjis.pdf'
          ? [['あいうえお', 1], ['あいうえおか', 0]]
          : [['Font Types', 1], ['[CID-0 font] : abcdefghijklmnopqrstuvwxyzABCDEFGHI', 1], ['font types', 0]];
        for (const [needle, count] of queries) if (count) assert(texts.poppler.includes(needle));
        entry.recovery_integration = await verifyRecovery(pdf, wrapped.value.inspection, extraction.text, queries);
      }
    } finally { await loading.destroy(); }
    assert.deepEqual(identity(source), entry.identity); entry.source_unchanged = true;
  }

  // Deterministic pathname replacement, without touching either real original.
  const sourceA = readFileSync(path.join(corpus, 'basicapi.pdf'));
  const sourceB = readFileSync(path.join(corpus, 'copy_paste_ligatures.pdf'));
  const selected = path.join(output, 'selected.pdf');
  writeFileSync(selected, sourceA, { flag: 'wx' });
  const openA = successJson('replacement.open-A', probe, ['open', selected]);
  checkIdentity(openA.value.pdf_identity, bytesIdentity(sourceA));
  const acceptedEnvelope = successJson('envelope.valid', probe, ['validate-open', selected, JSON.stringify(openA.value)]);
  assert.deepEqual(acceptedEnvelope.value, { session_token: '1' });
  report.controls.push({ case: 'actual-worker-envelope-commits-to-production-session', response: acceptedEnvelope.evidence });
  const envelopeControls = [
    ['missing-identity', value => { delete value.pdf_identity; }, 'PDF 源身份无效'],
    ['null-identity', value => { value.pdf_identity = null; }, 'PDF 源身份无效'],
    ['identity-length-mismatch', value => { value.pdf_identity.bytes++; }, 'PDF 源身份无效'],
    ['identity-over-limit', value => { value.pdf_identity.bytes = value.document.inspection.bytes = 64 * 1024 * 1024 + 1; }, 'PDF 源身份无效'],
    ['identity-too-short', value => { value.pdf_identity.bytes = value.document.inspection.bytes = 4; }, 'PDF 源身份无效'],
    ['non-pdf-with-identity', value => { value.document.inspection.format = 'docx'; }, 'PDF 源身份无效'],
    ['wrong-document-path', value => { value.document.path += '.other'; }, '文档路径或格式不匹配'],
    ['short-digest', value => { value.pdf_identity.sha256.pop(); }, 'invalid length'],
    ['unknown-identity-field', value => { value.pdf_identity.extra = true; }, 'unknown field'],
    ['unknown-envelope-field', value => { value.extra = true; }, 'unknown field'],
  ];
  for (const [name, mutate, diagnostic] of envelopeControls) {
    const value = structuredClone(openA.value); mutate(value);
    const rejected = command(`envelope.${name}`, probe, ['validate-open', selected, JSON.stringify(value)]);
    commandOk(rejected.result, name, 1); assert.equal(rejected.result.stdout.length, 0);
    assert(text(rejected.result.stderr).includes(diagnostic), `${name}: unrelated failure`);
    report.controls.push({ case: `production-session-envelope-${name}`, rejection: rejected.evidence });
  }
  const forgedToken = structuredClone(openA.value); forgedToken.document.session_token = '999';
  const overwrittenToken = successJson('envelope.ignores-worker-token', probe, ['validate-open', selected, JSON.stringify(forgedToken)]);
  assert.deepEqual(overwrittenToken.value, { session_token: '1' });
  report.controls.push({ case: 'production-session-does-not-trust-worker-token', response: overwrittenToken.evidence });
  renameSync(selected, path.join(output, 'retained-A.pdf'));
  writeFileSync(selected, sourceB, { flag: 'wx' });
  const rejectB = verified('replacement.reject-B', selected, openA.value.pdf_identity);
  const openB = successJson('replacement.reopen-B', probe, ['open', selected]);
  checkIdentity(openB.value.pdf_identity, bytesIdentity(sourceB));
  const acceptB = verified('replacement.accept-B-after-reopen', selected, openB.value.pdf_identity, sourceB);
  unlinkSync(selected); writeFileSync(selected, sourceA, { flag: 'wx' });
  const restoreA = verified('replacement.exact-A-restored', selected, openA.value.pdf_identity, sourceA);
  report.controls.push({ case: 'unchanged-real-A-to-real-B-requires-reopen', open_A: openA.evidence, rejected_B: rejectB.evidence, reopen_B: openB.evidence, accepted_B: acceptB.evidence,
    restored_A: restoreA.evidence, historical_change_tracking: false, originals_changed: false });
  const equalLength = Buffer.from(sourceA); equalLength[equalLength.length - 1] ^= 1;
  const changed = path.join(output, 'equal-length-control.pdf'); writeFileSync(changed, equalLength, { flag: 'wx' });
  assert.equal(equalLength.length, sourceA.length); assert.notEqual(sha(equalLength), sha(sourceA));
  report.controls.push({ case: 'isolated-same-length-byte-change', fixture_kind: 'mutated scratch copy; not an unchanged upstream original', identity: bytesIdentity(equalLength),
    rejection: verified('same-length.reject', changed, openA.value.pdf_identity).evidence });
  const wrongDigest = { ...openA.value.pdf_identity, sha256: [...openA.value.pdf_identity.sha256] }; wrongDigest.sha256[0] ^= 1;
  report.controls.push({ case: 'wrong-expected-digest', rejection: verified('wrong-digest.reject', selected, wrongDigest).evidence });
  const wrongLength = { ...openA.value.pdf_identity, bytes: openA.value.pdf_identity.bytes + 1 };
  report.controls.push({ case: 'wrong-expected-length', rejection: verified('wrong-length.reject', selected, wrongLength).evidence });
  for (const [name, value] of [['missing-digest', { bytes: sourceA.length }], ['short-digest', { bytes: sourceA.length, sha256: [0] }]]) {
    const rejected = command(`identity-shape.${name}`, probe, ['verify', selected, JSON.stringify(value)]);
    commandOk(rejected.result, name, 1); assert.equal(rejected.result.stdout.length, 0);
    assert(/missing field|invalid length/i.test(text(rejected.result.stderr)), `${name}: unrelated failure`);
    report.controls.push({ case: name, rejection: rejected.evidence });
  }
  const unknown = successJson('worker.unknown-operation', probe, ['--document-worker'], JSON.stringify({ operation: 'invalid', path: selected }));
  assert.equal(unknown.value.result, 'error'); assert(/unknown variant/i.test(unknown.value.message));
  report.controls.push({ case: 'actual-worker-unknown-operation', response: unknown.evidence });
  const invalidSignature = path.join(output, 'invalid-signature.pdf');
  writeFileSync(invalidSignature, 'not a PDF', { flag: 'wx' });
  for (const [name, source, detail] of [['invalid-signature', invalidSignature, 'File signature'], ['missing-source', path.join(output, 'missing.pdf'), 'Cannot open']]) {
    const rejected = command(`read-error.${name}`, probe, ['verify', source, JSON.stringify(openA.value.pdf_identity)]);
    commandOk(rejected.result, name, 1); assert.equal(rejected.result.stdout.length, 0);
    assert(text(rejected.result.stderr).includes('PDF 源文件无法验证，请重新打开文档后再预览'));
    assert(text(rejected.result.stderr).includes(detail), `${name}: unrelated failure`);
    report.controls.push({ case: `${name}-rejected-with-reopen-guidance`, rejection: rejected.evidence });
  }
  const malformedSource = path.join(output, 'malformed-preview-control.pdf'), malformedBytes = Buffer.from('%PDF-1.7\ninvalid');
  writeFileSync(malformedSource, malformedBytes, { flag: 'wx' });
  const malformed = successJson('malformed.preview-candidate', probe, ['preview', malformedSource]);
  assert.equal(malformed.value.inspection.text, ''); assert(malformed.value.inspection.text_error);
  checkIdentity(malformed.value.pdf_identity, bytesIdentity(malformedBytes));
  const malformedDelivery = verified('malformed.verified-byte-identity', malformedSource, malformed.value.pdf_identity, malformedBytes);
  const malformedLoading = pdfjs.getDocument({ data: new Uint8Array(malformedDelivery.result.stdout), isEvalSupported: false });
  try { await assert.rejects(malformedLoading.promise, /Invalid PDF structure/); }
  finally { await malformedLoading.destroy(); }
  report.controls.push({ case: 'malformed-preview-byte-identity-does-not-claim-PDF-validity', fixture_kind: 'isolated synthetic malformed control',
    preview: malformed.evidence, verified_delivery: malformedDelivery.evidence, pdfjs_rejected_invalid_structure: true });

  const docManifest = JSON.parse(readFileSync(path.join(root, 'scripts/corpus.json')));
  for (const name of ['simple.doc', 'SampleDoc.doc', 'SampleDoc.docx']) {
    const fixture = docManifest.fixtures.find(item => item.name === name); assert(fixture);
    const source = path.join(root, 'tmp/corpus', name), before = identity(source);
    assert.equal(before.bytes, fixture.bytes); assert.equal(before.sha256, fixture.sha256);
    report.protected_before[source] = before;
    const old = successJson(`${name}.baseline`, baseline, ['inspect', source]);
    const next = successJson(`${name}.candidate`, candidate, ['inspect', source]);
    const preview = successJson(`${name}.preview`, probe, ['preview', source]);
    const opened = successJson(`${name}.worker-open`, probe, ['open', source]);
    assert.deepEqual(withoutElapsed(next.value), withoutElapsed(old.value));
    assert.deepEqual(withoutElapsed(preview.value.inspection), withoutElapsed(old.value));
    assert.equal(preview.value.pdf_identity ?? null, null);
    assert.equal(opened.value.result, 'opened'); assert(!('pdf_identity' in opened.value));
    report.non_pdf.push({ name, identity: before, baseline: old.evidence, candidate: next.evidence, preview: preview.evidence, worker_open: opened.evidence,
      exact_strict_and_preview_json_parity_excluding_elapsed: true, worker_pdf_identity_absent: true });
  }
  for (const name of ['encrypted.pdf', 'encrypted-text.pdf']) {
    const source = path.join(root, 'tests/fixtures', name), original = readFileSync(source);
    const old = successJson(`${name}.baseline-preview`, baselinePreview, [source]);
    const preview = successJson(`${name}.identity-preview`, probe, ['preview', source]);
    assert.deepEqual(withoutElapsed(preview.value.inspection), withoutElapsed(old.value));
    assert.equal(preview.value.inspection.text, ''); assert(preview.value.inspection.text_error);
    checkIdentity(preview.value.pdf_identity, bytesIdentity(original));
    const bytes = verified(`${name}.verified-bytes`, source, preview.value.pdf_identity, original);
    report.controls.push({ case: 'encrypted-preview-keeps-identity', name, baseline_preview: old.evidence, preview: preview.evidence, verified_delivery: bytes.evidence,
      password_entry_or_rendering_tested: false });
  }
  report.summary = {
    unchanged_originals: report.fixtures.length,
    exact_successful_strict_json_parity: report.fixtures.filter(item => item.exact_strict_json_and_text_parity_excluding_elapsed).length,
    exact_preview_json_parity: report.fixtures.filter(item => item.exact_preview_json_parity_excluding_elapsed).length,
    independently_verified_worker_fingerprints: report.fixtures.filter(item => item.identity_equals_independent_sha256).length,
    exact_raw_verified_original_deliveries: report.fixtures.filter(item => item.verified_delivery).length,
    real_pdfjs_documents_from_verified_bytes: report.fixtures.filter(item => item.pdfjs_from_verified_bytes).length,
    panic_recovery_documents: report.fixtures.filter(item => item.recovery_integration).length,
    complete_recovery_repetitions: report.fixtures.reduce((sum, item) => sum + (item.recovery_integration?.repetitions.length ?? 0), 0),
    incomplete_recovery_controls: report.fixtures.reduce((sum, item) => sum + (item.recovery_integration?.incomplete_controls.length ?? 0), 0),
    controls: report.controls.length, non_pdf_parity_sources: report.non_pdf.length,
    actual_browser_native_acceptance: false,
  };
  report.status = 'passed';
} catch (error) {
  failure = error; report.status = 'failed'; report.error = String(error.stack ?? error);
} finally {
  try {
    report.sources_after = {};
    for (const [name, before] of Object.entries(report.sources_before)) {
      report.sources_after[name] = identity(path.join(corpus, name));
      assert.deepEqual(report.sources_after[name], before, `${name}: original changed`);
    }
    report.protected_after = {};
    for (const [filename, before] of Object.entries(report.protected_before)) {
      report.protected_after[filename] = identity(filename);
      assert.deepEqual(report.protected_after[filename], before, `${filename}: protected identity changed`);
    }
    report.identities_unchanged = true;
  } catch (error) {
    failure ??= error; report.status = 'failed'; report.final_identity_error = String(error.stack ?? error);
  }
  report.finished_utc = new Date().toISOString();
  save('report.json', json(report));
}
console.log(json({ status: report.status, report: path.join(output, 'report.json'), report_sha256: sha(readFileSync(path.join(output, 'report.json'))), summary: report.summary, error: report.error, final_identity_error: report.final_identity_error }));
if (failure) process.exitCode = 1;
