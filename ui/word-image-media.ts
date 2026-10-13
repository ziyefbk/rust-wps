// A bounded metadata scan, not a pixel decoder. Explicit geometry excludes
// orientation and animation metadata until browser and DOCX consumers agree.
const mediaError = '仅支持嵌入的 PNG/JPEG 图片；每张最多 8 MiB，合计最多 20 MiB、100 张。';
const displayMediaError = '这张图片包含不支持的方向或动画元数据，或图片结构无效/源像素超限；暂不能设置显示尺寸，原始图片不会改写。';
interface WordImageMedia {bytes: number; displayError: string | null; width: number | null; height: number | null}
const cache = new Map<string, WordImageMedia>();
let cacheBytes = 0;
export function supportedImagePixelDimensions(width: number, height: number): boolean {
  return Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0
    && width <= 20000 && height <= 20000 && width * height <= 40000000;
}
function metadataError(bytes: Uint8Array, mime: string): string | null {
  const fail = () => displayMediaError;
  if (mime === 'png') {
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    if (bytes.length < 33 || signature.some((value, index) => bytes[index] !== value)) return fail();
    const integer = (offset: number) => bytes[offset] * 16777216 + bytes[offset + 1] * 65536 + bytes[offset + 2] * 256 + bytes[offset + 3];
    if (integer(8) !== 13 || String.fromCharCode(...bytes.subarray(12, 16)) !== 'IHDR'
      || !supportedImagePixelDimensions(integer(16), integer(20))) return fail();
    let position = 8;
    while (position + 12 <= bytes.length) {
      const length = bytes[position] * 16777216 + bytes[position + 1] * 65536 + bytes[position + 2] * 256 + bytes[position + 3];
      if (length > bytes.length - position - 12) return fail();
      const tag = String.fromCharCode(...bytes.subarray(position + 4, position + 8));
      if (position !== 8 && tag === 'IHDR' || ['eXIf', 'acTL', 'fcTL', 'fdAT'].includes(tag)) return fail();
      position += length + 12;
      if (tag === 'IEND') return length === 0 && position === bytes.length ? null : fail();
    }
    return fail();
  }
  if (bytes.length < 4 || bytes[0] !== 255 || bytes[1] !== 216) return fail();
  let position = 2, entropy = false, hasDimensions = false;
  while (position < bytes.length) {
    if (entropy) while (position < bytes.length && bytes[position] !== 255) position++;
    if (position >= bytes.length || bytes[position] !== 255) return fail();
    while (position < bytes.length && bytes[position] === 255) position++;
    if (position >= bytes.length) return fail();
    const marker = bytes[position++];
    if (entropy && (marker === 0 || marker >= 208 && marker <= 215)) continue;
    if (marker === 225) return fail(); // APP1 includes EXIF and XMP, even after SOS.
    if (marker === 217) return position === bytes.length && hasDimensions ? null : fail();
    if (marker === 0 || marker === 1 || marker >= 208 && marker <= 216) return fail();
    if (position + 2 > bytes.length) return fail();
    const length = bytes[position] * 256 + bytes[position + 1];
    if (length < 2 || length > bytes.length - position) return fail();
    if ([192, 193, 194, 195, 197, 198, 199, 201, 202, 203, 205, 206, 207].includes(marker)) {
      if (length < 8 || !supportedImagePixelDimensions(bytes[position + 5] * 256 + bytes[position + 6], bytes[position + 3] * 256 + bytes[position + 4])) return fail();
      hasDimensions = true;
    }
    position += length;
    entropy = marker === 218;
  }
  return fail();
}
// Read the same bounded intrinsic frame used by the historical DOCX exporter.
// This does not apply EXIF orientation; explicit page geometry requires the
// strict metadata check above to pass before these dimensions are consumed.
function intrinsicFrame(bytes: Uint8Array, mime: string): {width: number | null; height: number | null} {
  if (mime === 'png' && bytes.length >= 24) {
    const integer = (offset: number) => bytes[offset] * 16777216 + bytes[offset + 1] * 65536 + bytes[offset + 2] * 256 + bytes[offset + 3];
    return {width: integer(16), height: integer(20)};
  }
  let position = 2;
  while (position + 4 <= bytes.length && bytes[position] === 255) {
    while (bytes[position] === 255) position++;
    const marker = bytes[position++], length = bytes[position] * 256 + bytes[position + 1];
    if (length < 2 || position + length > bytes.length) break;
    if ([192, 193, 194, 195, 197, 198, 199, 201, 202, 203, 205, 206, 207].includes(marker) && length >= 8) {
      return {width: bytes[position + 5] * 256 + bytes[position + 6], height: bytes[position + 3] * 256 + bytes[position + 4]};
    }
    if (marker === 218 || marker === 217) break;
    position += length;
  }
  return {width: null, height: null};
}
export function inspectWordImageMedia(src: unknown): WordImageMedia {
  if (typeof src !== 'string') throw new Error(mediaError);
  const cached = cache.get(src);
  if (cached) return cached;
  const prefix = /^data:image\/(png|jpeg);base64,/.exec(src);
  if (!prefix) throw new Error(mediaError);
  const encoded = src.slice(prefix[0].length);
  const padding = encoded.endsWith('==') ? 2 : encoded.endsWith('=') ? 1 : 0;
  if (!encoded.length || encoded.length % 4 || encoded.length > Math.ceil(8 * 1024 * 1024 / 3) * 4
    || /[^A-Za-z0-9+/=]/.test(encoded) || encoded.slice(0, encoded.length - padding).includes('=')) throw new Error(mediaError);
  const size = encoded.length / 4 * 3 - padding;
  if (size > 8 * 1024 * 1024) throw new Error(mediaError);
  let binary: string;
  try {binary = atob(encoded);} catch {throw new Error(mediaError);}
  if (btoa(binary) !== encoded) throw new Error('图片 base64 编码无效。');
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
  const result = {bytes: size, displayError: metadataError(bytes, prefix[1]), ...intrinsicFrame(bytes, prefix[1])};
  // Retain only small bounded metadata, never decoded image bytes. Bound keys
  // as well: closed/replaced documents cannot leave unbounded base64 in cache.
  while (cache.size >= 100 || cacheBytes + src.length > 32 * 1024 * 1024) {
    const first = cache.keys().next().value;
    if (first === undefined) break;
    cacheBytes -= first.length; cache.delete(first);
  }
  cache.set(src, result); cacheBytes += src.length;
  return result;
}
export function validateImageDisplayMedia(src: unknown): void {
  const result = inspectWordImageMedia(src);
  if (result.displayError) throw new Error(result.displayError);
}
