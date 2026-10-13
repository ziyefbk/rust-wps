"""Independent bounded page-geometry DOCX regression.

python-docx source authoring is separate from the preset expected JSON manifest.
OOXML/python-docx readers, full trusted-model reopen, no-clobber/atomicity probes,
forged-package tests and LibreOffice/PDF coordinates inspect the actual output.
Untouched, hash-pinned upstream files remain separate from generated fixtures.
LibreOffice proves its own controlled external-consumer behavior, never the app,
Windows/WebView2, Microsoft Word/WPS or arbitrary pagination fidelity.

  python scripts/check-word-page-geometry.py --generate-only
  python scripts/check-word-page-geometry.py --cli target/debug/rust-wps --render
"""
import argparse
import copy
import hashlib
import importlib.util
import io
import json
import math
import os
import platform
import shutil
import subprocess
import tempfile
import zipfile
from importlib.metadata import version
from pathlib import Path

from docx import Document
from docx.enum.section import WD_ORIENT
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.shared import Emu, Twips
from lxml import etree as ET
from PIL import Image, ImageDraw
from pypdf import PdfReader

_spec = importlib.util.spec_from_file_location("paragraph_checks", Path(__file__).with_name("check-word-image-paragraphs.py"))
p = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(p)
w, d, NS, qn = p.w, p.d, p.NS, p.qn
check, digest, run_cli = w.check, w.digest, w.run_cli
ROOT = Path(__file__).resolve().parent.parent
EXPECTED = ROOT / "tests/fixtures/word-page-geometry-expected.json"
LEGACY = ROOT / "tests/fixtures/word-page-geometry-legacy.json"
CORPUS = Path(__file__).with_name("page-geometry-corpus.json")
KEY = "pageGeometry"
KEYS = ("widthTwips", "heightTwips", "marginTopTwips", "marginRightTwips", "marginBottomTwips", "marginLeftTwips")
TOLERANCE = 0.35


def geometry(values):
    return dict(zip(KEYS, values))


def inventory(path):
    """Independent XML/OPC inventory, cross-checked with python-docx section API."""
    with zipfile.ZipFile(path) as z:
        root = ET.fromstring(z.read("word/document.xml"))
        sections = root.findall(".//w:sectPr", NS)
        body = root.find("w:body", NS)
        rows = []
        for section in sections:
            size, margin = section.find("w:pgSz", NS), section.find("w:pgMar", NS)
            row = {"terminal_body_section": section.getparent() is body and body[-1] is section,
                   "size_attributes": w.declaration(size), "margin_attributes": w.declaration(margin),
                   "column_attributes": w.declaration(section.find("w:cols", NS)),
                   "grid_attributes": w.declaration(section.find("w:docGrid", NS)),
                   "children": [ET.QName(x).localname for x in section]}
            try:
                row[KEY] = geometry([int(size.get(qn("w:w"))), int(size.get(qn("w:h"))),
                                    *[int(margin.get(qn("w:" + edge))) for edge in ("top", "right", "bottom", "left")]])
            except (TypeError, ValueError, AttributeError):
                row[KEY] = None
            rows.append(row)
        # Actual relationships, not conventional part names, determine side stories.
        rels = ET.fromstring(z.read("word/_rels/document.xml.rels"))
        story_parts = [x.get("Target") for x in rels if x.get("Type", "").rsplit("/", 1)[-1] in ("header", "footer")]
        media = {n: hashlib.sha256(z.read(n)).hexdigest() for n in z.namelist() if n.startswith("word/media/")}
    py = []
    try:
        for s in Document(path).sections:
            values = [s.page_width, s.page_height, s.top_margin, s.right_margin, s.bottom_margin, s.left_margin]
            py.append(None if any(x is None for x in values) else geometry([x.twips for x in values]))
    except (ValueError, TypeError, AttributeError):
        py = ["unsupported declarations; see XML inventory"]
    return {"section_count": len(rows), "sections": rows, "python_docx": py,
            "actual_header_footer_parts": story_parts, "media_sha256": media}


def verify_geometry(path, expected, label):
    got = inventory(path)
    check(got["section_count"] == 1 and got["sections"][0]["terminal_body_section"], label + ": not one terminal body section")
    check(got["sections"][0][KEY] == expected, label + ": independent XML geometry differs")
    check(got["python_docx"] == [expected], label + ": python-docx geometry differs")
    check(not got["actual_header_footer_parts"], label + ": unexpected side story")
    return got


def set_geometry(doc, values):
    s = doc.sections[0]
    s.orientation = WD_ORIENT.LANDSCAPE if values[0] > values[1] else WD_ORIENT.PORTRAIT
    s.page_width, s.page_height = map(Twips, values[:2])
    s.top_margin, s.right_margin, s.bottom_margin, s.left_margin = map(Twips, values[2:])
    return s._sectPr


def build_source(values, rich=True):
    doc = w.fresh_document()
    p.set_font(doc.styles["Normal"], p.BASE_FONT)
    set_geometry(doc, values)
    # All source font/paragraph metrics are deliberate. No importer-generated
    # content, model or output is used to construct this independent reference.
    for text, align in [("LeftText", "left"), ("CenterText", "center"), ("RightText", "right")]:
        para = doc.add_paragraph(text);p.set_spacing(para, p.props(align=align));p.set_font(para.runs[0], p.BASE_FONT)
    if not rich:
        return doc
    raster = Image.new("RGB", (90, 30), "#2477ad")
    ImageDraw.Draw(raster).rectangle((4, 4, 28, 25), fill="#e96048")
    buffer = io.BytesIO();raster.save(buffer, "PNG")
    for align in ["left", "center", "right"]:
        para = doc.add_paragraph();p.set_spacing(para, p.props(align=align))
        run = para.add_run();p.set_font(run, p.BASE_FONT)
        shape = run.add_picture(io.BytesIO(buffer.getvalue()), width=Emu(914400), height=Emu(304800))
        for side in ["distT", "distB", "distL", "distR"]:shape._inline.set(side, "0")
    for align in ["left", "center", "right"]:
        table = w.add_table(doc, [1440, 1440], rows=1)
        w.child(table._tbl.tblPr, "jc", val=align)
        for edge in ["left", "right"]:
            margins = table._tbl.tblPr.find("w:tblCellMar", NS)
            if margins is None:margins = w.child(table._tbl.tblPr, "tblCellMar")
            w.child(margins, edge, w=108, type="dxa")
        for index, cell in enumerate(table.rows[0].cells):
            cell.text = "T" + align.title() + str(index)
            para = cell.paragraphs[0];p.set_spacing(para, p.props());p.set_font(para.runs[0], p.BASE_FONT)
        # A fixed empty separator keeps table order distinct in all consumers.
        para = doc.add_paragraph("After" + align.title());p.set_spacing(para, p.props());p.set_font(para.runs[0], p.BASE_FONT)
    return doc


def make_fixtures(directory, expected):
    # Intentionally independent declarations rather than deriving source values
    # from the expected-geometry file.
    definitions = {
        "a4-portrait": [11906, 16838, 1440, 1440, 1440, 1440],
        "a4-landscape": [16838, 11906, 720, 1080, 1440, 1800],
        "letter-portrait": [12240, 15840, 1080, 720, 1800, 1440],
        "letter-landscape": [15840, 12240, 900, 1260, 1620, 1980],
        "a5-portrait": [8391, 11906, 720, 720, 1080, 1440],
        "a5-landscape": [11906, 8391, 720, 1080, 1440, 1800],
        "a4-tolerance-plus": [11908, 16840, 1440, 1440, 1440, 1440],
        "a4-tolerance-minus": [11904, 16836, 1440, 1440, 1440, 1440],
        "letter-asymmetric": [12240, 15840, 1260, 1800, 1080, 720],
        "a4-zero-margins": [11906, 16838, 0, 0, 0, 0],
        "a4-max-margins": [11906, 16838, 4320, 4320, 4320, 4320],
        "a5-min-content": [8391, 11906, 4320, 2631, 4320, 4320],
    }
    sources = {}
    def save(name, doc, action="roundtrip", mutation=None, render=False, rich=False, reason=None):
        path = directory / (name + ".docx")
        check(not path.exists(), "Source already exists")
        doc.save(path)
        if mutation:
            with zipfile.ZipFile(path) as z:parts = {n: z.read(n) for n in z.namelist()}
            root = ET.fromstring(parts["word/document.xml"])
            settings = ET.fromstring(parts["word/settings.xml"])
            mutation(root, settings, parts)
            parts["word/document.xml"] = ET.tostring(root, xml_declaration=True, encoding="UTF-8", standalone=True)
            parts["word/settings.xml"] = ET.tostring(settings, xml_declaration=True, encoding="UTF-8", standalone=True)
            with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
                for n, b in parts.items():z.writestr(n, b)
        item = {"path": str(path), "sha256": digest(path), "fixture_kind": "directed synthetic python-docx source",
                "expected_action": action, "reason": reason, "render_probe": render, "rich": rich,
                "source_independent": inventory(path)}
        if action == "roundtrip":item["expected_geometry"] = expected["expected"][name];verify_geometry(path, item["expected_geometry"], name)
        sources[name] = item
    for name, values in definitions.items():
        rich = name not in ["a4-zero-margins", "a4-max-margins", "a5-min-content"]
        save(name, build_source(values, rich), render=rich, rich=rich)
    base = [11906, 16838, 1440, 1440, 1440, 1440]
    def sect(root):return root.find("w:body/w:sectPr", NS)
    def neutral_section(root, settings, parts):
        s = sect(root)
        for name, attrs in [("type", {"val": "nextPage"}), ("bidi", {"val": "0"}), ("rtlGutter", {"val": "false"}),
                            ("textDirection", {"val": "lrTb"}), ("vAlign", {"val": "top"}), ("formProt", {"val": "0"})]:w.child(s, name, **attrs)
        w.child(s, "docGrid", type="default", linePitch="360")
    save("neutral-section-options", build_source(base, False), mutation=neutral_section)
    def neutral_settings(root, settings, parts):
        for tag in ["mirrorMargins", "gutterAtTop", "bookFoldPrinting", "bookFoldRevPrinting"]:w.child(settings, tag, val="0")
    save("neutral-settings-options", build_source(base, False), mutation=neutral_settings)
    save("paper-code-metadata", build_source([11907,16840,1440,1440,1440,1440], False),
         mutation=lambda r,s,p:sect(r).find("w:pgSz",NS).set(qn("w:code"), "9"))
    def distances(root, settings, parts):
        m=sect(root).find("w:pgMar",NS);m.set(qn("w:header"),"1134");m.set(qn("w:footer"),"850")
    save("harmless-distances", build_source(base, False), mutation=distances)
    for name in [*expected["normalized_without_geometry"], "missing-margin-edge"]:
        def missing(root, settings, parts, name=name):
            s = sect(root)
            if name == "missing-section":s.getparent().remove(s)
            elif name == "missing-size":w.remove(s,"pgSz")
            elif name == "missing-margins":w.remove(s,"pgMar")
            else:del s.find("w:pgMar",NS).attrib[qn("w:left")]
        save(name, build_source(base, False), "reject" if name == "missing-margin-edge" else "normalize", mutation=missing, reason="partial malformed geometry" if name == "missing-margin-edge" else "absent geometry declaration")
    mutations = {}
    def prop(tag, attrs):
        return lambda root, settings, parts:w.child(sect(root), tag, **attrs)
    for tag, attrs in [("type",{"val":"continuous"}), ("type",{"val":"evenPage"}),
                       ("bidi",{"val":"1"}), ("rtlGutter",{"val":"1"}),
                       ("textDirection",{"val":"tbRl"}), ("vAlign",{"val":"center"}),
                       ("docGrid",{"type":"lines","linePitch":"360"}),
                       ("pgBorders",{}), ("lnNumType",{"countBy":"1"})]:
        mutations["section-"+tag+"-"+next(iter(attrs.values()),"present")]=prop(tag,attrs)
    for setting in ["mirrorMargins","gutterAtTop","bookFoldPrinting","bookFoldRevPrinting"]:
        mutations["settings-"+setting]=lambda r,s,p,tag=setting:w.child(s,tag,val="1")
    def col(root,settings,parts):
        s=sect(root);w.remove(s,"cols");w.child(s,"cols",num="2",space="720")
    mutations["two-columns"]=col
    def multisection(root,settings,parts):
        body=root.find("w:body",NS);para=body.find("w:p",NS);pp=para.find("w:pPr",NS);pp.append(copy.deepcopy(sect(root)))
    mutations["multiple-sections"]=multisection
    mutations["gutter"]=lambda r,s,p:sect(r).find("w:pgMar",NS).set(qn("w:gutter"),"120")
    mutations["wrong-orientation"]=lambda r,s,p:sect(r).find("w:pgSz",NS).set(qn("w:orient"),"landscape")
    for attr,value in [("w","11909"),("h","16841"),("w","0"),("w","-1"),("h","15840"),("w","11906.5"),("w","NaN")]:
        mutations["size-"+attr+"-"+value]=lambda r,s,p,a=attr,v=value:sect(r).find("w:pgSz",NS).set(qn("w:"+a),v)
    for attr,value in [("top","-1"),("right","4321"),("bottom","1.5"),("left","x")]:
        mutations["margin-"+attr+"-"+value]=lambda r,s,p,a=attr,v=value:sect(r).find("w:pgMar",NS).set(qn("w:"+a),v)
    for tag in ["pgSz","pgMar"]:
        for kind in ["duplicate","foreign-element","foreign-attribute","unknown-attribute","wrapped","child"]:
            def malformed(root,settings,parts,tag=tag,kind=kind):
                s=sect(root);n=s.find("w:"+tag,NS)
                if kind=="duplicate":s.append(copy.deepcopy(n))
                elif kind=="foreign-element":n.tag="{urn:wrong}"+tag
                elif kind=="foreign-attribute":n.set("{urn:wrong}w" if tag=="pgSz" else "{urn:wrong}left","720")
                elif kind=="unknown-attribute":n.set(qn("w:unknown"),"1")
                elif kind=="wrapped":s.remove(n);w.child(s,"wrapper").append(n)
                else:w.child(n,"hidden")
            mutations[tag+"-"+kind]=malformed
    def foreign_section(root,settings,parts):sect(root).tag="{urn:wrong}sectPr"
    mutations["foreign-section"]=foreign_section
    def nonterminal(root,settings,parts):
        body=root.find("w:body",NS);s=sect(root);body.remove(s);body.insert(0,s)
    mutations["nonterminal-section"]=nonterminal
    def wrapped_section(root,settings,parts):
        body=root.find("w:body",NS);s=sect(root);body.remove(s);w.child(body,"wrapper").append(s)
    mutations["wrapped-section"]=wrapped_section
    def underwidth(root,settings,parts):
        s=sect(root);s.find("w:pgSz",NS).set(qn("w:w"),"8391");s.find("w:pgSz",NS).set(qn("w:h"),"11906")
        m=s.find("w:pgMar",NS);m.set(qn("w:left"),"4320");m.set(qn("w:right"),"2632")
    mutations["content-width-below-minimum"]=underwidth
    for name,mutation in mutations.items():save("reject-"+name,build_source(base,False),"reject",mutation=mutation,reason=name)
    for layout in ["fixed"]:
        large=build_source([8391,11906,720,720,1080,1440],False)
        w.add_table(large,[3600,3600] if layout else [1440,1440],rows=1,layout=layout)
        save("reject-a5-table-"+str(layout),large,"reject",reason="automatic 9000twip or explicit fixed width exceeds declared page content area")
    for name,values in [("a5-unpositioned-auto",[8391,11906,720,720,1080,1440]),("letter-default-unpositioned-auto",[12240,15840,1440,1800,1440,1800])]:
        doc=build_source(values,False);w.add_table(doc,[1440,1440],rows=1,layout=None)
        save(name,doc)
        sources[name]["automatic_normalization"]=expected["automatic_table_expectations"][name]
    for position in ["left","zero"]:
        doc=build_source([8391,11906,720,720,1080,1440],False);table=w.add_table(doc,[1440,1440],rows=1,layout=None)
        if position=="left":w.child(table._tbl.tblPr,"jc",val="left")
        else:w.child(table._tbl.tblPr,"tblInd",w="0",type="dxa")
        save("reject-a5-positioned-auto-"+position,doc,"reject",reason="explicit automatic position keeps9000twip author width and cannot fit declared body")
    header_doc=build_source(base,False);header_doc.sections[0].header.paragraphs[0].text="Actual side story"
    save("reject-actual-header",header_doc,"reject",reason="actual header relationship/content")
    # Independent reference for every model edit, not a re-saved tested output.
    ref=directory/"edit-reference.docx";build_source([15840,12240,1260,1620,1080,900]).save(ref)
    verify_geometry(ref,expected["edit_geometry"],"edit reference")
    return sources,ref


def export_verify(cli,evidence,directory,name,model,expected):
    path=directory/(name+".json");w.write_model(path,model);out=path.with_suffix(".docx")
    run_cli(cli,evidence["commands"],"create-docx",path,out)
    snap=verify_geometry(out,expected,name)
    reopened=run_cli(cli,evidence["commands"],"read-docx-model",out)
    check(reopened==model,name+": complete trusted model differs")
    imported=run_cli(cli,evidence["commands"],"import-docx",out)["document"]
    check(imported.get("attrs",{}).get(KEY)==expected,name+": external reimport geometry differs")
    before=digest(out);error=run_cli(cli,evidence["commands"],"create-docx",path,out,success=False)
    check("Output already exists" in error and digest(out)==before,name+": existing destination overwritten")
    return {"path":str(out),"model_path":str(path),"sha256":before,"independent":snap,
            "trusted_full_model_equal":True,"external_geometry_equal":True,"no_clobber":True}


def render_pdf(soffice,directory,path,expected,rich=False):
    out=directory/"libreoffice-pdf";out.mkdir(exist_ok=True)
    profile=directory/"lo-profile"
    command=[str(soffice),"-env:UserInstallation="+profile.as_uri(),"--headless","--convert-to","pdf","--outdir",str(out),str(path)]
    proc=subprocess.run(command,capture_output=True,text=True,timeout=120,env=dict(os.environ,XDG_CACHE_HOME=str(directory/"lo-cache")))
    pdf=out/(path.stem+".pdf");check(proc.returncode==0 and pdf.is_file(),proc.stdout+proc.stderr)
    reader=PdfReader(pdf);pages=[];images=[];segments=[];pending=[];point=None
    def transform(x,y,cm):return (float(x)*cm[0]+float(y)*cm[2]+cm[4],float(x)*cm[1]+float(y)*cm[3]+cm[5])
    for page_index,page in enumerate(reader.pages):
        dims=[float(page.mediabox.width),float(page.mediabox.height)]
        check(all(abs(a-b/20)<=0.15 for a,b in zip(dims,[expected["widthTwips"],expected["heightTwips"]])),path.name+": PDF MediaBox differs")
        pages.append(dims);resources=page["/Resources"].get_object();xobjects=resources.get("/XObject",{}).get_object() if "/XObject" in resources else {}
        def operand(op,args,cm,tm):
            nonlocal pending,point
            if op==b"Do" and args[0] in xobjects:
                obj=xobjects[args[0]].get_object()
                if obj.get("/Subtype")=="/Image":
                    width,height=math.hypot(cm[0],cm[1]),math.hypot(cm[2],cm[3])
                    images.append({"page":page_index+1,"x":cm[4],"top":dims[1]-cm[5]-height,"width":width,"height":height,"center_x":cm[4]+width/2})
            elif op==b"m":point=transform(*args,cm)
            elif op==b"l":
                end=transform(*args,cm)
                if point is not None:pending.append((point,end))
                point=end
            elif op==b"re":
                x,y,ww,hh=map(float,args);points=[transform(a,b,cm) for a,b in [(x,y),(x+ww,y),(x+ww,y+hh),(x,y+hh),(x,y)]];pending.extend(zip(points,points[1:]))
            elif op in [b"S",b"s",b"B",b"B*",b"b",b"b*",b"f",b"f*",b"F"]:
                for a,b in pending:
                    if abs(a[0]-b[0])<0.1 and abs(a[1]-b[1])>10:segments.append({"x":(a[0]+b[0])/2,"top":dims[1]-max(a[1],b[1]),"bottom":dims[1]-min(a[1],b[1])})
                pending=[];point=None
            elif op==b"n":pending=[];point=None
        page.extract_text(visitor_operand_before=operand)
    bbox=pdf.with_suffix(".bbox.xml")
    subprocess.run(["pdftotext","-bbox",str(pdf),str(bbox)],capture_output=True,text=True,check=True,timeout=30)
    root=ET.parse(str(bbox));words={}
    markers=["LeftText","CenterText","RightText",*["T"+a+str(i) for a in ["Left","Center","Right"] for i in [0,1]]]
    for node in root.iter():
        if ET.QName(node).localname=="word" and node.text in markers:
            check(node.text not in words,"Duplicate PDF marker "+node.text);words[node.text]={k:float(node.get(k)) for k in ["xMin","yMin","xMax","yMax"]}
    # Merge the two edges of a thin filled border without erasing table groups.
    borders=[]
    for row in sorted(segments,key=lambda x:(round(x["top"],0),x["x"])):
        if not any(abs(row["x"]-b["x"])<0.8 and abs(row["top"]-b["top"])<0.8 and abs(row["bottom"]-b["bottom"])<0.8 for b in borders):borders.append(row)
    if rich:
        check(len(reader.pages)==1,path.name+": controlled one-page source repaginated")
        check(set(words)==set(markers),path.name+": missing PDF text markers")
        check(len(images)==3,path.name+": expected 3 PDF image frames")
        check(len(borders)==9,path.name+": expected 9 table border anchors, found "+str(borders))
        left,right=expected["marginLeftTwips"]/20,(expected["widthTwips"]-expected["marginRightTwips"])/20
        center=(left+right)/2
        check(abs(words["LeftText"]["xMin"]-left)<=TOLERANCE,path.name+": left text misses content edge")
        check(abs((words["CenterText"]["xMin"]+words["CenterText"]["xMax"])/2-center)<=TOLERANCE,path.name+": centered text misses content center")
        check(abs(words["RightText"]["xMax"]-right)<=TOLERANCE,path.name+": right text misses content edge")
        for image,want in zip(images,[left+36,center,right-36]):
            check(abs(image["center_x"]-want)<=TOLERANCE,path.name+": image center misses exact content anchor")
    return {"path":str(pdf),"sha256":digest(pdf),"consumer":"LibreOffice only; not app/native/Word/WPS","pages":pages,"words":words,"images":images,"borders":borders,"command":command,"mediabox_tolerance_pt":0.15,"consumer_note":"LibreOffice may snap near-nominal paper sizes; exact twips are verified in OOXML and python-docx"}


def compare_pdf(source,output,label):
    check(source["pages"]==output["pages"],label+": MediaBox/page count changed")
    deltas={}
    for marker,coords in source["words"].items():
        check(marker in output["words"],label+": text marker missing")
        deltas[marker]={k:output["words"][marker][k]-v for k,v in coords.items()}
    for index,(a,b) in enumerate(zip(source["images"],output["images"])):
        deltas["image-"+str(index)]={k:b[k]-a[k] for k in ["x","top","width","height","center_x"]}
    check(len(source["borders"])==len(output["borders"]),label+": table border count changed")
    for index,(a,b) in enumerate(zip(source["borders"],output["borders"])):deltas["table-border-"+str(index)]={k:b[k]-a[k] for k in a}
    check(all(abs(v)<=TOLERANCE for row in deltas.values() for v in row.values()),label+": coordinates changed: "+str(deltas))
    return {"tolerance_pt":TOLERANCE,"max_abs_delta_pt":max([abs(v) for row in deltas.values() for v in row.values()] or [0]),"deltas":deltas}


def invalid_models(cli,evidence,directory,seed):
    base=seed["attrs"][KEY];cases={}
    for value in [None,[],True,0,"A4"]:cases["whole-"+repr(value)]=value
    for key in KEYS:
        value=copy.deepcopy(base);del value[key];cases["missing-"+key]=value
        for index,bad in enumerate([None,True,False,"1440",1.5,-1,{},[]]):
            value=copy.deepcopy(base);value[key]=bad;cases[key+"-type-"+str(index)]=value
    for key,bad in [("widthTwips",11909),("heightTwips",16841),("widthTwips",12240),
                    ("marginTopTwips",4321),("marginRightTwips",4321),("marginBottomTwips",4321),("marginLeftTwips",4321)]:
        value=copy.deepcopy(base);value[key]=bad;cases[key+"-out-of-bound-"+str(bad)]=value
    value=copy.deepcopy(base);value["orientation"]="portrait";cases["unknown-field"]=value
    cases["content-width-1439"]=geometry([8391,11906,1440,2632,1440,4320])
    cases["content-height-1439"]=geometry([11906,8391,4320,1440,2632,1440])
    for name,value in cases.items():
        model=copy.deepcopy(seed);model["attrs"][KEY]=value
        path=directory/("invalid-"+name+".json");w.write_model(path,model);out=path.with_suffix(".docx")
        before=set(directory.iterdir());error=run_cli(cli,evidence["commands"],"create-docx",path,out,success=False)
        check(not out.exists() and set(directory.iterdir())==before,name+": rejected write was not atomic")
        evidence["invalid_models"][name]={"error":error,"no_output_or_tempfile":True}
    for node_type in ["paragraph","text","image","table"]:
        model=copy.deepcopy(seed);del model["attrs"][KEY]
        nodes=[]
        def visit(node):
            if node.get("type")==node_type:nodes.append(node)
            for child in node.get("content",[]):visit(child)
        visit(model);check(nodes,"seed missing node "+node_type);nodes[0].setdefault("attrs",{})[KEY]=base
        path=directory/("invalid-nonroot-"+node_type+".json");w.write_model(path,model);out=path.with_suffix(".docx")
        error=run_cli(cli,evidence["commands"],"create-docx",path,out,success=False);check(not out.exists(),"Nonroot geometry emitted output")
        evidence["invalid_models"]["nonroot-"+node_type]={"error":error,"no_output":True}


def verify_automatic_table(path,expected,label):
    snap=w.independent_snapshot(path)
    check(len(snap["ooxml"])==len(snap["python_docx"])==1,label+": automatic table count differs")
    xml,py=snap["ooxml"][0],snap["python_docx"][0]
    check(xml["layout"] is None and xml["alignment"] is None and xml["indent"] is None,label+": invented fixed layout or position")
    check(xml["total"]=={"w":str(expected["export_total_twips"]),"type":"dxa"},label+": normalized automatic tblW differs")
    columns=expected["export_columns_twips"]
    check(xml["grid"]==[{"w":str(x)} for x in columns],label+": automatic grid differs")
    check(xml["cells"]==[[{"w":str(x),"type":"dxa"} for x in columns]],label+": automatic cell widths differ")
    check(py["autofit"] and py["column_widths_twips"]==columns and py["cell_widths_twips"]==[columns],label+": python-docx automatic declarations disagree")
    return snap


def auto_reference(directory,name,page,table_expected):
    doc=build_source([page[k] for k in KEYS],False)
    table=w.add_table(doc,table_expected["export_columns_twips"],rows=1,layout=None)
    table._tbl.tblPr.find("w:tblW",NS).set(qn("w:w"),str(table_expected["export_total_twips"]))
    path=directory/(name+"-auto-reference.docx");doc.save(path)
    verify_automatic_table(path,table_expected,name+" independently authored reference")
    return path


def automatic_reflow(cli,evidence,directory,name,item,model,expected,soffice):
    target=item["automatic_normalization"];edited_target=expected["automatic_edit_expectation"]
    tables=list(w.model_tables(model));check(len(tables)==1,"Expected one automatic source table")
    check(not any(k in tables[0].get("attrs",{}) for k in ["columnWidthsTwips","tableAlign","tableIndentTwips"]),name+": invented automatic widths/position model metadata")
    check(any("Unpositioned automatic table widths normalized to the explicit page content area" in risk for risk in item["compatibility"]["risks"]),name+": automatic width normalization warning absent")
    item["automatic_readback"]=verify_automatic_table(Path(item["unchanged_export"]["path"]),target,name)
    item["automatic_edit_readback"]=verify_automatic_table(Path(item["edited_export"]["path"]),edited_target,name+" page reflow")
    reset=copy.deepcopy(model);reset["attrs"].pop(KEY)
    reset_json=directory/(name+"-page-reset.json");w.write_model(reset_json,reset);reset_docx=reset_json.with_suffix(".docx")
    run_cli(cli,evidence["commands"],"create-docx",reset_json,reset_docx)
    check(run_cli(cli,evidence["commands"],"read-docx-model",reset_docx)==reset,name+": reset full model changed")
    item["automatic_reset_readback"]=verify_automatic_table(reset_docx,edited_target,name+" absent-page reset")
    golden=json.loads(LEGACY.read_text())["cases"]["unpositioned-auto-page-reset"]
    check(reset==golden["model"] and digest(reset_docx)==golden["sha256"],name+": page reset did not restore pre-feature automatic model/bytes")
    item["automatic_reset_historical_sha256"]=golden["sha256"]
    if soffice:
        refs=[("normalized",item["expected_geometry"],target,"unchanged_export"),("edited",expected["edit_geometry"],edited_target,"edited_export")]
        for label,page,widths,out_key in refs:
            reference=auto_reference(directory,name+"-"+label,page,widths)
            ref_pdf=render_pdf(soffice,directory,reference,page)
            output_pdf=render_pdf(soffice,directory,Path(item[out_key]["path"]),page)
            check(len(ref_pdf["borders"])==len(output_pdf["borders"])==3,name+": missing automatic PDF border anchors")
            border_width=max(x["x"] for x in output_pdf["borders"])-min(x["x"] for x in output_pdf["borders"])
            check(abs(border_width-widths["export_total_twips"]/20)<=0.8,name+": automatic physical width does not match normalized target")
            item["automatic_"+label+"_pdf"]={"independent_reference":ref_pdf,"output":output_pdf,"comparison":compare_pdf(ref_pdf,output_pdf,name+" automatic "+label),"physical_width_pt":border_width}
        item["automatic_layout_scope"]="Original automatic width is explicitly normalized. PDF comparison is against a separately authored normalized-width reference, not a claim that source table width or layout is preserved."


def tamper_packages(cli,evidence,directory,good):
    with zipfile.ZipFile(good) as z:original={n:z.read(n) for n in z.namelist()}
    results={}
    for mode in ["xml-only","xml-and-forged-hash","embedded-model-only","embedded-model-and-hash"]:
        parts=copy.deepcopy(original);embedded=json.loads(parts["rust-wps/model-v2.json"])
        if mode.startswith("xml"):
            root=ET.fromstring(parts["word/document.xml"]);m=root.find(".//w:sectPr/w:pgMar",NS);m.set(qn("w:left"),str(int(m.get(qn("w:left")))+20))
            parts["word/document.xml"]=ET.tostring(root,xml_declaration=True,encoding="UTF-8",standalone=True)
            if mode=="xml-and-forged-hash":embedded["hashes"]["word/document.xml"]=hashlib.sha256(parts["word/document.xml"]).hexdigest()
        else:
            embedded["document"]["attrs"][KEY]["marginLeftTwips"]+=20
            if mode=="embedded-model-and-hash":embedded["hashes"]["word/document.xml"]=hashlib.sha256(parts["word/document.xml"]).hexdigest()
        parts["rust-wps/model-v2.json"]=json.dumps(embedded,separators=(",",":")).encode()
        path=directory/("tamper-"+mode+".docx")
        with zipfile.ZipFile(path,"w",zipfile.ZIP_DEFLATED) as z:
            for n,b in parts.items():z.writestr(n,b)
        before=digest(path);error=run_cli(cli,evidence["commands"],"read-docx-model",path,success=False)
        check(digest(path)==before,"tampered source changed");results[mode]={"error":error,"sha256":before}
    evidence["tampered_packages"]=results


def legacy_bytes(cli,evidence,directory):
    manifest=json.loads(LEGACY.read_text());results={}
    for name,item in manifest["cases"].items():
        check(KEY not in item["model"].get("attrs",{}),"legacy golden contains new attribute")
        path=directory/("legacy-"+name+".json");w.write_model(path,item["model"]);out=path.with_suffix(".docx")
        run_cli(cli,evidence["commands"],"create-docx",path,out)
        check(digest(out)==item["sha256"],"Absent geometry changed historical whole-package bytes: "+name)
        with zipfile.ZipFile(out) as z:parts={n:hashlib.sha256(z.read(n)).hexdigest() for n in z.namelist()}
        check(parts==item["parts_sha256"],"Absent geometry changed historical parts: "+name)
        check(run_cli(cli,evidence["commands"],"read-docx-model",out)==item["model"],"legacy full-model reopen differs")
        # Add then remove geometry must recover the exact prior bytes.
        edited=copy.deepcopy(item["model"]);edited.setdefault("attrs",{})[KEY]=geometry([12240,15840,1440,1440,1440,1440])
        page_path=directory/("legacy-"+name+"-with-page.json");w.write_model(page_path,edited)
        page_out=page_path.with_suffix(".docx");run_cli(cli,evidence["commands"],"create-docx",page_path,page_out)
        edited=run_cli(cli,evidence["commands"],"read-docx-model",page_out)
        edited["attrs"].pop(KEY)
        if not edited["attrs"]:edited.pop("attrs")
        check(edited==item["model"],"reset mutated unrelated model")
        reset_path=directory/("legacy-"+name+"-reset.json");w.write_model(reset_path,edited)
        reset_out=reset_path.with_suffix(".docx");run_cli(cli,evidence["commands"],"create-docx",reset_path,reset_out)
        check(digest(reset_out)==item["sha256"],"Removing geometry did not recover exact historical bytes")
        results[name]={"sha256":item["sha256"],"parts":len(parts),"full_model_equal":True}
    evidence["legacy_byte_identity"]={"baseline_cli_sha256":manifest["baseline_cli_sha256"],"cases":results}


def original_corpus(args,cli,evidence,directory,soffice):
    for fixture in json.loads(CORPUS.read_text())["fixtures"]:
        path=(args.corpus_dir/fixture["name"]) if args.corpus_dir else ROOT/fixture["local_default"]
        check(path.is_file(),"Missing untouched corpus fixture: "+str(path))
        blob=path.read_bytes();check(len(blob)==fixture["bytes"] and digest(path)==fixture["sha256"],"Unmodified upstream hash mismatch")
        check(hashlib.sha1(b"blob "+str(len(blob)).encode()+b"\0"+blob).hexdigest()==fixture["git_blob_sha"],"Upstream git blob hash mismatch")
        snap=verify_geometry(path,fixture["expected_geometry"],fixture["name"])
        inv=fixture["inventory"];row=snap["sections"][0]
        for key in ["size_attributes","margin_attributes","column_attributes","grid_attributes","terminal_body_section"]:check(row[key]==inv[key],"Pinned inventory drift: "+key)
        imported=run_cli(cli,evidence["commands"],"import-docx",path)
        check(imported["document"].get("attrs",{}).get(KEY)==fixture["expected_geometry"],"Original source geometry differs")
        item={"source":str(path),"source_sha256":digest(path),"source_inventory":snap,"provenance":fixture,
              "compatibility":imported["compatibility"],"export":export_verify(cli,evidence,directory,"original-"+path.stem,imported["document"],fixture["expected_geometry"])}
        if soffice:
            item["source_pdf"]=render_pdf(soffice,directory,path,fixture["expected_geometry"])
            item["output_pdf"]=render_pdf(soffice,directory,Path(item["export"]["path"]),fixture["expected_geometry"])
            item["visual_scope"]="MediaBox verified; layout and pagination not asserted equivalent for this untouched upstream document"
        check(digest(path)==fixture["sha256"],"Original upstream source changed")
        evidence["original_corpus"][fixture["name"]]=item


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli",type=Path,default=Path("target/debug/rust-wps"))
    parser.add_argument("--out",type=Path,default=Path("tmp/word-page-geometry"))
    parser.add_argument("--corpus-dir",type=Path)
    parser.add_argument("--skip-corpus",action="store_true")
    parser.add_argument("--generate-only",action="store_true")
    parser.add_argument("--render",action="store_true")
    parser.add_argument("--soffice")
    args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
    directory=Path(tempfile.mkdtemp(prefix="run-",dir=args.out)).resolve();cli=args.cli.resolve()
    soffice=(args.soffice or shutil.which("soffice")) if args.render else None
    check(not args.render or soffice,"LibreOffice required for --render")
    expected=json.loads(EXPECTED.read_text())
    paths=[Path(__file__).resolve(),EXPECTED,LEGACY,CORPUS,Path(p.__file__),Path(d.__file__),Path(w.__file__)]
    if not args.generate_only:paths.append(cli)
    initial={str(x):digest(x) for x in paths}
    evidence={"status":"running","commands":[],"tool_hashes":initial,"python":platform.python_version(),
              "dependencies":{n:version(n) for n in ["python-docx","lxml","Pillow","pypdf"]},
              "invalid_models":{},"original_corpus":{},
              "limitations":"Controlled LibreOffice DOCX-to-PDF page sizes/text/image/table positions only; no app browser/native/Word/WPS or arbitrary pagination fidelity claim."}
    try:
        evidence["sources"],reference=make_fixtures(directory,expected)
        if soffice:evidence["libreoffice_version"]=subprocess.run([soffice,"--version"],capture_output=True,text=True,check=True,timeout=30).stdout.strip()
        if not args.generate_only:
            seed=None
            if soffice:evidence["edited_source_reference_pdf"]=render_pdf(soffice,directory,reference,expected["edit_geometry"],True)
            for name,item in evidence["sources"].items():
                source=Path(item["path"])
                if item["expected_action"]=="reject":
                    item["audit"]=run_cli(cli,evidence["commands"],"audit-docx",source)
                    item["import_error"]=run_cli(cli,evidence["commands"],"import-docx",source,success=False)
                    check(item["audit"].get("risks"),name+": rejected source lacks audit diagnostic")
                    if name.startswith("reject-a5-"):
                        check("Table width and indentation exceed the explicit page content width" in item["import_error"],name+": refused for an unrelated reason")
                else:
                    imported=run_cli(cli,evidence["commands"],"import-docx",source);model=imported["document"];item["compatibility"]=imported["compatibility"]
                    if item["expected_action"]=="normalize":
                        check(KEY not in model.get("attrs",{}),name+": missing source geometry invented metadata")
                        check(any("page" in x.lower() and ("normaliz" in x.lower() or "missing" in x.lower() or "unspecified" in x.lower() or "incomplete" in x.lower()) for x in imported["compatibility"]["risks"]),name+": missing normalization warning")
                        check(digest(source)==item["sha256"],name+": normalized source changed")
                        continue
                    check(model.get("attrs",{}).get(KEY)==item["expected_geometry"],name+": model geometry differs from independent preset")
                    if name in ["paper-code-metadata","harmless-distances"]:
                        check(any("header/footer" in risk and "normalized" in risk for risk in item["compatibility"]["risks"]),name+": ancillary metadata normalization warning missing")
                    if seed is None:seed=model
                    item["unchanged_export"]=export_verify(cli,evidence,directory,name+"-unchanged",model,item["expected_geometry"])
                    edited=copy.deepcopy(model);edited["attrs"][KEY]=copy.deepcopy(expected["edit_geometry"])
                    item["edited_export"]=export_verify(cli,evidence,directory,name+"-edited",edited,expected["edit_geometry"])
                    source_images=d.snapshot(source)["images"]
                    for which in ["unchanged_export","edited_export"]:
                        output_images=d.snapshot(Path(item[which]["path"]))["images"]
                        fields=["sha256","mime","bytes","pixel_width","pixel_height","wp_extent","a_extent"]
                        check([{k:i.get(k) for k in fields} for i in output_images]==[{k:i.get(k) for k in fields} for i in source_images],name+": image media or extents changed")
                    error=run_cli(cli,evidence["commands"],"create-docx",item["unchanged_export"]["model_path"],source,success=False)
                    check("Output already exists" in error,name+": source overwrite not rejected")
                    item["source_no_clobber"]=True
                    if "automatic_normalization" in item:automatic_reflow(cli,evidence,directory,name,item,model,expected,soffice)
                    if soffice and item["render_probe"]:
                        item["source_pdf"]=render_pdf(soffice,directory,source,item["expected_geometry"],True)
                        item["output_pdf"]=render_pdf(soffice,directory,Path(item["unchanged_export"]["path"]),item["expected_geometry"],True)
                        item["edited_pdf"]=render_pdf(soffice,directory,Path(item["edited_export"]["path"]),expected["edit_geometry"],True)
                        item["source_vs_output"]=compare_pdf(item["source_pdf"],item["output_pdf"],name)
                        item["edit_vs_independent_reference"]=compare_pdf(evidence["edited_source_reference_pdf"],item["edited_pdf"],name+" actual page edit")
                check(digest(source)==item["sha256"],name+": source changed")
            invalid_models(cli,evidence,directory,seed)
            tamper_packages(cli,evidence,directory,Path(evidence["sources"]["a4-portrait"]["unchanged_export"]["path"]))
            legacy_bytes(cli,evidence,directory)
            if not args.skip_corpus:original_corpus(args,cli,evidence,directory,soffice)
            if soffice:
                picked=[evidence["sources"][name][key]["path"] for name in ["a4-landscape","a5-portrait","letter-asymmetric"] for key in ["source_pdf","output_pdf","edited_pdf"]]
                evidence["visual_inspection_artifacts"]=p.raster_preview(directory,picked)
        check(all(digest(Path(path))==sha for path,sha in initial.items()),"CLI or validation inputs changed during run")
        evidence["summary"]={"supported_synthetic":sum(i["expected_action"]=="roundtrip" for i in evidence["sources"].values()),
                             "normalized_synthetic":sum(i["expected_action"]=="normalize" for i in evidence["sources"].values()),
                             "refused_synthetic":sum(i["expected_action"]=="reject" for i in evidence["sources"].values()),
                             "invalid_models":len(evidence["invalid_models"]),"tampered_packages":len(evidence.get("tampered_packages",{})),
                             "legacy_byte_identical":len(evidence.get("legacy_byte_identity",{}).get("cases",{})),
                             "untouched_original_positive":len(evidence["original_corpus"]),"pdfs":len(list((directory/"libreoffice-pdf").glob("*.pdf")))}
        evidence["status"]="generated" if args.generate_only else "passed"
    except Exception as exc:evidence["status"]="failed";evidence["error"]=str(exc);raise
    finally:
        (directory/"report.json").write_text(json.dumps(evidence,indent=2,ensure_ascii=False),encoding="utf-8");print(directory/"report.json",flush=True)
    print(json.dumps(evidence["summary"]))


if __name__=="__main__":main()
