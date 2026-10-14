import {test, expect} from '@playwright/test';
import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history, closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {liftListItem, sinkListItem, splitListItem} from '@tiptap/pm/schema-list';
import {joinBackward} from '@tiptap/pm/commands';
import {DOMParser as ModelDOMParser, DOMSerializer, Slice, Fragment} from '@tiptap/pm/model';
import {readFileSync} from 'node:fs';
import {extensions as baseExtensions, interactionExtensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const list = await loadTsModule('./ui/word-list-geometry.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const spacing = await loadTsModule('./ui/word-paragraph-spacing.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const {preparePrintSnapshot} = await loadTsModule('./ui/printing.ts');
const rendered = baseExtensions.map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {
  return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);
}}) : extension);
const schema = getSchema([...rendered, list.PreservedListGeometry]);
const geometry = page.presetPageGeometry('A4', 'portrait');
const font = {fontFamily: 'Calibri', fontSize: 22};
const frame = {leftTwips: 720, rightTwips: 180, hangingTwips: 360};
const p = (text = 'Alpha beta', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const item = (text = 'Alpha beta', attrs = {}, geometryValue = frame) => schema.node('listItem', null, p(text, {...attrs, listGeometry: geometryValue}));
const ordered = (items = [item(), item('Second')], attrs = {}) => schema.node('orderedList', {start: 9, markerFont: font, ...attrs}, items);
const doc = (content = [ordered()], pageGeometry = geometry) => schema.node('doc', {styleProfile: 'standard-v2', pageGeometry}, content);
const snapshot = document => list.omitDefaultListGeometry(baseSnapshot(document));
const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), list.listGeometryGuard(message => errors.push(message)), page.pageGeometryGuard(message => errors.push(message)), indent.paragraphIndentGuard(message => errors.push(message))]});
const apply = (state, tr) => state.apply(closeHistory(tr));
const raw = () => structuredClone(snapshot(doc()));
const paragraph = model => model.content[0].content[0].content[0];
const invalidScalar = [null, undefined, false, '1', 0.5, -1, 31681, NaN, Infinity];

test('complete exact integer geometry and literal half-point marker font contract', () => {
  expect(list.isListGeometry(frame)).toBe(true); expect(list.isListMarkerFont(font)).toBe(true);
  for (const key of Object.keys(frame)) for (const value of invalidScalar) expect(list.isListGeometry({...frame, [key]: value})).toBe(false);
  for (const key of Object.keys(frame)) {const partial = {...frame}; delete partial[key]; expect(list.isListGeometry(partial)).toBe(false);}
  for (const value of [[], {}, {...frame, extra: 1}, {...frame, hangingTwips: 721}]) expect(list.isListGeometry(value)).toBe(false);
  for (const value of [null, {}, [], {...font, extra: 1}, {...font, fontSize: 0}, {...font, fontSize: 3277}, {...font, fontSize: 22.5}, {...font, fontFamily: ''}, {...font, fontFamily: 'x'.repeat(32)}, {...font, fontFamily: '\ud800'}]) expect(list.isListMarkerFont(value)).toBe(false);
  for (const frame of [{leftTwips: 0, rightTwips: 0, hangingTwips: 0}, {leftTwips: 1, rightTwips: 0, hangingTwips: 1}]) expect(list.isListGeometry(frame)).toBe(true);
  expect(list.listMarkerFontStyle({fontFamily: 'a";color:red;/*\\b', fontSize: 22}, 9)).toContain('"a\\";color:red;/*\\\\b"');
});

test('raw validation requires explicit proven page, every item frame and strict metadata location', () => {
  expect(() => list.validateListGeometryDocument(raw())).not.toThrow();
  const mutations = [m => delete m.attrs.pageGeometry, m => m.attrs.pageGeometry = null, m => m.attrs.pageGeometry = {...geometry, extra: 1},
    m => delete paragraph(m).attrs.listGeometry, m => m.content[0].attrs.markerFont = null, m => paragraph(m).attrs.listGeometry = null,
    m => m.attrs.markerFont = font, m => m.content[0].content[0].attrs = {listGeometry: frame}, m => paragraph(m).content[0].marks = [{type:'bold', attrs:{listGeometry:frame}}],
    ...['indentLeftTwips', 'indentRightTwips', 'firstLineIndentTwips', 'emptyParagraphFont'].map(key => m => paragraph(m).attrs[key] = null),
    m => m.content[0].attrs.type = '1', m => m.content[0].attrs.start = '9', m => m.content[0].attrs.start = 1000001];
  for (const mutate of mutations) {const m = raw(); mutate(m); expect(() => list.validateListGeometryDocument(m)).toThrow();}
});

test('bounded frame, sole paragraph, left alignment and plain inline restrictions', () => {
  const width = geometry.widthTwips - geometry.marginLeftTwips - geometry.marginRightTwips;
  for (const mutate of [m => paragraph(m).attrs.listGeometry.leftTwips = width - frame.rightTwips,
    m => paragraph(m).attrs.textAlign = 'center', m => paragraph(m).attrs.indentLeftTwips = 0, m => paragraph(m).attrs.emptyParagraphFont = {family:'Calibri',halfPoints:22,bold:false,italic:false},
    m => paragraph(m).content.push({type:'image',attrs:{src:'data:image/png;base64,'}}), m => paragraph(m).content[0].text = 'a\tb',
    m => m.content[0].content[0].content.push({type:'paragraph'}), m => paragraph(m).type = 'heading',
    m => {const list = m.content.pop(); m.content.push({type:'bulletList',content:[{type:'listItem',content:[{type:'paragraph'},list]}]});}]) {
    const m = raw(); mutate(m); expect(() => list.validateListGeometryDocument(m)).toThrow();
  }
  const m = raw(); paragraph(m).attrs.listGeometry = {leftTwips:width - 1,rightTwips:0,hangingTwips:width - 1};
  paragraph(m).attrs.textAlign = 'left'; paragraph(m).content.push({type:'hardBreak'}); expect(() => list.validateListGeometryDocument(m)).not.toThrow();
});

test('ordinary no-metadata lists retain schema defaults and legacy validator behavior', () => {
  const ordinary = doc([ordered([schema.node('listItem', null, [p('ordinary'), schema.node('bulletList', null, schema.node('listItem',null,p('nested')))])], {markerFont:null})], null);
  const value = snapshot(ordinary); expect(JSON.stringify(value)).not.toContain('listGeometry'); expect(JSON.stringify(value)).not.toContain('markerFont');
  expect(() => list.validateListGeometryDocument(value)).not.toThrow();
  let state = stateFor(ordinary); expect(splitListItem(schema.nodes.listItem)(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.firstChild.childCount).toBe(2);
});

test('plain text marks and paragraph spacing remain editable with exact draft and one undo', () => {
  const initial = doc(), errors = []; let state = stateFor(initial, TextSelection.create(initial, 4), errors);
  state = apply(state, state.tr.insertText('X')); expect(state.doc.textContent).toContain('AXlpha');
  const edited = state.doc; expect(schema.nodeFromJSON(snapshot(edited)).eq(edited)).toBe(true);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(initial)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
  state = apply(state, spacing.createParagraphSpacingTransaction(state, {spaceAfter:0,lineSpacing:300}));
  expect(state.doc.firstChild.firstChild.firstChild.attrs.lineSpacing).toBe(300); expect(errors).toEqual([]);
});

for (const where of ['start','middle','end','empty','selection']) test(`safe Enter ${where} inherits exact frame, font, spacing and one history entry`, () => {
  const initial = doc([ordered([item(where === 'empty' ? '' : 'Alpha beta', {spaceAfter:0,lineSpacing:300})])]);
  const from = where === 'end' ? 13 : where === 'middle' ? 7 : 3;
  let state = stateFor(initial, TextSelection.create(initial, from, where === 'selection' ? 7 : from));
  const shared = state.tr, chainable = Object.create(state); Object.defineProperty(chainable, 'tr', {get:()=>shared});
  state = apply(state, list.createPreservedListSplitTransaction(chainable));
  expect(shared.steps).toEqual([]); expect(state.doc.firstChild.childCount).toBe(2);
  state.doc.firstChild.forEach(child => expect(child.firstChild.attrs).toEqual(initial.firstChild.firstChild.firstChild.attrs));
  expect(state.doc.firstChild.attrs.markerFont).toEqual(font); expect(state.selection.$from.parent.type.name).toBe('paragraph');
  const edited = state.doc; expect(undoDepth(state)).toBe(1); expect(undo(state, tr => {state=state.apply(tr);})).toBe(true); expect(state.doc.eq(initial)).toBe(true);
  expect(redo(state, tr => {state=state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
});

test('unsupported lift nest merge reorder toggle and block deletion are atomic', () => {
  const initial = doc(), errors = [], state = stateFor(initial, TextSelection.create(initial, 19), errors), candidates = [];
  for (const command of [liftListItem(schema.nodes.listItem), sinkListItem(schema.nodes.listItem), joinBackward]) command(state, tr => candidates.push(tr));
  candidates.push(state.tr.replaceWith(0, initial.firstChild.nodeSize, ordered([initial.firstChild.child(1),initial.firstChild.child(0)])));
  candidates.push(state.tr.replaceWith(0, initial.firstChild.nodeSize, schema.node('bulletList',null,initial.firstChild.content)));
  candidates.push(state.tr.replaceWith(0, initial.firstChild.nodeSize,p('replacement')));
  candidates.push(state.tr.delete(1,1+initial.firstChild.firstChild.nodeSize));
  expect(candidates.length).toBeGreaterThanOrEqual(5);
  for (const tr of candidates) {const result = state.applyTransaction(tr); expect(result.transactions).toEqual([]); expect(result.state).toBe(state); expect(undoDepth(state)).toBe(0);}
  expect(errors).toHaveLength(candidates.length);
});

test('safe split cannot authorize an appended topology replacement', () => {
  const initial=doc(), state=stateFor(initial), tr=list.createPreservedListSplitTransaction(state);
  tr.replaceWith(0,tr.doc.content.size,p('erased')); expect(state.applyTransaction(tr).transactions).toEqual([]);
});

test('adjacent pasted complete list stays separate; joining or moving an existing list refuses', () => {
  const initial=doc([ordered(),p('gap'),ordered([item('other')])]), state=stateFor(initial);
  const insertion=state.tr.insert(initial.content.size, ordered([item('pasted')]));
  const inserted=state.applyTransaction(insertion); expect(inserted.transactions).toHaveLength(1); expect(inserted.state.doc.childCount).toBe(4);
  const changed=state.tr.replaceWith(0,initial.content.size,[ordered([...Array.from({length:2},(_,i)=>initial.firstChild.child(i)),initial.lastChild.firstChild]),p('gap')]);
  expect(state.applyTransaction(changed).transactions).toEqual([]);
  const moved=state.tr.delete(0,initial.firstChild.nodeSize).insert(initial.content.size-initial.firstChild.nodeSize,initial.firstChild);
  expect(state.applyTransaction(moved).transactions).toEqual([]);
});

test('page shrink reset and direct geometry loss reject before shared history/draft mutation', () => {
  const initial=doc([ordered([item('wide',{}, {leftTwips:8000,rightTwips:180,hangingTwips:360})])]);
  const errors=[], state=stateFor(initial,TextSelection.atStart(initial),errors), shared=state.tr, chainable=Object.create(state);
  Object.defineProperty(chainable,'tr',{get:()=>shared});
  for (const candidate of [null,page.presetPageGeometry('A5','portrait')]) {
    expect(()=>page.createPageGeometryTransaction(chainable,candidate)).toThrow('编号');
    expect(state.applyTransaction(state.tr.setDocAttribute('pageGeometry',candidate)).transactions).toEqual([]);
  }
  expect(shared.steps).toEqual([]); expect(state.doc).toBe(initial); expect(undoDepth(state)).toBe(0);
  const ppos=2; expect(state.applyTransaction(state.tr.setNodeMarkup(ppos,undefined,{...initial.firstChild.firstChild.firstChild.attrs,listGeometry:null})).transactions).toEqual([]);
});

// Real PM parser/serializer with a narrow DOM transport, not browser layout.
class Element {
  nodeType=1; namespaceURI='http://www.w3.org/1999/xhtml'; dataset={};
  constructor(tag='div',attrs={}) {this.tagName=this.nodeName=tag.toUpperCase();this.attrs={...attrs};this.childNodes=[];
    const propertyValue=property=>(this.attrs.style??'').split(';').map(part=>part.split(':')).find(([name])=>name.trim()===property)?.[1]?.trim()??'';
    this.style=new Proxy({getPropertyValue:propertyValue},{get:(o,k)=>k==='length'?(this.attrs.style?1:0):k in o?o[k]:propertyValue(String(k).replace(/[A-Z]/g,ch=>`-${ch.toLowerCase()}`)),set:(o,k,v)=>{if(k==='cssText')this.attrs.style=String(v);else o[k]=v;return true;}});}
  get firstChild(){return this.childNodes[0]??null;} get children(){return this.childNodes.filter(n=>n.nodeType===1);} get firstElementChild(){return this.children[0]??null;}
  get parentElement(){return this.parentNode??null;} get textContent(){return this.childNodes.map(n=>n.nodeType===3?n.nodeValue:n.textContent).join('');} set textContent(v){this.childNodes=[];this.appendChild(fakeDocument.createTextNode(v));}
  get nextSibling(){return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this)+1]??null;}
  getAttribute(name){return this.attrs[name]??null;} hasAttribute(name){return name in this.attrs;} setAttribute(name,value){this.attrs[name]=String(value);} removeAttribute(name){delete this.attrs[name];}
  appendChild(child){child.parentNode=this;this.childNodes.push(child);return child;} replaceChildren(...nodes){this.childNodes=[];nodes.forEach(n=>this.appendChild(n));}
  matches(selector){if(selector==='*')return true;if(selector.includes(','))return selector.split(',').some(s=>this.matches(s.trim())); const m=/^([\w-]+)?(?:\[([\w-]+)\])?$/.exec(selector);return !!m&&(!m[1]||this.tagName.toLowerCase()===m[1])&&(!m[2]||this.hasAttribute(m[2]));}
  querySelectorAll(s){return this.children.flatMap(c=>[...(c.matches(s)?[c]:[]),...c.querySelectorAll(s)]);} querySelector(s){return this.querySelectorAll(s)[0]??null;}
  closest(s){for(let node=this;node;node=node.parentElement)if(node.matches(s))return node;return null;}
  cloneNode(){const node=new Element(this.tagName,this.attrs);node.dataset={...this.dataset};this.childNodes.forEach(c=>node.appendChild(c.cloneNode()));return node;}
}
const fakeDocument={createElement:tag=>new Element(tag),createElementNS:(_,tag)=>new Element(tag),createDocumentFragment:()=>new Element('fragment'),
  createTextNode:text=>({nodeType:3,nodeValue:text,textContent:text,get nextSibling(){return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this)+1]??null;},cloneNode(){return fakeDocument.createTextNode(text);}})};
const serialize=node=>DOMSerializer.fromSchema(schema).serializeNode(node,{document:fakeDocument});
const parse=element=>{const root=new Element();root.appendChild(element);return ModelDOMParser.fromSchema(schema).parse(root);};

test('DOM roundtrip keeps real text, separate literal marker font and integer item geometry', () => {
  const original=ordered(), element=serialize(original), parsed=parse(element).firstChild;
  expect(parsed.eq(original)).toBe(true); expect(element.textContent).toBe(original.textContent);
  expect(element.getAttribute('style')).toContain('counter-reset: word-preserved-decimal 8');
  expect(element.getAttribute('style')).toContain('--word-list-marker-size: 11pt');
  expect(element.firstChild.firstChild.getAttribute('style')).toContain('--word-list-left: 36pt');
  expect(serialize(ordered([schema.node('listItem',null,p())],{markerFont:null})).hasAttribute(list.LIST_MARKER_FONT_METADATA)).toBe(false);
});

test('malformed metadata remains a sentinel through parsing and transaction refusal', () => {
  const state=stateFor(doc());
  for(const value of ['null','{}','{"leftTwips":0}','bad',' '.repeat(4097)]) {
    const element=new Element('p',{[list.LIST_GEOMETRY_METADATA]:value}), parsed=parse(element);
    expect(parsed.firstChild.attrs.listGeometry).toBe('invalid-list-geometry');
    expect(state.applyTransaction(state.tr.insert(0,parsed.content)).transactions).toEqual([]);
  }
});

test('raw paste rejects unsupported tags, direction, inline objects and stray item text before normalization', () => {
  const saved=globalThis.DOMParser;let root;
  globalThis.DOMParser=class{parseFromString(){return root;}};
  const valid=()=>{root=new Element();root.appendChild(serialize(ordered()));};
  try {
    valid();expect(list.preflightListGeometryHTML('valid')).toBe('valid');
    for(const mutate of [()=>root.firstChild.setAttribute('dir','rtl'),()=>root.firstChild.setAttribute('style','direction:rtl'),
      ()=>root.firstChild.setAttribute('style','list-style-position:inside'),()=>root.firstChild.setAttribute('start','9evil'),
      ()=>root.firstChild.firstChild.appendChild(fakeDocument.createTextNode('stray')),
      ()=>root.firstChild.firstChild.firstChild.appendChild(new Element('img')),
      ()=>root.firstChild.firstChild.firstChild.appendChild(fakeDocument.createTextNode('\t')),
      ()=>root.appendChild(new Element('span',{[list.LIST_MARKER_FONT_METADATA]:JSON.stringify(font)}))]) {
      valid();mutate();expect(list.preflightListGeometryHTML('invalid')).toContain('data-word-list-geometry="invalid"');
    }
  } finally {globalThis.DOMParser=saved;}
});

test('complete clipboard list validates against destination page and open fragments reject', () => {
  const plugin=list.listGeometryGuard(), state=stateFor(doc()), view={state};
  expect(plugin.props.handlePaste(view,null,new Slice(Fragment.from(ordered()),0,0))).toBe(false);
  expect(plugin.props.handlePaste(view,null,new Slice(Fragment.from(ordered()),3,3))).toBe(true);
  expect(plugin.props.handlePaste({state:stateFor(doc([p()],null))},null,new Slice(Fragment.from(ordered()),0,0))).toBe(true);
});

test('immutable print clone retains scoped list frame font and counter through font wait', async () => {
  const saved=globalThis.document,host=new Element(),source=new Element('div',{class:'tiptap',contenteditable:'true'}),element=serialize(ordered());source.appendChild(element);
  let release;globalThis.document={...fakeDocument,getElementById:()=>host,fonts:{ready:new Promise(resolve=>{release=resolve;})}};
  try {const pending=preparePrintSnapshot(source,geometry);element.setAttribute('start','100');release();await pending;
    const cloned=host.children[1].firstChild;expect(cloned.getAttribute('start')).toBe('9');expect(cloned.getAttribute('style')).toContain('counter-reset: word-preserved-decimal 8');
    expect(cloned.firstChild.firstChild.getAttribute('style')).toContain('--word-list-hanging: 18pt');
  } finally {globalThis.document=saved;}
  const css=readFileSync('ui/style.css','utf8');expect(css).toContain('left:calc(-1 * var(--word-list-hanging))');expect(css).toContain('content:counter(word-preserved-decimal, decimal)');
});

export {schema,snapshot,stateFor,doc,ordered,item,p,geometry,list,spacing,apply};

const editors=[];
function fullEditor(document,locked=()=>false){
  const errors=[], extensions=interactionExtensions({onError:message=>errors.push(message)}).map(extension=>extension.name==='starterKit'?extension.extend({addExtensions(){return(this.parent?.()??[]).map(node=>node instanceof TiptapNode?list.withPreservedListRendering(node):node);}}):extension);
  const editor=new Editor({element:null,extensions:[...extensions,list.PreservedListGeometry.configure({onError:message=>errors.push(message),locked}),empty.EmptyParagraphFont.configure({onError:message=>errors.push(message)})],content:snapshot(document)});
  for(const plugin of editor.extensionManager.plugins)editor.registerPlugin(plugin);
  const base=editor.view,dispatch=editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view=new Proxy(base,{get(target,property){return property==='dispatch'?dispatch:property==='state'?editor.state:Reflect.get(target,property);}});
  Object.defineProperty(editor,'view',{get:()=>view});editors.push(editor);return{editor,errors};
}
function key(editor,key){const event={key,keyCode:key==='Enter'?13:key==='Backspace'?8:key==='Delete'?46:9,ctrlKey:false,altKey:false,metaKey:false,shiftKey:false};return editor.state.plugins.some(plugin=>plugin.props.handleKeyDown?.(editor.view,event));}
test.afterEach(()=>{for(const editor of editors.splice(0))editor.destroy();});

test('registered high-priority Enter and full app-equivalent extension chain preserve geometry through undo redo',()=>{
  const {editor,errors}=fullEditor(doc([ordered(),p('outside')]));editor.commands.setTextSelection(7);
  const initial=editor.state.doc;expect(key(editor,'Enter')).toBe(true);expect(editor.state.doc.firstChild.childCount).toBe(3);
  const split=editor.state.doc;split.firstChild.forEach(child=>expect(child.firstChild.attrs.listGeometry).toEqual(frame));
  expect(editor.commands.undo()).toBe(true);expect(editor.state.doc.eq(initial)).toBe(true);
  expect(editor.commands.redo()).toBe(true);expect(editor.state.doc.eq(split)).toBe(true);expect(errors).toEqual([]);
});

test('registered Enter does not fall through or mutate while readonly or app locked',()=>{
  for(const mode of ['readonly','locked']){
    const {editor,errors}=fullEditor(doc([ordered(),p('outside')]),()=>mode==='locked');editor.commands.setTextSelection(7);
    if(mode==='readonly')editor.setEditable(false,false);
    const initial=editor.state.doc;expect(key(editor,'Enter')).toBe(true);expect(editor.state.doc.eq(initial)).toBe(true);expect(undoDepth(editor.state)).toBe(0);expect(errors).toEqual([]);
  }
});

test('full extension chain preserves inline deletion but refuses item Backspace Tab and ordered-list toggle',()=>{
  const {editor,errors}=fullEditor(doc([ordered(),p('outside')]));editor.commands.setTextSelection({from:3,to:8});editor.commands.deleteSelection();
  expect(editor.state.doc.firstChild.firstChild.textContent).toBe(' beta');expect(editor.commands.undo()).toBe(true);
  const initial=editor.state.doc,second=initial.firstChild.firstChild.nodeSize+3;editor.commands.setTextSelection(second);
  key(editor,'Backspace');expect(editor.state.doc.eq(initial)).toBe(true);key(editor,'Tab');expect(editor.state.doc.eq(initial)).toBe(true);
  editor.commands.toggleOrderedList();expect(editor.state.doc.eq(initial)).toBe(true);expect(undoDepth(editor.state)).toBe(0);expect(errors.length).toBeGreaterThan(0);
});

test('ordinary open clipboard list slices ignore null geometry schema defaults and keep legacy fitting path',()=>{
  const plugin=list.listGeometryGuard(),view={state:stateFor(doc([p()],null))};
  const ordinary=ordered([schema.node('listItem',null,p('ordinary'))],{markerFont:null});
  expect(ordinary.attrs.markerFont).toBeNull();expect(ordinary.firstChild.firstChild.attrs.listGeometry).toBeNull();
  for(const depth of [0,1,2,3])expect(plugin.props.handlePaste(view,null,new Slice(Fragment.from(ordinary),depth,depth))).toBe(false);
  const plainParagraph=p('the word markerFont is ordinary text');expect(plugin.props.handlePaste(view,null,new Slice(Fragment.from(plainParagraph),1,1))).toBe(false);
});

test('preservation-mode raw block attributes and marks reject before schema normalization without extending legacy rules',()=>{
  for(const mutate of [m=>m.content[0].attrs.reversed=true,m=>m.content[0].attrs.listStyle='upper-roman',
    m=>m.content[0].content[0].attrs={value:42},m=>paragraph(m).attrs.rtl=true,m=>paragraph(m).attrs.level=2,
    ...[m=>m.content[0],m=>m.content[0].content[0],paragraph].map(target=>m=>target(m).marks=[{type:'bold'}])]){
    const model=raw();mutate(model);expect(()=>list.validateListGeometryDocument(model)).toThrow('编号');
  }
  const ordinary={type:'doc',content:[{type:'orderedList',attrs:{reversed:true},content:[{type:'listItem',attrs:{value:42},content:[{type:'paragraph',attrs:{rtl:true}}]}]}]};
  expect(()=>list.validateListGeometryDocument(ordinary)).not.toThrow();
  expect(()=>list.validateListGeometryState(doc())).not.toThrow();
});
