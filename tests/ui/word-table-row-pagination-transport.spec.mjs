import {test, expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
import {DOMParser as ModelDOMParser, DOMSerializer, Slice, Fragment} from '@tiptap/pm/model';
import {TextSelection} from '@tiptap/pm/state';
import {undoDepth, undo} from '@tiptap/pm/history';
import {rows, tables, schema, p, table, doc, cells, flags, snapshot, stateAt, fullEditor, paste} from './word-table-row-pagination-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const {WordTableRowPaginationControls} = await loadTsModule('./ui/word-table-row-pagination-controls.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
// Small DOM transport double, not a browser HTML parser or layout engine.
class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; childNodes = []; dataset = {}; listeners = new Map(); value = ''; disabled = false; textContent = '';
  constructor(tag = 'div', attrs = {}) {this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.style = new Proxy({length: 0, setProperty(key, value) {this[key] = value;}}, {get: (obj, key) => {
    if (key in obj) return obj[key]; const property = String(key).replace(/[A-Z]/g, ch => `-${ch.toLowerCase()}`);
    return (this.attrs.style ?? '').split(';').map(part => part.split(':')).find(([name]) => name.trim() === property)?.[1]?.trim() ?? '';
  }}); this.classList = {remove() {}};}
  get children() {return this.childNodes.filter(node => node.nodeType === 1);}
  get parentElement() {return this.parentNode?.nodeType === 1 ? this.parentNode : null;}
  get firstChild() {return this.childNodes[0] ?? null;}
  get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}
  getAttribute(name) {return this.attrs[name] ?? null;} hasAttribute(name) {return name in this.attrs;}
  setAttribute(name, value) {this.attrs[name] = String(value);} removeAttribute(name) {delete this.attrs[name];}
  appendChild(child) {child.parentNode = this; this.childNodes.push(child); return child;}
  replaceChildren(...children) {this.childNodes = []; children.forEach(child => this.appendChild(child));}
  matches(selector) {return selector.split(',').some(part => {const match = /^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(part.trim()); return !!match && (!match[1] || this.tagName.toLowerCase() === match[1]) && (!match[2] || this.hasAttribute(match[2]));});}
  closest(selector) {for (let node = this; node; node = node.parentElement) if (node.matches(selector)) return node; return null;}
  querySelector(selector) {return this.querySelectorAll(selector)[0] ?? null;}
  querySelectorAll(selector) {const result = []; const visit = node => {for (const child of node.children) {if (selector === 'colgroup > col' ? child.tagName === 'COL' && child.parentElement?.tagName === 'COLGROUP' : child.matches(selector)) result.push(child); visit(child);}}; visit(this); return result;}
  cloneNode(deep = true) {const result = new Element(this.tagName, this.attrs); if (deep) for (const child of this.childNodes) result.appendChild(child.nodeType === 1 ? child.cloneNode(true) : dom.createTextNode(child.nodeValue)); return result;}
  addEventListener(type, callback) {this.listeners.set(type, [...this.listeners.get(type) ?? [], callback]);}
  emit(type) {for (const callback of this.listeners.get(type) ?? []) callback({preventDefault() {}});}
}
const dom = {createElement: tag => new Element(tag), createElementNS: (_, tag) => new Element(tag), createDocumentFragment: () => new Element('fragment'),
  createTextNode: value => ({nodeType: 3, nodeValue: value, get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}})};
const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: dom});
const parse = element => {const root = new Element(); root.appendChild(element); return ModelDOMParser.fromSchema(schema).parse(root);};
const wrapped = element => {const root = new Element('body'); root.appendChild(element); root.body = root; return root;};
function parserResult(root, run) {const previous = globalThis.DOMParser; globalThis.DOMParser = class {parseFromString() {return root;}}; try {run();} finally {globalThis.DOMParser = previous;}}

test('real schema DOM serialization parsing retains false true absence height header and draft equality', () => {
  let state = stateAt(doc()); state = state.apply(tables.createTableVerticalTransaction(state, {rowMinHeightTwips: 0, cellVerticalAlign: 'center'}));
  const tableNode = state.doc.firstChild, element = serialize(tableNode), parsed = parse(element);
  expect(parsed.firstChild.eq(tableNode)).toBe(true); expect(snapshot(parsed)).toEqual(snapshot(state.doc));
  expect(element.querySelectorAll('tr').map(row => row.getAttribute('data-row-cant-split'))).toEqual([null, 'false', 'true']);
  expect(element.querySelectorAll('tr')[1].attrs).not.toHaveProperty('rowCantSplit'); expect(element.querySelectorAll('tr')[1].getAttribute('style')).toBeNull();
});
test('invalid metadata remains invalid after schema parse and writes no history', () => {
  for (const value of ['', 'FALSE', '0', '1', 'false ', ' true', 'null']) {
    const element = serialize(table([null])), row = element.querySelector('tr'); row.setAttribute('data-row-cant-split', value);
    const parsed = parse(element); expect(parsed.firstChild.firstChild.attrs.rowCantSplit).toBe('invalid-row-pagination');
    expect(() => tables.validateTableWidthsDocument(snapshot(parsed))).toThrow();
    const state = stateAt(); expect(state.applyTransaction(state.tr.replaceWith(0, state.doc.content.size, parsed.content)).transactions).toEqual([]); expect(undoDepth(state)).toBe(0);
  }
});
test('raw wrong-node orphan duplicate and deep owned HTML evidence cannot disappear', () => {
  for (const tag of ['p', 'td', 'th', 'table', 'span', 'img']) {
    const root = wrapped(new Element(tag, {'data-row-cant-split': 'false'})); expect(rows.validTableRowPaginationHTML(root)).toBe(false);
    parserResult(root, () => {expect(rows.preflightTableRowPaginationHTML(`<${tag} data-row-cant-split="false"></${tag}>`)).toContain('data-row-cant-split="invalid"');});
  }
  const clean = wrapped(serialize(table([false])));
  parserResult(clean, () => {
    expect(rows.isValidTableRowPaginationHTML('<table><tr data-row-cant-split="false"><td>x</td><td>y</td></tr></table>')).toBe(true);
    expect(rows.isValidTableRowPaginationHTML('<table><tr data-row-cant-split="false" data-row-cant-split="true"><td>x</td></tr></table>')).toBe(false);
  });
  parserResult(wrapped(new Element('p')), () => expect(rows.isValidTableRowPaginationHTML('<tr data-row-cant-split="false"><td>x</td></tr>')).toBe(false));
  const deep = wrapped(serialize(table([false]))); let end = deep.querySelector('td,th'); for (let i = 0; i < 34; i++) end = end.appendChild(new Element());
  parserResult(deep, () => expect(rows.isValidTableRowPaginationHTML('<table><tr data-row-cant-split="false"></tr></table>')).toBe(false));
});
test('raw nested listed malformed rectangular and merged table contexts reject owned row metadata', () => {
  for (const change of [element => {element.querySelector('tr').appendChild(new Element('td'));},
    element => element.querySelector('td').setAttribute('colspan', '2'),
    element => {element.querySelector('td,th').appendChild(serialize(table([null])));},
    element => {const list = new Element('li'); list.appendChild(element); return list;}]) {
    const element = serialize(table([false, null])), replacement = change(element), root = wrapped(replacement ?? element);
    expect(rows.validTableRowPaginationHTML(root)).toBe(false);
  }
});
test('foreign page-break-inside break-inside and MSO styles never become authoritative', () => {
  const element = serialize(table([null])), row = element.querySelector('tr'); row.setAttribute('style', 'page-break-inside:avoid;break-inside:avoid;mso-row-margin-top:0');
  element.setAttribute('style', 'page-break-inside:avoid'); row.firstChild.setAttribute('style', 'page-break-inside:avoid');
  expect(flags(parse(element))).toEqual([null]); expect(rows.isValidTableRowPaginationHTML('<tr style="break-inside:avoid">foreign</tr>')).toBe(true);
});
test('full application paste order rejects invalid raw wrong-node metadata before tableEditing drops it', () => {
  const {editor, errors} = fullEditor(doc([table([true])])), original = editor.state.doc; editor.commands.setTextSelection(cells(original)[0] + 2);
  const source = editor.schema.nodeFromJSON(table([null]).toJSON()), root = wrapped(new Element('img', {'data-row-cant-split': 'false'}));
  try {parserResult(root, () => {expect(paste(editor, new Slice(Fragment.from(source), 0, 0), {clipboardData: {getData: () => '<img data-row-cant-split="false">'}})).toBe(true);});
    expect(editor.state.doc).toBe(original); expect(undoDepth(editor.state)).toBe(0); expect(errors.at(-1)).toContain('行内不分页');
  } finally {editor.destroy();}
});
function controlsHarness(run) {
  const previous = globalThis.document, elements = new Map(), element = id => {if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id);};
  globalThis.document = {getElementById: element}; let state = stateAt(), writes = 0, locked = false, composing = false, editable = true, pendingSelection;
  let controls, editor = {get state() {return state;}, get isEditable() {return editable;}, view: {get composing() {return composing;}, focus() {}, dispatch(tr) {
    const result = state.applyTransaction(tr); state = result.state; if (result.transactions.some(item => item.docChanged)) writes++; controls.update();
  }}};
  controls = new WordTableRowPaginationControls({editor: () => editor, locked: () => locked, notice() {}, syncSelection() {if (pendingSelection) {state = state.apply(state.tr.setSelection(pendingSelection)); pendingSelection = null;}}}); controls.update();
  const select = element('edit-row-cant-split');
  try {run({controls, select, get state() {return state;}, get writes() {return writes;}, change(value, event = 'pointerdown') {if (event) select.emit(event); select.value = value; select.emit('change');},
    documentChange() {state = state.apply(state.tr.insertText('x')); controls.update();}, selectionChange() {state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, cells(state.doc)[2] + 2))); controls.update();},
    pending() {pendingSelection = TextSelection.create(state.doc, cells(state.doc)[4] + 2);}, lock(value) {locked = value; controls.update();}, composing(value) {composing = value; controls.update();}, editable(value) {editable = value; controls.update();},
    swap() {editor = Object.defineProperties({}, Object.getOwnPropertyDescriptors(editor)); controls.reset(); controls.update();}, undo() {undo(state, tr => {state = state.apply(tr); controls.update();});}});}
  finally {globalThis.document = previous;}
}
test('control keyboard focus and pointer capture preserve explicit false current row and one undo', () => {
  controlsHarness(h => {h.change('false', 'focus'); expect(flags(h.state.doc)).toEqual([false, false, true]); expect(h.writes).toBe(1); expect(h.select.dataset.rowPaginationState).toBe('explicit');
    h.undo(); expect(flags(h.state.doc)).toEqual([null, false, true]); h.pending(); h.change('false'); expect(flags(h.state.doc)).toEqual([null, false, false]); h.change('default'); expect(flags(h.state.doc)).toEqual([null, false, null]);});
});
test('control pending selection doc owner token locks IME and disabled noneditable changes are inert', () => {
  for (const change of ['documentChange', 'selectionChange', 'swap']) controlsHarness(h => {h.select.emit('focus'); h[change](); const before = h.state.doc; h.change('true', null); expect(h.state.doc).toBe(before); expect(h.writes).toBe(0);});
  for (const mode of ['lock', 'composing', 'editable']) controlsHarness(h => {h.select.emit('focus'); h[mode](mode !== 'editable'); expect(h.select.disabled).toBe(true); h.change('true', null); expect(h.writes).toBe(0);});
  controlsHarness(h => {h.change('default'); h.change('mixed'); h.change('invalid'); expect(h.writes).toBe(0); expect(undoDepth(h.state)).toBe(0);});
});
test('print clone snapshots explicit false true absence before waits and later reset does not alter prior clone', async () => {
  const previous = globalThis.document, host = new Element(); let ready;
  globalThis.document = {...dom, getElementById: () => host, fonts: {ready: new Promise(resolve => {ready = resolve;})}};
  try {
    const source = new Element(); source.appendChild(serialize(table([false, true, null])));
    const pending = preparePrintSnapshot(source); for (const row of source.querySelectorAll('tr')) row.removeAttribute('data-row-cant-split'); ready(); await pending;
    expect(host.querySelectorAll('tr').map(row => row.getAttribute('data-row-cant-split'))).toEqual(['false', 'true', null]);
    await preparePrintSnapshot(source); expect(host.querySelectorAll('tr').map(row => row.getAttribute('data-row-cant-split'))).toEqual([null, null, null]);
  } finally {globalThis.document = previous;}
});
test('authoring registration raw preflight toolbar and historical absent-row print CSS contract stay explicit', () => {
  const css = readFileSync('ui/style.css', 'utf8'), html = readFileSync('ui/index.html', 'utf8'), authoring = readFileSync('ui/authoring.ts', 'utf8');
  expect(css).toContain('#print-document tr { break-inside:avoid; }'); expect(css).toContain('#print-document tr[data-row-cant-split="false"] { break-inside:auto; }');
  expect(css).toContain('#print-document tr[data-row-cant-split="true"] { break-inside:avoid; }');
  expect(html).toContain('id="edit-row-cant-split"'); expect(html).toContain('超出一页的行仍可能分页'); expect(html).toContain('不同查看器的分页可能不同');
  expect(authoring.indexOf('preflightTableRowPaginationHTML(html)')).toBeLessThan(authoring.indexOf("parsed.querySelectorAll('img')"));
  expect(authoring).toContain('TableRowPagination.configure'); expect(authoring).toContain('tableRowPaginationControls?.reset()');
});

test('installed extension-manager HTML composition invokes configured row preflight before schema parse', () => {
  const {editor} = fullEditor(doc([p('')]));
  const original = globalThis.DOMParser;
  globalThis.DOMParser = class {parseFromString(html) {
    if (html.includes('data-row-cant-split="invalid"')) {const root = wrapped(serialize(table([null]))); root.querySelector('tr').setAttribute('data-row-cant-split', 'invalid'); return root;}
    return wrapped(new Element('img', {'data-row-cant-split': 'false'}));
  }};
  try {
    let baseCalled = false;
    const transform = editor.extensionManager.transformPastedHTML(html => {baseCalled = true; return html;});
    const result = transform('<img data-row-cant-split="false">', editor.view);
    expect(baseCalled).toBe(true); expect(result).toContain('data-row-cant-split="invalid"');
  } finally {globalThis.DOMParser = original; editor.destroy();}
});
test('early handled row paste preserves older raw table metadata refusals', () => {
  for (const [field, value] of [['data-row-min-height-twips', '0'], ['data-cell-vertical-align', 'top'], ['data-cell-fill-rgb', 'FFFFFF'], ['data-table-cell-margins-twips', '0,0']]) {
    const {editor, errors} = fullEditor(doc([table([true])])), original = editor.state.doc; editor.commands.setTextSelection(cells(original)[0] + 2);
    const source = editor.schema.nodeFromJSON(table([null]).toJSON()), root = wrapped(new Element('p', {[field]: value}));
    try {parserResult(root, () => expect(paste(editor, new Slice(Fragment.from(source), 0, 0), {clipboardData: {getData: () => `<p ${field}="${value}">x</p>`}})).toBe(true));
      expect(editor.state.doc).toBe(original); expect(errors).toHaveLength(1); expect(undoDepth(editor.state)).toBe(0);
    } finally {editor.destroy();}
  }
});

test('ordinary raw text comments quoted values and rawtext spelling are exact no-owned-attribute no-ops', () => {
  const plain = [
    '<p>data-row-cant-split=true</p>', '<p>data-row-cant-split = "false"</p>', '<!-- data-row-cant-split="true" --><p>plain</p>',
    '<p title="data-row-cant-split=true">plain</p>', `<p title='data-row-cant-split="false"'>plain</p>`,
    `<script>const x = '<tr data-row-cant-split="true">';</script><p>plain</p>`,
    '<textarea><tr data-row-cant-split="false"></textarea>', '<style>/* data-row-cant-split=true */</style>',
  ];
  parserResult(wrapped(new Element('p')), () => {for (const html of plain) {
    expect(rows.countRowPaginationHTMLAttributes(html)).toBe(0); expect(rows.isValidTableRowPaginationHTML(html)).toBe(true); expect(rows.preflightTableRowPaginationHTML(html)).toBe(html);
  }});
  expect(rows.countRowPaginationHTMLAttributes('<tr data-row-cant-split="false" DATA-ROW-CANT-SPLIT="true">')).toBe(2);
  expect(rows.countRowPaginationHTMLAttributes('<tr title="data-row-cant-split=true" data-row-cant-split="false">')).toBe(1);
  expect(rows.countRowPaginationHTMLAttributes('<tr data-row-cant-split>')).toBe(1);
});

test('deep and oversized ordinary HTML with literal owned spelling remains unchanged when it owns no attrs', () => {
  const deep = wrapped(new Element('p')); let end = deep.firstChild; for (let i = 0; i < 40; i++) end = end.appendChild(new Element());
  const html = '<div>'.repeat(40) + '<p>data-row-cant-split=true</p>' + '</div>'.repeat(40);
  parserResult(deep, () => {expect(rows.isValidTableRowPaginationHTML(html)).toBe(true); expect(rows.preflightTableRowPaginationHTML(html)).toBe(html);});
  const large = '<p>' + 'x'.repeat(32 * 1024 * 1024) + 'data-row-cant-split=true</p>';
  expect(rows.isValidTableRowPaginationHTML(large)).toBe(true);
});

// Release 38: exercise the actual schema serializer/parser on a DOM double,
// never a browser parser, system clipboard or computed layout claim.
test('preferred widths survive owned DOM transport with independent row properties', () => {
  const field='autoPreferredColumnWidthsTwips', source=table([false,true],{[field]:[1870,1870],tableIndentTwips:0});
  const element=serialize(source), parsed=parse(element);
  expect(parsed.firstChild.eq(source)).toBe(true);
  expect(element.getAttribute('data-auto-preferred-column-widths-twips')).toBe('1870,1870');
  expect(element.style.cssText).toContain('table-layout: auto');
  expect(element.querySelectorAll('col').map(col=>col.style.cssText)).toEqual(['width: 124.66666666666667px','width: 124.66666666666667px']);
  for(const bad of ['', '0,1870', '1870,', 'null']) {
    const malformed=serialize(source);malformed.setAttribute('data-auto-preferred-column-widths-twips',bad);
    expect(parse(malformed).firstChild.attrs[field]).toBe('invalid-auto-preferred-widths');
  }
});
test('preferred widths raw metadata is checked before row-pagination handled paste', () => {
  const source=serialize(table([false,true],{autoPreferredColumnWidthsTwips:[1870,1870]}));
  source.setAttribute('data-auto-preferred-column-widths-twips','bad');
  const root=wrapped(source), errors=[];const state=stateAt(doc(),0,errors);
  parserResult(root,()=>{
    const handled=rows.tableRowPaginationGuard(error=>errors.push(error)).props.handlePaste({state,dispatch(){throw new Error('must not dispatch');}},
      {clipboardData:{getData:()=>'<table data-auto-preferred-column-widths-twips="bad"></table>'}},new Slice(Fragment.from(table([false,true])),0,0));
    expect(handled).toBe(true);expect(errors.join('')).toContain('自动首选列宽');
  });
});
