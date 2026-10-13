export interface PresentationSnapshot {json:string;slide:number;box:number}
/** A shared budget bounds retained undo and redo snapshots together. */
export class PresentationHistory {
  private past:PresentationSnapshot[]=[];
  private future:PresentationSnapshot[]=[];
  constructor(private limit=100,private byteLimit=32*1024*1024){}
  get canUndo(){return this.past.length>0;}
  get canRedo(){return this.future.length>0;}
  private trim(){
    const bytes=(items:PresentationSnapshot[])=>items.reduce((n,s)=>n+new TextEncoder().encode(s.json).length,0);
    while(this.past.length+this.future.length>this.limit||bytes(this.past)+bytes(this.future)>this.byteLimit){
      if(this.past.length)this.past.shift();else this.future.shift();
    }
  }
  commit(before:PresentationSnapshot,after:PresentationSnapshot){
    if(before.json===after.json)return false;
    this.past.push({...before});this.future=[];this.trim();return true;
  }
  undo(current:PresentationSnapshot){
    const previous=this.past.pop();if(!previous)return null;
    this.future.push({...current});this.trim();return previous;
  }
  redo(current:PresentationSnapshot){
    const next=this.future.pop();if(!next)return null;
    this.past.push({...current});this.trim();return next;
  }
}
