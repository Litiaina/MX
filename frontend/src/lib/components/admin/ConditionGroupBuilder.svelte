<script lang="ts">
  import type {FieldDefinition} from '../../api/domain';
  import type {Condition,ConditionGroup} from '../../util/formRules';
  import RuleValueInput from './RuleValueInput.svelte';
  import ConditionGroupBuilder from './ConditionGroupBuilder.svelte';
  let {group,fields,onChange,depth=1,disabled=false}:{group:ConditionGroup;fields:FieldDefinition[];onChange:(group:ConditionGroup)=>void;depth?:number;disabled?:boolean}=$props();
  const comparisons=[['eq','Equals'],['neq','Does not equal'],['in','Is one of'],['not_in','Is not one of'],['empty','Is empty'],['not_empty','Is not empty'],['gt','Greater than'],['gte','At least'],['lt','Less than'],['lte','At most']];
  function operators(field?:FieldDefinition){return comparisons.filter(([op])=>field?.field_type==='relationship'?['empty','not_empty'].includes(op):['gt','gte','lt','lte'].includes(op)?['integer','decimal'].includes(field?.field_type||''):true);}
  function update(index:number,change:Partial<Condition>){onChange({...group,conditions:group.conditions.map((c,i)=>i===index?{...c,...change}:c)});}
  function add(){if(!fields.length)return;onChange({...group,conditions:[...group.conditions,{field:fields[0].key,operator:fields[0].field_type==='relationship'?'not_empty':'eq',value:null}]});}
</script>
<div class="condition-group">
  <div class="group-heading"><label>Match<select {disabled} value={group.match} onchange={e=>onChange({...group,match:e.currentTarget.value as 'all'|'any'})}><option value="all">All conditions (AND)</option><option value="any">Any condition (OR)</option></select></label><button type="button" class="button small" disabled={disabled||!fields.length} onclick={add}>Add condition</button>{#if depth<4}<button type="button" class="button small" {disabled} onclick={()=>onChange({...group,groups:[...(group.groups||[]),{match:'all',conditions:[]}]})}>Add group</button>{/if}</div>
  {#each group.conditions as condition,index}
    {@const source=fields.find(f=>f.key===condition.field)}
    <div class="condition-row"><label>Control field<select {disabled} value={condition.field} onchange={e=>{const f=fields.find(f=>f.key===e.currentTarget.value);update(index,{field:e.currentTarget.value,operator:f?.field_type==='relationship'?'not_empty':'eq',value:null});}}><option value="">Choose field…</option>{#each fields as field}<option value={field.key}>{field.label}</option>{/each}</select></label>
      <label>Comparison<select {disabled} value={condition.operator} onchange={e=>update(index,{operator:e.currentTarget.value,value:['in','not_in'].includes(e.currentTarget.value)?[]:null})}>{#each operators(source) as [op,label]}<option value={op}>{label}</option>{/each}</select></label>
      {#if source&&!['empty','not_empty'].includes(condition.operator)}
        {#if ['in','not_in'].includes(condition.operator)}
          {#if source.field_type==='select'||source.field_type==='boolean'}<label>Allowed values<select multiple {disabled} onchange={e=>update(index,{value:[...e.currentTarget.selectedOptions].map(o=>source.field_type==='boolean'?o.value==='true':o.value)})}>{#each (source.field_type==='boolean'?[true,false]:source.config.options as string[]||[]) as option}<option value={String(option)} selected={Array.isArray(condition.value)&&condition.value.includes(option)}>{String(option)}</option>{/each}</select></label>
          {:else}<label>Values (comma separated)<input {disabled} value={(Array.isArray(condition.value)?condition.value:[]).join(', ')} oninput={e=>update(index,{value:e.currentTarget.value.split(',').map(v=>v.trim()).filter(Boolean).map(v=>['integer','decimal'].includes(source.field_type)?Number(v):v)})}/></label>{/if}
        {:else}<RuleValueInput field={source} value={condition.value??null} {disabled} onChange={value=>update(index,{value})}/>{/if}
      {/if}
      <button type="button" class="button small" aria-label="Remove condition" {disabled} onclick={()=>onChange({...group,conditions:group.conditions.filter((_,i)=>i!==index)})}>Remove</button>
    </div>
  {/each}
  {#each group.groups||[] as child,index}<div class="nested"><ConditionGroupBuilder group={child} {fields} depth={depth+1} {disabled} onChange={next=>onChange({...group,groups:group.groups?.map((g,i)=>i===index?next:g)})}/><button type="button" class="button small" {disabled} onclick={()=>onChange({...group,groups:group.groups?.filter((_,i)=>i!==index)})}>Remove group</button></div>{/each}
  {#if !group.conditions.length&&!group.groups?.length}<p class="muted small">Add a condition to define when this applies.</p>{/if}
</div>
<style>.condition-group{border:1px solid var(--line);border-radius:.5rem;padding:.75rem;min-width:0}.group-heading{display:flex;align-items:end;gap:.5rem;flex-wrap:wrap}.condition-row{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr) minmax(0,1fr) auto;gap:.5rem;align-items:end;margin-top:.65rem}label{display:grid;gap:.3rem;min-width:0;font-size:.8rem}input,select{width:100%;min-width:0}.nested{margin-top:.75rem;padding-left:.5rem;border-left:2px solid var(--line)}.nested>button{margin-top:.4rem}@media(max-width:650px){.condition-row{grid-template-columns:minmax(0,1fr) minmax(0,1fr)}.condition-row>button{justify-self:start}.condition-group{padding:.5rem}.group-heading>label{width:100%}}</style>
