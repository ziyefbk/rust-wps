"""Independent image-only paragraph spacing, alignment and baseline regression.

Generated sources use python-docx/Pillow with small, explicit OOXML mutations.
Expected attrs are fixed before importing; an independent OOXML cascade reader,
python-docx, and LibreOffice PDF coordinates inspect the actual source/output.
The pinned untouched mammoth.js fixture remains separate from generated cases.
No source or existing output is overwritten. No Word/WPS/native-app claim is made.

  python scripts/check-word-image-paragraphs.py --generate-only
  python scripts/check-word-image-paragraphs.py --render --corpus-dir tmp/image-corpus
  python scripts/check-word-image-paragraphs.py --legacy-evidence-dir tmp/word-image-dimensions/run-a0fe72vj
"""
import argparse
import copy
import hashlib
import importlib.util
import io
import json
import os
import platform
import shutil
import subprocess
import tempfile
import zipfile
from importlib.metadata import version
from pathlib import Path

from docx import Document
from docx.enum.style import WD_STYLE_TYPE
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.shared import Emu, Pt, Twips
from lxml import etree as ET
from PIL import Image, ImageDraw

_spec = importlib.util.spec_from_file_location("image_dimensions", Path(__file__).with_name("check-word-image-dimensions.py"))
d = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(d)
w, NS, check, digest, run_cli = d.w, d.NS, d.check, d.digest, d.run_cli
KEY = "imageParagraph"
WARNING = "Image-only paragraph spacing and alignment preserved; other page and paragraph layout may be normalized"
METRIC_WARNING = "Image paragraph baseline font is unspecified; editor defaults may change its line box"
ALIGN = {"left": WD_ALIGN_PARAGRAPH.LEFT, "center": WD_ALIGN_PARAGRAPH.CENTER,
         "right": WD_ALIGN_PARAGRAPH.RIGHT, "justify": WD_ALIGN_PARAGRAPH.JUSTIFY}
BASE_FONT = {"family": "DejaVu Sans", "halfPoints": 24, "bold": False, "italic": False}
TOLERANCE = 0.30


def child(parent, tag, **attrs):
    return w.child(parent, tag, **attrs)


def remove(parent, tag):
    return w.remove(parent, tag)


def props(before=0, after=0, line=240, align="left", paragraph_font=BASE_FONT, run_font=None):
    value = {"spaceBefore": before, "spaceAfter": after, "lineSpacing": line, "textAlign": align}
    if paragraph_font is not None:
        value.update(paragraphFont=copy.deepcopy(paragraph_font),
                     runFont=copy.deepcopy(paragraph_font if run_font is None else run_font))
    return value


def set_font(target, value):
    f = target.font
    f.name, f.size, f.bold, f.italic = value["family"], Pt(value["halfPoints"] / 2), value["bold"], value["italic"]
    # OOXML script-specific metrics agree, so no consumer chooses a different strut.
    rp = target.element.get_or_add_rPr() if hasattr(target, "element") else target._r.get_or_add_rPr()
    rf = rp.find("w:rFonts", NS)
    for name in ["asciiTheme", "hAnsiTheme", "eastAsiaTheme", "cstheme", "csTheme"]:
        rf.attrib.pop(qn("w:" + name), None)
    for name in ["ascii", "hAnsi", "eastAsia", "cs"]:
        rf.set(qn("w:" + name), value["family"])
    for name in ["szCs", "bCs", "iCs"]:
        remove(rp, name)
    child(rp, "szCs", val=value["halfPoints"])
    child(rp, "bCs", val="1" if value["bold"] else "0")
    child(rp, "iCs", val="1" if value["italic"] else "0")


def set_spacing(target, value, only=None):
    fmt = target.paragraph_format
    if only is None or "spaceBefore" in only: fmt.space_before = Twips(value["spaceBefore"])
    if only is None or "spaceAfter" in only: fmt.space_after = Twips(value["spaceAfter"])
    if only is None or "lineSpacing" in only: fmt.line_spacing = value["lineSpacing"] / 240
    if only is None or "textAlign" in only: fmt.alignment = ALIGN[value["textAlign"]]


def independent_paragraphs(path):
    """Read independent effective paragraph/style and drawing-run declarations.

    Only the generated, admitted subset is interpreted here. Theme-dependent
    families use the actual package theme Latin declaration, never the OS.
    """
    with zipfile.ZipFile(path) as z:
        root, styles = ET.fromstring(z.read("word/document.xml")), ET.fromstring(z.read("word/styles.xml"))
        theme_fonts = {}
        relationships = ET.fromstring(z.read("word/_rels/document.xml.rels"))
        theme_rel = next((r for r in relationships if r.get("Type", "").endswith("/theme") and r.get("TargetMode") != "External"), None)
        if theme_rel is not None:
            target = theme_rel.get("Target")
            theme_path = target.lstrip("/") if target.startswith("/") else d.posixpath.normpath("word/" + target)
            theme = ET.fromstring(z.read(theme_path))
            for group in ["major", "minor"]:
                latin = theme.find("a:themeElements/a:fontScheme/a:" + group + "Font/a:latin", NS)
                if latin is not None:theme_fonts[group] = latin.get("typeface")
    style_map = {s.get(qn("w:styleId")): s for s in styles.findall("w:style", NS)}
    default = next((s.get(qn("w:styleId")) for s in style_map.values()
                    if s.get(qn("w:type")) == "paragraph" and s.get(qn("w:default")) in ("1", "true", "on")), None)
    def chain(identifier, seen=()):
        if identifier is None: return []
        check(identifier not in seen and identifier in style_map, "Independent reader: missing/cyclic style")
        s = style_map[identifier]
        base = s.find("w:basedOn", NS)
        return chain(None if base is None else base.get(qn("w:val")), seen + (identifier,)) + [s]
    def apply_p(value, ppr):
        if ppr is None: return
        spacing = ppr.find("w:spacing", NS)
        if spacing is not None:
            for attr, key in [("before", "spaceBefore"), ("after", "spaceAfter"), ("line", "lineSpacing")]:
                v = spacing.get(qn("w:" + attr))
                if v is not None: value[key] = int(v)
        jc = ppr.find("w:jc", NS)
        if jc is not None: value["textAlign"] = {"both": "justify"}.get(jc.get(qn("w:val")), jc.get(qn("w:val")))
    def apply_r(value, rpr, style=False):
        if rpr is None: return
        rf = rpr.find("w:rFonts", NS)
        if rf is not None:
            for attr in ("ascii", "hAnsi", "asciiTheme", "hAnsiTheme"):
                v = rf.get(qn("w:" + attr))
                if v is not None: value[attr] = v
            # Direct explicit family wins over earlier theme declarations.
            for literal, themed in [("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme")]:
                if rf.get(qn("w:" + literal)) is not None and rf.get(qn("w:" + themed)) is None:
                    value.pop(themed, None)
        size = rpr.find("w:sz", NS)
        if size is not None: value["halfPoints"] = int(size.get(qn("w:val")))
        for tag, key in [("b", "bold"), ("i", "italic")]:
            n = rpr.find("w:" + tag, NS)
            if n is not None:
                active = n.get(qn("w:val"), "1") not in ("0", "false", "off")
                value[key] = (not value.get(key, False)) if style and active else (value.get(key, False) if style else active)
    def font(value):
        value = copy.deepcopy(value)
        for literal, themed in [("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme")]:
            if value.get(themed):value[literal] = theme_fonts.get(value[themed][:5])
        if not value.get("ascii") or value.get("ascii") != value.get("hAnsi") or "halfPoints" not in value:
            return None
        return {"family": value["ascii"], "halfPoints": value["halfPoints"],
                "bold": value.get("bold", False), "italic": value.get("italic", False)}
    result = []
    py_doc = Document(path)
    py_paragraphs = [p for p in py_doc.paragraphs if p._p.find(".//wp:inline", NS) is not None]
    for p, py_p in zip(root.findall("w:body/w:p", NS), py_doc.paragraphs):
        if p.find(".//wp:inline", NS) is None: continue
        ppr = p.find("w:pPr", NS)
        sid = None if ppr is None else ppr.find("w:pStyle", NS)
        sid = default if sid is None else sid.get(qn("w:val"))
        # Default paragraph style is inherited when a named style has no basedOn.
        chain_p = chain(sid)
        if default and default not in [s.get(qn("w:styleId")) for s in chain_p]: chain_p = chain(default) + chain_p
        effective = props(paragraph_font=None)
        apply_p(effective, styles.find("w:docDefaults/w:pPrDefault/w:pPr", NS))
        run = {"bold": False, "italic": False}
        apply_r(run, styles.find("w:docDefaults/w:rPrDefault/w:rPr", NS))
        for style in chain_p:
            apply_p(effective, style.find("w:pPr", NS));apply_r(run, style.find("w:rPr", NS), True)
        paragraph_font = font(run)
        apply_p(effective, ppr)
        drawing_run = next(r for r in p.findall("w:r", NS) if r.find("w:drawing", NS) is not None)
        rpr = drawing_run.find("w:rPr", NS)
        rid = None if rpr is None else rpr.find("w:rStyle", NS)
        if rid is not None:
            for style in chain(rid.get(qn("w:val"))): apply_r(run, style.find("w:rPr", NS), True)
        apply_r(run, rpr)
        run_font = font(run)
        if paragraph_font is not None and run_font is not None:
            effective.update(paragraphFont=paragraph_font, runFont=run_font)
        spacing = None if ppr is None else ppr.find("w:spacing", NS)
        direct = {} if spacing is None else {ET.QName(k).localname: v for k, v in spacing.attrib.items()}
        py_direct = {}
        pf = py_p.paragraph_format
        if pf.space_before is not None: py_direct["before"] = str(pf.space_before.twips)
        if pf.space_after is not None: py_direct["after"] = str(pf.space_after.twips)
        if isinstance(pf.line_spacing, float): py_direct["line"] = str(round(pf.line_spacing * 240))
        for k, v in py_direct.items(): check(direct.get(k) == v, "python-docx direct spacing disagrees")
        result.append({"effective": effective, "direct_spacing": direct, "style_id": sid,
                       "python_docx_direct": py_direct, "python_docx_alignment": None if pf.alignment is None else int(pf.alignment),
                       "paragraph_xml": ET.tostring(p, encoding="unicode"),
                       "style_xml": None if sid is None else ET.tostring(style_map[sid], encoding="unicode")})
    check(len(result) == len(py_paragraphs), "Independent image paragraph sequence differs")
    return result


def make_fixtures(directory):
    raster = Image.new("RGB", (120, 80), "#176eab")
    draw = ImageDraw.Draw(raster);draw.rectangle((0, 0, 119, 79), outline="black", width=3)
    draw.rectangle((8, 8, 45, 35), fill="#e84b45")
    buffer = io.BytesIO();raster.save(buffer, "PNG");blob = buffer.getvalue()
    sources = {}
    def create(name, settings=None, heights=None, mutation=None, reject=None, fallback=False, render=True, package_mutation=None):
        settings = copy.deepcopy(settings or [props()]);heights = heights or [635000] * len(settings)
        doc = w.fresh_document();set_font(doc.styles["Normal"], BASE_FONT)
        before = doc.add_paragraph("BeforeImage")
        # Markers are direct-styled so changing an image-only style does not move them.
        for p in [before]: set_font(p.runs[0], BASE_FONT);set_spacing(p, props())
        expected, image_paragraphs = [], []
        for index, (value, height) in enumerate(zip(settings, heights)):
            paragraph = doc.add_paragraph()
            style = doc.styles.add_style("ImageProbe" + str(index), WD_STYLE_TYPE.PARAGRAPH)
            style.base_style = doc.styles["Normal"]
            set_font(style, value.get("paragraphFont", BASE_FONT));set_spacing(style, props())
            paragraph.style = style;set_spacing(paragraph, value)
            run = paragraph.add_run();set_font(run, value.get("runFont", BASE_FONT))
            shape = run.add_picture(io.BytesIO(blob), width=Emu(1270000 if height != 95250 else 95250), height=Emu(height))
            for side in ["distT", "distB", "distL", "distR"]:shape._inline.set(side, "0")
            expected.append(d.expected_image(blob, "image/png", int(shape.width), height));image_paragraphs.append(paragraph)
            after = doc.add_paragraph("AfterImage" + str(index));set_font(after.runs[0], BASE_FONT);set_spacing(after, props())
        if mutation:mutation(doc, image_paragraphs)
        path = directory / (name + ".docx");check(not path.exists(), "Fixture already exists");doc.save(path)
        if package_mutation:
            with zipfile.ZipFile(path) as z:parts = {n: z.read(n) for n in z.namelist()}
            package_mutation(parts)
            with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
                for n, b in parts.items():z.writestr(n, b)
        item = {"path": str(path), "sha256": digest(path), "bytes": path.stat().st_size,
                "fixture_kind": "Directed synthetic python-docx/Pillow fixture, not an original upstream or user document",
                "expected_action": "reject" if reject else "roundtrip", "unsupported_form": reject,
                "expected": expected, "expected_paragraphs": settings, "render_probe": render and not fallback and not reject,
                "font_fallback": fallback}
        if not reject:
            item["source_independent"] = d.snapshot(path);d.verify_snapshot(item["source_independent"], expected, name)
            item["source_paragraphs"] = independent_paragraphs(path)
            check([p["effective"] for p in item["source_paragraphs"]] == settings, name + ": source differs from predeclared expected attrs")
        sources[name] = item
    create("tall-single")
    create("tiny-single", heights=[95250])
    create("tall-onehalf-after6", [props(after=120, line=360)])
    create("tiny-onehalf-after6", [props(after=120, line=360)], [95250])
    for alignment in ALIGN:
        create("align-" + alignment, [props(before=80, after=160, line=300, align=alignment)], [635000])
    create("repeated-media", [props(before=100, after=80), props(before=60, after=200, line=360, align="right")], [95250, 635000])
    serif = {"family": "DejaVu Serif", "halfPoints": 36, "bold": True, "italic": False}
    mono = {"family": "DejaVu Sans Mono", "halfPoints": 18, "bold": False, "italic": True}
    create("different-style-run-fonts", [props(paragraph_font=serif, run_font=mono)], [95250])
    create("font-size-strut", [props(paragraph_font={**BASE_FONT, "halfPoints": 48})], [95250])
    def cascade(doc, paragraphs):
        p = paragraphs[0];base = doc.styles.add_style("ImageParent", WD_STYLE_TYPE.PARAGRAPH)
        base.base_style = doc.styles["Normal"];set_spacing(base, props(before=140, after=220, line=360, align="center"))
        set_font(base, {**BASE_FONT, "family": "DejaVu Serif", "halfPoints": 32})
        p.style.base_style = base
        rp = p.style.element.find("w:rPr", NS);p.style.element.remove(rp)
        pp = p.style.element.find("w:pPr", NS);remove(pp, "spacing");remove(pp, "jc")
        ppr = p._p.get_or_add_pPr();remove(ppr, "spacing");remove(ppr, "jc")
        p.paragraph_format.space_after = Twips(0)
        p.paragraph_format.alignment = ALIGN["right"]
        run = p.runs[0];rpr = run._r.get_or_add_rPr()
        for n in list(rpr):rpr.remove(n)
        charbase = doc.styles.add_style("ImageCharacterParent", WD_STYLE_TYPE.CHARACTER)
        set_font(charbase, {**BASE_FONT, "family": "DejaVu Sans Mono", "halfPoints": 20, "italic": True})
        char = doc.styles.add_style("ImageCharacterChild", WD_STYLE_TYPE.CHARACTER);char.base_style = charbase
        run.style = char;run.font.size = Pt(14)
    create("style-cascade-partial-zero", [props(before=140, after=0, line=360, align="right",
           paragraph_font={**BASE_FONT, "family": "DejaVu Serif", "halfPoints": 32},
           run_font={**BASE_FONT, "family": "DejaVu Sans Mono", "halfPoints": 28, "italic": True})], [95250], mutation=cascade)
    def defaults_only(doc, paragraphs):
        p = paragraphs[0]
        for s in [doc.styles["Normal"], p.style]:
            pp = s.element.find("w:pPr", NS)
            if pp is not None:remove(pp, "spacing");remove(pp, "jc")
        pp = p._p.get_or_add_pPr();remove(pp, "spacing");remove(pp, "jc")
        pp = doc.styles.element.find("w:docDefaults/w:pPrDefault/w:pPr", NS)
        remove(pp, "spacing");child(pp, "spacing", before=60, after=100, line=288, lineRule="auto");child(pp, "jc", val="center")
    create("document-default-spacing", [props(before=60, after=100, line=288, align="center")], [95250], mutation=defaults_only)
    def mark_same(doc, paragraphs):
        rp = child(paragraphs[0]._p.get_or_add_pPr(), "rPr")
        child(rp, "rFonts", ascii="DejaVu Sans", hAnsi="DejaVu Sans");child(rp, "sz", val=24)
    create("equal-paragraph-mark-font", mutation=mark_same, heights=[95250])
    def theme_only(doc, paragraphs):
        for s in [doc.styles["Normal"], paragraphs[0].style]:
            rp = s.element.find("w:rPr", NS)
            if rp is not None:remove(rp, "rFonts")
        remove(paragraphs[0].runs[0]._r.get_or_add_rPr(), "rFonts")
    create("resolved-theme-font", [props(after=120, line=360, paragraph_font={**BASE_FONT, "family": "Cambria"})], [95250], mutation=theme_only, render=False)
    def missing_family(doc, paragraphs):
        theme_only(doc, paragraphs)
        rp = doc.styles.element.find("w:docDefaults/w:rPrDefault/w:rPr", NS)
        remove(rp, "rFonts")
    create("missing-family-fallback", [props(after=120, line=360, paragraph_font=None)], [95250], mutation=missing_family, fallback=True)
    def no_font_size(doc, paragraphs):
        for rp in [doc.styles.element.find("w:docDefaults/w:rPrDefault/w:rPr", NS),
                   doc.styles["Normal"].element.find("w:rPr", NS), paragraphs[0].style.element.find("w:rPr", NS),
                   paragraphs[0].runs[0]._r.get_or_add_rPr()]:
            for tag in ["sz", "szCs"]:remove(rp, tag)
    create("missing-size-fallback", [props(before=100, paragraph_font=None)], [95250], mutation=no_font_size, fallback=True)
    def mutate_property(tag, attrs, inherited=False, run=False, mark=False):
        def fn(doc, paragraphs):
            p = paragraphs[0]
            parent = p.runs[0]._r.get_or_add_rPr() if run else p._p.get_or_add_pPr()
            if inherited:parent = p.style.element.get_or_add_rPr() if run else p.style.element.get_or_add_pPr()
            if mark:parent = child(p._p.get_or_add_pPr(), "rPr")
            remove(parent, tag);child(parent, tag, **attrs)
        return fn
    refusals = [
        ("exact-line", "spacing", {"line": "240", "lineRule": "exact"}, False),
        ("atleast-line", "spacing", {"line": "240", "lineRule": "atLeast"}, False),
        ("before-auto", "spacing", {"beforeAutospacing": "1"}, False),
        ("after-lines", "spacing", {"afterLines": "100"}, False),
        ("negative-before", "spacing", {"before": "-1"}, False),
        ("large-after", "spacing", {"after": "31681"}, False),
        ("small-line", "spacing", {"line": "119", "lineRule": "auto"}, False),
        ("large-line", "spacing", {"line": "2401", "lineRule": "auto"}, False),
        ("contextual-spacing", "contextualSpacing", {"val": "1"}, False),
        ("right-to-left", "bidi", {"val": "1"}, False),
        ("indent", "ind", {"left": "200"}, False),
        ("distributed", "jc", {"val": "distribute"}, False),
        ("textalignment-top", "textAlignment", {"val": "top"}, False),
        ("run-position", "position", {"val": "2"}, True),
        ("run-character-spacing", "spacing", {"val": "20"}, True),
        ("run-kern", "kern", {"val": "16"}, True),
        ("run-rtl", "rtl", {"val": "1"}, True),
    ]
    for name, tag, attrs, run in refusals:
        for inherited in [False, True]:
            normalized = name in ("indent", "run-kern")
            create(("normalize-" if normalized else "reject-") + ("style-" if inherited else "direct-") + name,
                   mutation=mutate_property(tag, attrs, inherited, run), reject=None if normalized else name, render=False)
    create("reject-mark-font-size-differs", mutation=mutate_property("sz", {"val": "40"}, mark=True), reject="paragraph mark font differs from paragraph-style strut")
    create("reject-mark-font-family-differs", mutation=mutate_property("rFonts", {"ascii": "DejaVu Serif", "hAnsi": "DejaVu Serif"}, mark=True), reject="paragraph mark family differs from paragraph-style strut")
    def xml_mutation(part, modify):
        def change(parts):
            root = ET.fromstring(parts[part]);modify(root);parts[part] = ET.tostring(root, xml_declaration=True, encoding="UTF-8", standalone=True)
        return change
    def image_p(root): return next(p for p in root.findall("w:body/w:p", NS) if p.find(".//wp:inline", NS) is not None)
    def wrong_namespace(root):
        pp = image_p(root).find("w:pPr", NS);s = pp.find("w:spacing", NS)
        s.set("{urn:wrong}before", "100")
    create("reject-spacing-namespace-attribute", package_mutation=xml_mutation("word/document.xml", wrong_namespace), reject="foreign spacing attribute")
    def wrong_element(root):
        pp = image_p(root).find("w:pPr", NS);s = pp.find("w:spacing", NS);s.tag = "{urn:wrong}spacing"
    create("reject-spacing-namespace-element", package_mutation=xml_mutation("word/document.xml", wrong_element), reject="foreign spacing element")
    def duplicate(root):
        pp = image_p(root).find("w:pPr", NS);pp.append(copy.deepcopy(pp.find("w:spacing", NS)))
    create("reject-duplicate-spacing", package_mutation=xml_mutation("word/document.xml", duplicate), reject="duplicate spacing declarations")
    def wrong_font(root):
        rp = image_p(root).find("w:r/w:rPr", NS);rp.find("w:rFonts", NS).set("{urn:wrong}ascii", "DejaVu Sans")
    create("reject-font-namespace-attribute", package_mutation=xml_mutation("word/document.xml", wrong_font), reject="foreign font attribute")
    (directory / "synthetic-manifest.json").write_text(json.dumps(sources, indent=2), encoding="utf-8")
    return sources


def verify_paragraphs(path, expected, label, explicit=False):
    actual = independent_paragraphs(path)
    check(len(actual) == len(expected), label + ": independent paragraph count differs")
    for got, want in zip(actual, expected):
        value = copy.deepcopy(got["effective"])
        if "paragraphFont" not in want:
            value.pop("paragraphFont", None);value.pop("runFont", None)
        check(value == want, label + ": independently resolved image paragraph attrs differ")
    if explicit:
        for row, want in zip(actual, expected):
            check(row["direct_spacing"] == {"before": str(want["spaceBefore"]), "after": str(want["spaceAfter"]),
                  "line": str(want["lineSpacing"]), "lineRule": "auto"}, label + ": export spacing is not complete/explicit")
            check(row["python_docx_alignment"] == int(ALIGN[want["textAlign"]]), label + ": explicit alignment differs")
            if "paragraphFont" in want:
                check(row["style_id"] and row["style_id"] != "Normal", label + ": paragraph strut must use dedicated style")
    return actual


def export_verify(cli, evidence, directory, name, model, expected, paragraphs):
    model_path = directory / (name + ".json");w.write_model(model_path, model)
    output = directory / (name + ".docx");run_cli(cli, evidence["commands"], "create-docx", model_path, output)
    independent = d.snapshot(output);d.verify_snapshot(independent, expected, name)
    rows = verify_paragraphs(output, paragraphs, name, explicit=True)
    reopened = run_cli(cli, evidence["commands"], "read-docx-model", output)
    check(reopened == model, name + ": trusted full-model reopen differs")
    imported = run_cli(cli, evidence["commands"], "import-docx", output)["document"]
    d.verify_model(imported, expected, name + " external reimport")
    check([n["attrs"].get(KEY) for n in d.images(imported)] == [row["effective"] for row in rows], name + ": external paragraph reimport differs from actual exported cascade")
    sha = digest(output)
    error = run_cli(cli, evidence["commands"], "create-docx", model_path, output, success=False)
    check("Output already exists" in error and digest(output) == sha, name + ": output no-clobber failed")
    return {"path": str(output), "model_path": str(model_path), "sha256": sha, "independent": independent,
            "paragraphs": rows, "trusted_model_exact_equal": True, "external_reimport_equal": True, "output_no_clobber": True}


def marker(render, name):
    hits = [x for x in render["texts"] if x["text"] == name]
    check(len(hits) == 1, "PDF marker missing/nonunique: " + name)
    return hits[0]


def compare_pdf(source, exported, label):
    check(source["page_count"] == exported["page_count"] == 1, label + ": controlled one-page probe repaginated")
    deltas = []
    for index, (before, after) in enumerate(zip(source["images"], exported["images"])):
        delta = {key: after[key] - before[key] for key in ["x_pt", "y_pt", "width_pt", "height_pt"]}
        src_text, out_text = marker(source, "AfterImage" + str(index)), marker(exported, "AfterImage" + str(index))
        delta["following_baseline_pt"] = out_text["y_pt"] - src_text["y_pt"]
        delta["image_to_following_baseline_pt"] = (after["y_pt"] - out_text["y_pt"]) - (before["y_pt"] - src_text["y_pt"])
        check(all(abs(v) <= TOLERANCE for v in delta.values()), label + ": image placement/following baseline differs: " + str(delta))
        deltas.append(delta)
    return {"tolerance_pt": TOLERANCE, "images": deltas, "controlled_A4_72pt_margins_only": True}


def raster_preview(directory, pdfs):
    outputs = []
    for pdf in pdfs:
        prefix = directory / (Path(pdf).stem + "-page1")
        subprocess.run(["pdftoppm", "-f", "1", "-singlefile", "-scale-to", "1100", "-png", str(pdf), str(prefix)], capture_output=True, text=True, check=True, timeout=90)
        outputs.append(prefix.with_suffix(".png"))
    # A source/output/edit contact sheet makes vertical gaps and alignment easy to inspect.
    thumbs = []
    for path in outputs:
        im = Image.open(path).convert("RGB");im.thumbnail((390, 555))
        tile = Image.new("RGB", (410, 590), "#eeeeee");tile.paste(im, ((410 - im.width) // 2, 28))
        ImageDraw.Draw(tile).text((8, 6), path.stem.replace("-page1", "")[:58], fill="black");thumbs.append(tile)
    columns = 3;rows = (len(thumbs) + columns - 1) // columns
    contact = Image.new("RGB", (410 * columns, 590 * rows), "white")
    for index, tile in enumerate(thumbs):contact.paste(tile, ((index % columns) * 410, (index // columns) * 590))
    path = directory / "rendered-contact-sheet.png";contact.save(path)
    return {"pages": [str(x) for x in outputs], "contact_sheet": str(path)}


def run_supported(name, item, cli, evidence, directory, soffice):
    imported = run_cli(cli, evidence["commands"], "import-docx", item["path"])
    model = imported["document"];item["compatibility"] = imported["compatibility"]
    check(WARNING in item["compatibility"]["risks"], name + ": missing bounded layout warning")
    if item.get("font_fallback"):
        check(any("font" in r.lower() and ("metric" in r.lower() or "baseline" in r.lower()) for r in item["compatibility"]["risks"]), name + ": missing unresolved metric warning")
    d.verify_model(model, item["expected"], name)
    check([n["attrs"].get(KEY) for n in d.images(model)] == item["expected_paragraphs"], name + ": exact imported metadata differs")
    item["unchanged_export"] = export_verify(cli, evidence, directory, name + "-unchanged", model, item["expected"], item["expected_paragraphs"])
    edited = copy.deepcopy(model);edited_paragraphs = copy.deepcopy(item["expected_paragraphs"])
    for node, value in zip(d.images(edited), edited_paragraphs):
        value["spaceAfter"] += 240
        value["textAlign"] = "center" if value["textAlign"] != "center" else "right"
        node["attrs"][KEY] = value
    item["edited_expected_paragraphs"] = edited_paragraphs
    item["edited_export"] = export_verify(cli, evidence, directory, name + "-edited", edited, item["expected"], edited_paragraphs)
    check(digest(Path(item["path"])) == item["sha256"], name + ": source changed")
    error = run_cli(cli, evidence["commands"], "create-docx", item["unchanged_export"]["model_path"], item["path"], success=False)
    check("Output already exists" in error and digest(Path(item["path"])) == item["sha256"], name + ": source overwrite not refused")
    item["source_no_clobber"] = True
    if soffice and item.get("render_probe"):
        for key, path in [("source_render", item["path"]), ("unchanged_render", item["unchanged_export"]["path"]), ("edited_render", item["edited_export"]["path"])]:
            item[key] = d.render_pdf(soffice, directory, Path(path), item["expected"])
        item["placement_comparison"] = compare_pdf(item["source_render"], item["unchanged_render"], name)
        changes = []
        for index, (old, new) in enumerate(zip(item["unchanged_render"]["images"], item["edited_render"]["images"])):
            want = edited_paragraphs[index];width = old["page_width_pt"] - 144
            expected_x = 72 + (width - old["width_pt"]) / (2 if want["textAlign"] == "center" else 1)
            check(abs(new["x_pt"] - expected_x) <= TOLERANCE, name + ": alignment edit does not reach expected x")
            baseline_delta = marker(item["edited_render"], "AfterImage" + str(index))["y_pt"] - marker(item["unchanged_render"], "AfterImage" + str(index))["y_pt"]
            check(abs(baseline_delta + 12 * (index + 1)) <= TOLERANCE, name + ": after-spacing edit did not move baseline by 12 pt/image")
            check(abs(new["y_pt"] - old["y_pt"] + 12 * index) <= TOLERANCE, name + ": after edit unexpectedly changed own image baseline")
            changes.append({"image_x_pt": new["x_pt"], "expected_x_pt": expected_x, "following_baseline_delta_pt": baseline_delta})
        item["edited_pdf_changes"] = changes
    return model


def cross_source_edits(cli, evidence, directory, soffice):
    """Compare edits with independently authored sources having those settings."""
    original = evidence["sources"]["tiny-single"]
    base = json.loads(Path(original["unchanged_export"]["model_path"]).read_text())
    result = {}
    for name, target_name in [("font-size-edit", "font-size-strut"), ("line-spacing-edit", "tiny-onehalf-after6")]:
        target = evidence["sources"][target_name]
        edited = copy.deepcopy(base)
        next(d.images(edited))["attrs"][KEY] = copy.deepcopy(target["expected_paragraphs"][0])
        entry = export_verify(cli, evidence, directory, name, edited, original["expected"], target["expected_paragraphs"])
        if soffice:
            entry["render"] = d.render_pdf(soffice, directory, Path(entry["path"]), original["expected"])
            entry["independent_source_reference_comparison"] = compare_pdf(target["source_render"], entry["render"], name)
            delta = marker(entry["render"], "AfterImage0")["y_pt"] - marker(original["source_render"], "AfterImage0")["y_pt"]
            check(abs(delta) > 2, name + ": edit has no measurable baseline effect")
            entry["following_baseline_delta_vs_unedited_pt"] = delta
        result[name] = entry
    evidence["cross_source_edit_probes"] = result


def invalid_models(cli, evidence, directory, seed):
    cases = {}
    base = next(d.images(seed))["attrs"][KEY]
    for value in [None, [], 0, True, "default"]:cases["whole-" + repr(value)] = value
    for key in ["spaceBefore", "spaceAfter", "lineSpacing", "textAlign", "paragraphFont", "runFont"]:
        v = copy.deepcopy(base);del v[key];cases["missing-" + key] = v
        v = copy.deepcopy(base);v[key] = None;cases["null-" + key] = v
    for key, values in [("spaceBefore", [-1, 31681, 1.5, "0", True, [], {}]),
                        ("spaceAfter", [-1, 31681, 1.5, "0", False]),
                        ("lineSpacing", [0, 119, 2401, 240.5, "240", True]),
                        ("textAlign", ["both", "start", "distribute", "LEFT", 0, False])]:
        for index, value in enumerate(values):
            v = copy.deepcopy(base);v[key] = value;cases[key + "-" + str(index)] = v
    v = copy.deepcopy(base);v["unknown"] = 1;cases["unknown-paragraph-field"] = v
    for role in ["paragraphFont", "runFont"]:
        for key, values in [("family", [None, "", " DejaVu Sans", "x" * 32, 12]),
                            ("halfPoints", [None, 0, 1, 3277, 2.5, "24", True]),
                            ("bold", [None, 0, "false"]), ("italic", [None, 1, "true"])]:
            v = copy.deepcopy(base);del v[role][key];cases[role + "-missing-" + key] = v
            for index, value in enumerate(values):
                v = copy.deepcopy(base);v[role][key] = value;cases[role + "-" + key + "-" + str(index)] = v
        v = copy.deepcopy(base);v[role]["unknown"] = 1;cases[role + "-unknown"] = v
    for name, value in cases.items():
        model = copy.deepcopy(seed);next(d.images(model))["attrs"][KEY] = value
        path = directory / ("invalid-" + name.replace("'", "") + ".json");w.write_model(path, model)
        out = path.with_suffix(".docx")
        error = run_cli(cli, evidence["commands"], "create-docx", path, out, success=False)
        check(not out.exists(), name + ": invalid model published output")
        evidence["invalid_models"][name] = error
    # Omitting the whole optional object is valid and preserves old authored bytes.
    legacy = copy.deepcopy(seed)
    for node in d.images(legacy):node["attrs"].pop(KEY)
    legacy_path = directory / "absent-metadata.json";w.write_model(legacy_path, legacy)
    outputs = [directory / ("absent-metadata-" + suffix + ".docx") for suffix in ["first", "second"]]
    for path in outputs:
        run_cli(cli, evidence["commands"], "create-docx", legacy_path, path)
        check(run_cli(cli, evidence["commands"], "read-docx-model", path) == legacy, "Absent metadata is not canonical on reopen")
    check(digest(outputs[0]) == digest(outputs[1]), "Absent metadata encoding is nondeterministic")
    evidence["absent_metadata"] = {"deterministic_bytes": True, "trusted_model_omits_object": True,
                                   "sha256": digest(outputs[0]), "historical_bytes_claim": "Existing Rust golden/profile tests own historical byte-identity proof"}


def valid_model_boundaries(cli, evidence, directory, seed):
    expected = evidence["sources"]["tall-single"]["expected"]
    result = {}
    for name, value in [
        ("minimums", props(line=120, paragraph_font={**BASE_FONT, "halfPoints": 2})),
        ("maximums", props(before=31680, after=31680, line=2400, paragraph_font={**BASE_FONT, "halfPoints": 3276})),
    ]:
        model = copy.deepcopy(seed);next(d.images(model))["attrs"][KEY] = value
        result[name] = export_verify(cli, evidence, directory, "valid-boundary-" + name, model, expected, [value])
    evidence["valid_boundary_models"] = result


def verify_historical_bytes(old_directory, cli, evidence, directory):
    """Use independently pinned prior run outputs as whole-package byte goldens."""
    prior_report = old_directory / "report.json"
    prior = json.loads(prior_report.read_text())
    results = {}
    for name, item in prior["sources"].items():
        old = item.get("unchanged_export")
        if old is None:continue
        path = Path(old["path"]);sha = digest(path)
        check(sha == old["sha256"], "Historical DOCX differs from pinned report: " + name)
        with zipfile.ZipFile(path) as z:
            embedded = json.loads(z.read("rust-wps/model-v2.json"))["document"]
        model = json.loads(Path(old["model_path"]).read_text())
        check(embedded == model, "Historical embedded model does not match source model: " + name)
        check(all(KEY not in n["attrs"] for n in d.images(model)), "Historical golden contains new metadata")
        model_path = directory / ("historical-" + name + ".json");w.write_model(model_path, model)
        output = model_path.with_suffix(".docx")
        run_cli(cli, evidence["commands"], "create-docx", model_path, output)
        check(digest(output) == sha, "Absent metadata changed historical whole-DOCX bytes: " + name)
        check(digest(path) == sha, "Historical golden was changed")
        results[name] = {"historical_path": str(path), "new_path": str(output), "identical_sha256": sha}
    check(results, "Historical evidence contains no absent-metadata goldens")
    evidence["historical_byte_identity"] = {"prior_report": str(prior_report), "prior_report_sha256": digest(prior_report), "cases": results}


def original_corpus(args, cli, evidence, directory, soffice):
    manifest = json.loads(Path(__file__).with_name("image-dimensions-corpus.json").read_text())
    for pinned in manifest["fixtures"]:
        path = (args.corpus_dir / pinned["name"]).resolve();blob = path.read_bytes()
        check(len(blob) == pinned["bytes"] and digest(path) == pinned["sha256"], "Original source hash mismatch")
        check(hashlib.sha1(b"blob " + str(len(blob)).encode() + b"\0" + blob).hexdigest() == pinned["git_blob_sha"], "Original source Git hash mismatch")
        item = {**pinned, "path": str(path), "expected": pinned["images"],
                "expected_paragraphs": [props(after=200, line=276, paragraph_font={**BASE_FONT, "family": "Calibri", "halfPoints": 22})],
                "font_fallback": False, "render_probe": False,
                "scope": "Unmodified upstream positive for spacing/alignment/dimensions/media/trusted model. Theme Latin family resolves to Calibri; full page/baseline fidelity is not claimed."}
        evidence["original_corpus"][pinned["name"]] = item
        item["source_paragraphs"] = verify_paragraphs(path, item["expected_paragraphs"], pinned["name"])
        run_supported("original-" + path.stem, item, cli, evidence, directory, None)
        if soffice:
            for key, docx in [("source_render", path), ("unchanged_render", Path(item["unchanged_export"]["path"]))]:
                item[key] = d.render_pdf(soffice, directory, docx, item["expected"])
            item["consumer_difference_measured_not_claimed_preserved"] = {
                k: item["unchanged_render"]["images"][0][k] - item["source_render"]["images"][0][k]
                for k in ["x_pt", "y_pt", "width_pt", "height_pt"]}
        check(digest(path) == pinned["sha256"], "Original upstream changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=Path("target/debug/rust-wps"))
    parser.add_argument("--out", type=Path, default=Path("tmp/word-image-paragraphs"))
    parser.add_argument("--corpus-dir", type=Path)
    parser.add_argument("--legacy-evidence-dir", type=Path)
    parser.add_argument("--generate-only", action="store_true")
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--soffice")
    args = parser.parse_args();args.out.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="run-", dir=args.out)).resolve();cli = args.cli.resolve()
    soffice = (args.soffice or shutil.which("soffice")) if args.render else None
    check(not args.render or soffice, "LibreOffice is required for --render")
    paths = [Path(__file__).resolve(), Path(__file__).with_name("check-word-image-dimensions.py").resolve(),
             Path(__file__).with_name("check-word-table-widths.py").resolve(), Path(__file__).with_name("image-dimensions-corpus.json").resolve()]
    if not args.generate_only:paths.append(cli)
    initial = {str(p): digest(p) for p in paths}
    evidence = {"status": "running", "python": platform.python_version(), "dependencies": {n: version(n) for n in ["python-docx", "lxml", "Pillow", "pypdf"]},
                "commands": [], "invalid_models": {}, "original_corpus": {}, "tool_hashes": initial,
                "limitations": "Exact position claims cover controlled single-page A4/72pt-margin documents, supported image-only spacing/alignment and explicit portable font metrics in LibreOffice. No native app/browser/Word/WPS or arbitrary-page fidelity claim."}
    try:
        evidence["sources"] = make_fixtures(directory)
        if soffice:evidence["libreoffice_version"] = subprocess.run([soffice, "--version"], capture_output=True, text=True, check=True, timeout=30).stdout.strip()
        if not args.generate_only:
            seed = None
            for name, item in evidence["sources"].items():
                if item["expected_action"] == "reject":
                    item["audit"] = run_cli(cli, evidence["commands"], "audit-docx", item["path"])
                    check(any("Unsupported" in r and ("image" in r or "spacing" in r) for r in item["audit"]["risks"]), name + ": missing explicit refusal diagnostic")
                    item["import_error"] = run_cli(cli, evidence["commands"], "import-docx", item["path"], success=False)
                else:
                    model = run_supported(name, item, cli, evidence, directory, soffice)
                    if seed is None:seed = model
                check(digest(Path(item["path"])) == item["sha256"], name + ": source changed")
            cross_source_edits(cli, evidence, directory, soffice)
            invalid_models(cli, evidence, directory, seed)
            valid_model_boundaries(cli, evidence, directory, seed)
            if args.legacy_evidence_dir:verify_historical_bytes(args.legacy_evidence_dir.resolve(), cli, evidence, directory)
            if args.corpus_dir:original_corpus(args, cli, evidence, directory, soffice)
            if soffice:
                picked = []
                for name in ["tiny-single", "tall-onehalf-after6", "different-style-run-fonts"]:
                    picked += [evidence["sources"][name][k]["path"] for k in ["source_render", "unchanged_render", "edited_render"]]
                evidence["visual_inspection_artifacts"] = raster_preview(directory, picked)
        check(all(digest(Path(p)) == sha for p, sha in initial.items()), "CLI or validation inputs changed during run")
        evidence["summary"] = {"supported_synthetic": sum(x["expected_action"] == "roundtrip" for x in evidence["sources"].values()),
                               "refused_synthetic": sum(x["expected_action"] == "reject" for x in evidence["sources"].values()),
                               "invalid_models": len(evidence["invalid_models"]), "valid_boundary_models": len(evidence.get("valid_boundary_models", {})),
                               "historical_byte_identical": len(evidence.get("historical_byte_identity", {}).get("cases", {})),
                               "original_positive": len(evidence["original_corpus"]),
                               "pdfs": len(list((directory / "libreoffice-pdf").glob("*.pdf")))}
        evidence["status"] = "generated" if args.generate_only else "passed"
    except Exception as exc:
        evidence["status"] = "failed";evidence["error"] = str(exc);raise
    finally:
        (directory / "report.json").write_text(json.dumps(evidence, indent=2, ensure_ascii=False), encoding="utf-8")
        print(directory / "report.json", flush=True)
    print(json.dumps(evidence["summary"]))


if __name__ == "__main__":main()
