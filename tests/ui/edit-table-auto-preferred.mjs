// Frozen independent import oracle -> full installed editor stack -> exact
// independently specified state changes. Detached view, no native claims.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {schema,snapshot,fullEditor,cells} from './word-table-row-pagination-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const tables=await loadTsModule('./ui/word-table-widths.ts');
const geometry=await loadTsModule('./ui/word-page-geometry.ts');
const [actualPath,oraclePath,outArg]=process.argv.slice(2);
assert(actualPath&&oraclePath&&outArg,'Usage: ACTUAL_IMPORT_JSON FROZEN_EXPECTED_IMPORT_JSON NEW_OUTPUT_DIR');
const actual=JSON.parse(readFileSync(actualPath)),oracle=JSON.parse(readFileSync(oraclePath));
assert.deepEqual(actual,oracle,'Actual full import response, including all risks, differs from frozen independent oracle');
const field='autoPreferredColumnWidthsTwips',out=resolve(outArg);mkdirSync(out,{recursive:false});
const plain=value=>JSON.parse(JSON.stringify(value));
const write=(name,value)=>writeFileSync(join(out,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
// Independently enumerated established schema defaults, not generated from
// candidate CLI/editor output. All source fields/mark arrays remain exact.
function editorExpected(raw) {
  const node=structuredClone(raw);
  if(['paragraph','heading'].includes(node.type)) node.attrs={textAlign:null,...node.attrs};
  if(['tableCell','tableHeader'].includes(node.type)) node.attrs={colspan:1,rowspan:1,align:null,...node.attrs};
  if(node.content)node.content=node.content.map(editorExpected);
  return node;
}
const expectedBase=editorExpected(oracle.document),states=[];
function record(name,editor,expected){
  const actualState=plain(snapshot(editor.state.doc));assert.deepEqual(actualState,expected,`${name}: complete editor model mismatch`);
  const draft=JSON.parse(JSON.stringify({version:1,document:actualState}));
  const reopened=fullEditor(schema.nodeFromJSON(draft.document));
  try{assert.deepEqual(plain(snapshot(reopened.editor.state.doc)),expected,`${name}: draft reopen changed model`);assert.deepEqual(reopened.errors,[]);}finally{reopened.editor.destroy();}
  write(`${name}.json`,actualState);write(`${name}.expected.json`,expected);write(`${name}.draft.json`,draft);states.push(name);
}
function scenario(name,modify,expectedFn){
  const {editor,errors}=fullEditor(schema.nodeFromJSON(actual.document));
  try{
    editor.commands.setTextSelection(cells(editor.state.doc)[0]+2);
    assert.deepEqual(plain(snapshot(editor.state.doc)),expectedBase,'Schema initialization differs from explicit defaults oracle');
    const expected=expectedFn(structuredClone(expectedBase));modify(editor);record(name,editor,expected);
    assert(editor.commands.undo(),`${name}: undo missing`);record(`${name}-undo`,editor,expectedBase);
    assert(editor.commands.redo(),`${name}: redo missing`);record(`${name}-redo`,editor,expected);
    assert.deepEqual(errors,[],`${name}: unexpected diagnostics`);
  }finally{editor.destroy();}
}
const firstTable=d=>d.content.find(n=>n.type==='table');
const original=fullEditor(schema.nodeFromJSON(actual.document));
try{record('retained',original.editor,expectedBase);assert.deepEqual(original.errors,[]);}finally{original.editor.destroy();}
scenario('fixed-conversion',editor=>{const tr=tables.createTableColumnWidthTransaction(editor.state,1800);assert(tr);editor.view.dispatch(tr);},d=>{
  const table=firstTable(d);table.attrs.columnWidthsTwips=[...table.attrs[field]];table.attrs.columnWidthsTwips[0]=1800;delete table.attrs[field];return d;
});
scenario('reset',editor=>{const tr=tables.createTableColumnWidthTransaction(editor.state,null);assert(tr);editor.view.dispatch(tr);},d=>{
  const table=firstTable(d);delete table.attrs[field];for(const row of table.content)for(const cell of row.content)cell.attrs.colwidth=null;return d;
});
scenario('delete-column',editor=>{assert(editor.commands.deleteColumn());},d=>{
  const table=firstTable(d);table.attrs[field].splice(0,1);for(const row of table.content)row.content.splice(0,1);return d;
});
scenario('delete-row',editor=>{assert(editor.commands.deleteRow());},d=>{firstTable(d).content.splice(0,1);return d;});
scenario('text-edit',editor=>{assert(editor.commands.insertContent({type:'text',text:'Preferred width edit'}));},d=>{
  const p=firstTable(d).content[0].content[0].content[0];assert(!p.content?.length,'Text case requires genuine empty first cell');p.content=[{type:'text',text:'Preferred width edit'}];return d;
});
scenario('row-pagination',editor=>{assert(editor.commands.setTableRowCantSplit(false));},d=>{
  firstTable(d).content[0].attrs={...firstTable(d).content[0].attrs,rowCantSplit:false};return d;
});
// Budget refusal must leave full model, selection and history unchanged.
const refused=fullEditor(schema.nodeFromJSON(actual.document));
try{
  const {editor}=refused;editor.commands.setTextSelection(cells(editor.state.doc)[0]+2);
  const state=editor.state;
  assert.throws(()=>tables.createTableStructureTransaction(state,'addColumnBefore'));
  assert.throws(()=>geometry.createPageGeometryTransaction(state,null));
  assert.throws(()=>geometry.createPageGeometryTransaction(state,{...state.doc.attrs.pageGeometry,marginLeftTwips:4320,marginRightTwips:4320}));
  assert.equal(editor.state,state);record('budget-refusal-unchanged',editor,expectedBase);assert.deepEqual(refused.errors,[]);
}finally{refused.editor.destroy();}
write('report.json',{states,oracle:resolve(oraclePath),actualImport:resolve(actualPath),scope:'Actual full detached application extension stack; complete models, independent explicit mutation expectations, drafts, undo/redo and rejected budgets. No native browser/clipboard/print claim.'});
process.stdout.write(JSON.stringify({states})+'\n');
