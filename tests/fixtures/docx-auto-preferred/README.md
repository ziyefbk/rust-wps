# Genuine automatic preferred-width regression (release 38 draft)

Not yet runtime-qualified. This is a small Python 3.8+ stdlib check using a supplied, already-built platform-compatible rust-wps CLI. No build, package installation, download or network access is performed.

Keep the repository layout: this directory is tests/fixtures/docx-auto-preferred, alongside docx-cell-no-fill. The shared sibling supplies the unchanged genuine Bug60337.docx and its Apache LICENSE / NOTICE. To relocate, copy both sibling directories with their names intact. No private historical path is read.

Run from a checkout:

    python3 tests/fixtures/docx-auto-preferred/check.py --cli path/to/rust-wps

Optional --output-parent selects an existing directory for unique retained evidence. The runner records the actual CLI hash and retains raw stdout/stderr and result JSON, including failures. Five commands have 30-second deadlines and 2 MiB output caps; ZIPs are limited to 100 members and 2 MiB. No old/private baseline binary is needed. Existing-output refusal checks use the new output and a fresh identical source copy, never the genuine fixture as an output target.

The exact full import expectation is copied byte-for-byte from an independently frozen pre-candidate source binding. It includes all model fields and risks. No accepted candidate response is used to generate it. Raw source proof comes from direct independent ZIP/XML inspection of the untouched fixture: auto/0, absent layout, five 1870-twip grid hints, fifteen matching dxa cell widths, exact 3x5 text, header and row flags. The runner also checks actual OPC/styles/no-shading, full trusted reopen and package hash envelope, exported auto/0/grid/tcW without fixed layout, retained zero indent/no-fill/row state, and exact overwrite refusals.

This is one pinned genuine-file semantic regression. Preferences are not fixed physical bounds. It does not render or prove intrinsic AutoFit, general content containment, pagination, browser or Word/WPS fidelity. Both separate AUTO-versus-FIXED consumer discrimination controls were inconclusive; the broader original consumer gate was not met. Trusted reopen checks model/package consistency, not authorship or authenticity.

Provenance hashes in PROVENANCE.json identify historical records but those records are not runtime dependencies. Preserve the shared fixture license and notice when copying. The release37 bundle remains unchanged.
