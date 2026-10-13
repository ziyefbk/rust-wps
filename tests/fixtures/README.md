# Test fixtures

`authoring.json` is a project-authored editor model.

`authoring-before-font-size.docx` was exported from that model using the CLI
before font-size marks were added. It is a self-authored compatibility fixture:
the current application must still open its verified embedded model, and exporting
the unchanged model must reproduce every package part's bytes. SHA-256:
`3eff5df57c4a0ee11604226c985c9d8c552f08414ad63e9b46862f3a565e1f45`.
Do not regenerate it with a newer exporter merely to make a regression pass.

`encrypted.pdf` is a project-generated blank 300 × 400 point PDF page encrypted
with AES-256 using the test-only password `test-password`. It contains no user
document or third-party artwork. Generated with the bundled pypdf:

```python
from pypdf import PdfWriter
w = PdfWriter()
w.add_blank_page(width=300, height=400)
w.encrypt('test-password', algorithm='AES-256')
w.write('tests/fixtures/encrypted.pdf')
```

Encryption uses random values, so regeneration changes the bytes. Tests check
behavior rather than a fixed digest. This fixture verifies independent text
extraction failure and PDF.js password retries/cancellation/local rendering;
it does not validate complex encrypted documents or text extraction after unlock.

`encrypted-text.pdf` is another project-generated AES-256 fixture, containing
two pages of Helvetica text. Regenerate with `scripts/generate-pdf-text-fixture.py`.
It uses the same test password and verifies text extraction and search after
unlocking. It is not a Chinese font or visual fidelity reference.
# Layout placeholder fixtures

`layout-placeholders.pptx` and `layout-placeholders-ambiguous.pptx` are
self-authored using `scripts/make-pptx-layout-fixture.py` and python-pptx.
They carry default title/subtitle placeholders without slide-local geometry,
plus a directly positioned text box. The second deliberately duplicates
the layout's title index to verify ambiguous geometry is not guessed.
Expected EMU positions are independently read through python-pptx.

`master-placeholders.pptx` additionally removes title/subtitle geometry
from the layout, exercising title/body role inheritance from its master.
`master-placeholders-ambiguous.pptx` deliberately duplicates the master's
title role. Both are self-authored by the same generator, with independent
python-pptx position checks; no external production deck is implied.

# Speaker notes fixture

`notes-slides.pptx` is self-authored with python-pptx: three blank slides,
Chinese notes on the first two and no notes on the third. It tests that
speaker notes are associated with pages, displayed separately and cleared
when navigating to a page without notes.
