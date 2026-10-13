import type {Editor} from '@tiptap/core';
import {validateImageDisplayMedia} from './word-image-media';
import {closeHistory} from '@tiptap/pm/history';
import {captureImageSelection, createImageDimensionsTransaction, formatImageDimensionPoints, imageDimensionFields,
  imageDimensionsError, imageDimensionsWithRatio, isImageDimensions, matchesImageSelection, naturalImageDimensions,
  parseImageDimensionPoints, selectedWordImage, MAX_IMAGE_HEIGHT_EMU, type ImageDimensionField, type ImageDimensions,
  type ImageSelectionToken} from './word-image-dimensions';

interface ImageControlsOptions {
  editor: () => Editor | null;
  locked: () => boolean;
  notice: (message: string, error?: boolean) => void;
}
const ids: Record<ImageDimensionField, string> = {displayWidthEmu: 'edit-image-width', displayHeightEmu: 'edit-image-height'};
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class WordImageControls {
  private token: ImageSelectionToken | null = null;
  private owner: Editor | null = null;
  private editing = false;
  private request = 0;
  private natural: {src: string; width: number; height: number} | null = null;
  private decodeState: 'idle' | 'loading' | 'failed' | 'unsupported' | 'ready' = 'idle';

  constructor(private readonly options: ImageControlsOptions) {
    el('image-dimension-controls').addEventListener('focusout', event => {
      if (event.relatedTarget instanceof HTMLElement && el('image-dimension-controls').contains(event.relatedTarget)) return;
      this.editing = false; this.update();
    });
    for (const field of imageDimensionFields) {
      const input = el<HTMLInputElement>(ids[field]);
      input.addEventListener('focus', () => {this.editing = true;});
      input.addEventListener('input', () => {
        this.editing = true;
        if (!el<HTMLInputElement>('edit-image-keep-ratio').checked || !this.current()) return;
        const value = parseImageDimensionPoints(input.value, field), ratio = this.ratio();
        const pair = value !== null && ratio ? imageDimensionsWithRatio(value, field, ratio) : null;
        if (pair) {
          const other = field === 'displayWidthEmu' ? 'displayHeightEmu' : 'displayWidthEmu';
          el<HTMLInputElement>(ids[other]).value = formatImageDimensionPoints(pair[other]);
          el('edit-image-dimension-state').textContent = '保持比例；另一边四舍五入到最近的 EMU';
        } else if (value !== null && ratio) {
          // Leave the invalid companion empty so Apply cannot accidentally use
          // an old valid size and silently violate the checked aspect ratio.
          el<HTMLInputElement>(ids[field === 'displayWidthEmu' ? 'displayHeightEmu' : 'displayWidthEmu']).value = '';
          el('edit-image-dimension-state').textContent = '保持比例后超出尺寸上限，请缩小';
        }
      });
      input.addEventListener('keydown', event => {
        if (event.isComposing || event.keyCode === 229) return;
        if (event.key === 'Enter') {event.preventDefault(); this.apply();}
        if (event.key === 'Escape') {event.preventDefault(); this.editing = false; this.options.editor()?.view.focus(); this.update();}
      });
    }
    el('edit-image-dimensions-apply').addEventListener('click', () => this.apply());
    el('edit-image-dimensions-reset').addEventListener('click', () => this.commit(null));
    el('edit-image-natural-ratio').addEventListener('click', () => {
      if (!this.current() || !this.natural || this.decodeState !== 'ready') return;
      const natural = naturalImageDimensions(this.natural.width, this.natural.height)!;
      const width = this.token!.image.dimensions?.displayWidthEmu ?? natural.displayWidthEmu;
      const pair = imageDimensionsWithRatio(width, 'displayWidthEmu', this.natural)
        ?? imageDimensionsWithRatio(MAX_IMAGE_HEIGHT_EMU, 'displayHeightEmu', this.natural);
      if (pair) this.commit(pair);
      else this.options.notice(imageDimensionsError, true);
    });
  }

  reset() {
    this.request++; this.token = null; this.owner = null; this.editing = false;
    this.natural = null; this.decodeState = 'idle';
  }
  private current(): Editor | null {
    const editor = this.options.editor();
    return editor && editor === this.owner && !this.options.locked() && this.token
      && matchesImageSelection(editor.state, this.token) ? editor : null;
  }
  private ratio(): {width: number; height: number} | null {
    const dimensions = this.token?.image.dimensions;
    return dimensions ? {width: dimensions.displayWidthEmu, height: dimensions.displayHeightEmu}
      : this.decodeState === 'ready' ? this.natural : null;
  }
  private apply() {
    if (!this.current()) return;
    const dimensions = Object.fromEntries(imageDimensionFields.map(field => [field, parseImageDimensionPoints(el<HTMLInputElement>(ids[field]).value, field)]));
    if (!isImageDimensions(dimensions)) {this.options.notice(`${imageDimensionsError} 小数须精确到整数 EMU；也可输入 1/12700 这样的精确 pt 分数。`, true); return;}
    // If no ratio has been decoded, a checked toggle must never imply that an
    // arbitrary user-entered pair preserves the original image's proportion.
    if (el<HTMLInputElement>('edit-image-keep-ratio').checked) {
      const ratio = this.ratio();
      if (!ratio) {this.options.notice('图片原始比例尚未读取，请稍候或取消“保持比例”后设置宽高。', true); return;}
      // Either side can be the exact edited value; its companion may differ
      // from the mathematical ratio by no more than the documented rounding.
      const byWidth = imageDimensionsWithRatio(dimensions.displayWidthEmu, 'displayWidthEmu', ratio);
      const byHeight = imageDimensionsWithRatio(dimensions.displayHeightEmu, 'displayHeightEmu', ratio);
      if (byWidth?.displayHeightEmu !== dimensions.displayHeightEmu && byHeight?.displayWidthEmu !== dimensions.displayWidthEmu) {
        this.options.notice('宽高不符合当前比例，请重新输入一边，或取消“保持比例”。', true); return;
      }
    }
    this.commit(dimensions);
  }
  private commit(dimensions: ImageDimensions | null) {
    const editor = this.current();
    if (!editor) return;
    try {
      const transaction = createImageDimensionsTransaction(editor.state, dimensions, this.token!);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
      this.editing = false; editor.view.focus(); this.update();
    } catch (error) {this.options.notice(String(error), true);}
  }
  update() {
    const editor = this.options.editor(), selected = editor ? selectedWordImage(editor.state) : null;
    if (editor !== this.owner || !this.token || !editor || !matchesImageSelection(editor.state, this.token)) {
      this.request++; this.owner = editor; this.token = editor ? captureImageSelection(editor.state) : null; this.editing = false;
      this.decodeState = 'idle';
      const src = this.token?.image.node.attrs.src;
      if (this.token && typeof src === 'string' && /^data:image\/(png|jpeg);base64,/.test(src)) {
        if (this.natural?.src === src) this.decodeState = 'ready';
        else {
          this.natural = null; this.decodeState = 'loading';
          const request = this.request, token = this.token, owner = editor!;
          const image = new window.Image();
          // Metadata is checked before browser EXIF orientation/animation can
          // produce a ratio that differs from the DOCX image consumer.
          let decode: Promise<void>, unsupported = false;
          try {validateImageDisplayMedia(src);}
          catch {unsupported = true;}
          if (unsupported) decode = Promise.reject(new Error('unsupported image metadata'));
          else {image.src = src; decode = image.decode();}
          void decode.then(() => {
            if (request !== this.request || this.options.editor() !== owner || !matchesImageSelection(owner.state, token)) return;
            if (!naturalImageDimensions(image.naturalWidth, image.naturalHeight)) throw new Error('invalid natural image dimensions');
            this.natural = {src, width: image.naturalWidth, height: image.naturalHeight};
            this.decodeState = 'ready'; this.update();
          }).catch(() => {
            if (request !== this.request || this.options.editor() !== owner || !matchesImageSelection(owner.state, token)) return;
            this.decodeState = unsupported ? 'unsupported' : 'failed'; this.update();
          });
        }
      }
    }
    const disabled = this.options.locked() || !selected?.supported || this.decodeState === 'unsupported';
    for (const id of ['edit-image-width', 'edit-image-height', 'edit-image-keep-ratio', 'edit-image-dimensions-apply']) {
      el<HTMLInputElement | HTMLButtonElement>(id).disabled = disabled;
    }
    el<HTMLButtonElement>('edit-image-dimensions-reset').disabled = disabled || !selected?.dimensions;
    el<HTMLButtonElement>('edit-image-natural-ratio').disabled = disabled || this.decodeState !== 'ready';
    if (!this.editing) {
      const dimensions = selected?.dimensions ?? (this.decodeState === 'ready' && this.natural ? naturalImageDimensions(this.natural.width, this.natural.height) : null);
      for (const field of imageDimensionFields) {
        const input = el<HTMLInputElement>(ids[field]);
        input.value = dimensions ? formatImageDimensionPoints(dimensions[field]) : '';
        input.placeholder = selected?.supported ? '输入尺寸' : '先选正文图片';
      }
      el('edit-image-dimension-state').textContent = !selected ? '' : !selected.supported ? '暂不支持表格或列表内图片尺寸'
        : selected.dimensions ? '显式显示尺寸' : this.decodeState === 'loading' ? '正在读取原始比例'
          : this.decodeState === 'unsupported' ? '图片方向/动画元数据、结构或源像素暂不支持设置尺寸，保留默认布局'
          : this.decodeState === 'failed' ? '原始比例读取失败；可取消保持比例后设置宽高' : '默认布局；应用后固定显示尺寸';
    }
  }
}
