# Apache POI PPT fixture

`basic_test_ppt_file.ppt` is from Apache POI, commit
`ae62bb5116b9aee19ebd5834e3a82066132c9f7f`:
https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/slideshow/basic_test_ppt_file.ppt

SHA-256: `e8fd4c38ad72ce3735acbca0b60dd5870fb60f819c169e3947d166f4f1bcc79d`.
Upstream LICENSE and NOTICE are retained here. Expected page text is checked
against `poi-scratchpad/src/test/java/org/apache/poi/hslf/extractor/TestExtractor.java`
at the same commit. This is an upstream compatibility sample, distinct from
the project's self-authored structural fixture. Text inventory normalizes
blank paragraph spacing and excludes notes; no layout fidelity is asserted.

Additional fixtures from the same commit/path: `with_textbox.ppt`,
`WithMaster.ppt`, `Password_Protected-hello.ppt`. Textbox content and master
static text are compared with upstream TestExtractor; the password sample
must fail with an encryption indication. They retain the same LICENSE/NOTICE.

SHA-256:

- Password_Protected-hello.ppt: `8d0dd9f53317a720687cf5234af0c021fab665194b09b9bb56a3f71b495bf45a`
- with_textbox.ppt: `56a900d5784d9cc245c6580ea26b00ec7fd430d39c26ad5b5e2fc071a45a629d`
- WithMaster.ppt: `307cb62b279b1b1014058f26866d905e1f80ac649ecedeb63604371d807233dd`
- WithMaster.pptx: `54ae7677fd2813a815160cc72bd8960a4e482690dfc369b392868521c9b61549`

`WithMaster.pptx` is also from the same pinned commit and slideshow directory.
It verifies static master text reading and blocked basic import; placeholder
prompts must not be presented as visible master text.

`sample.pptx` is from the same pinned commit and slideshow directory:
https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/slideshow/sample.pptx

SHA-256: `108d7219130a8460298896f45ff777dfe52b8b5527a4b4628b10731ed01d6a8e`.
The local file was byte-compared with that pinned upstream source. LICENSE
and NOTICE apply as above. Its two pages exercise layout title/subtitle and
master title/body positions. Original text and resolved geometry are checked
independently using python-pptx; edited output is also read independently.
This proves a basic editing conversion with disclosed style normalization,
not complete theme, bullet, template or visual fidelity.
