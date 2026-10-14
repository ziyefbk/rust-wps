#!/usr/bin/env python3
"""Supplemental neutral-color Heading1 keepNext true/false page-boundary proof.

Authors two independent python-docx/OOXML sources. Requires genuine imported
heading nodes and an explicit false override, complete trusted reopens and
source/output no-overwrite. Four fresh-profile PDFs must have exact full text,
coordinates and raster pixels, with Heading1 visibly moved by keepNext.
"""
import argparse
import importlib.util
import json
import shutil
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
HELPER = ROOT / 'scripts/check-paragraph-pagination-independent.py'
FROZEN_HELPER = '95046da4104a7f5e605d549974c44f33b6842f513aebdcbc30207e57e26c031c'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--expected-cli-sha256', required=True)
    parser.add_argument('--root', type=Path, default=ROOT / 'tmp/heading-pagination-boundary-independent')
    parser.add_argument('--soffice')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('independent_pagination_oracle', HELPER)
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    check, digest = helper.check, helper.digest
    check(digest(HELPER) == FROZEN_HELPER, 'Supplement requires the reviewed frozen independent helper')
    cli = args.cli.resolve()
    check(digest(cli) == args.expected_cli_sha256, 'Wrong frozen CLI')
    args.root.mkdir(parents=True, exist_ok=True)
    helper.disk_guard(args.root)
    directory = Path(tempfile.mkdtemp(prefix='run-', dir=args.root)).resolve()
    evidence = {'status': 'running', 'directory': str(directory), 'cli': str(cli), 'cli_sha256': digest(cli),
                'script': str(Path(__file__).resolve()), 'script_sha256': digest(__file__),
                'independent_helper': str(HELPER), 'independent_helper_sha256': FROZEN_HELPER,
                'commands': [], 'sources': {}, 'started_at_utc': datetime.now(timezone.utc).isoformat(),
                'invocation': [sys.executable, *sys.argv], 'limitations': helper.LIMITATIONS,
                'classification': 'Two newly independently authored neutral-font/color synthetic heading controls; no additional genuine originals'}
    try:
        soffice = args.soffice or shutil.which('soffice')
        check(soffice is not None, 'LibreOffice required')
        evidence['consumers'] = helper.consumer_identity(soffice, directory)
        for enabled in (True, False):
            label = 'heading-keepNext-' + str(enabled).lower()
            doc = helper.base_document()
            style = doc.styles['Heading 1']
            # Keep outline/heading identity and neutralize only unrelated theme
            # typography. The ordinary bold Heading1 default remains active.
            rpr = style.element.get_or_add_rPr()
            fonts = helper.singleton(rpr, 'rFonts')
            for key in list(fonts.attrib):
                if key.endswith('Theme') or key.endswith('theme'):
                    del fonts.attrib[key]
            color = helper.singleton(rpr, 'color')
            if color is None:
                color = helper.child(rpr, 'color')
            color.attrib.clear()
            color.set(helper.W + 'val', '000000')
            for index in range(49):
                helper.para(doc, f'HEADING FILL {index + 1:02}')
            heading = helper.para(doc, 'HEADING ANCHOR',
                                  {'keepNext': enabled, 'keepLines': False, 'pageBreakBefore': False}, 'Heading 1')
            for run in heading.runs:
                run.font.color.rgb = helper.RGBColor.from_string('000000')
            helper.para(doc, 'HEADING FOLLOWER')
            path = directory / (label + '.docx')
            doc.save(path)
            independent = helper.inspect_docx(path)
            check(independent['rows'][49]['type'] == 'heading' and independent['rows'][49]['level'] == 1,
                  'Independent source lost real Heading1 identity')
            check(independent['rows'][49]['effective'] == dict(zip(helper.FLAGS, (enabled, False, False))),
                  'Independent source heading flag vector differs')
            item = {'path': str(path), 'sha256': digest(path), 'independent': independent,
                    'classification': 'new independently authored neutral-color/font synthetic Heading1 boundary control',
                    'expected_action': 'roundtrip', 'render': True}
            evidence['sources'][label] = item
            model = helper.roundtrip(cli, evidence, directory, label, item)
            node = model['content'][49]
            check(node['type'] == 'heading' and node['attrs']['level'] == 1 and node['attrs']['keepNext'] is enabled,
                  'Actual imported Heading1 lost explicit keepNext, including false override')
            item['actual_imported_heading_node'] = node
            item['source_pdf'] = helper.render(soffice, directory, path, label + '-source')
            item['output_pdf'] = helper.render(soffice, directory, Path(item['export']['path']), label + '-output')
            item['comparison'] = helper.compare_render(item['source_pdf'], item['output_pdf'])
            check(item['comparison']['strictly_equal'], label + ': strict consumer comparison failed: ' + json.dumps(item['comparison']))
            expected = {'HEADING ANCHOR': 2 if enabled else 1, 'HEADING FOLLOWER': 2}
            item['expected_heading_boundary_pages'] = expected
            item['actual_boundaries'] = {}
            for consumer in ('source_pdf', 'output_pdf'):
                actual = {}
                for text, page in expected.items():
                    found = [line for line in item[consumer]['lines'] if line['text'] == text]
                    check(len(found) == 1 and found[0]['page'] == page, label + ': predetermined heading page differs')
                    actual[text] = found[0]
                item['actual_boundaries'][consumer] = actual
            print('strict Heading1 consumer and boundary equal', label, flush=True)
        evidence['summary'] = {'independent_source_controls': 2, 'fresh_consumer_pdfs': 4, 'strict_pixel_text_coordinate_pairs': 2,
                               'actual_imported_heading_true_false_vectors_proved': True, 'complete_trusted_reopens': 2, 'no_overwrite_checks': 4}
        evidence['status'] = 'passed'
    except BaseException as error:
        evidence['status'], evidence['error'] = 'failed', type(error).__name__ + ': ' + str(error)
        raise
    finally:
        evidence['source_hashes_unchanged'] = all(digest(i['path']) == i['sha256'] for i in evidence['sources'].values())
        evidence['cli_unchanged'] = digest(cli) == args.expected_cli_sha256
        evidence['helper_unchanged'] = digest(HELPER) == FROZEN_HELPER
        evidence['script_unchanged'] = digest(__file__) == evidence['script_sha256']
        evidence['consumer_binaries_unchanged'] = all(digest(p) == h for p, h in evidence.get('consumers', {}).get('binaries_and_launcher_sha256', {}).items())
        valid = all(evidence[k] for k in ('source_hashes_unchanged', 'cli_unchanged', 'helper_unchanged', 'script_unchanged', 'consumer_binaries_unchanged'))
        if not valid:
            evidence['status'], evidence['identity_error'] = 'failed', 'Protected identity changed'
        evidence['finished_at_utc'] = datetime.now(timezone.utc).isoformat()
        evidence['free_disk_bytes_at_finish'] = shutil.disk_usage(directory).free
        report = directory / 'report.json'
        helper.save_json(report, evidence)
        print('REPORT', report, flush=True)
        check(valid, 'Protected supplemental identity changed')


if __name__ == '__main__':
    main()
