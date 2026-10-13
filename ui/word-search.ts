import type {Node} from '@tiptap/pm/model';
export interface WordMatch {from:number;to:number}
export function searchWord(doc:Node,query:string):{matches:WordMatch[];truncated:boolean}{
 const matches:WordMatch[]=[];let truncated=false,remaining=2*1024*1024,visited=0;
 if(!query)return {matches,truncated};
 if(query.length>256||/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(query))return {matches,truncated:true};
 const stack=[{node:doc,index:0,offset:0,pos:-1}];
 function flush(text:string,start:number){
   let index=0;
   while((index=text.indexOf(query,index))!==-1){matches.push({from:start+index,to:start+index+query.length});index+=query.length;if(matches.length===1000){truncated=true;return;}}
 }
 while(stack.length&&!truncated){
   const frame=stack[stack.length-1];if(frame.index===frame.node.childCount){stack.pop();continue;}
   const node=frame.node.child(frame.index++),pos=frame.pos+1+frame.offset;frame.offset+=node.nodeSize;
   if(++visited>100000){truncated=true;break;}
   if(!node.isTextblock){if(node.childCount)stack.push({node,index:0,offset:0,pos});continue;}
   let segment='',start=pos+1,offset=0;
   for(let i=0;i<node.childCount&&!truncated;i++){
     const child=node.child(i);if(++visited>100000){truncated=true;break;}
     if(child.isText){
       const text=child.text!;if(text.length>remaining){flush(segment,start);truncated=true;break;}
       remaining-=text.length;if(!segment)start=pos+1+offset;segment+=text;
     }else {flush(segment,start);segment='';}
     offset+=child.nodeSize;
   }
   if(!truncated)flush(segment,start);
 }
 return {matches,truncated};
}
