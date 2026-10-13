import type {Workbook} from './spreadsheet';
export interface WorkbookMatch {sheet:number;row:number;column:number;field:'value'|'formula'}
export interface WorkbookSearch {matches:WorkbookMatch[];truncated:boolean}
export function searchWorkbook(workbook:Workbook,query:string):WorkbookSearch {
  const result:WorkbookSearch={matches:[],truncated:false};
  if(!query)return result;
  if(query.length>256)return {...result,truncated:true};
  const needle=query.toLowerCase();let cells=0,units=0;
  for(const [sheetIndex,sheet] of workbook.sheets.entries())for(const cell of sheet.cells){
    if(++cells>100000)return {...result,truncated:true};
    const fields:Array<['value'|'formula',string]>=[['value',cell.value.type==='empty'?'':String(cell.value.value??'')]];
    if(cell.formula!==null)fields.push(['formula',`=${cell.formula}`]);
    for(const [field,text] of fields){
      units+=text.length;
      if(units>2*1024*1024)return {...result,truncated:true};
      if(text.toLowerCase().includes(needle)){
        result.matches.push({sheet:sheetIndex,row:cell.row,column:cell.column,field});
        if(result.matches.length>=1000)return {...result,truncated:true};
      }
    }
  }
  return result;
}
