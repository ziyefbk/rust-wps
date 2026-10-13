import { cpSync, mkdirSync } from 'node:fs';
mkdirSync('ui/public/pdf', { recursive: true });
for (const name of ['cmaps', 'standard_fonts', 'wasm', 'iccs']) {
  cpSync(`node_modules/pdfjs-dist/${name}`, `ui/public/pdf/${name}`, { recursive: true });
}
