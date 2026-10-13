"""Independent DOCX evidence for registered heading shortcuts and input plugins.

Reuses untouched, hash-pinned source files from the prior toolbar evidence run.
Calls actual full Tiptap extension-manager plugins, then the frozen Rust CLI.
Independently reads OOXML/python-docx; optional 4 paired LibreOffice/Poppler
comparisons isolate paragraph numerics with explicit matching typography.
This is not browser/native key delivery, app printing, or Word/WPS validation.
"""
import argparse
import copy
import importlib.util
import json
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HELPER = ROOT / 'scripts/convert-word-block-interactions.mjs'
FROZEN_CLI_SHA = 'a6179a5e683c7c2b0ddbf93d55afad3fac67b6549f442ee093b8ea035f21e80f'
SOURCE_REPORT_SHA = '8acc4da17a13a411d68bcc34b8ab5e244968a5304d9d0e6a76c0f5dbe5e44196'
spec = importlib.util.spec_from_file_location('block_type', ROOT / 'scripts/check-word-block-type.py')
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
check, digest, run_cli = b.check, b.digest, b.run_cli


def expected_output(source, mode, level):
    target = 0 if mode == 'toggle' else level
    result = b.expected_conversion(source, target)
    tail = source['content'][-1]['type'] != 'paragraph' or level > 0
    if tail:
        result['content'].append({'type': 'paragraph'})
    return result, int(tail)


def convert(evidence, directory, name, source, mode, level, reject=False):
    input_path, output = directory / (name + '-input.json'), directory / (name + '-converted.json')
    with input_path.open('x', encoding='utf-8') as stream:
        json.dump(source, stream, ensure_ascii=False, indent=2)
    before = digest(input_path)
    command = ['node', str(HELPER), str(input_path), str(output), mode, str(level)]
    run = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=90)
    item = {'command': command, 'returncode': run.returncode, 'stdout': run.stdout, 'stderr': run.stderr,
            'input_sha256': before, 'input_unchanged': digest(input_path) == before}
    evidence['commands'].append(item)
    check(item['input_unchanged'], name + ': helper changed input')
    if reject:
        check(run.returncode != 0 and not output.exists() and '列表首段须保留为正文' in run.stderr,
              name + ': missing exact list rejection or wrote an output')
        return {'rejected_without_output': True, 'input_unchanged': True,
                'scope': 'whole selected shortcut atomic refusal' if mode == 'shortcut' else 'individual list-first input-rule refusal; earlier separate paragraph operations may succeed'}
    check(run.returncode == 0 and output.is_file(), name + ': ' + run.stdout + run.stderr)
    metadata = json.loads(run.stdout)
    check(metadata['registeredPluginDispatch'] and metadata['trailingNodeIncluded'] and metadata['fullManagerPluginCount'] > 0,
          name + ': full actual plugin registration missing')
    model = json.loads(output.read_text())
    expected, tails = expected_output(source, mode, level)
    check(b.comparable_model(model) == b.comparable_model(expected), name + ': model differs beyond declared schema defaults/mark order and exact trailing paragraph')
    check(metadata['finalAppendedParagraphs'] == tails, name + ': unexpected trailing paragraph count')
    item.update(metadata=metadata, model_path=str(output), model_sha256=digest(output), exact_expected_model=True,
                declared_trailing_paragraphs=tails)
    retry = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=90)
    check(retry.returncode != 0 and 'EEXIST' in retry.stderr and digest(output) == item['model_sha256'] and digest(input_path) == before,
          name + ': helper overwrote source/output')
    item['helper_no_clobber'] = True
    return model, item


def export(evidence, cli, directory, name, model, source, tails):
    path = directory / (name + '.json')
    with path.open('x', encoding='utf-8') as stream:
        json.dump(model, stream, ensure_ascii=False, indent=2)
    output = path.with_suffix('.docx')
    run_cli(cli, evidence['commands'], 'create-docx', path, output)
    check(run_cli(cli, evidence['commands'], 'read-docx-model', output) == model, name + ': complete trusted reopen differs')
    rows = b.independent(output)
    source_model, source_rows = copy.deepcopy(model), rows
    if tails:
        tail = source_model['content'].pop()
        check(b.comparable_model(tail) == {'type': 'paragraph'}, name + ': appended tail contains unexpected data')
        tail_row, source_rows = rows[-1], rows[:-1]
        # Existing standard-v2 body defaults are 0/120/360; the application's
        # bare appended node deliberately has no direct paragraph declarations.
        defaults = {'spaceBefore': 0, 'spaceAfter': 120, 'lineSpacing': 360} if model.get('attrs', {}).get('styleProfile') == 'standard-v2' else {'spaceBefore': 0, 'spaceAfter': 0, 'lineSpacing': 240}
        check(tail_row['text'] == '' and tail_row['style'] is None and not tail_row['direct_indent'] and not tail_row['direct_spacing']
              and tail_row['attrs'] == defaults, name + ': independent trailing paragraph differs')
    b.compare_to_model(source_rows, source_model, name + ' independent source paragraphs')
    original_hash, output_hash = digest(source), digest(output)
    for target in [source, output]:
        failure = run_cli(cli, evidence['commands'], 'create-docx', path, target, success=False)
        check('Output already exists' in failure, name + ': overwrite was not refused')
    check(digest(source) == original_hash and digest(output) == output_hash, name + ': no-clobber changed bytes')
    return {'path': str(output), 'sha256': output_hash, 'trusted_complete_model_exact': True,
            'independent_seven_fields_text_marks_equal': True, 'source_and_output_no_clobber': True,
            'exact_default_trailing_paragraphs': tails, 'independent': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'tmp/contextual-final2-cli')
    parser.add_argument('--source-report', type=Path, default=ROOT / 'tmp/word-block-type/run-xmx2wu5y/report.json')
    parser.add_argument('--root', type=Path, default=ROOT / 'tmp/word-block-interactions')
    parser.add_argument('--render', action='store_true')
    parser.add_argument('--soffice')
    args = parser.parse_args()
    args.root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='run-', dir=args.root)).resolve()
    report = directory / 'report.json'
    cli = args.cli.resolve()
    identities = [Path(__file__).resolve(), HELPER, ROOT / 'ui/authoring.ts', ROOT / 'ui/word-block-type.ts',
                  ROOT / 'ui/word-block-type-interactions.ts', ROOT / 'tests/ui/word-block-type-harness.mjs',
                  ROOT / 'tests/ui/load-ts.mjs', ROOT / 'tests/fixtures/word-block-type-expected.json',
                  ROOT / 'package-lock.json', Path(b.__file__), Path(b.s.__file__), Path(b.i.__file__), Path(b.w.__file__)]
    evidence = {'status': 'running', 'commands': [], 'sources': {}, 'cases': {}, 'renders': {},
                'git_head': subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip(),
                'cli_path': str(cli), 'cli_sha256': digest(cli), 'source_report': str(args.source_report.resolve()),
                'source_report_sha256': digest(args.source_report), 'python_version': platform.python_version(),
                'identities': {str(path.relative_to(ROOT)): digest(path) for path in identities},
                'limitations': [
                    'Five independently authored synthetic DOCX and one untouched pinned Apache POI regression DOCX; no user production corpus.',
                    'Actual registered keymaps use synthetic Ctrl/Mod-Alt event objects and input plugins use handleTextInput directly, with all manager plugins; exact counts are recorded per output. No browser/native event delivery, IME, app save UI, or app printing.',
                    'Source-only shortcut range excludes exactly one ordinary auto-appended trailing paragraph. The tail is independently checked and not silently discarded.',
                    'Full trusted reopen is exact JSON equality. Expected source-model comparison permits only declared schema defaults, mark ordering, and exactly one default trailing paragraph when required.',
                    'Source theme/tint fallback RGB is not the imported RGB oracle. Existing importer normalization is disclosed; entry conversion and output preserve imported RGB.',
                    'Optional consumer renders use explicit matching font/size/marks and independent target-style references. They are bounded Latin x/y probes, not old-style appearance retention or general Word/WPS fidelity.',
                    'No Rust rebuild, browser/native retries, source-file writes, commits, or publication.']}
    try:
        check(evidence['cli_sha256'] == FROZEN_CLI_SHA, 'Frozen CLI identity differs')
        check(evidence['source_report_sha256'] == SOURCE_REPORT_SHA, 'Prior source report identity differs')
        prior = json.loads(args.source_report.read_text())
        manifest = json.loads((ROOT / 'tests/fixtures/word-block-type-expected.json').read_text())
        for name, old in prior['sources'].items():
            source = Path(old['path'])
            check(source.is_file() and digest(source) == old['sha256'], name + ': source bytes differ')
            rows = b.independent(source)
            if name in manifest['sources']:
                b.compare_manifest(rows, manifest['sources'][name], name + ' pinned source')
            imported = run_cli(cli, evidence['commands'], 'import-docx', source)
            b.compare_to_model(rows, imported['document'], name + ' source import', direct_only=True)
            evidence['sources'][name] = {'path': str(source), 'sha256': digest(source),
                'kind': 'untouched pinned Apache POI original' if name == 'original-SampleDoc' else 'independently authored synthetic source reused unchanged',
                'model': imported['document'], 'compatibility': imported['compatibility'], 'independent': rows}
        plan = []
        for name in ['mixed-formatting', 'stock-defaults', 'original-SampleDoc']:
            plan.extend((name, 'shortcut', level, False) for level in range(4))
            plan.extend((name, 'input', level, False) for level in [1, 2, 3])
            plan.append((name, 'toggle', 2, False))
        plan += [('render-body', 'shortcut', 1, False), ('render-body', 'shortcut', 3, False),
                 ('render-body', 'input', 2, False), ('render-heading', 'shortcut', 0, False)]
        plan.extend(('list-incompatible', mode, level, True) for mode in ['shortcut', 'input'] for level in [1, 2, 3])
        for name, mode, level, reject in plan:
            label = f'{name}-{mode}-{level}'
            source = evidence['sources'][name]
            result = convert(evidence, directory, label, source['model'], mode, level, reject)
            if reject:
                evidence['cases'][label] = result
                continue
            model, record = result
            record['export'] = export(evidence, cli, directory, label, model, Path(source['path']), record['declared_trailing_paragraphs'])
            evidence['cases'][label] = record
            if args.render and name.startswith('render-'):
                soffice = args.soffice or shutil.which('soffice')
                check(soffice, 'LibreOffice unavailable')
                reference = directory / (label + '-independent-expected.docx')
                doc = b.render_document(name == 'render-heading', level)
                if record['declared_trailing_paragraphs']:
                    doc.add_paragraph()
                doc.save(reference)
                # Target typography/paragraph values are authored independently
                # from fixed render_document specifications, never from output.
                reference_rows = b.independent(reference)
                b.compare_manifest(reference_rows[:-1] if record['declared_trailing_paragraphs'] else reference_rows,
                                   manifest['sources'][name], label + ' independent render reference')
                check(all(row['style'] == (f'Heading{level}' if level else None) for row in reference_rows[:3]),
                      label + ': reference target style differs')
                ref_pdf = b.i.render_pdf(soffice, directory, reference)
                actual_pdf = b.i.render_pdf(soffice, directory, Path(record['export']['path']))
                evidence['renders'][label] = {'reference': str(reference), 'reference_sha256': digest(reference),
                    'reference_fixed_manifest_equal': True, 'reference_pdf': ref_pdf, 'output_pdf': actual_pdf,
                    'comparison': b.compare_pdf(ref_pdf, actual_pdf, label)}
        for name, source in evidence['sources'].items():
            check(digest(Path(source['path'])) == source['sha256'], name + ': source modified during verification')
            source['source_unchanged'] = True
        check(digest(cli) == evidence['cli_sha256'] and digest(args.source_report) == evidence['source_report_sha256'], 'Frozen CLI/source report changed')
        for label, render in evidence['renders'].items():
            check(digest(Path(render['reference'])) == render['reference_sha256'], label + ': independent reference changed')
            for kind in ['reference_pdf', 'output_pdf']:
                check(digest(Path(render[kind]['path'])) == render[kind]['sha256'], label + ': measured PDF changed')
        for path, sha in evidence['identities'].items():
            check(digest(ROOT / path) == sha, path + ': changed during verification; rerun required')
        evidence['counts'] = {'source_packages': len(evidence['sources']), 'synthetic_sources': 5, 'untouched_originals': 1,
            'successful_exports': sum('export' in item for item in evidence['cases'].values()),
            'explicit_list_refusals': sum(item.get('rejected_without_output', False) for item in evidence['cases'].values()),
            'shortcut_outputs': sum('export' in item and item['metadata']['mode'] == 'shortcut' for item in evidence['cases'].values()),
            'input_rule_outputs': sum('export' in item and item['metadata']['mode'] == 'input' for item in evidence['cases'].values()),
            'toggle_outputs': sum('export' in item and item['metadata']['mode'] == 'toggle' for item in evidence['cases'].values()),
            'input_rule_paragraph_dispatches': sum(len(item['metadata']['records']) for item in evidence['cases'].values() if 'export' in item and item['metadata']['mode'] == 'input'),
            'helper_no_clobber': sum(item.get('helper_no_clobber', False) for item in evidence['cases'].values()),
            'rust_no_clobber': 2 * sum('export' in item for item in evidence['cases'].values()),
            'render_pairs': len(evidence['renders']), 'new_pdfs': 2 * len(evidence['renders'])}
        evidence['status'] = 'passed'
    except Exception as error:
        evidence.update(status='failed', error=str(error))
        raise
    finally:
        report.write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding='utf-8')
        print(json.dumps({'status': evidence['status'], 'report': str(report), 'counts': evidence.get('counts'), 'error': evidence.get('error')}))


if __name__ == '__main__':
    main()
