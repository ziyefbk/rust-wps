// Actual frontend edit/history evidence on an imported model. No browser or
// native clipboard/layout proof. Geometry is preserved, not edited by a UI.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {getSchema,Node as TiptapNode} from '@tiptap/core';
import {EditorState,TextSelection} from '@tiptap/pm/state';
import {history,closeHistory,undo,redo} from '@tiptap/pm/history';
import {extensions as baseExtensions,snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const [inputFile,outputDirectory]=process.argv.slice(2);
if(!inputFile||!outputDirectory)throw new Error('Usage: node tests/ui/edit-list-geometry.mjs IMPORT_JSON OUTPUT_DIR');
const bytes=readFileSync(inputFile),raw=JSON.parse(bytes).document??JSON.parse(bytes);
const list=await loadTsModule('./ui/word-list-geometry.ts');
const empty=await loadTsModule('./ui/word-empty-paragraph-font.ts');
const page=await loadTsModule('./ui/word-page-geometry.ts');
const indent=await loadTsModule('./ui/word-paragraph-indent.ts');
const spacing=await loadTsModule('./ui/word-paragraph-spacing.ts');
const {createFontSizeTransaction}=await loadTsModule('./ui/word-font-size.ts');
const rendered=baseExtensions.map(extension=>extension.name==='starterKit'?extension.extend({addExtensions(){return(this.parent?.()??[]).map(node=>node instanceof TiptapNode?list.withPreservedListRendering(node):node);}}):extension);
const schema=getSchema([...rendered,list.PreservedListGeometry,empty.EmptyParagraphFont]);
list.validateListGeometryDocument(raw);page.validatePageGeometryDocument(raw);indent.validateParagraphIndentDocument(raw);
const document=schema.nodeFromJSON(raw);document.check();
function retained(source,restored){assert.equal(restored.type,source.type);if(source.text!==undefined)assert.equal(restored.text,source.text);
  for(const[key,value]of Object.entries(source.attrs??{}))assert.deepEqual(restored.attrs?.[key],value,`Lost ${source.type}.${key}`);
  for(const mark of source.marks??[]){const found=restored.marks?.find(m=>m.type===mark.type);assert(found);for(const[key,value]of Object.entries(mark.attrs??{}))assert.deepEqual(found.attrs?.[key],value);}
  assert.equal(restored.content?.length??0,source.content?.length??0);source.content?.forEach((child,index)=>retained(child,restored.content[index]));}
retained(raw,document.toJSON());
let target;document.forEach((node,pos)=>{if(!target&&node.type.name==='orderedList'&&node.attrs.markerFont)target={node,pos};});assert(target,'No preserved list');
const first=target.pos+3,paragraph=target.node.firstChild.firstChild;
const rejected=[],initial=EditorState.create({doc:document,selection:TextSelection.create(document,first),plugins:[history(),list.listGeometryGuard(message=>rejected.push(message)),page.pageGeometryGuard(),indent.paragraphIndentGuard()]});
const plain=value=>Array.isArray(value)?value.map(plain):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,plain(v)])):value;
const snapshot=state=>plain(list.omitDefaultListGeometry(baseSnapshot(state.doc)));
const outcomes={initial:snapshot(initial)},operations=[];
function exercise(name,state,tr,description){const result=state.applyTransaction(closeHistory(tr));assert.equal(result.transactions.length,1);state=result.state;const edited=state.doc;outcomes[name]=snapshot(state);
  assert(undo(state,tr=>{state=state.apply(tr);}));assert(state.doc.eq(document));outcomes[`${name}-undo`]=snapshot(state);
  assert(redo(state,tr=>{state=state.apply(tr);}));assert(state.doc.eq(edited));outcomes[`${name}-redo`]=snapshot(state);operations.push({name,description});}
exercise('text-edited',initial,initial.tr.insertText('Edited ',first),'Insert literal Edited plus one space at the beginning of the first preserved item; all geometry and marks retained');
exercise('spacing-edited',initial,spacing.createParagraphSpacingTransaction(initial,{spaceAfter:240}),'Set first preserved item spaceAfter to 240 twips using actual paragraph spacing candidate');
const selected=initial.apply(initial.tr.setSelection(TextSelection.create(document,first,first+paragraph.content.size)));
exercise('font-edited',selected,createFontSizeTransaction(selected,26),'Set only first preserved item text size to 26 half-points; marker remains its original independent size');
exercise('enter-split',initial,list.createPreservedListSplitTransaction(initial),'App-owned safe Enter at first item start adds an empty numbered item and retains all item paragraph geometry/format');
assert.deepEqual(rejected,[]);
const dir=resolve(outputDirectory);mkdirSync(dir,{recursive:true});
for(const[name,model]of Object.entries(outcomes)){list.validateListGeometryDocument(model);page.validatePageGeometryDocument(model);indent.validateParagraphIndentDocument(model);
  writeFileSync(join(dir,`${name}.json`),JSON.stringify({version:2,document:model},null,2)+'\n',{flag:'wx'});}
const manifest={input:resolve(inputFile),inputSha256:createHash('sha256').update(bytes).digest('hex'),evidence:'Real frontend app candidate functions, ProseMirror guards and undo/redo; no browser/native layout',
  target:{listPosition:target.pos,firstTextPosition:first},operations,files:Object.keys(outcomes).map(name=>`${name}.json`),assertions:{sourceFieldsRetained:true,allUndoExact:true,allRedoExact:true,rejected:0}};
writeFileSync(join(dir,'manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(manifest,null,2));
