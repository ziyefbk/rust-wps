export type ReadingSearchMode = 'literal' | 'pdf-ligatures';
export interface ReadingMatch { start: number; end: number }

const expansions: Readonly<Record<string, string>> = {
  '\uFB00': 'ff', '\uFB01': 'fi', '\uFB02': 'fl', '\uFB03': 'ffi',
  '\uFB04': 'ffl', '\uFB05': 'st', '\uFB06': 'st', '\u017F': 's',
};
const project = (text: string) => text.replace(/[\uFB00-\uFB06\u017F]/g, char => expansions[char]);

// A maximal ASCII whitespace run may contain one logical line ending. Keep
// blank lines and form feeds verbatim as barriers; queries containing those
// barriers are rejected. No inferred paragraph, column or page reconstruction.
function visitWhitespaceProjection(text: string, ligatures: boolean, visit: (value: string, end: number, barrier: boolean) => void) {
  const tokens = /[ \t\r\n\f]+|[^ \t\r\n\f]/gu;
  for (const match of text.matchAll(tokens)) {
    const token = match[0];
    const whitespace = /^[ \t\r\n\f]/u.test(token);
    const barrier = whitespace && (token.includes('\f') || (token.match(/\r\n|\r|\n/g)?.length ?? 0) > 1);
    visit(whitespace && !barrier ? ' ' : ligatures ? project(token) : token, match.index + token.length, barrier);
  }
}

function whitespaceProjection(text: string, ligatures: boolean, query: boolean): { text: string; boundaries?: Uint32Array } | undefined {
  if (query && /^\s*$/u.test(text)) return undefined;
  let hasBarrier = false;
  const collapsed = text.replace(/[ \t\r\n\f]+/g, run => {
    const barrier = run.includes('\f') || (run.match(/\r\n|\r|\n/g)?.length ?? 0) > 1;
    hasBarrier ||= barrier;
    return barrier ? run : ' ';
  });
  const projected = ligatures ? project(collapsed) : collapsed;
  if (query) return hasBarrier ? undefined : { text: projected };
  const boundaries = new Uint32Array(projected.length + 1);
  boundaries[0] = 1;
  let offset = 0;
  visitWhitespaceProjection(text, ligatures, (value, end) => { offset += value.length; boundaries[offset] = end + 1; });
  return { text: projected, boundaries };
}

/** Read-only search; spans always address the untouched source in UTF-16 units.
 * Compatibility is deliberately limited to the eight mappings above, not NFKC.
 * Optional ASCII whitespace compatibility preserves whole collapsed-run spans.
 * Only whole source code points/expansions may match in compatibility modes.
 * Literal mode preserves the existing indexOf behavior, including UTF-16 matching.
 */
export function findReadingMatches(text: string, query: string, mode: ReadingSearchMode = 'literal', whitespaceCompatible = false): ReadingMatch[] {
  if (!query || !text) return [];
  const compatible = mode === 'pdf-ligatures';
  const whitespaceNeedle = whitespaceCompatible ? whitespaceProjection(query, compatible, true) : undefined;
  if (whitespaceCompatible && !whitespaceNeedle) return [];
  const whitespaceSource = whitespaceCompatible ? whitespaceProjection(text, compatible, false) : undefined;
  const needle = whitespaceNeedle?.text ?? (compatible ? project(query) : query);
  const haystack = whitespaceSource?.text ?? (compatible ? project(text) : text);
  if (needle.length > haystack.length) return [];

  // Zero is an invalid interior; valid entries store original offset + 1.
  // for-of visits an astral pair once, so its interior cannot become an endpoint.
  let boundaries: Uint32Array | undefined = whitespaceSource?.boundaries;
  if (compatible && !whitespaceCompatible) {
    boundaries = new Uint32Array(haystack.length + 1);
    boundaries[0] = 1;
    let originalOffset = 0, projectedOffset = 0;
    for (const char of text) {
      originalOffset += char.length;
      projectedOffset += (expansions[char] ?? char).length;
      boundaries[projectedOffset] = originalOffset + 1;
    }
  }
  const matches: ReadingMatch[] = [];
  let at = haystack.indexOf(needle);
  while (at !== -1 && matches.length < 2000) {
    const end = at + needle.length;
    if (!boundaries || (boundaries[at] && boundaries[end])) {
      matches.push(boundaries ? { start: boundaries[at] - 1, end: boundaries[end] - 1 } : { start: at, end });
      at = haystack.indexOf(needle, end);
    } else {
      // An invalid partial expansion must not skip a later, valid candidate.
      at = haystack.indexOf(needle, at + 1);
    }
  }
  return matches;
}
