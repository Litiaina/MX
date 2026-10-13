<script lang="ts">
  import type {FieldDefinition,JsonValue} from '../../api/domain';
  let {field,value,label='Value',onChange,disabled=false}:{field:FieldDefinition;value:JsonValue;label?:string;disabled?:boolean;onChange:(value:JsonValue)=>void}=$props();
</script>
<label>{label}
  {#if field.field_type==='boolean'}<select {disabled} value={String(value??'')} onchange={e=>onChange(e.currentTarget.value===''?null:e.currentTarget.value==='true')}><option value="">Empty</option><option value="true">Yes / true</option><option value="false">No / false</option></select>
  {:else if field.field_type==='select'}<select {disabled} value={String(value??'')} onchange={e=>onChange(e.currentTarget.value||null)}><option value="">Empty</option>{#each (field.config.options as JsonValue[]||[]) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>
  {:else}<input {disabled} type={field.field_type==='date'?'date':['integer','decimal'].includes(field.field_type)?'number':'text'} step={field.field_type==='integer'?1:'any'} value={String(value??'')} oninput={e=>onChange(e.currentTarget.value===''?null:['integer','decimal'].includes(field.field_type)?Number(e.currentTarget.value):e.currentTarget.value)}/>{/if}
</label>
<style>label{display:grid;gap:.3rem;min-width:0;font-size:.8rem}input,select{width:100%;min-width:0}</style>
