# DOC pagination regression fixtures

These two fixtures are authored entirely in this repository. They contain only
the literal text shown below and have no third-party document content. Regenerate
them with `python scripts/make-doc-pagination-fixtures.py`; `--check` verifies the
saved bytes without changing files.

The generator assembles a minimal CFB container with a `WordDocument` stream and
a `0Table` stream. Each fixture has one UTF-16 piece covering all main text, one
PAPX FKP run, and one section. It has no macros, images, other stories, or fields.
These are targeted parser regression inputs, not documents exported or independently
validated by Microsoft Word or LibreOffice.

- `blank.doc`: main text is `hello.\r\r\x0C\rworld.\r`. The paragraph property run
  covers all text. Editable import should retain `hello.`, one real empty paragraph,
  one manual page break, and `world.` in that order. The standalone page-break
  paragraph should not manufacture another empty paragraph.
- `partial.doc`: main text is `A\rB\x0CC\r`. The piece table covers all text, but
  the PAPX run ends after `A\r` (CP 2 of 6). This deliberate inconsistency reproduces
  a partial structured recovery that flat-text completeness does not detect.
  Editable import must fail with `DOC_STRUCTURED_TEXT_INCOMPLETE` while read-only
  extraction can still expose all text.

Both files are 9,728 bytes. The generator prints SHA-256 hashes for reproducibility.
