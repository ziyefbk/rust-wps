// Print an immutable snapshot so edits made while a platform print dialog is
// open cannot change the document being printed. Native print UI chooses the
// printer or PDF destination; this does not claim that a file was saved.
export async function preparePrintSnapshot(source: HTMLElement): Promise<void> {
  const host = document.getElementById('print-document')!;
  host.replaceChildren();
  const snapshot = source.cloneNode(true) as HTMLElement;
  snapshot.removeAttribute('contenteditable'); snapshot.removeAttribute('aria-label');
  snapshot.querySelectorAll('[contenteditable]').forEach(node => node.removeAttribute('contenteditable'));
  snapshot.querySelectorAll('.selectedCell,.ProseMirror-selectednode').forEach(node => node.classList.remove('selectedCell', 'ProseMirror-selectednode'));
  const images = Array.from(snapshot.querySelectorAll('img'));
  for (const image of images) {
    if (!/^data:image\/(png|jpeg);base64,/.test(image.src)) throw new Error('打印图片必须嵌入文档');
  }
  await document.fonts.ready;
  await Promise.all(images.map(image => image.decode()));
  host.replaceChildren(snapshot);
}
