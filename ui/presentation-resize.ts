import type {PresentationModel} from './presentation-editor';

/** Uniform fit around the page center; never stretches pictures. */
export function resizePresentation(model:PresentationModel,size:[number,number]):PresentationModel {
  if(![...model.size_emu,...size].every(n=>Number.isSafeInteger(n)&&n>=914400&&n<=91440000))throw new Error('页面尺寸必须为 1–100 英寸');
  const candidate=structuredClone(model);
  const scale=Math.min(size[0]/model.size_emu[0],size[1]/model.size_emu[1]);
  const offsets=[(size[0]-model.size_emu[0]*scale)/2,(size[1]-model.size_emu[1]*scale)/2];
  const bounds=(value:[number,number,number,number]):[number,number,number,number]=>{
    const x=Math.round(offsets[0]+value[0]*scale),y=Math.round(offsets[1]+value[1]*scale);
    return [x,y,Math.max(1,Math.round(offsets[0]+(value[0]+value[2])*scale)-x),Math.max(1,Math.round(offsets[1]+(value[1]+value[3])*scale)-y)];
  };
  candidate.size_emu=[...size];
  for(const slide of candidate.slides){
    for(const box of slide.text_boxes){box.bounds_emu=bounds(box.bounds_emu);box.font_size_hundredths_pt=Math.round(box.font_size_hundredths_pt*scale);}
    for(const picture of slide.images??[])picture.bounds_emu=bounds(picture.bounds_emu);
  }
  return candidate;
}
