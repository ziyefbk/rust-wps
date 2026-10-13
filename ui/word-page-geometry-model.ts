// Pure page contract shared by page and paragraph transaction validators.
export const pageGeometryFields = ['widthTwips', 'heightTwips', 'marginTopTwips', 'marginRightTwips', 'marginBottomTwips', 'marginLeftTwips'] as const;
export const pageMarginFields = ['marginTopTwips', 'marginRightTwips', 'marginBottomTwips', 'marginLeftTwips'] as const;
export type PageGeometry = Record<typeof pageGeometryFields[number], number>;
export type PagePreset = 'A4' | 'Letter' | 'A5';
export type PageOrientation = 'portrait' | 'landscape';
export const pagePresets: Record<PagePreset, readonly [number, number]> = {A4: [11906, 16838], Letter: [12240, 15840], A5: [8391, 11906]};
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export function pagePreset(value: Pick<PageGeometry, 'widthTwips' | 'heightTwips'>): PagePreset | null {
  for (const [name, [width, height]] of Object.entries(pagePresets)) {
    if (Math.abs(value.widthTwips - width) <= 2 && Math.abs(value.heightTwips - height) <= 2
      || Math.abs(value.widthTwips - height) <= 2 && Math.abs(value.heightTwips - width) <= 2) return name as PagePreset;
  }
  return null;
}
export function isPageGeometry(value: unknown): value is PageGeometry {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const page = value as PageGeometry;
  return Object.keys(page).length === pageGeometryFields.length && pageGeometryFields.every(field => own(page, field) && Number.isInteger(page[field]))
    && pagePreset(page) !== null && pageMarginFields.every(field => page[field] >= 0 && page[field] <= 4320)
    && page.widthTwips - page.marginLeftTwips - page.marginRightTwips >= 1440
    && page.heightTwips - page.marginTopTwips - page.marginBottomTwips >= 1440;
}
