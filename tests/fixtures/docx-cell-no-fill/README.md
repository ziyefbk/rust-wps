# Portable genuine no-fill regression

Release 37 portable bundle, 2026-10-15. The original bundle passed same-host stable and MSRV qualification (positive plus four negative controls). Exact public-copy qualification is recorded in docs/REGRESSION.md in the repository; no fresh-host execution is claimed.

## Proposed standalone use

Copy this entire directory into a clean checkout (or another directory), then use
Python 3.8+ and an already-built platform-compatible 简文/rust-wps CLI:

```
python3 path/to/this-bundle/check.py --cli /absolute/path/to/rust-wps-cli
```

Optional `--output-parent EXISTING_DIRECTORY` controls where a unique temporary
evidence directory is created. The script prints its path; successful and failed
artifacts are retained. It never builds, installs dependencies, accesses the
network, downloads evidence, or reads historical absolute paths. Python standard
library only. It accepts any supplied built CLI and records its actual SHA256;
no accepted candidate binary hash is used as an acceptance gate. Five CLI calls
have 30-second timeouts and a 2 MiB cap per stdout/stderr stream. ZIP input/output
is limited to 100 members and 2 MiB compressed/expanded data.

The bundle is sufficient for a clean-checkout run once supplied a built CLI.
No actual fresh-host or clean-checkout execution is claimed. This small check
does not replay the huge historical verifier and must not replace its coverage.

## Genuine fixture, legal notices and expectation provenance

Bug60337.docx is an untouched Apache POI test fixture (13,463 bytes), from revision
`ae62bb5116b9aee19ebd5834e3a82066132c9f7f`:
https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/Bug60337.docx

SHA256: `87f983bdd93b3b5b8469104a5b09ccff41e0f5c144060e13fd4db07bfd2e39da`.
Bundled LICENSE and NOTICE are exact copies of the repository’s existing Apache
POI notices. Preserve fixture provenance and both notices upon any promotion.

PROVENANCE.json records repository-relative historical audit locators/hashes. These are audit
references, never runtime dependencies. The derivation is:

1. PRE-CANDIDATE-BINDING-v1.json SHA256
   `e1b3476192efeb1963ec0292ba40035d521579d97f440a6c91e7cde99440bbe2`
   pins row-endpoints-proposed-v3.json SHA256
   `35b4423262cf44f3d45c091547fd342529790973e2ff3bba3c26ddcf0f4fa16f`.
2. Select exactly cases[name=genuine-original].endpoints.import.expected_complete_json.
   expected-import.json is that complete JSON, freshly serialized without deleting
   or replacing any fields. SHA256:
   `fc220c57bdba3de0a27dcc92abf5296ceb043cbcd89c9d8e7dfff1d93be3d691`.
3. baseline-import.json is the exact pinned release36 response bytes, SHA256
   `68ddd844d017d1ad19f3219549d6364104cc0409847d5a363523d7254b8403c4`.
   Add cellFillMode="none" to precisely 15 cells of its sole table; the entire
   response must equal the prospective frozen expectation. This derivation is
   rechecked before every run. Neither candidate output nor consumer output was
   used to construct expected-import.json. No path parameterization is needed.
4. source-proof.json extracts that endpoint’s independently reviewed raw-source
   proof, including exact 3x5 paragraph text, first-header identity and active
   style chains. The frozen source-proof parent hash is recorded in provenance.

The source/style audit functions were adapted from the pre-existing stdlib
consumer-plan/run-consumer-v2.py. This bundle does not run that consumer tool or
require any of its render binaries/evidence. Its donor hash is in provenance.

## Acceptance scope

- Pins fixture, complete frozen expected response, baseline response and proof
- Independently reads actual OPC main/styles relationships, unique style IDs,
  active/default basedOn chains and docDefaults; rejects active shading
- Proves exactly one direct 3x5 table, first row enabled header, row 3 cantSplit,
  exact intended cell/paragraph texts, raw no-shading and source grid
- Compares actual CLI import’s entire JSON (including all risks and fidelity flag)
  with the independent expected response, with duplicate/nonfinite JSON rejected
- Creates DOCX from the actual captured model, checks complete export report,
  exact trusted reopen, strict emitted part set, XML parseability, intended cells,
  enabled header/cantSplit, zero main-document shading and normalized output grid
- Checks embedded model exactly and every non-model part’s SHA256 binding
- Tests exact existing-output refusals against the newly exported file and a
  scratch copy of the original; the real fixture is NEVER an output destination
- Uses exclusive script writes and verifies protected source/expectation inputs
  and created output hashes on both normal and exceptional exits

This is a narrowly pinned real-file regression, not a generic source certifier.
There is no full original-layout, rendering, Microsoft Word or WPS claim. Source
1870-twip columns still normalize to 1800-twip columns. Body-cell absent versus
none can be canonically equivalent in the product’s metadata checks: trusted
reopen checks model/XML consistency, not authenticity or authorship. This test
requires the prospective exact imported model, but does not turn that consistency
mechanism into an authenticity guarantee. No consumer/render tools are invoked.

## Qualification and publication changes

The public copy removes private absolute audit paths. source-proof.json changes only its source locator to Bug60337.docx; check.py updates the matching PINS hash. Fixture, baseline, full expectation, LICENSE and NOTICE bytes are unchanged. Requalify this exact public copy with both unchanged frozen CLIs before release. Historical qualification report hashes are recorded in PROVENANCE.json; those reports qualify the original bundle, not this modified copy.
