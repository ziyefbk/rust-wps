# Bounded Word workflow sources

`independent-chinese-minutes.docx` is a newly authored, synthetic Chinese meeting-minutes fixture made with python-docx 1.2.0, independently of the product importer/model/exporter. It is **not** a genuine third-party Chinese corpus sample or a user's document. The adjacent generator and frozen expected JSON preserve its origin and exact text. The text and generator are new test material authored for this repository; the DOCX starts from python-docx's bundled default template, not a third-party sample document. No personal data is used; names and meeting details are fictional test text.

Frozen DOCX SHA-256: `2c9cfb2aefdc058b18ad3daf2ba5b2e6f30d5b6bbb54e85f537725edd75aebf7`.

The generator refuses to overwrite an existing fixture. Regeneration in a fresh directory can produce a different ZIP-byte hash because of ZIP timestamps; do not replace the frozen oracle to hide a product mismatch. It creates A4/25.4-mm margins, Heading1 + two Normal paragraphs, with no headers, footers, tables or product-embedded model.

The workflow's genuine external sample is `../docx-cell-no-fill/Bug60337.docx`, an English Apache POI test document; consult that directory's provenance and license notices. Chinese text added by the test does not turn it into a genuine Chinese source.
