export interface TextRun {text:string;font_size_hundredths_pt?:number|null;bold?:boolean|null;italic?:boolean|null;color_rgb?:number[]|null}
export interface TextBudget {units:number;runs:number}
const graphemes=new Intl.Segmenter(undefined,{granularity:'grapheme'});
const tokens=/\n|[^\S\n]+|[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]|[^\s\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]+/gu;

// Greedy word wrapping with grapheme fallback for long words. It is an
// approximation, not Office line breaking or a complex-script shaper.
export function wrapTextRuns<T extends TextRun>(runs:readonly T[],width:number,measure:(text:string,run:T)=>number,budget:TextBudget={units:20000,runs:10000},maxLines=100){
 const lines:T[][]=[[]];let used=0,truncated=false;
 const next=()=>{if(lines.length>=maxLines){truncated=true;return false;}lines.push([]);used=0;return true;};
 const append=(run:T,text:string,advance:number)=>{lines[lines.length-1].push({...run,text});used+=advance;};
 if(!Number.isFinite(width)||width<=0||!Number.isSafeInteger(maxLines)||maxLines<1)return {lines,truncated:true};
 for(const run of runs){
  if(budget.runs--<=0){truncated=true;break;}
  let end=0;
  for(const part of graphemes.segment(run.text)){
   if(part.index+part.segment.length>budget.units)break;
   end=part.index+part.segment.length;
  }
  const clipped=end<run.text.length;budget.units-=end;
  for(const match of run.text.slice(0,end).matchAll(tokens)){
   const token=match[0];
   if(token==='\n'){if(!next())break;continue;}
   const advance=measure(token,run);
   if(!Number.isFinite(advance)||advance<0){truncated=true;break;}
   // Keep boundary whitespace with the preceding line. Wrapping it alone
   // would create a spurious blank line between two full-width words.
   if(/^\s+$/u.test(token)){append(run,token,advance);continue;}
   if(advance<=width){
    if(used>0&&used+advance>width&&!next())break;
    append(run,token,advance);
   }else{
    for(const part of graphemes.segment(token)){
     const size=measure(part.segment,run);
     if(!Number.isFinite(size)||size<0||size>width){truncated=true;break;}
     if(used>0&&used+size>width&&!next())break;
     append(run,part.segment,size);
    }
   }
   if(truncated)break;
  }
  if(clipped)truncated=true;
  if(truncated)break;
 }
 return {lines,truncated};
}
