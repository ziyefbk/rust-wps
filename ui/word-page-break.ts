import {Extension, Node as TiptapNode} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {Plugin} from '@tiptap/pm/state';
import {BREAK_PARAGRAPH_METADATA, SPLIT_PAGE_BREAK_METADATA, isBreakParagraph, omitDefaultPageBreak,
  pageBreakParagraphError, parseBreakParagraphMetadata, validatePageBreakDocument} from './word-page-break-model';
export * from './word-page-break-model';

export function validatePageBreakState(document: Node): void {
  validatePageBreakDocument(omitDefaultPageBreak(document.toJSON()));
}
export function pageBreakGuard(onError: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validatePageBreakState(transaction.doc); return true;}
    catch (error) {onError(String(error)); return false;}
  }, props: {handlePaste(_view, _event, slice) {
    try {
      // Validate before slice fitting can discard malformed atom attributes.
      validatePageBreakDocument(omitDefaultPageBreak({type: 'doc', content: slice.content.toJSON() ?? []}));
      return false;
    } catch (error) {onError(String(error)); return true;}
  }}});
}

export function preflightPageBreakHTML(html: string): string {
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  try {
    parsed.querySelectorAll(`[${BREAK_PARAGRAPH_METADATA}]`).forEach(element => {
      if (element.tagName !== 'DIV' || !element.hasAttribute('data-page-break') || element.childNodes.length !== 0
        || element.closest('p,h1,h2,h3,li,td,th,table')) throw new Error(pageBreakParagraphError);
      parseBreakParagraphMetadata(element.getAttribute(BREAK_PARAGRAPH_METADATA));
    });
    // A copied editor wrapper is not a request to change the target document.
    // Root compatibility metadata is deliberately never parsed by the schema.
    parsed.querySelectorAll(`[${SPLIT_PAGE_BREAK_METADATA}]`).forEach(element => {
      if (element.getAttribute(SPLIT_PAGE_BREAK_METADATA) !== 'true') throw new Error(pageBreakParagraphError);
    });
    return html;
  } catch {return `<div data-page-break="" ${BREAK_PARAGRAPH_METADATA}="invalid"></div>`;}
}

export const PageBreak = TiptapNode.create({
  name: 'pageBreak', group: 'block', atom: true, selectable: true,
  addAttributes: () => ({breakParagraph: {default: null, rendered: false, parseHTML: (element: HTMLElement) => {
    try {
      if (element.hasAttribute(BREAK_PARAGRAPH_METADATA) && element.childNodes.length !== 0) throw new Error(pageBreakParagraphError);
      return parseBreakParagraphMetadata(element.getAttribute(BREAK_PARAGRAPH_METADATA));
    } catch {return 'invalid-break-paragraph';}
  }}}),
  parseHTML: () => [{tag: 'div[data-page-break]'}],
  renderHTML: ({node}) => ['div', {'data-page-break': '', class: 'page-break', 'aria-label': '手动分页符',
    ...(isBreakParagraph(node.attrs.breakParagraph) ? {[BREAK_PARAGRAPH_METADATA]: JSON.stringify(node.attrs.breakParagraph)} : {})}],
});

export const PageBreakParagraph = Extension.create<{onError: (message: string) => void}>({
  name: 'pageBreakParagraph',
  addOptions: () => ({onError: () => {}}),
  addGlobalAttributes: () => [{types: ['doc'], attributes: {splitPageBreakAndParagraphMark: {
    default: null, rendered: false, parseHTML: () => null,
  }}}],
  transformPastedHTML: preflightPageBreakHTML,
  addProseMirrorPlugins() {return [pageBreakGuard(this.options.onError)];},
});
