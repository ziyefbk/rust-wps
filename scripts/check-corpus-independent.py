"""Independent python-docx check of a check-corpus.ps1 output directory.

Checks paragraph text and explicit page breaks, not rendered pagination.
Requires python-docx; uses only already downloaded/local corpus files.
"""
import argparse
import json
from pathlib import Path

from docx import Document


def snapshot(path):
    doc = Document(path)
    return {
        "paragraphs": [paragraph.text for paragraph in doc.paragraphs],
        "page_breaks": sum(
            item.get("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}type")
            == "page"
            for paragraph in doc.paragraphs
            for item in paragraph._p.xpath(".//w:br")
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_directory", type=Path)
    args = parser.parse_args()
    corpus = Path(__file__).resolve().parent.parent / "tmp" / "corpus"
    source = snapshot(corpus / "SampleDoc.docx")
    converted = snapshot(args.run_directory / "SampleDoc.doc.docx")
    edited = snapshot(args.run_directory / "edited.docx")
    simple = snapshot(args.run_directory / "simple.doc.docx")
    # This pinned pair also exercises one explicit manual page break. These
    # assertions describe this fixture, not general rendered DOC fidelity.
    assert [s for s in converted["paragraphs"] if s] == [
        s for s in source["paragraphs"] if s
    ], "Paired DOC conversion changed nonempty paragraph text"
    assert converted["page_breaks"] == source["page_breaks"] == 1, "DOC conversion lost the paired manual page break"
    assert converted["paragraphs"] == source["paragraphs"], "DOC conversion changed the paired paragraph boundaries"
    assert any("test document" in s for s in source["paragraphs"])
    assert edited["paragraphs"] == [
        s.replace("test document", "Rust 办公测试文档") for s in source["paragraphs"]
    ], "DOCX replacement changed unexpected paragraph text"
    assert edited["page_breaks"] == source["page_breaks"], "DOCX replacement lost page breaks"
    assert any(s.strip() for s in simple["paragraphs"]), "Simple DOC export has no text"
    print(json.dumps({
        "consumer": "python-docx",
        "nonempty_paired_paragraphs_match": True,
        "replacement_paragraphs_and_page_breaks_match": True,
        "source_page_breaks": source["page_breaks"],
        "converted_page_breaks": converted["page_breaks"],
        "legacy_paragraph_structure_matches": converted == source,
        "limitation": "No independent DOC parser or visual/layout oracle; differences are reported, not hidden.",
    }, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
