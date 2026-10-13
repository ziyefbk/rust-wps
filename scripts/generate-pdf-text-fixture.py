"""Generate project-owned test material; requires pypdf, no user documents."""
from pathlib import Path
from pypdf import PdfWriter
from pypdf.generic import DictionaryObject, NameObject, DecodedStreamObject

w = PdfWriter()
for text in ["First page searchable secret", "Second page searchable text"]:
    page = w.add_blank_page(width=300, height=400)
    font = DictionaryObject({NameObject('/Type'): NameObject('/Font'), NameObject('/Subtype'): NameObject('/Type1'), NameObject('/BaseFont'): NameObject('/Helvetica')})
    page[NameObject('/Resources')] = DictionaryObject({NameObject('/Font'): DictionaryObject({NameObject('/F1'): w._add_object(font)})})
    stream = DecodedStreamObject()
    stream.set_data(f'BT /F1 12 Tf 20 350 Td ({text}) Tj ET'.encode('ascii'))
    page[NameObject('/Contents')] = w._add_object(stream)
w.encrypt('test-password', algorithm='AES-256')
w.write(Path(__file__).resolve().parents[1] / 'tests/fixtures/encrypted-text.pdf')
