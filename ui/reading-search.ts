export type ReadingSearchMode = 'literal' | 'pdf-ligatures';
export interface ReadingMatch { start: number; end: number }

const expansions: Readonly<Record<string, string>> = {
  '\uFB00': 'ff', '\uFB01': 'fi', '\uFB02': 'fl', '\uFB03': 'ffi',
  '\uFB04': 'ffl', '\uFB05': 'st', '\uFB06': 'st', '\u017F': 's',
};
const project = (text: string) => text.replace(/[\uFB00-\uFB06\u017F]/g, char => expansions[char]);

/** Read-only search; spans always address the untouched source in UTF-16 units.
 * Compatibility is deliberately limited to the eight mappings above, not NFKC.
 * Only whole source code points/expansions may match in compatibility mode.
 * Literal mode preserves the existing indexOf behavior, including UTF-16 matching.
 */
export function findReadingMatches(text: string, query: string, mode: ReadingSearchMode = 'literal'): ReadingMatch[] {
  if (!query || !text) return [];
  const compatible = mode === 'pdf-ligatures';
  const needle = compatible ? project(query) : query;
  const haystack = compatible ? project(text) : text;
  if (needle.length > haystack.length) return [];

  // Zero is an invalid interior; valid entries store original offset + 1.
  // for-of visits an astral pair once, so its interior cannot become an endpoint.
  let boundaries: Uint32Array | undefined;
  if (compatible) {
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
