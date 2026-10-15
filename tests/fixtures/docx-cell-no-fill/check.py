#!/usr/bin/env python3
"""Draft, portable real-file regression; Python 3 stdlib and a supplied built CLI."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import posixpath
import signal
import subprocess
import tempfile
import threading
import time
import xml.etree.ElementTree as ET
import zipfile

HERE = Path(__file__).resolve().parent
W = '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/'
MODEL = 'rust-wps/model-v2.json'
CAP = 2 * 1024 * 1024
PINS = {
    'Bug60337.docx': '87f983bdd93b3b5b8469104a5b09ccff41e0f5c144060e13fd4db07bfd2e39da',
    'baseline-import.json': '68ddd844d017d1ad19f3219549d6364104cc0409847d5a363523d7254b8403c4',
    'expected-import.json': 'fc220c57bdba3de0a27dcc92abf5296ceb043cbcd89c9d8e7dfff1d93be3d691',
    'source-proof.json': '1f8ad99ffb65189801090e457e9ef35409b7ea39a711eecf3ed02774bc2b4781',
}


def req(value, message):
    if not value:
        raise AssertionError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    with Path(path).open('rb') as f:
        h = hashlib.sha256()
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
        return h.hexdigest()


def strict_json(data):
    def pairs(items):
        out = {}
        for key, value in items:
            req(key not in out, 'Duplicate JSON key: ' + key)
            out[key] = value
        return out
    def invalid(value):
        raise ValueError('Non-finite JSON: ' + value)
    return json.loads(data, object_pairs_hook=pairs, parse_constant=invalid)


def same(left, right):
    # Python bool/int equality must not hide a JSON type change.
    return json.dumps(left, sort_keys=True, ensure_ascii=False, allow_nan=False) == json.dumps(right, sort_keys=True, ensure_ascii=False, allow_nan=False)


def zip_records(path):
    req(Path(path).stat().st_size <= CAP, 'Package size limit')
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        req(0 < len(infos) <= 100, 'ZIP member count')
        req(len({i.filename for i in infos}) == len(infos), 'Duplicate ZIP members')
        req(sum(i.file_size for i in infos) <= CAP, 'ZIP expanded size limit')
        parts = {i.filename: z.read(i) for i in infos}  # read validates CRC
        for name, data in parts.items():
            if name.endswith(('.xml', '.rels')):
                req(b'<!DOCTYPE' not in data and b'<!ENTITY' not in data, 'DTD/entity forbidden')
                ET.fromstring(data)
        return {'parts': parts}

def actual_part(parts, parent, kind):
    path = '_rels/.rels' if not parent else posixpath.join(posixpath.dirname(parent), '_rels', posixpath.basename(parent) + '.rels')
    rels = list(ET.fromstring(parts[path]))
    req(len({r.get('Id') for r in rels}) == len(rels), 'Relationship IDs')
    selected = [r for r in rels if r.get('Type') == REL + kind]
    req(len(selected) == 1 and selected[0].get('TargetMode', 'Internal') == 'Internal', 'Actual ' + kind)
    target = posixpath.normpath(posixpath.join(posixpath.dirname(parent), selected[0].get('Target'))).lstrip('/')
    req(target in parts, 'Missing actual part')
    return target


def package_audit(path):
    z = zip_records(path)
    parts = z['parts']
    main = actual_part(parts, '', 'officeDocument')
    sheet = actual_part(parts, main, 'styles')
    req(main == 'word/document.xml', 'Unexpected actual main')
    types = list(ET.fromstring(parts['[Content_Types].xml']))
    overrides = [t for t in types if t.tag.endswith('}Override')]
    req(len({t.get('PartName') for t in overrides}) == len(overrides), 'Duplicate content types')
    type_map = {t.get('PartName'): t.get('ContentType') for t in overrides}
    req(type_map.get('/' + main) == 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml', 'Main content type')
    req(type_map.get('/' + sheet) == 'application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml', 'Styles content type')
    tree = ET.fromstring(parts[main])
    styles = ET.fromstring(parts[sheet])
    nodes = styles.findall(W + 'style')
    ids = {s.get(W + 'styleId'): s for s in nodes}
    req(len(ids) == len(nodes) and None not in ids, 'Style identity')
    defaults = {}
    for kind in ['table', 'paragraph', 'character']:
        found = [s for s in nodes if s.get(W + 'type') == kind and s.get(W + 'default') in ['1', 'true', 'on']]
        req(len(found) <= 1, 'Default style identity')
        defaults[kind] = found[0].get(W + 'styleId') if found else None
    dd = styles.find(W + 'docDefaults')
    req(dd is None or not list(dd.iter(W + 'shd')), 'Shaded docDefaults')
    references = [('table', n.get(W + 'val')) for n in tree.iter(W + 'tblStyle')]
    references += [('paragraph', n.get(W + 'val')) for n in tree.iter(W + 'pStyle')]
    references += [('character', n.get(W + 'val')) for n in tree.iter(W + 'rStyle')]
    references += list(defaults.items())
    chains = []
    for kind, ident in references:
        chain = []
        while ident:
            req(ident in ids and ident not in chain and len(chain) < 20, 'Active chain')
            style = ids[ident]
            req(style.get(W + 'type') == kind and not list(style.iter(W + 'shd')), 'Active style shading/type')
            chain.append(ident)
            bases = style.findall(W + 'basedOn')
            req(len(bases) <= 1 and len(list(style.iter(W + 'basedOn'))) == len(bases), 'BasedOn path')
            ident = bases[0].get(W + 'val') if bases else None
        chains.append({'kind': kind, 'ids': chain})
    tables = tree.findall(W + 'body/' + W + 'tbl')
    req(len(tables) == len(list(tree.iter(W + 'tbl'))) == 1, 'One direct witness table')
    rows = tables[0].findall(W + 'tr')
    req(len(rows) == 3 and all(len(r.findall(W + 'tc')) == 5 for r in rows), 'Witness3x5')
    headers = [r.findall(W + 'trPr/' + W + 'tblHeader') for r in rows]
    req([len(h) for h in headers] == [1, 0, 0], 'Exactly first header row')
    req(headers[0][0].get(W + 'val', 'true') in ('1', 'true', 'on'), 'Header must be enabled')
    cant = rows[2].findall(W + 'trPr/' + W + 'cantSplit')
    req(len(cant) == 1 and cant[0].get(W + 'val', 'true') in ('1', 'true', 'on'), 'Witness row3 cantSplit')
    paragraphs = [[[''.join(n.text or '' for n in p.iter(W + 't')) for p in c.findall(W + 'p')]
                   for c in r.findall(W + 'tc')] for r in rows]
    return {'main': main, 'styles': sheet, 'chains': chains, 'paragraphs': paragraphs,
            'grid': [n.get(W + 'w') for n in tables[0].findall(W + 'tblGrid/' + W + 'gridCol')],
            'shading_count': len(list(tree.iter(W + 'shd'))), 'part_hashes': {n: digest(b) for n, b in parts.items()}}, z


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', required=True, type=Path, help='Already built CLI; never built or downloaded here')
    parser.add_argument('--output-parent', type=Path, help='Existing parent for a new unique evidence directory')
    args = parser.parse_args()
    cli = args.cli.resolve(strict=True)
    req(cli.is_file(), 'CLI must be a file')
    out = Path(tempfile.mkdtemp(prefix='no-fill-portable-', dir=args.output_parent)).resolve()
    print('Evidence: ' + str(out), flush=True)
    source = HERE / 'Bug60337.docx'
    report = {'status': 'FAIL', 'cli': str(cli), 'cli_sha256': sha(cli), 'commands': [], 'scope': 'Genuine 3x5 no-fill import/export/model consistency only; no render or authenticity claim'}
    protected = {HERE / name: pin for name, pin in PINS.items()}

    def save(name, data):
        with (out / name).open('xb') as f:
            f.write(data)
        return out / name

    def run(*arguments, expected=0):
        argv = [str(cli), *map(str, arguments)]
        buffers = [bytearray(), bytearray()]
        overflow = threading.Event()
        proc = subprocess.Popen(argv, cwd=out, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, start_new_session=(os.name == 'posix'))
        def drain(stream, target):
            try:
                while True:
                    chunk = stream.read(65536)
                    if not chunk:
                        break
                    if len(target) + len(chunk) > CAP:
                        overflow.set()
                        break
                    target.extend(chunk)
            finally:
                stream.close()
        threads = [threading.Thread(target=drain, args=(stream, buf), daemon=True)
                   for stream, buf in zip((proc.stdout, proc.stderr), buffers)]
        for thread in threads:
            thread.start()
        deadline = time.monotonic() + 30
        failure = None
        try:
            while proc.poll() is None or any(t.is_alive() for t in threads):
                if overflow.is_set() or time.monotonic() >= deadline:
                    failure = 'CLI output limit or 30-second timeout'
                    break
                time.sleep(0.02)
        except BaseException:
            failure = 'Interrupted command'
            raise
        finally:
            if failure:
                try:
                    if os.name == 'posix':
                        os.killpg(proc.pid, signal.SIGKILL)
                    else:
                        proc.kill()
                except ProcessLookupError:
                    pass
            proc.wait(timeout=5)
            for thread in threads:
                thread.join(timeout=1)
        idx = len(report['commands']) + 1
        stdout, stderr = map(bytes, buffers)
        save(f'command-{idx}.stdout', stdout)
        save(f'command-{idx}.stderr', stderr)
        report['commands'].append({'argv': argv, 'returncode': proc.returncode,
            'stdout_sha256': digest(stdout), 'stderr_sha256': digest(stderr), 'failure': failure})
        req(not failure and not overflow.is_set(), failure or 'CLI output limit')
        req(proc.returncode == expected, 'Unexpected return code: ' + str(arguments[0]))
        return stdout, stderr

    try:
        for path, pin in protected.items():
            req(sha(path) == pin, 'Bundled input pin: ' + path.name)
        expected = strict_json((HERE / 'expected-import.json').read_bytes())
        reconstructed = strict_json((HERE / 'baseline-import.json').read_bytes())
        cells = [c for r in reconstructed['document']['content'][0]['content'] for c in r['content']]
        req(len(cells) == 15, 'Baseline cell count')
        for cell in cells:
            attrs = cell.setdefault('attrs', {})
            req('cellFillMode' not in attrs and 'cellFillRgb' not in attrs, 'Baseline fill already set')
            attrs['cellFillMode'] = 'none'
        req(same(reconstructed, expected), 'Independent baseline+15none full expectation')
        audit, _ = package_audit(source)
        proof = strict_json((HERE / 'source-proof.json').read_bytes())
        expected_text = [[c for c in r['cells']] for r in proof['raw_candidate_tables'][0]['rows']]
        req(audit['paragraphs'] == expected_text and audit['shading_count'] == 0, 'Independent raw text/no-shading')
        req(audit['grid'] == ['1870'] * 5, 'Raw source grid')
        raw, err = run('import-docx', source)
        req(err == b'', 'Import stderr')
        actual = strict_json(raw)
        req(same(actual, expected), 'Entire import response/model/risks differs')
        model = actual['document']
        model_path = save('actual-model.json', json.dumps(model, ensure_ascii=False, allow_nan=False).encode())
        exported = out / 'export.docx'
        raw, err = run('create-docx', model_path, exported)
        req(err == b'', 'Export stderr')
        req(same(strict_json(raw), {'blocks': 2, 'images': 0, 'bytes': exported.stat().st_size,
            'warnings': ['Basic document export; Word/WPS pagination and font substitution require visual verification']}), 'Complete export report')
        protected[exported] = sha(exported)
        raw, err = run('read-docx-model', exported)
        req(err == b'' and same(strict_json(raw), model), 'Exact trusted reopen model')
        emitted, package = package_audit(exported)
        req(emitted['paragraphs'] == expected_text and emitted['shading_count'] == 0, 'Emitted intended cells/no shd')
        req(emitted['grid'] == ['1800'] * 5, 'Expected normalized output grid')
        parts = package['parts']
        req(set(parts) == {'[Content_Types].xml', '_rels/.rels', 'word/document.xml', 'word/styles.xml',
            'word/numbering.xml', 'word/_rels/document.xml.rels', 'word/settings.xml', MODEL}, 'Strict emitted part set')
        envelope = strict_json(parts[MODEL])
        req(set(envelope) == {'version', 'document', 'hashes'} and type(envelope['version']) is int and envelope['version'] == 2, 'Model envelope')
        req(same(envelope['document'], model), 'Embedded model consistency')
        req(envelope['hashes'] == {name: digest(data) for name, data in parts.items() if name != MODEL}, 'Complete XML/relationship part hash binding')
        # Never offer the original fixture as an output destination, even for refusal tests.
        copied_source = save('existing-source-copy.docx', source.read_bytes())
        protected[copied_source] = sha(copied_source)
        for target in (exported, copied_source):
            raw, err = run('create-docx', model_path, target, expected=1)
            req(raw == b'' and err == b'Error: Output already exists; choose a new file\n', 'Exact no-overwrite refusal')
            req(sha(target) == protected[target], 'Existing output changed')
        report['source_audit'] = audit
        report['export_audit'] = emitted
        report['status'] = 'PASS'
    except BaseException as exc:
        report['error'] = str(exc)
        raise
    finally:
        unchanged = {str(path): path.is_file() and sha(path) == pin for path, pin in protected.items()}
        report['protected_unchanged'] = unchanged
        if not all(unchanged.values()):
            report['status'] = 'FAIL'
            report['error'] = 'Protected input/output changed'
        save('report.json', (json.dumps(report, indent=2, ensure_ascii=False) + '\n').encode())
        req(all(unchanged.values()), 'Protected input/output changed (including failure path)')
    print('PASS: full import, raw source proof, export XML/model, trusted reopen and no-overwrite')


if __name__ == '__main__':
    main()
