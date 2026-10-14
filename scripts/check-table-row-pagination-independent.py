#!/usr/bin/env python3
"""Source-first cantSplit verifier, independent of the product parser/writer.

prepare creates small authored controls, never invokes a product executable.
bind uses only the hash-pinned pre-feature CLI to freeze complete expectations.
run compares a pinned candidate against those already-frozen expectations.
All phases require an explicitly allocated serial resource slot. No rendering is
performed here; source-authored consumer controls and stale-integrity diagnostic
ZIPs remain distinct from the actual product outputs this checker produces.
"""
import argparse
import copy
import hashlib
import json
import os
import posixpath
import re
import shutil
import signal
import subprocess
import tempfile
import time
import traceback
import zipfile
from pathlib import Path
from lxml import etree as E

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / 'tmp/docx-row-pagination-verify'
ORIGINAL = ROOT / 'tmp/paragraph-indentation-source/poi/Bug60337.docx'
ORIGINAL_SHA = '87f983bdd93b3b5b8469104a5b09ccff41e0f5c144060e13fd4db07bfd2e39da'
ORIGINAL_BLOB = '06d0339fc5f854d90fe6721d991c512c815da4c5'
BASELINE = ROOT / 'tmp/docx-font-family-final2-stable-cli'
BASELINE_SHA = '0a3b8989776adc75dd5aac6698caefa92b28e83cee22aed3a02070b28d51b14f'
CONTRACT = ROOT / 'tmp/docx-row-pagination-design/CONTRACT.md'
CONTRACT_SHA = '17861ef07a8c990ed271b73c71a9eab419222c6e5077a7b9443a374a7409ee88'
W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/package/2006/relationships'
OR = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/'
NS = {'w': W}
Q = '{' + W + '}'
KEY = 'rowCantSplit'
PRESERVED = 'Direct table row cantSplit preserved; automatic pagination remains consumer-dependent'
FALLBACK = 'Table row cantSplit not preserved: '
PACKAGE = 'package or active style identity is unproved'
STRUCTURE = 'source structure or inherited row pagination is unsupported'
DIRECT = 'direct row declarations are invalid or outside the supported subset'
IDENTITY = 'source and converted table identities differ'
RESOURCE = 'proof resource limits exceeded'
FLOOR = 600 * 1024**2
ENTRY = 950 * 1024**2
MAX_GROWTH = 150 * 1024**2
# Exact pre-feature early-blocking vocabulary, independently read from published
# 6bb54c0 src/importing.rs and its referenced risk constants. This is a command
# boundary, not a classifier learned from candidate responses.
OLD_BLOCKING_RISKS = {
    'Tracked changes', 'Hidden text cannot be preserved in an editable copy',
    'Unsupported paragraph spacing cannot be preserved in an editable copy',
    'Unsupported paragraph indentation cannot be preserved in an editable copy',
    'Unsupported or unproved paragraph pagination',
    'Unsupported page geometry cannot be preserved in an editable copy',
    'Unsupported table geometry cannot be preserved in an editable copy',
    'Unsupported image geometry cannot be preserved in an editable copy',
    'Custom theme color mapping is unsupported', 'Unsupported theme color scheme content',
    'Unsupported or unresolved hyperlink target', 'Fields and generated text',
    'Merged table cells', 'Mathematical equations', 'Content controls or embedded content',
    'Floating objects, legacy drawings or text boxes',
}


def check(value, message):
    if not value:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    data = value if isinstance(value, bytes) else (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()
    with Path(path).open('xb') as stream:
        stream.write(data)


def load(path):
    return json.loads(Path(path).read_text())


def guard():
    free = shutil.disk_usage(ROOT).free
    check(free >= FLOOR, 'Disk floor reached; stop and notify parent, do not clean up')
    return free


def parts(path):
    with zipfile.ZipFile(path) as archive:
        check(len(archive.namelist()) == len(set(archive.namelist())), 'Duplicate ZIP entry')
        check(archive.testzip() is None, 'ZIP CRC failure')
        return {name: archive.read(name) for name in archive.namelist()}


def package(data, path):
    guard()
    with zipfile.ZipFile(path, 'x', zipfile.ZIP_DEFLATED) as archive:
        for name, content in data.items():
            archive.writestr(name, content)


def xml(data):
    return E.fromstring(data, E.XMLParser(resolve_entities=False, no_network=True))


def encode(node):
    return E.tostring(node, encoding='UTF-8', xml_declaration=True, standalone=True)


def child(parent, tag, **attrs):
    return E.SubElement(parent, Q + tag, {Q + k: str(v) for k, v in attrs.items()})


def related(data, base, kind, optional=False):
    path = '_rels/.rels' if base is None else posixpath.join(posixpath.dirname(base), '_rels', posixpath.basename(base) + '.rels')
    if optional and path not in data:
        return None
    root = xml(data[path])
    check(root.tag == '{' + R + '}Relationships', 'Relationship root namespace')
    ids = [node.get('Id') for node in root]
    check(len(ids) == len(set(ids)), 'Relationship ID uniqueness')
    candidates = [n for n in root if n.tag == '{' + R + '}Relationship' and n.get('Type') == OR + kind]
    check(len(candidates) == 1 or (optional and not candidates), 'Actual relationship cardinality')
    if not candidates:
        return None
    node = candidates[0]
    check(node.get('TargetMode') is None, 'Actual relationship internal')
    target = node.get('Target')
    check(target and not any(c in target for c in ('?', '#', '\\')), 'Actual relationship target syntax')
    path = target.lstrip('/') if target.startswith('/') else posixpath.normpath(posixpath.join(posixpath.dirname(base or ''), target))
    check(not path.startswith('../') and path in data, 'Actual relationship target exists in package')
    return path


def raw_text(paragraph):
    return ''.join(n.text or '' if n.tag == Q + 't' else '\t' if n.tag == Q + 'tab' else '\n'
                   for n in paragraph.iter() if n.tag in (Q + 't', Q + 'tab', Q + 'br'))


def raw_tables(data):
    main = related(data, None, 'officeDocument')
    root = xml(data[main])
    result = []
    for table in root.findall('w:body/w:tbl', NS):
        rows = []
        for row in table.findall('w:tr', NS):
            rows.append({'paragraphs': [[raw_text(p) for p in cell.findall('w:p', NS)] for cell in row.findall('w:tc', NS)],
                         'flags': [dict(n.attrib) for n in row.findall('w:trPr/w:cantSplit', NS)],
                         'header': [dict(n.attrib) for n in row.findall('w:trPr/w:tblHeader', NS)]})
        result.append(rows)
    return result


def model_tables(model):
    return [n for n in model.get('content', []) if n['type'] == 'table']


def model_text(node):
    if node['type'] == 'text':
        return node['text']
    if node['type'] == 'hardBreak':
        return '\n'
    return ''.join(model_text(n) for n in node.get('content', []))


def bind_identity(source, model):
    tables = model_tables(model)
    check(len(tables) == len(source), 'Complete direct-body table count')
    for table, rows in zip(tables, source):
        check(len(table['content']) == len(rows), 'Complete ordered row count')
        for row, expected in zip(table['content'], rows):
            cells = row['content']
            check(len(cells) == len(expected['paragraphs']), 'Complete ordered cell count')
            for cell, paragraphs in zip(cells, expected['paragraphs']):
                check(all(n['type'] in ('paragraph', 'heading') for n in cell.get('content', [])), 'No flattened synthetic source paragraphs')
                check([model_text(n) for n in cell.get('content', [])] == paragraphs, 'Exact cell paragraph order/text/empties')
            header = expected['header']
            if header:
                val = header[0].get(Q + 'val', 'true')
                if val in ('1', 'true', 'on', '0', 'false', 'off'):
                    check(all(c['type'] == ('tableHeader' if val in ('1', 'true', 'on') else 'tableCell') for c in cells), 'Independent direct header type')


def set_flag(row, value):
    props = row.find(Q + 'trPr')
    if props is None:
        props = E.Element(Q + 'trPr'); row.insert(0, props)
    for n in list(props):
        if E.QName(n).localname == 'cantSplit':
            props.remove(n)
    if value != 'absent':
        n = child(props, 'cantSplit')
        if value is not None:
            n.set(Q + 'val', value)


def declare_styles_part(data, path):
    types = xml(data['[Content_Types].xml'])
    original = next(n for n in types if n.get('PartName') == '/word/styles.xml')
    extra = copy.deepcopy(original); extra.set('PartName', '/' + path); types.append(extra)
    data['[Content_Types].xml'] = encode(types)


def source_recipes():
    """Declarative outcomes are fixed independently of either executable."""
    recipes = []
    def add(name, action, outcome='preserved', flags=None, reason=None):
        recipes.append({'name': name, 'action': action, 'outcome': outcome,
                        'expected_flags': flags if flags is not None else [[None, None, True]], 'reason': reason})
    add('genuine-original', None)
    for value in (None, '1', 'true', 'on', '0', 'false', 'off', 'absent'):
        add('direct-' + ('empty' if value is None else value), ('flag', value),
            'unchanged' if value == 'absent' else 'preserved', [[None, None, None if value == 'absent' else value in (None, '1', 'true', 'on')]])
    add('tri-state-rows', ('tri',), flags=[[None, False, True]])
    for value in ('', 'TRUE', 'FALSE', 'True', ' true', 'true ', '2', '-1', 'yes'):
        add('invalid-value-' + str(len(recipes)), ('flag', value), 'fallback', reason=DIRECT)
    for kind in ('duplicate-flag', 'duplicate-trpr', 'flag-child', 'flag-text', 'flag-unknown-attr', 'foreign-val', 'alternate-prefix', 'wrong-namespace', 'wrapped-flag'):
        add(kind, (kind,), 'fallback', reason=DIRECT)
    for kind in ('active-style-true', 'active-style-false', 'conditional-true', 'base-style-false', 'inherited-no-direct', 'direct-false-inherited-true', 'default-style-true'):
        add(kind, (kind,), 'fallback', reason=STRUCTURE)
    for kind in ('unused-style-true', 'unused-malformed-default', 'no-styles-relationship', 'actual-style-decoy', 'relocated-styles'):
        add(kind, (kind,))
    add('unused-style-no-direct', ('unused-style-no-direct',), 'unchanged', [[None, None, None]])
    for kind in ('active-style-missing', 'active-style-cycle', 'active-style-duplicate', 'active-style-wrapped', 'styles-missing-target', 'styles-malformed-part', 'styles-external', 'styles-duplicate-rel', 'styles-duplicate-id'):
        add(kind, (kind,), 'fallback', reason=PACKAGE)
    # Independent scope controls: an unresolved style cannot turn an unrelated
    # flagged template into active semantics. These are exact old responses.
    for kind in ('active-style-missing', 'active-style-cycle', 'styles-missing-target', 'styles-malformed-part', 'styles-external', 'styles-duplicate-rel', 'styles-duplicate-id'):
        add('no-candidate-' + kind, ('no-candidate', kind), 'unchanged', [[None, None, None]])
    for kind in ('unused-flag-missing-chain', 'unused-flag-broken-base'):
        add(kind, (kind,), 'unchanged', [[None, None, None]])
    for kind in ('reachable-base-flag', 'default-inherited-only'):
        add(kind, (kind,), 'fallback', reason=STRUCTURE)
    for kind in ('reachable-duplicate-flag', 'reachable-wrapped-flag', 'default-unknown-flag'):
        add(kind, (kind,), 'fallback', reason=PACKAGE)
    for kind in ('root-duplicate-main', 'root-wrapped-main', 'root-foreign-main', 'root-foreign-container', 'root-duplicate-id', 'root-external-main'):
        add(kind, (kind,), 'fallback', reason=PACKAGE)
        add('no-candidate-' + kind, ('no-candidate', kind), 'unchanged', [[None, None, None]])
    for kind in ('types-duplicate-main', 'types-wrapped-main', 'types-foreign-main', 'types-foreign-container', 'types-duplicate-styles', 'types-wrong-styles', 'types-missing-styles'):
        add(kind, (kind,), 'fallback', reason=PACKAGE)
        add('no-candidate-' + kind, ('no-candidate', kind), 'unchanged', [[None, None, None]])
    # Root OPC authority is required without expanding the old main-path subset.
    add('relocated-main', ('relocated-main',), 'required-old-refusal')
    for kind in ('two-supported', 'identical-tables', 'unrelated-before', 'unrelated-after', 'first-row', 'middle-row'):
        flags = {'two-supported': [[None, None, True], [False, None, None]], 'identical-tables': [[None, None, True], [None, None, True]],
                 'unrelated-before': [[None, None, None], [None, None, True]], 'unrelated-after': [[None, None, True], [None, None, None]],
                 'first-row': [[True, None, None]], 'middle-row': [[None, False, None]]}[kind]
        add(kind, (kind,), flags=flags)
    for kind in ('atomic-invalid-last', 'atomic-invalid-first'):
        add(kind, (kind,), 'fallback', reason=DIRECT)
    for kind in ('atomic-inherited-last', 'atomic-inherited-first'):
        add(kind, (kind,), 'fallback', reason=STRUCTURE)
    for kind in ('height-true', 'height-false', 'vertical-true', 'vertical-false', 'header-with-height', 'merged-cells', 'hidden-row', 'revised-row'):
        add(kind, (kind,), 'old-gate', reason=STRUCTURE)
    for kind in ('bookmark-unpaired', 'bookmark-duplicate', 'bookmark-foreign-id', 'bookmark-nonleaf', 'proof-unpaired', 'proof-unknown', 'proof-nonleaf', 'cell-wrapper', 'hard-page-break', 'inherited-header'):
        add(kind, (kind,), 'fallback', reason=STRUCTURE)
    for kind in ('header-false', 'header-off'):
        add(kind, (kind,))
    for kind in ('header-invalid', 'header-duplicate', 'header-wrapped'):
        add(kind, (kind,), 'fallback', reason=DIRECT)
    for parent in ('pPr', 'rPr', 'tcPr', 'tblPr'):
        for marker in ('proofErr', 'bookmarkStart', 'bookmarkEnd'):
            name = 'hidden-marker-' + parent + '-' + marker
            add(name, ('hidden-marker', parent, marker), 'fallback', reason=STRUCTURE)
            add('no-candidate-' + name, ('no-candidate-action', 'hidden-marker', parent, marker), 'unchanged', [[None, None, None]])
        for marker in ('paired-proof', 'paired-bookmark'):
            add('hidden-marker-' + parent + '-' + marker, ('hidden-marker', parent, marker), 'fallback', reason=STRUCTURE)
    for marker in ('paired-proof', 'paired-bookmark'):
        add('direct-marker-' + marker, ('direct-marker', marker))
    for reference in ('basedon', 'tblstyle'):
        for direct_flag in (False, True):
            add('missing-' + reference + '-unused-flag-' + ('direct' if direct_flag else 'no-direct'),
                ('missing-reference', reference, direct_flag), 'fallback' if direct_flag else 'unchanged',
                [[None, None, True if direct_flag else None]], PACKAGE if direct_flag else None)
    for reference in ('missing-tblstyle', 'wrapped-tblstyle'):
        add(reference + '-actual-default-flag', ('default-reference', reference), 'fallback', reason=PACKAGE)
    add('inactive-false-default-flag-no-direct', ('inactive-default', False), 'unchanged', [[None, None, None]])
    add('inactive-false-default-flag-direct', ('inactive-default', True))
    return recipes


def resource_recipes():
    """Separately allocated bounded resource phase; never in normal prepare."""
    result = []
    for name, action, outcome, flags, reason in [
        ('resource-rows-500', ('resource', 'rows', 500), 'preserved', [[None] * 499 + [True]], None),
        ('resource-rows-501', ('resource', 'rows', 501), 'fallback', None, STRUCTURE),
        ('resource-cells-50', ('resource', 'cells', 50), 'preserved', [[True]], None),
        ('resource-cells-51', ('resource', 'cells', 51), 'fallback', None, STRUCTURE),
        ('resource-chain-64', ('resource', 'chain', 64), 'preserved', [[True]], None),
        ('resource-chain-65', ('resource', 'chain', 65), 'fallback', None, PACKAGE),
        ('resource-text-below', ('resource', 'text', 2 * 1024**2 - 1), 'preserved', [[True]], None),
        ('resource-text-exact', ('resource', 'text', 2 * 1024**2), 'preserved', [[True]], None),
        ('resource-text-above', ('resource', 'text', 2 * 1024**2 + 1), 'fallback', None, RESOURCE),
        ('resource-work-exact', ('resource', 'work', 2_000_000), 'preserved', None, None),
        ('resource-work-above', ('resource', 'work', 2_000_001), 'fallback', None, RESOURCE),
        ('resource-old-chain-20', ('resource', 'chain', 20), 'preserved', [[True]], None),
        ('resource-old-chain-21', ('resource', 'chain', 21), 'fallback', None, PACKAGE),
        ('resource-compact-work-exact', ('resource', 'work-compact', 2_000_000), 'preserved', None, None),
        ('resource-compact-work-above', ('resource', 'work-compact', 2_000_001), 'fallback', None, RESOURCE),
    ]:
        result.append({'name': name, 'action': action, 'outcome': outcome, 'expected_flags': flags, 'reason': reason})
    return result


def resource_source(original, kind, limit):
    data = dict(original); doc = xml(data['word/document.xml']); styles = xml(data['word/styles.xml'])
    body = doc.find(Q + 'body'); old_table = body.find(Q + 'tbl'); index = list(body).index(old_table); body.remove(old_table)
    rows, cells = (limit, 1) if kind == 'rows' else (1, limit) if kind == 'cells' else (1, 1)
    chain_length = limit if kind == 'chain' else 64 if kind == 'work' else 16 if kind == 'work-compact' else 0
    style_nodes = []
    for i in range(chain_length):
        style = child(styles, 'style', type='table', styleId='IndependentBudget' + str(i)); style_nodes.append(style)
        if i: child(style, 'basedOn', val='IndependentBudget' + str(i - 1))
        if kind in ('work', 'work-compact'):
            # Twelve distinct legal conditional table-style slots, each with
            # ordinary bold/italic run properties; no inherited row flags.
            for slot in ('firstRow', 'lastRow', 'firstCol', 'lastCol', 'band1Vert', 'band2Vert', 'band1Horz', 'band2Horz', 'neCell', 'nwCell', 'seCell', 'swCell'):
                conditional = child(style, 'tblStylePr', type=slot)
                if kind == 'work':
                    props = child(conditional, 'rPr'); child(props, 'b'); child(props, 'i')
                else:
                    borders = child(child(conditional, 'tcPr'), 'tcBorders')
                    for edge in ('top', 'left', 'bottom', 'right', 'insideH', 'insideV'):
                        child(borders, edge, val='single', sz='4', color='auto')
    table = E.Element(Q + 'tbl'); props = child(table, 'tblPr')
    if chain_length: child(props, 'tblStyle', val='IndependentBudget' + str(chain_length - 1))
    child(props, 'tblW', w='0', type='auto'); grid = child(table, 'tblGrid')
    for _ in range(cells): child(grid, 'gridCol', w='100')
    for ri in range(rows):
        row = child(table, 'tr')
        if ri == rows - 1: child(child(row, 'trPr'), 'cantSplit', val='true')
        for ci in range(cells):
            cell = child(row, 'tc'); child(child(cell, 'tcPr'), 'tcW', w='100', type='dxa')
            child(child(child(cell, 'p'), 'r'), 't').text = 'x' * limit if kind == 'text' else 'X'
    tables = [table]
    if kind in ('work', 'work-compact'):
        chain_nodes = sum(sum(1 for _ in style.iter()) for style in style_nodes)
        table_nodes = sum(1 for _ in table.iter()); unit = table_nodes + chain_nodes
        count, padding = divmod(limit, unit)
        check(1 <= count <= 1500 and padding < 10_000, 'Independent bounded work construction')
        tables = [copy.deepcopy(table) for _ in range(count)]
        cell = tables[-1].find('w:tr/w:tc', NS)
        for _ in range(padding): child(cell, 'p')
        measured = sum(sum(1 for _ in t.iter()) + chain_nodes for t in tables)
        check(measured == limit, 'Independent exact XML node/chain work arithmetic')
    for offset, table in enumerate(tables): body.insert(index + offset, table)
    data['word/document.xml'], data['word/styles.xml'] = encode(doc), encode(styles)
    check(sum(1 for _ in doc.iter()) <= 100_000 and sum(1 for _ in styles.iter()) <= 100_000, 'Resource controls stay below independent XML node caps')
    return data


def mutate(original, action):
    data = dict(original)
    if action is None:
        return data
    if action[0] == 'resource':
        return resource_source(original, action[1], action[2])
    if action[0] in ('no-candidate', 'no-candidate-action'):
        data = mutate(original, action[1:])
        doc = xml(data['word/document.xml'])
        for row in doc.findall('w:body/w:tbl/w:tr', NS):
            set_flag(row, 'absent')
        data['word/document.xml'] = encode(doc)
        return data
    doc, styles = xml(data['word/document.xml']), xml(data['word/styles.xml'])
    body = doc.find(Q + 'body'); table = body.find(Q + 'tbl'); rows = table.findall(Q + 'tr')
    kind = action[0]
    style = next(n for n in styles if n.get(Q + 'styleId') == 'TableGrid')
    base = next(n for n in styles if n.get(Q + 'styleId') == 'TableNormal')
    rels = xml(data['word/_rels/document.xml.rels'])
    sr = next(n for n in rels if n.get('Type') == OR + 'styles')
    if kind == 'flag':
        set_flag(rows[2], action[1])
    elif kind == 'tri':
        set_flag(rows[1], 'false')
    elif kind in ('first-row', 'middle-row'):
        set_flag(rows[2], 'absent'); set_flag(rows[0 if kind == 'first-row' else 1], 'true' if kind == 'first-row' else 'false')
    elif kind.startswith('active-style-') or kind in ('conditional-true', 'base-style-false', 'inherited-no-direct', 'direct-false-inherited-true', 'default-style-true'):
        if kind == 'active-style-missing':
            table.find('w:tblPr/w:tblStyle', NS).set(Q + 'val', 'MissingTable')
        elif kind == 'active-style-cycle':
            child(base, 'basedOn', val='TableGrid')
        elif kind == 'active-style-duplicate':
            styles.append(copy.deepcopy(style))
        elif kind == 'active-style-wrapped':
            styles.remove(style); child(styles, 'wrapper').append(style)
        else:
            parent = base if kind == 'base-style-false' else style
            if kind == 'conditional-true': parent = child(parent, 'tblStylePr', type='firstRow')
            if kind == 'default-style-true':
                table.find(Q + 'tblPr').remove(table.find('w:tblPr/w:tblStyle', NS)); parent = base
            child(child(parent, 'trPr'), 'cantSplit', val='false' if kind in ('active-style-false', 'base-style-false') else 'true')
            if kind == 'inherited-no-direct': set_flag(rows[2], 'absent')
            if kind == 'direct-false-inherited-true': set_flag(rows[2], 'false')
    elif kind in ('unused-style-true', 'unused-style-no-direct', 'unused-malformed-default', 'actual-style-decoy', 'unused-flag-missing-chain', 'unused-flag-broken-base'):
        unused = child(styles, 'style', type='table', styleId='UnusedFlagged')
        child(child(unused, 'trPr'), 'cantSplit', val='true')
        if kind == 'unused-malformed-default':
            unused.set(Q + 'default', 'true'); child(unused, 'basedOn', val='NoSuchStyle')
        if kind == 'unused-style-no-direct': set_flag(rows[2], 'absent')
        if kind.startswith('unused-flag-'):
            set_flag(rows[2], 'absent')
            if kind.endswith('missing-chain'):
                table.find('w:tblPr/w:tblStyle', NS).set(Q + 'val', 'UnrelatedMissing')
            else:
                style.find(Q + 'basedOn').set(Q + 'val', 'UnrelatedMissing')
        if kind == 'actual-style-decoy':
            clean = original['word/styles.xml']; data['word/actualStyles.xml'] = clean; sr.set('Target', 'actualStyles.xml')
            declare_styles_part(data, 'word/actualStyles.xml')
            child(child(style, 'trPr'), 'cantSplit', val='true')
    elif kind == 'no-styles-relationship':
        rels.remove(sr); table.find(Q + 'tblPr').remove(table.find('w:tblPr/w:tblStyle', NS))
    elif kind == 'relocated-styles':
        sr.set('Target', '../custom/tableStyles.xml'); data['custom/tableStyles.xml'] = data['word/styles.xml']
        declare_styles_part(data, 'custom/tableStyles.xml')
    elif kind == 'styles-missing-target': sr.set('Target', 'missing.xml')
    elif kind == 'styles-malformed-part': pass  # Applied after ordinary XML serialization below.
    elif kind == 'styles-external': sr.set('TargetMode', 'External'); sr.set('Target', 'https://example.invalid/styles.xml')
    elif kind == 'styles-duplicate-rel':
        new = copy.deepcopy(sr); new.set('Id', 'independentSecondStyles'); rels.append(new)
    elif kind == 'styles-duplicate-id':
        new = copy.deepcopy(sr); new.set('Type', OR + 'unknown'); rels.append(new)
    elif kind.startswith('root-'):
        roots = xml(data['_rels/.rels'])
        main = next(n for n in roots if n.get('Type') == OR + 'officeDocument')
        if kind == 'root-duplicate-main':
            other = copy.deepcopy(main); other.set('Id', 'independentSecondMain'); roots.append(other)
        elif kind == 'root-wrapped-main':
            roots.remove(main); E.SubElement(roots, '{' + R + '}wrapper').append(main)
        elif kind == 'root-foreign-main': main.tag = '{urn:foreign}Relationship'
        elif kind == 'root-foreign-container': roots.tag = '{urn:foreign}Relationships'
        elif kind == 'root-duplicate-id':
            other = copy.deepcopy(main); other.set('Type', OR + 'unknown'); roots.append(other)
        elif kind == 'root-external-main': main.set('TargetMode', 'External')
        data['_rels/.rels'] = encode(roots)
    elif kind == 'relocated-main': pass  # Move the finished serialized document below.
    elif kind.startswith('types-'):
        types = xml(data['[Content_Types].xml'])
        main_type = next(n for n in types if n.get('PartName') == '/word/document.xml')
        style_type = next(n for n in types if n.get('PartName') == '/word/styles.xml')
        if kind == 'types-duplicate-main': types.append(copy.deepcopy(main_type))
        elif kind == 'types-wrapped-main': types.remove(main_type); E.SubElement(types, '{http://schemas.openxmlformats.org/package/2006/content-types}wrapper').append(main_type)
        elif kind == 'types-foreign-main': main_type.tag = '{urn:foreign}Override'
        elif kind == 'types-foreign-container': types.tag = '{urn:foreign}Types'
        elif kind == 'types-duplicate-styles': types.append(copy.deepcopy(style_type))
        elif kind == 'types-wrong-styles': style_type.set('ContentType', 'application/octet-stream')
        elif kind == 'types-missing-styles': types.remove(style_type)
        data['[Content_Types].xml'] = encode(types)
    elif kind in ('reachable-base-flag', 'reachable-duplicate-flag', 'reachable-wrapped-flag', 'default-inherited-only', 'default-unknown-flag'):
        set_flag(rows[2], 'absent')
        child(child(base, 'trPr'), 'cantSplit', val='false')
        if kind == 'reachable-duplicate-flag': styles.append(copy.deepcopy(base))
        elif kind == 'reachable-wrapped-flag': styles.remove(base); child(styles, 'wrapper').append(base)
        elif kind.startswith('default-'):
            table.find(Q + 'tblPr').remove(table.find('w:tblPr/w:tblStyle', NS))
            if kind == 'default-unknown-flag': base.set(Q + 'default', 'UNKNOWN')
    elif kind in ('two-supported', 'identical-tables', 'unrelated-before', 'unrelated-after', 'atomic-invalid-last', 'atomic-invalid-first', 'atomic-inherited-last', 'atomic-inherited-first'):
        second = copy.deepcopy(table)
        # Marker IDs are table-local proof identities; use distinct bookmark IDs
        # so this control cannot accidentally test cross-table marker collisions.
        for n in second.iter():
            if n.tag in (Q + 'bookmarkStart', Q + 'bookmarkEnd'): n.set(Q + 'id', '100')
        body.insert(list(body).index(table) + 1, second)
        srws = second.findall(Q + 'tr')
        if kind == 'two-supported':
            set_flag(srws[2], 'absent'); set_flag(srws[0], 'false')
        elif kind.startswith('unrelated-'):
            target = rows if kind.endswith('before') else srws; set_flag(target[2], 'absent')
        elif kind.startswith('atomic-invalid-'):
            target = rows if kind.endswith('first') else srws; set_flag(target[2], 'TRUE')
        elif kind.startswith('atomic-inherited-'):
            target = table if kind.endswith('first') else second
            target.find('w:tblPr/w:tblStyle', NS).set(Q + 'val', 'InheritedFlag')
            inherited = child(styles, 'style', type='table', styleId='InheritedFlag')
            child(child(inherited, 'trPr'), 'cantSplit', val='true')
            set_flag(target.findall(Q + 'tr')[2], 'absent')
    elif kind.startswith('height-') or kind.startswith('vertical-') or kind == 'header-with-height':
        if kind.endswith('false'): set_flag(rows[2], 'false')
        if kind.startswith('vertical-'): child(rows[2].find('w:tc/w:tcPr', NS), 'vAlign', val='center')
        else: child(rows[0 if kind == 'header-with-height' else 2].find(Q + 'trPr'), 'trHeight', val='100', hRule='atLeast')
    elif kind == 'merged-cells': child(rows[2].find('w:tc/w:tcPr', NS), 'gridSpan', val='2')
    elif kind == 'hidden-row': child(rows[2].find(Q + 'trPr'), 'hidden')
    elif kind == 'revised-row': child(rows[2].find(Q + 'trPr'), 'ins', id='7')
    elif kind.startswith('bookmark-'):
        start = next(n for n in table.iter() if n.tag == Q + 'bookmarkStart')
        end = next(n for n in table.iter() if n.tag == Q + 'bookmarkEnd')
        if kind == 'bookmark-unpaired': end.getparent().remove(end)
        elif kind == 'bookmark-duplicate': start.addnext(copy.deepcopy(start))
        elif kind == 'bookmark-foreign-id': start.set('{urn:foreign}id', '1')
        elif kind == 'bookmark-nonleaf': child(start, 't').text = 'not-passive'
    elif kind.startswith('proof-'):
        proof = next(n for n in table.iter() if n.tag == Q + 'proofErr')
        if kind == 'proof-unpaired': proof.getparent().remove(proof)
        elif kind == 'proof-unknown': proof.set(Q + 'type', 'unknownStart')
        elif kind == 'proof-nonleaf': child(proof, 't').text = 'not-passive'
    elif kind == 'cell-wrapper':
        cell = rows[2].find(Q + 'tc'); p = cell.find(Q + 'p'); cell.remove(p); child(cell, 'wrapper').append(p)
    elif kind == 'hard-page-break':
        child(rows[2].find('w:tc/w:p/w:r', NS), 'br', type='page')
    elif kind == 'inherited-header': child(child(style, 'trPr'), 'tblHeader', val='false')
    elif kind.startswith('header-'):
        props = rows[0].find(Q + 'trPr'); header = props.find(Q + 'tblHeader')
        if kind in ('header-false', 'header-off', 'header-invalid'): header.set(Q + 'val', {'header-false': 'false', 'header-off': 'off', 'header-invalid': 'TRUE'}[kind])
        elif kind == 'header-duplicate': props.append(copy.deepcopy(header))
        elif kind == 'header-wrapped': props.remove(header); child(props, 'wrapper').append(header)
    elif kind == 'hidden-marker':
        parent, marker = action[1:]
        paths = {'pPr': 'w:tc/w:p/w:pPr', 'rPr': 'w:tc/w:p/w:r/w:rPr', 'tcPr': 'w:tc/w:tcPr'}
        target = table.find(Q + 'tblPr') if parent == 'tblPr' else rows[2].find(paths[parent], NS)
        if target is None:
            owners = {'pPr': 'w:tc/w:p', 'rPr': 'w:tc/w:p/w:r', 'tcPr': 'w:tc'}
            owner = table if parent == 'tblPr' else rows[2].find(owners[parent], NS)
            check(owner is not None, 'Known source-authored property owner exists')
            target = E.Element(Q + parent); owner.insert(0, target)
        if marker == 'paired-proof': child(target, 'proofErr', type='spellStart'); child(target, 'proofErr', type='spellEnd')
        elif marker == 'paired-bookmark': child(target, 'bookmarkStart', id='999', name='HiddenPair'); child(target, 'bookmarkEnd', id='999')
        elif marker == 'proofErr': child(target, marker, type='unknownStart')
        elif marker == 'bookmarkStart': child(target, marker, id='999', name='UnpairedHidden')
        else: child(target, marker, id='999')
    elif kind == 'direct-marker':
        target = rows[1].find('w:tc/w:p', NS); first_run = target.find(Q + 'r')
        if action[1] == 'paired-proof':
            start = E.Element(Q + 'proofErr', {Q + 'type': 'spellStart'}); end = E.Element(Q + 'proofErr', {Q + 'type': 'spellEnd'})
        else:
            start = E.Element(Q + 'bookmarkStart', {Q + 'id': '999', Q + 'name': 'DirectPair'}); end = E.Element(Q + 'bookmarkEnd', {Q + 'id': '999'})
        first_run.addprevious(start); first_run.addnext(end)
    elif kind == 'missing-reference':
        reference, direct_flag = action[1:]
        if not direct_flag: set_flag(rows[2], 'absent')
        unused = child(styles, 'style', type='table', styleId='UnrelatedUnusedFlag')
        child(child(unused, 'trPr'), 'cantSplit', val='true')
        if reference == 'basedon': style.find(Q + 'basedOn').attrib.pop(Q + 'val')
        else:
            table.find('w:tblPr/w:tblStyle', NS).attrib.pop(Q + 'val')
            for n in styles: n.attrib.pop(Q + 'default', None)
    elif kind == 'default-reference':
        set_flag(rows[2], 'absent'); child(child(base, 'trPr'), 'cantSplit', val='true')
        selected = table.find('w:tblPr/w:tblStyle', NS)
        if action[1] == 'missing-tblstyle': selected.attrib.pop(Q + 'val')
        else:
            props = table.find(Q + 'tblPr'); props.remove(selected); child(props, 'wrapper').append(selected)
    elif kind == 'inactive-default':
        if not action[1]: set_flag(rows[2], 'absent')
        table.find(Q + 'tblPr').remove(table.find('w:tblPr/w:tblStyle', NS))
        base.set(Q + 'default', 'false'); child(child(base, 'trPr'), 'cantSplit', val='true')
    else:
        props = rows[2].find(Q + 'trPr'); flag = props.find(Q + 'cantSplit')
        if kind == 'duplicate-flag': props.append(copy.deepcopy(flag))
        elif kind == 'duplicate-trpr': rows[2].insert(0, copy.deepcopy(props))
        elif kind == 'flag-child': child(flag, 't').text = 'nested'
        elif kind == 'flag-text': flag.text = 'text'
        elif kind == 'flag-unknown-attr': flag.set(Q + 'unknown', 'x')
        elif kind == 'foreign-val': flag.set('{urn:foreign}val', 'false')
        elif kind == 'alternate-prefix':
            replacement = E.Element(Q + 'cantSplit', nsmap={'alternate': W}); props.replace(flag, replacement)
        elif kind == 'wrong-namespace': props.replace(flag, E.Element('{urn:foreign}cantSplit', nsmap={'foreign': 'urn:foreign'}))
        elif kind == 'wrapped-flag': props.remove(flag); child(props, 'wrapper').append(flag)
        else: raise AssertionError('Unknown independent recipe ' + kind)
    data['word/document.xml'], data['word/styles.xml'], data['word/_rels/document.xml.rels'] = encode(doc), encode(styles), encode(rels)
    if kind == 'styles-malformed-part': data['word/styles.xml'] = b'<w:styles'
    if kind == 'relocated-main':
        roots = xml(data['_rels/.rels'])
        next(n for n in roots if n.get('Type') == OR + 'officeDocument').set('Target', 'word/actualMain.xml')
        data['_rels/.rels'] = encode(roots)
        data['word/actualMain.xml'] = data.pop('word/document.xml')
        data['word/_rels/actualMain.xml.rels'] = data.pop('word/_rels/document.xml.rels')
        types = xml(data['[Content_Types].xml'])
        for n in types:
            if n.get('PartName') == '/word/document.xml': n.set('PartName', '/word/actualMain.xml')
        data['[Content_Types].xml'] = encode(types)
    if kind == 'alternate-prefix':
        # lxml may re-use an ancestor w binding; force the valid alternate QName
        # only for the direct flag, keeping every other canonical source QName.
        data['word/document.xml'] = re.sub(rb'<(?:w|alternate):cantSplit\b[^>]*/>', ('<alternate:cantSplit xmlns:alternate="' + W + '"/>').encode(), data['word/document.xml'], count=1)
        check(b'<alternate:cantSplit' in data['word/document.xml'], 'Authored alternate QName retained lexically')
    return data


def prepare(args):
    check(guard() >= ENTRY, '950 MiB entry headroom required'); check(sha(ORIGINAL) == ORIGINAL_SHA and sha(CONTRACT) == CONTRACT_SHA, 'Immutable source/contract pins')
    raw = ORIGINAL.read_bytes(); check(hashlib.sha1(f'blob {len(raw)}\0'.encode() + raw).hexdigest() == ORIGINAL_BLOB, 'Genuine Git blob')
    args.root.mkdir(parents=True, exist_ok=True)
    dest = Path(tempfile.mkdtemp(prefix='source-fixtures-', dir=args.root))
    original = parts(ORIGINAL); cases = []
    recipes = resource_recipes() if args.resource_controls else source_recipes()
    if args.case:
        check(set(args.case) <= {case['name'] for case in recipes}, 'Known explicitly selected source recipes')
        recipes = [case for case in recipes if case['name'] in args.case]
    for recipe in recipes:
        recipe = dict(recipe); action = recipe.pop('action'); data = mutate(original, action)
        path = ORIGINAL if recipe['name'] == 'genuine-original' else dest / (recipe['name'] + '.docx')
        if path != ORIGINAL: package(data, path)
        recipe.update(path=str(path), sha256=sha(path), kind='genuine-unchanged-public-POI' if path == ORIGINAL else 'independently-source-authored-control')
        try: recipe['raw_tables'] = raw_tables(data)
        except (AssertionError, KeyError, E.XMLSyntaxError) as error: recipe['raw_table_read_failure'] = str(error)
        if args.resource_controls:
            recipe['resource_derivation'] = {'kind': action[1], 'boundary': action[2], 'document_xml_nodes': sum(1 for _ in xml(data['word/document.xml']).iter()), 'styles_xml_nodes': sum(1 for _ in xml(data['word/styles.xml']).iter())}
            if action[1] in ('work', 'work-compact'):
                style_nodes = [n for n in xml(data['word/styles.xml']) if (n.get(Q + 'styleId') or '').startswith('IndependentBudget')]
                chain_size = sum(sum(1 for _ in node.iter()) for node in style_nodes)
                ts = xml(data['word/document.xml']).findall('w:body/w:tbl', NS)
                work = sum(sum(1 for _ in node.iter()) + chain_size for node in ts)
                check(work == action[2], 'Prepared work boundary independently recounted')
                recipe['resource_derivation'].update(chain_node_sum=chain_size, table_count=len(ts), cumulative_work=work)
                if recipe['outcome'] == 'preserved': recipe['expected_flags'] = [[True] for _ in ts]
        if recipe['outcome'] == 'preserved':
            check(len(recipe['raw_tables']) == len(recipe['expected_flags']), 'Independent recipe table count')
            for rows, flags in zip(recipe['raw_tables'], recipe['expected_flags']):
                check(len(rows) == len(flags), 'Independent recipe row count')
                for row, value in zip(rows, flags):
                    declarations = row['flags']
                    check(len(declarations) == (0 if value is None else 1), 'Independent declared tri-state cardinality')
                    if value is not None:
                        check(set(declarations[0]) <= {Q + 'val'}, 'Independent direct attribute identity')
                        lexical = declarations[0].get(Q + 'val', 'true')
                        check(lexical in ('1', 'true', 'on', '0', 'false', 'off') and (lexical in ('1', 'true', 'on')) == value, 'Independent lexical flag matches prospective recipe')
        recipe['part_sha256'] = {name: hashlib.sha256(value).hexdigest() for name, value in data.items()}
        cases.append(recipe)
    # Existing authored boundary controls are genuine-source derivatives, never
    # diagnostic export interventions. Their roles are explicitly distinguished.
    for path in ([] if args.case or args.resource_controls else sorted((ROOT / 'tmp/docx-row-pagination-experiment/boundary36').glob('source-authored-*.docx'))):
        name = path.stem; value = name.rsplit('-', 1)[-1]
        check(value in ('true', 'false', 'absent'), 'Boundary control lexical flag')
        data = parts(path)
        cases.append({'name': 'boundary36-' + name, 'path': str(path), 'sha256': sha(path),
                      'kind': 'reused-independent-source-authored-boundary-control', 'outcome': 'unchanged' if value == 'absent' else 'preserved',
                      'expected_flags': [[None, None, None if value == 'absent' else value == 'true']], 'reason': None, 'raw_tables': raw_tables(data),
                      'part_sha256': {name: hashlib.sha256(value).hexdigest() for name, value in data.items()}})
    protected = {str(ORIGINAL): ORIGINAL_SHA, str(CONTRACT): CONTRACT_SHA, str(BASELINE): BASELINE_SHA}
    protected.update({case['path']: case['sha256'] for case in cases})
    manifest = {'status': 'SOURCE_EXPECTATIONS_FROZEN_NO_PRODUCT_RUN', 'checker_sha256': sha(__file__), 'cases': cases,
                'diagnostics': {'preserved': PRESERVED, 'fallback_prefix': FALLBACK, 'categories': [PACKAGE, STRUCTURE, DIRECT, IDENTITY, RESOURCE]}, 'protected': protected}
    save(dest / 'manifest.json', manifest); print(dest / 'manifest.json')


class Runner:
    def __init__(self, dest):
        self.dest, self.commands, self.minimum_free = dest, [], guard()
        self.entry_free = self.minimum_free
        check(self.entry_free >= ENTRY, '950 MiB entry headroom required')

    def run(self, cli, *args):
        self.minimum_free = min(self.minimum_free, guard())
        index = len(self.commands) + 1
        stdout, stderr = self.dest / f'command-{index:04d}.stdout', self.dest / f'command-{index:04d}.stderr'
        argv = list(map(str, (cli, *args))); start = time.monotonic()
        with stdout.open('xb') as out, stderr.open('xb') as err:
            process = subprocess.Popen(argv, cwd=ROOT, stdout=out, stderr=err, start_new_session=True)
            try:
                while process.poll() is None:
                    self.minimum_free = min(self.minimum_free, guard())
                    check(self.entry_free - self.minimum_free <= MAX_GROWTH, '150 MiB observed growth budget reached')
                    check(time.monotonic() - start < 90, 'Command timeout')
                    check(stdout.stat().st_size + stderr.stat().st_size < 32 * 1024**2, 'Command output bound')
                    time.sleep(.1)
            except BaseException:
                os.killpg(process.pid, signal.SIGKILL); process.wait(); raise
        result = {'argv': argv, 'returncode': process.returncode, 'stdout': str(stdout), 'stderr': str(stderr),
                  'stdout_sha256': sha(stdout), 'stderr_sha256': sha(stderr)}
        self.commands.append(result)
        save(self.dest / f'command-{index:04d}.json', result)
        return result


def response(record):
    check(sha(record['stdout']) == record['stdout_sha256'] and sha(record['stderr']) == record['stderr_sha256'], 'Recorded raw response stream identities')
    return record['returncode'], Path(record['stdout']).read_bytes(), Path(record['stderr']).read_bytes()


def successful(record):
    code, out, err = response(record)
    check(code == 0, 'Unexpected CLI failure: ' + err.decode(errors='replace'))
    return json.loads(out)


def expected_response(old, case):
    expected = copy.deepcopy(old)
    if case['outcome'] == 'preserved':
        bind_identity(case['raw_tables'], expected['document'])
        tables = model_tables(expected['document'])
        check(len(tables) == len(case['expected_flags']), 'Recipe all-table flag count')
        for table, flags in zip(tables, case['expected_flags']):
            check(len(table['content']) == len(flags), 'Recipe all-row flag count')
            for row, value in zip(table['content'], flags):
                check(KEY not in row.get('attrs', {}), 'Baseline cannot contain new row flag')
                if value is not None: row.setdefault('attrs', {})[KEY] = value
        warning = PRESERVED
    elif case['outcome'] in ('fallback', 'old-gate'):
        warning = FALLBACK + case['reason']
    else:
        return expected
    warnings = expected['compatibility']['risks']
    check(warning not in warnings, 'No duplicate feature warning')
    warnings.append(warning); warnings.sort()
    return expected


def expected_audit(old, case):
    expected = copy.deepcopy(old)
    warning = PRESERVED if case['outcome'] == 'preserved' else FALLBACK + case['reason'] if case['outcome'] in ('fallback', 'old-gate') else None
    if warning:
        check(warning not in expected['risks'], 'No duplicate independent audit diagnostic')
        expected['risks'].append(warning); expected['risks'].sort()
    return expected


def late_refusal_audit_expected(case):
    old_import, audit = case['baseline_import'], case['baseline_audit']
    if not old_import['returncode'] or audit['returncode'] or case['outcome'] not in ('fallback', 'old-gate'):
        return None
    old = successful(audit)
    blocked = any(risk in OLD_BLOCKING_RISKS or risk.startswith(('Additional document content:', 'Embedded object or executable content:')) for risk in old['risks'])
    if blocked:
        return None
    return expected_audit(old, case)


def amend_audits(args):
    """Apply a reviewed command-boundary rule to immutable pre-candidate data."""
    old = load(args.binding); check(old['status'] == 'BASELINE_AND_FULL_EXPECTATIONS_FROZEN_BEFORE_CANDIDATE', 'Completed original binding')
    protect(old['protected'])
    dest = Path(tempfile.mkdtemp(prefix='audit-binding-', dir=args.root)); revised = copy.deepcopy(old); changes = []
    for case in revised['cases']:
        expected = late_refusal_audit_expected(case)
        if expected is None or case.get('expected_audit'):
            continue
        path = dest / (case['name'] + '.expected-audit.json'); save(path, expected)
        case.update(expected_audit=str(path), expected_audit_sha256=sha(path))
        changes.append({'name': case['name'], 'operation': 'audit-docx', 'old_stdout_sha256': case['baseline_audit']['stdout_sha256'],
                        'new_expected_json': str(path), 'new_expected_sha256': sha(path),
                        'only_added_risk': FALLBACK + case['reason'], 'import_raw_triple_unchanged': True})
    revised['oracle_transition'] = {'rule': 'Successful nonblocked audit receives its prospectively declared source fallback independently of a later import refusal; audit failures/early blockers/import triples remain exact',
        'parent_binding': str(args.binding.resolve()), 'parent_binding_sha256': sha(args.binding),
        'parent_checker_sha256': old['checker_sha256'], 'checker_sha256': sha(__file__), 'changes': changes,
        'candidate_responses_used_to_construct_expectations': False}
    revised['checker_sha256'] = sha(__file__)
    save(dest / 'binding.json', revised); print(dest / 'binding.json')


def protect(pins):
    for path, digest in pins.items(): check(sha(path) == digest, 'Protected identity changed: ' + path)


def bind(args):
    check(sha(BASELINE) == BASELINE_SHA, 'Frozen old CLI identity')
    manifest = load(args.manifest); protect(manifest['protected'])
    dest = Path(tempfile.mkdtemp(prefix='old-binding-', dir=args.root)); runner = Runner(dest)
    bound = {'status': 'BINDING', 'manifest': str(args.manifest.resolve()), 'manifest_sha256': sha(args.manifest),
             'checker_sha256': sha(__file__), 'baseline_sha256': BASELINE_SHA, 'cases': [], 'protected': dict(manifest['protected'])}
    try:
        for case in manifest['cases']:
            item = copy.deepcopy(case); name = case['name']
            read = runner.run(BASELINE, 'import-docx', case['path']); item['baseline_import'] = read
            audit = runner.run(BASELINE, 'audit-docx', case['path']); item['baseline_audit'] = audit
            if case['outcome'] == 'required-old-refusal':
                check(read['returncode'] != 0, 'Known old main-part subset refusal remains authoritative')
            if read['returncode']:
                item['effective_outcome'] = 'exact-old-refusal'
                audit_expected = late_refusal_audit_expected(item)
                if audit_expected is not None:
                    ap = dest / (name + '.expected-audit.json'); save(ap, audit_expected)
                    item.update(expected_audit=str(ap), expected_audit_sha256=sha(ap))
            else:
                old = successful(read); expected = expected_response(old, case)
                out = dest / (name + '.expected.json'); save(out, expected)
                item.update(expected_response=str(out), expected_response_sha256=sha(out), effective_outcome=case['outcome'])
                if audit['returncode'] == 0:
                    ap = dest / (name + '.expected-audit.json'); save(ap, expected_audit(successful(audit), case))
                    item.update(expected_audit=str(ap), expected_audit_sha256=sha(ap))
                mp = dest / (name + '.old-model.json'); save(mp, old['document'])
                package_path = dest / (name + '.old-output.docx')
                successful(runner.run(BASELINE, 'create-docx', mp, package_path))
                item.update(baseline_package=str(package_path), baseline_package_sha256=sha(package_path))
            bound['cases'].append(item)
        bound['status'] = 'BASELINE_AND_FULL_EXPECTATIONS_FROZEN_BEFORE_CANDIDATE'
    except BaseException:
        bound['status'] = 'FAILED_RETAINED'; bound['error'] = traceback.format_exc(); raise
    finally:
        protect(manifest['protected'])
        bound.update(commands=runner.commands, minimum_free_bytes=runner.minimum_free)
        save(dest / 'binding.json', bound); print(dest / 'binding.json')


def xml_product(path, old_path, model):
    now, old = parts(path), parts(old_path)
    check(set(now) == set(old), 'No new parts/settings/style declarations')
    mp = next(name for name in now if name.startswith('rust-wps/model-v') and name.endswith('.json'))
    check(all(now[name] == old[name] for name in now if name not in ('word/document.xml', mp)), 'Unrelated package parts stay byte exact')
    root = xml(now['word/document.xml']); tables = root.findall('w:body/w:tbl', NS)
    desired = model_tables(model); check(len(tables) == len(desired), 'Written all-table identity')
    any_flag = False
    for table, wanted in zip(tables, desired):
        rows = table.findall(Q + 'tr'); check(len(rows) == len(wanted['content']), 'Written all-row identity')
        for row, want in zip(rows, wanted['content']):
            declarations = row.findall('w:trPr/w:cantSplit', NS); value = want.get('attrs', {}).get(KEY)
            check(len(declarations) == (0 if value is None else 1), 'Exact direct cantSplit cardinality')
            if value is not None:
                any_flag = True; n = declarations[0]
                check(n.prefix == 'w' and dict(n.attrib) == {Q + 'val': '1' if value else '0'} and len(n) == 0 and not n.text, 'Canonical direct 1/0 leaf')
    # This is a surgical byte oracle, not XML reserialization. Remove only each
    # independently expected canonical flag and its newly necessary empty trPr;
    # every other byte must then equal the frozen pre-feature writer output.
    row_pattern = re.compile(rb'<w:tr>.*?</w:tr>', re.S)
    actual_rows = list(row_pattern.finditer(now['word/document.xml']))
    old_rows = list(row_pattern.finditer(old['word/document.xml']))
    expected_rows = [row for table in desired for row in table['content']]
    check(len(actual_rows) == len(old_rows) == len(expected_rows), 'Surgical all-row byte identity count')
    stripped = []; position = 0; explicit_count = 0
    for actual_row, old_row, wanted in zip(actual_rows, old_rows, expected_rows):
        stripped.append(now['word/document.xml'][position:actual_row.start()])
        raw = actual_row.group(); prior = old_row.group(); value = wanted.get('attrs', {}).get(KEY)
        if value is not None:
            explicit_count += 1; token = b'<w:cantSplit w:val="' + (b'1' if value else b'0') + b'"/>'
            # CT_TrPr order places cantSplit before trHeight/tblHeader. Existing
            # row property bytes and all following cell bytes remain untouched.
            expected_row = prior.replace(b'<w:trPr>', b'<w:trPr>' + token, 1) if b'<w:trPr>' in prior else prior.replace(b'<w:tr>', b'<w:tr><w:trPr>' + token + b'</w:trPr>', 1)
            check(raw == expected_row, 'Exact independently inserted flag/property location and schema order')
            check(raw.count(token) == 1, 'Exactly one canonical byte insertion per explicit row')
            raw = raw.replace(token, b'', 1)
            if b'<w:trPr>' not in prior:
                check(raw.count(b'<w:trPr></w:trPr>') == 1, 'Only newly necessary row property wrapper')
                raw = raw.replace(b'<w:trPr></w:trPr>', b'', 1)
        check(raw == prior, 'All unrelated row XML bytes exact, including header/height/cell/paragraph formatting')
        stripped.append(raw); position = actual_row.end()
    stripped.append(now['word/document.xml'][position:])
    check(now['word/document.xml'].count(b'<w:cantSplit') == explicit_count, 'No out-of-scope extra cantSplit declarations')
    check(b''.join(stripped) == old['word/document.xml'], 'Entire document XML differs only by approved row flag insertions')
    if not any_flag: check(Path(path).read_bytes() == Path(old_path).read_bytes(), 'Absent full old package bytes')
    embedded = json.loads(now[mp]); check(embedded['document'] == model, 'Complete embedded document')
    check(set(embedded['hashes']) == set(now) - {mp}, 'Complete trusted part-set coverage')
    check(all(embedded['hashes'][name] == hashlib.sha256(content).hexdigest() for name, content in now.items() if name != mp), 'Complete trusted part hashes')
    bind_identity(raw_tables(now), model)
    return {'parts_exact_except_document_model': True, 'document_xml_exact_except_approved_flag_insertions': True,
            'all_row_flags_canonical': True, 'all_text_bound': True}


def create_checked(runner, cli, dest, stem, model, baseline_model=None):
    mp, output = dest / (stem + '.json'), dest / (stem + '.docx'); save(mp, model)
    successful(runner.run(cli, 'create-docx', mp, output))
    check(successful(runner.run(cli, 'read-docx-model', output)) == model, 'Complete trusted reopen')
    digest = sha(output); refusal = runner.run(cli, 'create-docx', mp, output)
    check(refusal['returncode'] != 0 and b'Output already exists' in Path(refusal['stderr']).read_bytes() and sha(output) == digest, 'No overwrite existing output')
    result = {'model': str(mp), 'path': str(output), 'sha256': digest, 'trusted_reopen': True, 'no_overwrite': True}
    if baseline_model is not None:
        bm, bo = dest / (stem + '.old.json'), dest / (stem + '.old.docx'); save(bm, baseline_model)
        successful(runner.run(BASELINE, 'create-docx', bm, bo)); result['xml_checks'] = xml_product(output, bo, model)
    return result


def strip_flags(model):
    result = copy.deepcopy(model)
    def walk(n):
        n.get('attrs', {}).pop(KEY, None)
        if n.get('attrs') == {}: n.pop('attrs')
        for child_node in n.get('content', []): walk(child_node)
    walk(result); return result


def author_checks(runner, cli, dest):
    cell = {'type': 'tableCell', 'content': [{'type': 'paragraph', 'content': [{'type': 'text', 'text': 'Independent cell'}]}]}
    row = {'type': 'tableRow', 'content': [copy.deepcopy(cell), copy.deepcopy(cell)]}
    base = {'type': 'doc', 'content': [{'type': 'table', 'content': [copy.deepcopy(row), copy.deepcopy(row), copy.deepcopy(row)]}]}
    valid = []
    for profile in (None, 'standard-v2'):
        for value in (None, False, True):
            m = copy.deepcopy(base)
            if profile: m['attrs'] = {'styleProfile': profile}
            if value is not None: m['content'][0]['content'][1]['attrs'] = {KEY: value}
            valid.append((str(profile) + '-' + str(value), m))
    mixed = copy.deepcopy(base); mixed['content'][0]['content'][1]['attrs'] = {KEY: False}; mixed['content'][0]['content'][2]['attrs'] = {KEY: True}
    valid.append(('tri-state', mixed))
    for value in (False, True):
        m = copy.deepcopy(base); r = m['content'][0]['content'][0]; r['attrs'] = {KEY: value, 'rowMinHeightTwips': 100}
        for c in r['content']: c['type'] = 'tableHeader'; c['attrs'] = {'cellVerticalAlign': 'bottom'}
        valid.append(('height-header-' + str(value), m))
        tall = copy.deepcopy(m)
        for c in tall['content'][0]['content'][0]['content']: c['content'] *= 100
        valid.append(('natural-tall-' + str(value), tall))
    outputs = []
    for name, model in valid:
        product = create_checked(runner, cli, dest, 'author-' + name, model, strip_flags(model))
        if name.startswith('height-header-'):
            old_import = runner.run(BASELINE, 'import-docx', product['path'])
            new_import = runner.run(cli, 'import-docx', product['path'])
            check(old_import['returncode'] != 0 and response(new_import) == response(old_import), 'Author height/header/flag trusted support must not relax generic-source old refusal')
            product['generic_reimport_exact_old_refusal'] = True
        outputs.append(product)
    invalid = []
    for value in (None, 0, 1, -1, '', 'true', 'false', [], {}, [True]):
        m = copy.deepcopy(base); m['content'][0]['content'][0]['attrs'] = {KEY: value}; invalid.append(m)
    for location in ('doc', 'table', 'cell', 'paragraph', 'text', 'mark'):
        for value in (False, True):
            m = copy.deepcopy(base); t = m['content'][0]; r = t['content'][0]; c = r['content'][0]; p = c['content'][0]; text = p['content'][0]
            nodes = {'doc': m, 'table': t, 'cell': c, 'paragraph': p, 'text': text}
            if location == 'mark': text['marks'] = [{'type': 'bold', 'attrs': {KEY: value}}]
            else: nodes[location].setdefault('attrs', {})[KEY] = value
            invalid.append(m)
    for value in (False, True):
        for mutation in ('nested', 'ragged', 'span', 'colwidth', 'overrows', 'overcells'):
            m = copy.deepcopy(base); t = m['content'][0]; t['content'][0]['attrs'] = {KEY: value}
            if mutation == 'nested': t['content'][0]['content'][0]['content'].append(copy.deepcopy(base['content'][0]))
            elif mutation == 'ragged': t['content'][1]['content'].pop()
            elif mutation == 'span': t['content'][0]['content'][0]['attrs'] = {'colspan': 2}
            elif mutation == 'colwidth': t['content'][0]['content'][0]['attrs'] = {'colwidth': [100]}
            elif mutation == 'overrows': t['content'] = [copy.deepcopy(t['content'][0]) for _ in range(501)]
            elif mutation == 'overcells':
                for r in t['content']: r['content'] = [copy.deepcopy(cell) for _ in range(51)]
            invalid.append(m)
    for i, model in enumerate(invalid):
        mp, out = dest / f'invalid-{i}.json', dest / f'invalid-{i}.docx'; save(mp, model)
        record = runner.run(cli, 'create-docx', mp, out)
        check(record['returncode'] != 0 and not out.exists() and not Path(record['stdout']).read_bytes(), 'Strict invalid model creates no file or success output')
    return {'valid': outputs, 'invalid_count': len(invalid)}


def tamper_checks(runner, cli, dest, output):
    original = parts(output); mp = next(n for n in original if n.startswith('rust-wps/model-'))
    changes = []
    for name, old, new in [('true-to-false', b'<w:cantSplit w:val="1"/>', b'<w:cantSplit w:val="0"/>'),
                           ('remove', b'<w:cantSplit w:val="1"/>', b''),
                           ('duplicate', b'<w:cantSplit w:val="1"/>', b'<w:cantSplit w:val="1"/><w:cantSplit w:val="1"/>')]:
        data = dict(original); data['word/document.xml'] = data['word/document.xml'].replace(old, new, 1)
        check(data != original, 'Tamper changes actual output'); changes.append((name, data))
    for name in ('extra-part', 'missing-part', 'relationship'):
        data = dict(original)
        if name == 'extra-part': data['word/unexpected.xml'] = b'<unexpected/>'
        elif name == 'missing-part': data.pop('word/styles.xml')
        else: data['_rels/.rels'] = data['_rels/.rels'].replace(b'word/document.xml', b'word/changed.xml')
        check(data != original, 'Part tamper changes output'); changes.append((name, data))
    data = dict(original); doc = xml(data['word/document.xml']); rows = doc.findall('w:body/w:tbl/w:tr', NS)
    old_parent = rows[2].find(Q + 'trPr'); flag = old_parent.find(Q + 'cantSplit'); old_parent.remove(flag)
    first_props = rows[0].find(Q + 'trPr'); first_props.append(flag)
    data['word/document.xml'] = encode(doc); changes.append(('relocated-row-ownership', data))
    count = 0
    for name, data in changes:
        for repaired in (False, True):
            data = dict(data)
            if repaired:
                embedded = json.loads(data[mp]); embedded['hashes'] = {n: hashlib.sha256(v).hexdigest() for n, v in data.items() if n != mp}
                data[mp] = json.dumps(embedded, separators=(',', ':'), ensure_ascii=False).encode()
            path = dest / ('tamper-' + name + '-' + str(repaired) + '.docx'); package(data, path)
            result = runner.run(cli, 'read-docx-model', path)
            check(result['returncode'] != 0 and not Path(result['stdout']).read_bytes(), 'Tampered source must not return any trusted model'); count += 1
    for value in (False, None, 'true'):
        data = dict(original); embedded = json.loads(data[mp]); rows = model_tables(embedded['document'])[0]['content']
        if value is None: rows[2]['attrs'].pop(KEY)
        else: rows[2]['attrs'][KEY] = value
        data[mp] = json.dumps(embedded, separators=(',', ':'), ensure_ascii=False).encode()
        path = dest / ('tamper-model-' + str(value) + '.docx'); package(data, path)
        result = runner.run(cli, 'read-docx-model', path)
        check(result['returncode'] != 0 and not Path(result['stdout']).read_bytes(), 'Model-only tamper must not return trusted model'); count += 1
    return count


def run(args):
    binding = load(args.binding)
    check(binding['status'] == 'BASELINE_AND_FULL_EXPECTATIONS_FROZEN_BEFORE_CANDIDATE', 'Completed independent baseline freeze required')
    frozen_commands = list(binding['commands'])
    manifests = {binding['manifest']: binding['manifest_sha256']}
    supplements = []
    for path in args.extra_binding:
        extra = load(path)
        check(extra['status'] == 'BASELINE_AND_FULL_EXPECTATIONS_FROZEN_BEFORE_CANDIDATE', 'Completed supplemental source freeze')
        check(not ({case['name'] for case in binding['cases']} & {case['name'] for case in extra['cases']}), 'Supplemental case names remain distinct')
        binding['cases'].extend(extra['cases']); binding['protected'].update(extra['protected'])
        frozen_commands.extend(extra['commands']); manifests[extra['manifest']] = extra['manifest_sha256']
        supplements.append({'path': str(path.resolve()), 'sha256': sha(path)})
    cli = args.cli.resolve(); check(sha(cli) == args.expected_cli_sha256, 'Pinned candidate CLI identity')
    pins = dict(binding['protected']); pins.update({str(cli): sha(cli), str(args.binding.resolve()): sha(args.binding)})
    if binding.get('oracle_transition'):
        transition = binding['oracle_transition']; pins[transition['parent_binding']] = transition['parent_binding_sha256']
    pins.update({entry['path']: entry['sha256'] for entry in supplements})
    pins.update(manifests); pins[str(Path(__file__).resolve())] = sha(__file__)
    for command in frozen_commands:
        for stream in ('stdout', 'stderr'): pins[command[stream]] = command[stream + '_sha256']
    for case in binding['cases']:
        if case.get('expected_response'): pins[case['expected_response']] = case['expected_response_sha256']
        if case.get('expected_audit'): pins[case['expected_audit']] = case['expected_audit_sha256']
        if case.get('baseline_package'): pins[case['baseline_package']] = case['baseline_package_sha256']
    protect(pins)
    dest = Path(tempfile.mkdtemp(prefix='candidate-run-', dir=args.root)); runner = Runner(dest)
    report = {'status': 'RUNNING', 'cli': str(cli), 'cli_sha256': sha(cli), 'checker_sha256': sha(__file__),
              'binding': str(args.binding.resolve()), 'binding_sha256': sha(args.binding), 'cases': [], 'new_pdfs': 0}
    report['supplemental_bindings'] = supplements
    try:
        for case in binding['cases']:
            actual = runner.run(cli, 'import-docx', case['path']); outcome = case['effective_outcome']
            item = {'name': case['name'], 'source': case['path'], 'source_sha256': case['sha256'], 'outcome': outcome, 'import': actual}
            audit = runner.run(cli, 'audit-docx', case['path']); item['audit'] = audit
            if case.get('expected_audit'):
                check(successful(audit) == load(case['expected_audit']), 'Complete source-frozen audit response: ' + case['name'])
                check(Path(audit['stderr']).read_bytes() == Path(case['baseline_audit']['stderr']).read_bytes(), 'Unchanged audit stderr')
                if outcome == 'unchanged': check(response(audit) == response(case['baseline_audit']), 'No-candidate audit exact bytes')
            else:
                check(response(audit) == response(case['baseline_audit']), 'Exact old rejected-source audit response')
            if outcome == 'exact-old-refusal': check(response(actual) == response(case['baseline_import']), 'Exact old hard refusal: ' + case['name'])
            else:
                expected = load(case['expected_response']); got = successful(actual)
                check(got == expected, 'Complete independently frozen response mismatch: ' + case['name'])
                check(Path(actual['stderr']).read_bytes() == Path(case['baseline_import']['stderr']).read_bytes(), 'Unchanged import stderr')
                if outcome == 'unchanged': check(response(actual) == response(case['baseline_import']), 'No-candidate exact response bytes')
                product = create_checked(runner, cli, dest, case['name'] + '.actual-product', got['document'])
                product['xml_checks'] = xml_product(product['path'], case['baseline_package'], got['document'])
                item['product'] = product
                before = sha(case['path']); result = runner.run(cli, 'create-docx', product['model'], case['path'])
                check(result['returncode'] != 0 and b'Output already exists' in Path(result['stderr']).read_bytes() and sha(case['path']) == before, 'Original source no-overwrite')
            report['cases'].append(item)
        if args.source_only:
            report['author'] = {'status': 'not_requested_in_separate_source_phase'}
            report['tamper_refusals'] = 0
            report['status'] = 'PASS_SEPARATE_SOURCE_PHASE_MODEL_WRITER_TRUSTED_CONSUMER_NOT_RUN'
        else:
            report['author'] = author_checks(runner, cli, dest)
            original = next(c for c in report['cases'] if c['name'] == 'genuine-original')['product']['path']
            report['tamper_refusals'] = tamper_checks(runner, cli, dest, original)
            report['status'] = 'PASS_SOURCE_MODEL_WRITER_TRUSTED_CHECKS_CONSUMER_NOT_RUN'
    except BaseException:
        report['status'] = 'FAILED_RETAINED'; report['error'] = traceback.format_exc(); raise
    finally:
        protect(pins); report.update(commands=runner.commands, minimum_free_bytes=runner.minimum_free, protected_verified=True)
        save(dest / 'report.json', report); print(dest / 'report.json')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('prepare', 'bind', 'amend-audits', 'run'))
    parser.add_argument('--root', type=Path, default=DEFAULT)
    parser.add_argument('--serialized-slot', action='store_true')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--case', action='append', default=[], help='Prepare only named prospective recipes in a new supplemental manifest')
    parser.add_argument('--resource-controls', action='store_true', help='Prepare only separately allocated bounded resource controls')
    parser.add_argument('--source-only', action='store_true', help='Separate supplemental/resource source phase; author/tamper suites are explicitly not rerun')
    parser.add_argument('--binding', type=Path)
    parser.add_argument('--extra-binding', type=Path, action='append', default=[])
    parser.add_argument('--cli', type=Path)
    parser.add_argument('--expected-cli-sha256')
    args = parser.parse_args()
    check(args.serialized_slot, 'Root must explicitly allocate a serial resource slot before execution')
    args.root.mkdir(parents=True, exist_ok=True)
    if args.phase == 'bind': check(args.manifest, '--manifest required')
    if args.phase == 'amend-audits': check(args.binding, '--binding required')
    if args.phase == 'run': check(args.binding and args.cli and args.expected_cli_sha256, 'Frozen binding and pinned candidate required')
    {'prepare': prepare, 'bind': bind, 'amend-audits': amend_audits, 'run': run}[args.phase](args)


if __name__ == '__main__':
    main()
