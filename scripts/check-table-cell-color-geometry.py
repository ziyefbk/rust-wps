"""Measure the exact existing cell-color PDFs, without rendering new PDFs.

Pairings come from the primary checker's source/reference contracts. Controlled
sources require exact recorded PDF edges, baselines, colors, fonts, sizes and
pages. The two original-source comparisons preserve y but disclose normalized
x/widths. This is one independent consumer, not Word/WPS or app-print evidence.
"""
import argparse
import hashlib
import json
import shutil
from pathlib import Path

import fitz


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def check(value, message):
    if not value:
        raise AssertionError(message)


def measure(path):
    doc = fitz.open(path)
    spans, fills = [], []
    for index, page in enumerate(doc):
        for block in page.get_text('dict')['blocks']:
            for line in block.get('lines', []):
                for item in line['spans']:
                    spans.append({'page': index, 'text': item['text'], 'color': f'{item["color"]:06X}',
                                  'origin': list(item['origin']), 'font': item['font'], 'size': item['size']})
        for item in page.get_drawings():
            if item.get('fill') is not None:
                fills.append({'page': index, 'rgb': ''.join(f'{round(x * 255):02X}' for x in item['fill']),
                              'rect': list(item['rect'])})
    return {'pages': len(doc), 'page_sizes': [[p.rect.width, p.rect.height] for p in doc], 'spans': spans, 'fills': fills}


def pairs(report):
    result = []
    for name, source in report['sources'].items():
        if source['render']:
            result.append((name, source['path'], report['cases'][name]['new']['path'], name != 'original-table-alignment'))
    result.append(('edited', report['model_edit']['reference']['path'], report['model_edit']['new']['path'], True))
    for variant in ('edited', 'reset-fill', 'reset-mode', 'reset-all'):
        source = report['sources']['literal-auto']
        reference = source['reference'] if variant == 'edited' else source['reset_references'][variant]
        result.append(('ui-' + variant, reference['path'], report['ui_driver']['exports'][variant]['path'], True))
    result.append(('ui-original-edited', report['original_ui_driver']['reference']['path'],
                   report['original_ui_driver']['exports']['edited']['path'], False))
    return result


def compare(name, source, output, strict):
    check(source['pages'] == output['pages'] and source['page_sizes'] == output['page_sizes'], name + ': pages changed')
    check(len(source['spans']) == len(output['spans']), name + ': text span count changed')
    span_deltas = []
    for a, b in zip(source['spans'], output['spans']):
        check(all(a[field] == b[field] for field in ('page', 'text', 'color', 'font', 'size')), name + ': text/color/font/size/page changed')
        delta = [b['origin'][i] - a['origin'][i] for i in range(2)]
        check(delta[1] == 0, name + ': vertical text baseline changed')
        if strict:
            check(delta[0] == 0, name + ': controlled horizontal text baseline changed')
        span_deltas.append({'text': a['text'], 'page': a['page'], 'source_origin': a['origin'], 'output_origin': b['origin'], 'delta_pt': delta})
    order = lambda f: (f['page'], f['rect'][1], f['rect'][0], f['rect'][3], f['rect'][2], f['rgb'])
    a_fills, b_fills = sorted(source['fills'], key=order), sorted(output['fills'], key=order)
    check(len(a_fills) == len(b_fills), name + ': fill primitive count changed')
    rectangles = []
    for a, b in zip(a_fills, b_fills):
        check(a['page'] == b['page'] and a['rgb'] == b['rgb'], name + ': fill page/color identity changed')
        delta = [b['rect'][i] - a['rect'][i] for i in range(4)]
        check(delta[1] == 0 and delta[3] == 0, name + ': fill vertical edges changed')
        if strict:
            check(delta[0] == 0 and delta[2] == 0, name + ': controlled fill horizontal edges changed')
        rectangles.append({'page': a['page'], 'rgb': a['rgb'], 'source_rect': a['rect'], 'output_rect': b['rect'],
                           'edge_delta_pt': delta, 'source_width_pt': a['rect'][2] - a['rect'][0],
                           'output_width_pt': b['rect'][2] - b['rect'][0]})
    return {'scope': 'exact controlled geometry' if strict else 'original y retained; x/width normalization recorded, not accepted as horizontal fidelity',
            'strict_horizontal': strict, 'text_color_font_size_page_equal': True, 'page_geometry_equal': True,
            'fill_count': len(rectangles), 'max_abs_fill_edge_delta_pt': [max((abs(x['edge_delta_pt'][i]) for x in rectangles), default=0) for i in range(4)],
            'max_abs_text_delta_pt': [max((abs(x['delta_pt'][i]) for x in span_deltas), default=0) for i in range(2)],
            'rectangles': rectangles, 'text_positions': span_deltas}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--report-sha256', required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    report_path = args.report.resolve()
    check(digest(report_path) == args.report_sha256, 'Primary report identity changed')
    report = json.loads(report_path.read_text())
    check(report['status'] == 'passed', 'Primary report did not pass')
    output = args.output or report_path.parent / 'geometry-supplement.json'
    check(not output.exists(), 'Refusing to overwrite geometry supplement')
    evidence = {'status': 'running', 'primary_report': str(report_path), 'primary_report_sha256': args.report_sha256,
                'script_sha256': digest(__file__), 'new_pdf_count': 0, 'method': 'Reopened exact hashed PDFs from the primary run; no LibreOffice processes or new PDF renders',
                'limits': ['LibreOffice consumer only, not application browser/native print or Word/WPS',
                           'Font identity is the actual PDF consumer face, including substitutions; it does not establish original font availability'],
                'pdfs': {}, 'comparisons': {}}
    snapshot = output.parent / Path(__file__).name
    check(not snapshot.exists(), 'Refusing to overwrite geometry script snapshot')
    shutil.copyfile(__file__, snapshot)
    try:
        measured = {}
        for input_path, known in report['renders'].items():
            pdf = Path(known['path'])
            check(digest(pdf) == known['sha256'], 'Final PDF identity changed: ' + str(pdf))
            current = measure(pdf)
            check(all(current[field] == known[field] for field in current), 'Independent PDF remeasurement differs from primary record: ' + str(pdf))
            measured[input_path] = current
            evidence['pdfs'][input_path] = {'path': str(pdf), 'sha256': known['sha256'], 'pages': current['pages']}
        for name, source, target, strict in pairs(report):
            evidence['comparisons'][name] = compare(name, measured[source], measured[target], strict)
            evidence['comparisons'][name].update(source_pdf=report['renders'][source]['path'], output_pdf=report['renders'][target]['path'])
        controlled = [c for c in evidence['comparisons'].values() if c['strict_horizontal']]
        evidence['summary'] = {'existing_pdf_count': len(evidence['pdfs']), 'new_pdf_count': 0, 'pair_count': len(evidence['comparisons']),
                               'controlled_pair_count': len(controlled), 'controlled_all_fill_edges_exact': True,
                               'controlled_text_origins_exact': True, 'all_text_color_font_size_page_identity_equal': True,
                               'all_vertical_fill_edges_exact': True,
                               'original_horizontal_differences': {name: {'max_abs_fill_edge_delta_pt': c['max_abs_fill_edge_delta_pt'],
                                                                        'max_abs_text_delta_pt': c['max_abs_text_delta_pt']}
                                                                    for name, c in evidence['comparisons'].items() if not c['strict_horizontal']}}
        check(digest(report_path) == args.report_sha256, 'Primary report modified during supplement')
        evidence['status'] = 'passed'
    except Exception as error:
        evidence['status'] = 'failed'; evidence['error'] = str(error); raise
    finally:
        with output.open('x') as file:
            json.dump(evidence, file, indent=2)
        print(output, digest(output), flush=True)


if __name__ == '__main__':
    main()
