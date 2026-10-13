"""Independent empty-heading Backspace evidence, with bounded source-derived edit.

Frozen CLI, actual app keymaps, python-docx/OOXML, exact trusted reopen and no
clobber. Optional PDF compares independently authored body references, not a
claim of browser/native input or Microsoft Word/WPS compatibility.
"""
import argparse
import copy
import importlib.util
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

e = load('empty_font_backspace', 'check-empty-paragraph-fonts.py')
b = load('block_backspace', 'check-word-block-type.py')
CLI_SHA = 'ed77eea6ce7d6f26eede7ee33af3655f42e1eeebbc9060ac086bb57c6a6d9430'
ORIGINAL_SHA = 'dc6a67b1c118658acadc87ba41200f0b90ead05ada95e7ce6fa72a97700430c5'
ORIGINAL_URL = 'https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/NoHeadFoot.docx'
FIELDS = ['textAlign', 'spaceBefore', 'spaceAfter', 'lineSpacing', 'indentLeftTwips', 'indentRightTwips', 'firstLineIndentTwips']

def authored(level, reference=False):
    font = e.font('Liberation Serif', 49, False, level == 3)
    document = e.fresh(mark=font)
    document.paragraphs[0]._p.getparent().remove(document.paragraphs[0]._p)
    p = document.paragraphs[0]
    p.style = document.styles['Normal' if reference else f'Heading {level}']
    spacing = (0, 120, 241) if level != 2 else (0, 0, 240)
    indentation = (360, 0, -180) if level != 2 else (0, 0, 0)
    alignment = 'center' if level != 3 else 'right'
    b.format_para(p, spacing, indentation, alignment)
    attrs = dict(zip(FIELDS, [alignment, *spacing, *indentation]))
    return document, dict(attrs, emptyParagraphFont=font)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'tmp/empty-font-final-cli')
    parser.add_argument('--original', type=Path, default=ROOT / 'tmp/paragraph-indentation-source/poi/NoHeadFoot.docx')
    parser.add_argument('--root', type=Path, default=ROOT / 'tmp/heading-backspace-independent')
    parser.add_argument('--render', action='store_true')
    args = parser.parse_args(); args.root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='run-', dir=args.root)).resolve()
    report = directory / 'report.json'
    evidence = {'status': 'running', 'commands': [], 'sources': {}, 'cases': {}, 'renders': {},
                'limits': ['Full plugin callbacks, not physical keyboard/browser/native input.', 'Original is an Apache POI regression fixture, not a user production file.',
                           'Original-derived scenario deliberately deletes its first paragraph only in the working model to expose the next empty paragraph.',
                           'Controlled PDF comparison is neighboring Latin baseline only, not original full-page/Word/WPS fidelity.']}
    paths = []
    identity = {str(path.relative_to(ROOT)): e.digest(path) for path in [ROOT / 'ui/word-block-type-interactions.ts', ROOT / 'ui/word-block-type.ts',
      ROOT / 'ui/word-empty-paragraph-font.ts', Path(__file__), ROOT / 'scripts/convert-word-heading-backspace.mjs',
      ROOT / 'scripts/check-empty-paragraph-fonts.py', ROOT / 'scripts/check-word-block-type.py']}
    try:
        e.check(e.digest(args.cli) == CLI_SHA, 'Frozen CLI identity differs')
        e.check(e.digest(args.original) == ORIGINAL_SHA, 'Pinned untouched source identity differs')
        evidence['cli'] = {'path': str(args.cli), 'sha256': CLI_SHA}; evidence['code_identity'] = identity
        for name, level in [('controlled-h1', 1), ('controlled-h2', 2), ('controlled-h3', 3), ('original-NoHeadFoot', 0)]:
            if level:
                document, expected = authored(level)
                source = e.save(document, directory / (name + '-source.docx'))
                reference = e.save(authored(level, True)[0], directory / (name + '-body-reference.docx'))
                paths.append(reference)
                source_info = {'path': str(source), 'sha256': e.digest(source), 'kind': 'independently authored true-empty heading',
                               'expected_first_body_attrs': expected, 'reference': {'path': str(reference), 'sha256': e.digest(reference)}}
            else:
                source = args.original.resolve()
                source_info = {'path': str(source), 'sha256': ORIGINAL_SHA, 'kind': 'untouched original; documented working-model edits only',
                               'source_url': ORIGINAL_URL, 'git_blob_sha1': '7af2a14583d09c337a7993fb744c33e31e28d687'}
            source_info['independent_source'] = e.snapshot(source)
            evidence['sources'][name] = source_info
            imported = e.run_cli(args.cli, evidence['commands'], 'import-docx', source)
            model = imported['document']
            if level:
                e.check(model['content'][0]['type'] == 'heading', 'Source heading not imported')
                e.check({key: model['content'][0]['attrs'][key] for key in expected} == expected, 'Imported attrs differ from fixed source manifest')
            input_path, output_path = directory / (name + '-input.json'), directory / (name + '-after.json')
            input_path.write_text(json.dumps(model, ensure_ascii=False, indent=2))
            command = ['node', 'scripts/convert-word-heading-backspace.mjs', str(input_path), str(output_path), 'existing' if level else 'expose-original-empty']
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
            evidence['commands'].append({'command': command, 'returncode': result.returncode, 'stderr': result.stderr, 'stdout': result.stdout})
            e.check(result.returncode == 0, name + ': plugin driver failed: ' + result.stderr)
            metadata, after = json.loads(result.stdout), json.loads(output_path.read_text())
            before = metadata['before']; target = copy.deepcopy(before)
            target['content'][0]['type'] = 'paragraph'; target['content'][0]['attrs'].pop('level')
            e.check(after == target, 'Conversion changed data outside first node type/level')
            expected_model = copy.deepcopy(model)
            if not level:
                expected_model['content'].pop(0)
            else:
                expected_model['content'][0]['type'] = 'paragraph'; expected_model['content'][0]['attrs'].pop('level')
            e.check(b.comparable_model(after) == b.comparable_model(expected_model), 'Unexpected source-derived edit or normalization')
            exported = e.export_model(args.cli, evidence['commands'], directory, name + '-after-export', after, source)
            prepared = e.export_model(args.cli, evidence['commands'], directory, name + '-before-export', before, source)
            rows = b.independent(Path(exported['path'])); old_rows = b.independent(Path(prepared['path']))
            e.check(rows[0]['attrs'] == old_rows[0]['attrs'], 'Seven independent paragraph fields differ')
            e.check(rows[0]['style'] is None and old_rows[0]['style'] in ('Heading1', 'Heading2', 'Heading3'), 'Independent style transition differs')
            e.check(rows[1:] == old_rows[1:], 'Neighboring independent text/marks/formatting changed')
            if level:
                e.check(rows[0]['attrs'] == {key: expected[key] for key in FIELDS}, 'Exported fields differ from fixed manifest')
                paths.append(Path(exported['path']))
            original_output = e.digest(output_path)
            repeated = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
            e.check(repeated.returncode != 0 and 'EEXIST' in repeated.stderr and e.digest(output_path) == original_output, 'Driver overwrote existing output')
            e.check(e.digest(source) == source_info['sha256'], 'Source bytes changed')
            evidence['cases'][name] = {'compatibility': imported['compatibility'], 'plugin': metadata, 'before': prepared, 'after': exported,
                                      'independent_paragraph_fields': rows[0]['attrs'], 'neighbors_exact': True, 'source_unchanged': True, 'driver_no_clobber': True}
        if args.render:
            evidence['renders'] = e.render_all(shutil.which('soffice'), directory, paths, evidence['commands'])
            evidence['comparisons'] = {}
            for name, case in evidence['cases'].items():
                if not name.startswith('controlled-'): continue
                reference = evidence['sources'][name]['reference']['path']
                evidence['comparisons'][name] = e.compare_baselines(evidence['renders'][reference], evidence['renders'][case['after']['path']], name)
        e.check(all(e.digest(ROOT / name) == sha for name, sha in identity.items()), 'Product/test identity changed during evidence run')
        e.check(e.digest(args.cli) == CLI_SHA, 'Frozen CLI changed')
        evidence['status'] = 'passed'
    except Exception as error:
        evidence['status'] = 'failed'; evidence['error'] = str(error); raise
    finally:
        report.write_text(json.dumps(evidence, ensure_ascii=False, indent=2)); print(report)

if __name__ == '__main__': main()
