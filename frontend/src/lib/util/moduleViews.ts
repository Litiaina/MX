import type {FieldDefinition,JsonValue,MxRecord} from '../api/domain';
import type {ModuleLayout,ModuleView,ModuleViewKind,RecordSection} from '../api/moduleViews';
import {optionLabel} from './formRules';
export const viewNames:Record<ModuleViewKind,string>={records:'Records',board:'Pipeline board',cards:'Cards / directory',overview:'Operations overview',documents:'Document library',ledger:'Accounting workspace',inventory:'Inventory workspace'};
export function moduleView(id:string,kind:ModuleViewKind,name=viewNames[kind]):ModuleView{return{id,kind,name,title_field:'',fields:[],summary_fields:[],description:''};}
export function presetLayout(kind:'crm'|'erp'|'accounting'|'inventory'|'dms'|'information'):ModuleLayout{
 const kinds:Record<typeof kind,[ModuleViewKind,string][]>= {
  crm:[['board','Pipeline'],['cards','Directory'],['records','All records']],
  erp:[['overview','Operations'],['board','Approvals'],['records','Requests']],
  accounting:[['ledger','Accounting'],['overview','Overview'],['records','Source records']],
  inventory:[['inventory','Stock'],['overview','Operations'],['records','Requests']],
  dms:[['documents','Documents'],['cards','Register'],['records','All records']],
  information:[['records','Records'],['cards','Directory'],['overview','Overview']]
 };
 const views=kinds[kind].map(([type,name],i)=>moduleView(`view-${i+1}`,type,name));
 return{default_view:views[0].id,views,sections:[]};
}
export function recordFieldText(record:MxRecord,key:string,fields:FieldDefinition[]):string{
 if(record.restricted_fields?.includes(key))return'Restricted';
 const relationship=record.relationships?.[key];
 if(relationship)return relationship.restricted?'Restricted':relationship.items.map(item=>item.label).join(' · ')||'—';
 const field=fields.find(f=>f.key===key);const value=record.values[key];
 if(value===null||value===undefined||value==='')return'—';
 if(field?.field_type==='select')return optionLabel(field,value);
 if(typeof value==='boolean')return value?'Yes':'No';
 if(Array.isArray(value))return value.map(v=>typeof v==='object'&&v?String((v as Record<string,JsonValue>).label??(v as Record<string,JsonValue>).title??'Linked record'):String(v)).join(' · ');
 if(typeof value==='object')return String(value.label??value.title??'Linked record');
 return String(value);
}
export function orderedRecordFields(fields:FieldDefinition[],sections:RecordSection[]):FieldDefinition[]{
 const ordered=sections.flatMap(s=>s.fields.map(key=>fields.find(f=>f.key===key)).filter((f):f is FieldDefinition=>!!f));
 const selected=new Set(ordered.map(f=>f.uid));return[...ordered,...fields.filter(f=>!selected.has(f.uid))];
}
export function sectionHeading(field:FieldDefinition,fields:FieldDefinition[],sections:RecordSection[]):string{
 for(const section of sections){const first=section.fields.find(key=>fields.some(f=>f.key===key));if(first===field.key)return section.name;}
 const assigned=new Set(sections.flatMap(s=>s.fields));
 if(sections.length&&fields.find(f=>!assigned.has(f.key))?.key===field.key)return'Other information';return'';
}
