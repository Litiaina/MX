import type {FieldDefinition,JsonValue} from '../api/domain';
export interface Condition {field:string;operator:string;value?:JsonValue}
export interface ConditionGroup {match:'all'|'any';conditions:Condition[];groups?:ConditionGroup[]}
export interface FormRules {visible_when?:ConditionGroup;enabled_when?:ConditionGroup;required_when?:ConditionGroup;exclusive_group?:string;exclusive_required?:boolean}
export interface OptionDetail {value:string;label?:string;number?:number;available_when?:ConditionGroup;fill?:Record<string,JsonValue>}
export interface FieldState {visible:boolean;enabled:boolean;required:boolean}
export const writable=(field:FieldDefinition)=>['text','long_text','integer','decimal','boolean','date','select','relationship'].includes(field.field_type);
export const fieldRules=(field:FieldDefinition)=>(field.config.form_rules||{}) as unknown as FormRules;
export const optionDetails=(field:FieldDefinition)=>(field.config.option_details||[]) as unknown as OptionDetail[];
export const empty=(value:JsonValue|undefined)=>value==null||typeof value==='string'&&!value.trim()||Array.isArray(value)&&!value.length;
const fold=(v:string)=>v.trim().replace(/[A-Z]/g,c=>c.toLowerCase());
export const equal=(a:JsonValue|undefined,b:JsonValue|undefined)=>typeof a==='string'&&typeof b==='string'?fold(a)===fold(b):JSON.stringify(a??null)===JSON.stringify(b??null);
export function matches(group:ConditionGroup,values:Record<string,JsonValue>):boolean {
  const results=(group.conditions||[]).map(c=>{const a=values[c.field]??null,b=c.value??null;switch(c.operator){case'eq':return equal(a,b);case'neq':return !equal(a,b);case'in':return Array.isArray(b)&&b.some(v=>equal(a,v));case'not_in':return Array.isArray(b)&&!b.some(v=>equal(a,v));case'empty':return empty(a);case'not_empty':return !empty(a);default:if(typeof a!=='number'||typeof b!=='number')return false;return c.operator==='gt'?a>b:c.operator==='gte'?a>=b:c.operator==='lt'?a<b:c.operator==='lte'?a<=b:false;}}).concat((group.groups||[]).map(g=>matches(g,values)));
  return !!results.length&&(group.match==='any'?results.some(Boolean):results.every(Boolean));
}
export function fieldState(field:FieldDefinition,values:Record<string,JsonValue>):FieldState {
  const r=fieldRules(field);const visible=!r.visible_when||matches(r.visible_when,values),enabled=visible&&(!r.enabled_when||matches(r.enabled_when,values));
  return {visible,enabled,required:visible&&enabled&&(field.required||!!r.required_when&&matches(r.required_when,values))};
}
export const optionLabel=(field:FieldDefinition,value:JsonValue)=>optionDetails(field).find(d=>equal(d.value,value))?.label||String(value??'');
export const availableOptions=(field:FieldDefinition,values:Record<string,JsonValue>)=>(field.config.options as string[]||[]).filter(value=>{const d=optionDetails(field).find(d=>equal(d.value,value));return !d?.available_when||matches(d.available_when,values);});
function refs(group?:ConditionGroup):string[]{return group?[...(group.conditions||[]).map(c=>c.field),...(group.groups||[]).flatMap(refs)]:[];}
export function dependencies(fields:FieldDefinition[]){
  const deps=Object.fromEntries(fields.map(f=>[f.key,new Set<string>()]));
  for(const f of fields){const r=fieldRules(f);for(const key of [...refs(r.visible_when),...refs(r.enabled_when),...refs(r.required_when)])deps[f.key].add(key);for(const d of optionDetails(f)){for(const key of refs(d.available_when))deps[f.key].add(key);for(const key of Object.keys(d.fill||{}))deps[key]?.add(f.key);}if(f.field_type==='relationship')for(const m of (f.config.auto_fill||[]) as unknown as {target_field:string}[])deps[m.target_field]?.add(f.key);}
  return deps;
}
export function orderedFields(fields:FieldDefinition[]):FieldDefinition[]{
  const deps=dependencies(fields),done=new Set<string>(),result:FieldDefinition[]=[];
  while(result.length<fields.length){const next=fields.filter(f=>!done.has(f.key)&&[...deps[f.key]].every(k=>done.has(k)));if(!next.length)throw new Error('Circular or missing form dependencies. Ask an administrator to check the module rules.');for(const f of next){done.add(f.key);result.push(f);}}
  return result;
}
/** Local preview only. Server repeats validation against the atomic latest row. */
export function prepareForm(fields:FieldDefinition[],current:Record<string,JsonValue>,input:Record<string,JsonValue>){
  const values={...current,...input},changed=new Set(Object.keys(input));
  const groups=[...new Set(fields.map(f=>fieldRules(f).exclusive_group).filter((g):g is string=>!!g))];
  for(const group of groups){const members=fields.filter(f=>fieldRules(f).exclusive_group===group);const checked=members.filter(f=>changed.has(f.key)&&values[f.key]===true);if(checked.length===1)for(const f of members)if(f.key!==checked[0].key){values[f.key]=false;changed.add(f.key);}}
  const resets=new Set<string>();
  for(const f of orderedFields(fields)){
    const state=fieldState(f,values);
    if(!state.visible&&writable(f)){if(!empty(values[f.key])){resets.add(f.key);values[f.key]=null;changed.add(f.key);}continue;}
    if(f.field_type==='select'&&state.visible){
      if(!empty(values[f.key])&&!availableOptions(f,values).some(v=>equal(v,values[f.key]))){resets.add(f.key);values[f.key]=null;changed.add(f.key);}
      if(changed.has(f.key)&&!equal(values[f.key],current[f.key])){const details=optionDetails(f),selected=details.find(d=>equal(d.value,values[f.key]));for(const key of new Set(details.flatMap(d=>Object.keys(d.fill||{}))))if(!Object.hasOwn(input,key)){const next=selected?.fill?.[key]??null;if(empty(next)&&!empty(values[key]))resets.add(key);values[key]=next;changed.add(key);}}
    }
  }
  const errors=fields.filter(f=>writable(f)&&fieldState(f,values).required&&empty(values[f.key])).map(f=>`${f.label} is required for the current selections.`);
  for(const group of groups){const members=fields.filter(f=>fieldRules(f).exclusive_group===group&&fieldState(f,values).visible);const count=members.filter(f=>values[f.key]===true).length;if(count>1||!count&&members.some(f=>fieldRules(f).exclusive_required))errors.push(`Choose ${count>1?'only one':'one'} option in ${group}.`);}
  return {values,resets:[...resets],errors};
}
