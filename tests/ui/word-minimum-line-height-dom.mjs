import {DOMParser as ModelDOMParser, DOMSerializer} from '@tiptap/pm/model';
import {schema} from './word-minimum-line-height-harness.mjs';
// Small DOM transport double, not a browser HTML parser or layout engine.
export class Element {
  nodeType = 1; namespaceURI = 'http://www.w3.org/1999/xhtml'; childNodes = []; dataset = {}; listeners = new Map(); value = ''; disabled = false; textContent = ''; hidden = false; validity = {badInput: false};
  constructor(tag = 'div', attrs = {}) {this.tagName = this.nodeName = tag.toUpperCase(); this.attrs = {...attrs}; this.style = new Proxy({length: 0, setProperty(key, value) {this[key] = value;}}, {get: (obj, key) => {
    if (key === 'length') return (this.attrs.style ?? '').split(';').filter(part => part.trim()).length;
    if (key === 'getPropertyValue') return name => (this.attrs.style ?? '').split(';').map(part => part.split(':')).find(([key]) => key.trim() === name)?.[1]?.trim() ?? '';
    if (key in obj) return obj[key]; const property = String(key).replace(/[A-Z]/g, ch => `-${ch.toLowerCase()}`);
    return (this.attrs.style ?? '').split(';').map(part => part.split(':')).find(([name]) => name.trim() === property)?.[1]?.trim() ?? '';
  }, set: (obj, key, value) => {
    if (key === 'cssText') this.attrs.style = String(value);
    obj[key] = value; return true;
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
  contains(node) {for (let next = node; next; next = next.parentNode) if (next === this) return true; return false;}
  focus() {}
  emit(type, event = {}) {for (const callback of this.listeners.get(type) ?? []) callback({preventDefault() {}, ...event});}
}
export const dom = {createElement: tag => new Element(tag), createElementNS: (_, tag) => new Element(tag), createDocumentFragment: () => new Element('fragment'),
  createTextNode: value => ({nodeType: 3, nodeValue: value, get nextSibling() {return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null;}})};
export const serialize = node => DOMSerializer.fromSchema(schema).serializeNode(node, {document: dom});
export const parse = element => {const root = new Element(); root.appendChild(element); return ModelDOMParser.fromSchema(schema).parse(root);};
export const wrapped = element => {const root = new Element('body'); root.appendChild(element); root.body = root; return root;};
export function parserResult(root, run) {const previous = globalThis.DOMParser; globalThis.DOMParser = class {parseFromString() {return root;}}; try {run();} finally {globalThis.DOMParser = previous;}}
