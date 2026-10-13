import ts from 'typescript';
import { readFileSync } from 'node:fs';
import path from 'node:path';
const urls = new Map();
async function moduleUrl(file) {
  file = path.resolve(file);
  if (urls.has(file)) return urls.get(file);
  const pending = (async () => {
    let js = ts.transpileModule(readFileSync(file, 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
    for (const match of [...js.matchAll(/(from\s*['"])([^'"]+)(['"])/g)]) {
      const dependency = match[2].startsWith('.')
        ? await moduleUrl(path.resolve(path.dirname(file), `${match[2]}.ts`))
        : import.meta.resolve(match[2]);
      js = js.replace(match[0], `${match[1]}${dependency}${match[3]}`);
    }
    return `data:text/javascript;base64,${Buffer.from(js).toString('base64')}`;
  })();
  urls.set(file, pending); return pending;
}
export async function loadTsModule(file) { return import(await moduleUrl(file)); }
