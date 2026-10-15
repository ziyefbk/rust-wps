import {test, expect} from '@playwright/test';
import {getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {TableKit} from '@tiptap/extension-table';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history, undo, redo, closeHistory} from '@tiptap/pm/history';
import {loadTsModule} from './load-ts.mjs';
const t = await loadTsModule('./ui/word-table-widths.ts');
const a = await loadTsModule('./ui/word-table-auto-preferred.ts');
const g = await loadTsModule('./ui/word-page-geometry.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const schema = getSchema([StarterKit, TableKit.configure({table: false}), t.WordTable, g.WordPageGeometry]);
const page = {widthTwips:12240,heightTwips:15840,marginLeftTwips:1440,marginRightTwips:1440,marginTopTwips:1440,marginBottomTwips:1440};
const field = a.autoPreferredAttribute;
const table = (attrs = {}, columns = 3) => schema.node('table', {[field]:[1000,2000,3000], ...attrs}, [0,1].map(row => schema.node('tableRow', {rowCantSplit: row === 1 ? true : null},
  Array.from({length:columns}, (_, col) => schema.node(row ? 'tableCell' : 'tableHeader', {cellFillMode:'none'}, schema.node('paragraph', null, schema.text(`${row}:${col}`)))))));
const doc = (attrs = {}, geometry = page) => schema.node('doc', {pageGeometry:geometry}, [table(attrs)]);
const snap = node => g.omitDefaultPageGeometry(t.omitDefaultTableWidths(node.toJSON()));
const state = document => EditorState.create({doc:document, selection:TextSelection.create(document,4),plugins:[history(),t.tableWidthGuard()]});
const vector = state => state.doc.firstChild.attrs[field];
for (const value of [null,[],[0],[-1],[1.5],['1'],[Infinity],Array(51).fill(1),[31680,1],{},'1']) test(`reject malformed raw preference ${JSON.stringify(value)}`, () => {
  const raw = snap(doc()); raw.content[0].attrs[field] = value;
  expect(() => t.validateTableWidthsDocument(raw)).toThrow();
});
test('canonical schema draft round trip preserves preferences and independent flags', () => {
  const raw = snap(doc({tableIndentTwips:0})); t.validateTableWidthsDocument(raw); g.validatePageGeometryDocument(raw);
  const restored = schema.nodeFromJSON(JSON.parse(JSON.stringify(raw))); expect(snap(restored)).toEqual(raw);
  expect(raw.content[0].attrs.columnWidthsTwips).toBeUndefined(); expect(raw.content[0].content[1].attrs.rowCantSplit).toBe(true);
  expect(raw.content[0].content[0].content[0].attrs.cellFillMode).toBe('none');
});
test('strict conflict, top-level, merged, mismatched and missing page cases', () => {
  for (const attrs of [{columnWidthsTwips:[1000,2000,3000]},{tableAlign:'center'},{tableAlign:'right'},{tableIndentTwips:1},{tableCellMarginsTwips:{left:0,right:0}}]) {
    expect(() => t.validateTableWidthsDocument(snap(doc(attrs)))).toThrow();
  }
  expect(() => t.validateTableWidthsDocument(snap(doc({},null)))).toThrow();
  for (const change of [raw => raw.content[0].content[0].content.pop(), raw => raw.content[0].content[0].content[0].attrs.colspan=2,
    raw => raw.content[0].content[0].content[0].content.push(structuredClone(raw.content[0])),
    raw => raw.attrs[field]=[1], raw => raw.content[0].content[0].content[0].content[0].attrs={indentLeftTwips:0}]) {
    const raw=snap(doc()); change(raw); expect(() => t.validateTableWidthsDocument(raw)).toThrow();
  }
});
test('preferred fixed conversion seeds all columns and reset clears preferred even with fixed null', () => {
  const before = state(doc()); const fixed = t.createTableColumnWidthTransaction(before,1200);
  expect(fixed.doc.firstChild.attrs.columnWidthsTwips).toEqual([1200,2000,3000]); expect(fixed.doc.firstChild.attrs[field]).toBeNull();
  const reset = t.createTableColumnWidthTransaction(before,null); expect(reset).not.toBeNull(); expect(reset.doc.firstChild.attrs[field]).toBeNull();
  expect(reset.doc.firstChild.attrs.columnWidthsTwips).toBeNull(); expect(vector(before)).toEqual([1000,2000,3000]);
});
test('conversion/reset/structural changes are undoable and redoable as one edit', () => {
  for (const edit of [s=>t.createTableColumnWidthTransaction(s,1200),s=>t.createTableColumnWidthTransaction(s,null),s=>t.createTableStructureTransaction(s,'addColumnAfter')]) {
    let current=state(doc()); const original=snap(current.doc); current=current.apply(closeHistory(edit(current))); const after=snap(current.doc);
    undo(current,tr=>current=current.apply(tr)); expect(snap(current.doc)).toEqual(original);
    redo(current,tr=>current=current.apply(tr)); expect(snap(current.doc)).toEqual(after);
  }
});
test('rows retain preferences and inserted/deleted columns change matching entries', () => {
  for (const action of ['addRowBefore','addRowAfter','deleteRow']) {
    const tr=t.createTableStructureTransaction(state(doc()),action); expect(tr.doc.firstChild.attrs[field]).toEqual([1000,2000,3000]);
  }
  for (const action of ['addColumnBefore','addColumnAfter']) expect(t.createTableStructureTransaction(state(doc()),action).doc.firstChild.attrs[field]).toEqual([1000,1000,2000,3000]);
  expect(t.createTableStructureTransaction(state(doc()),'deleteColumn').doc.firstChild.attrs[field]).toEqual([2000,3000]);
});
test('column insertion page overflow and stale raw structural steps reject atomically', () => {
  const before=state(doc({[field]:[3000,3000,3000]})); expect(()=>t.createTableStructureTransaction(before,'addColumnAfter')).toThrow();
  expect(vector(before)).toEqual([3000,3000,3000]);
  const invalid=before.tr.setNodeMarkup(0,undefined,{...before.doc.firstChild.attrs,[field]:[1]});
  expect(before.applyTransaction(invalid).transactions).toHaveLength(0);
});
test('page removal/shrink reject; nominal budget uses preferred sum rather than legacy 9000', () => {
  const small={...page,marginLeftTwips:3000,marginRightTwips:3000}; const before=state(doc());
  expect(()=>g.createPageGeometryTransaction(before,null)).toThrow();
  expect(()=>g.createPageGeometryTransaction(before,{...page,marginLeftTwips:4000,marginRightTwips:4000})).toThrow();
  expect(g.createPageGeometryTransaction(state(doc({tableIndentTwips:0})),small)).not.toBeNull();
  expect(g.pageAutoTableDOM({[field]:[1000]},page)).toEqual({});
});
test('position/margins incompatible edits explain reset or convert first', () => {
  const before=state(doc());
  for (const patch of [{tableAlign:'center'},{tableIndentTwips:1}]) expect(()=>t.createTablePositionTransaction(before,patch)).toThrow(a.autoPreferredError);
  expect(()=>t.createTableCellMarginsTransaction(before,{left:0,right:0})).toThrow(a.autoPreferredError);
  expect(t.createTablePositionTransaction(before,{tableIndentTwips:0}).doc.firstChild.attrs[field]).toEqual([1000,2000,3000]);
});
test('text edits and cell/header markup retain hints and no-fill/row pagination', () => {
  let current=state(doc()); current=current.apply(current.tr.insertText('new',4));
  const cell=current.doc.nodeAt(2); current=current.apply(current.tr.setNodeMarkup(2,schema.nodes.tableCell,cell.attrs));
  expect(vector(current)).toEqual([1000,2000,3000]); expect(current.doc.firstChild.lastChild.attrs.rowCantSplit).toBe(true);
  expect(current.doc.nodeAt(2).attrs.cellFillMode).toBe('none');
});
test('owned HTML metadata is exact, malformed and conflicting modes rejected', () => {
  expect(a.parseAutoPreferredMetadata('1000,2000,3000')).toEqual([1000,2000,3000]);
  for(const raw of ['','0','1.5','1,','[1]','1, 2','-1']) expect(a.parseAutoPreferredMetadata(raw)).toBeNull();
  const root=(tag,raw,fixed=false)=>({querySelectorAll:()=>[{tagName:tag,getAttribute:()=>raw,hasAttribute:()=>fixed}]});
  expect(a.validAutoPreferredHTML(root('TABLE','1,2'))).toBe(true);
  for(const input of [root('P','1,2'),root('TABLE','x'),root('TABLE','1,2',true)]) expect(a.validAutoPreferredHTML(input)).toBe(false);
  const output=schema.nodes.table.spec.toDOM(table({tableIndentTwips:0})); expect(output[1][a.autoPreferredDataAttribute]).toBe('1000,2000,3000');
  expect(output[1].style).toContain('table-layout: auto'); expect(output[1].style).not.toContain('600px'); expect(output[2][2][1].style).toBe(`width: ${1000/15}px`);
});
class Element {
  constructor(tag='div'){this.tag=tag;} children=[];attrs={};parentNode=null;dataset={};
  style={width:'',minWidth:'',tableLayout:'',marginLeft:'',marginRight:'',setProperty(k,v){this[k]=v;}};
  ownerDocument={createElement:tag=>new Element(tag)}; classList={remove(){}};
  get firstChild(){return this.children[0]??null;} get nextSibling(){return this.parentNode?.children[this.parentNode.children.indexOf(this)+1]??null;}
  appendChild(child){child.parentNode=this;this.children.push(child);return child;}
  removeChild(child){this.children.splice(this.children.indexOf(child),1);child.parentNode=null;}
  replaceChildren(...children){this.children=[];children.forEach(child=>this.appendChild(child));}
  setAttribute(k,v){this.attrs[k]=v;} removeAttribute(k){delete this.attrs[k];} querySelectorAll(){return [];}
  cloneNode(deep){const copy=new Element(this.tag);copy.attrs={...this.attrs};Object.assign(copy.style,this.style);if(deep)this.children.forEach(c=>copy.appendChild(c.cloneNode(true)));return copy;}
}
test('TableView transitions clear total widths/minimums and print clone keeps hints', async () => {
  const old=globalThis.document,host=new Element();globalThis.document={createElement:tag=>new Element(tag),getElementById:()=>host,fonts:{ready:Promise.resolve()}};
  try {
    const view=new t.FixedTableView(table({tableIndentTwips:0}),25);
    for(let i=0;i<2;i++) {
      expect(view.table.style).toMatchObject({width:'auto',minWidth:'0',tableLayout:'auto',marginLeft:'-5.4pt'});
      expect(view.colgroup.children.map(c=>c.style.width)).toEqual([1000,2000,3000].map(w=>`${w/15}px`));
      await preparePrintSnapshot(view.table); const printed=host.firstChild; expect(printed.attrs[a.autoPreferredDataAttribute]).toBe('1000,2000,3000');
      view.update(table({[field]:null,columnWidthsTwips:[1200,2000,3000]}));expect(view.table.style.width).toBe(`${6200/15}px`);expect(view.table.attrs[a.autoPreferredDataAttribute]).toBeUndefined();
      expect(printed.style.width).toBe('auto');
      view.update(table({[field]:null}));expect(view.table.style).toMatchObject({width:'',minWidth:'75px',tableLayout:''});
      view.update(table({tableIndentTwips:0}));
    }
  } finally {globalThis.document=old;}
});
test('full application schema defaults permit typing/rows/colors/pagination while explicit zero indent rejects', async () => {
  const h=await import('./word-table-row-pagination-harness.mjs');
  const indentation=await loadTsModule('./ui/word-paragraph-indent.ts');
  const document=h.schema.nodeFromJSON(snap(doc())); const errors=[]; let current=h.stateAt(document,0,errors);
  current=current.apply(current.tr.insertText('full',4)); expect(vector(current)).toEqual([1000,2000,3000]);
  for(const transaction of [t.createTableStructureTransaction(current,'addRowAfter'),t.createTableCellColorsTransaction(current,{cellFillMode:'none'}),t.createTableRowPaginationTransaction(current,false)]) {
    if(transaction) expect(current.applyTransaction(transaction).transactions.length).toBeGreaterThan(0);
  }
  expect(errors).toEqual([]); expect(()=>indentation.createParagraphIndentTransaction(current,{indentLeftTwips:0})).toThrow(a.autoPreferredError);
  const {editor,errors:editorErrors}=h.fullEditor(document);
  try {editor.commands.setTextSelection(4); expect(editor.commands.insertContent({type:'text',text:'typed'})).toBe(true);expect(editorErrors).toEqual([]);expect(editor.state.doc.firstChild.attrs[field]).toEqual([1000,2000,3000]);} finally {editor.destroy();}
});
test('partial preferred table paste rejects; closed top-level table and inline context remain bounded', async () => {
  const {Slice,Fragment}=await import('@tiptap/pm/model');
  const before=state(doc()), source=table();
  expect(()=>a.validateAutoPreferredPaste(before,new Slice(Fragment.from(source),1,1))).toThrow();
  expect(()=>a.validateAutoPreferredPaste(before,new Slice(Fragment.from(source),0,0))).toThrow();
  const empty=schema.node('doc',{pageGeometry:page},schema.node('paragraph'));
  const destination=EditorState.create({doc:empty,selection:TextSelection.create(empty,1)});
  const complete=new Slice(Fragment.from(source),0,0);expect(()=>a.validateAutoPreferredPaste(destination,complete)).not.toThrow();
  const pasted=destination.tr.replaceWith(0,2,complete.content);t.validateTableWidthsDocument(snap(pasted.doc));expect(pasted.doc.firstChild.attrs[field]).toEqual([1000,2000,3000]);
});
test('owned duplicate HTML evidence survives parser normalization through both early guards', () => {
  const old=globalThis.DOMParser;
  const root={querySelectorAll:()=>[{tagName:'TABLE',getAttribute:()=> '1000,2000,3000',hasAttribute:()=>false}]};
  globalThis.DOMParser=class {parseFromString(){return root;}};
  try {
    const good=`<table ${a.autoPreferredDataAttribute}="1000,2000,3000"></table>`;
    const bad=`<table ${a.autoPreferredDataAttribute}="1000,2000,3000" ${a.autoPreferredDataAttribute}="1"></table>`;
    expect(a.isValidAutoPreferredHTML(good)).toBe(true);expect(a.isValidAutoPreferredHTML(bad)).toBe(false);
    expect(a.preflightAutoPreferredHTML(bad)).toContain('="invalid"');
  } finally {globalThis.DOMParser=old;}
});
test('misplaced preferred metadata on character marks is rejected before schema erases it', () => {
  const raw=snap(doc());raw.content[0].content[0].content[0].content[0].content[0].marks=[{type:'bold',attrs:{[field]:[1]}}];
  expect(()=>t.validateTableWidthsDocument(raw)).toThrow(a.autoPreferredError);
});
