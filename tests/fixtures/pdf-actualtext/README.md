# Exact PDF replacement fixtures

These files are frozen inputs. Do not regenerate them from the patched extractor.

The three Chromium PDFs are actual print results from the project browser Word
workflow: an original failing meeting-minutes output, the current release43
meeting-minutes output, and a newly authored Chinese document. Their literal
expected text was captured from the independently authored document input plus
explicit browser edits, before extraction. Source expectations are retained next
to them. The manifest records the original evidence locations and SHA-256 values.
They contain project-authored test text, not private user documents.

The small control PDFs were independently byte-authored for release44 review,
without the Rust product or its PDF parser/writer. Expected text is an explicit
authored literal. They include occurrence-specific replacements, true radicals,
non-normalized Unicode, scopes, resources, encodings, Types3 and Forms. The source
manifest was copied before any candidate execution; contract decisions recorded
here classify nested overlapping replacement as unsupported and an unresolved
property without proven ActualText as unknown. Those decisions do not change
characters to match extraction output.

Whitespace-only folding is used for full-text comparisons because the extraction
API uses layout heuristics. Exact payload assertions separately protect literal
whitespace and Unicode. No NFKC, canonical normalization or radical substitution
is allowed. A small marked-content fix does not establish general reading order,
visual fidelity or hostile-parser resource isolation.

The Type3 lexer supplement was independently authored before the corrected
candidate. It covers leading d0/d1 width declarations, terminal-width-only glyphs,
comments, numeric syntax, malformed/non-leading/repeated declarations, hidden
ActualText, and genuine dash-setting d. lopdf 0.42 cannot distinguish d0/d1
from d plus numeric data. The audit recognizes only a validated leading width
declaration; a remaining Type3 d is therefore unverified, including a valid dash
operator. Without known ActualText it keeps exact legacy text with Unknown;
with known ActualText it refuses the unsupported replacement. Ordinary page
and Form dash operators remain unchanged. This is a lexer compatibility limit,
not a claim that dashed Type3 glyphs are malformed. Empty replacement fixtures
must produce exactly empty text, without synthetic layout separators.

The deep-scopes status is the original independent Unsupported expectation:
the bounded parsed-operand prepass positively observes ActualText before the
64-scope validation limit. An earlier root-corpus Unknown classification reflected
the previous early-exit scan and is no longer appropriate. Source bytes and text
expectations were not changed for this classification correction.
