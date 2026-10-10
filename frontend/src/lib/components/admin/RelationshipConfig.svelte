<script lang="ts">
  import type {FieldDefinition,ModuleDefinition,JsonValue} from '../../api/domain';
  import {loadModuleSchema} from '../../api/workspace';
  let {kind,config=$bindable({}),modules,fields}:{kind:string;config:Record<string,JsonValue>;modules:ModuleDefinition[];fields:FieldDefinition[]}=$props();
  let targetFields=$state<FieldDefinition[]>([]);let error=$state('');
  const relationships=$derived(fields.filter(f=>f.active&&f.field_type==='relationship'));
  const target=$derived(kind==='relationship'?String(config.target_module_uid||''):String(relationships.find(f=>f.key===config.relationship_field)?.config.target_module_uid||''));
  $effect(()=>{const uid=target;let disposed=false;targetFields=[];error='';if(uid)void loadModuleSchema(uid).then(schema=>{if(!disposed)targetFields=schema.fields.filter(f=>f.active&&!['attachments','relationship','lookup','rollup'].includes(f.field_type));}).catch(reason=>{if(!disposed)error=String(reason);});return()=>{disposed=true;};});
</script>
<div class="relationship-config">
  {#if kind==='relationship'}
    <label>Linked module<select value={String(config.target_module_uid||'')} onchange={event=>config={...config,target_module_uid:event.currentTarget.value,label_field:''}}><option value="">Choose a module…</option>{#each modules.filter(m=>m.active) as module}<option value={module.uid}>{module.name}</option>{/each}</select></label>
    <label>Display field<select value={String(config.label_field||'')} onchange={event=>config={...config,label_field:event.currentTarget.value}}><option value="">Choose a display field…</option>{#each targetFields as field}<option value={field.key}>{field.label}</option>{/each}</select></label>
    <label class="checkbox"><input type="checkbox" checked={config.multiple===true} onchange={event=>config={...config,multiple:event.currentTarget.checked}} /> Allow multiple linked records</label>
    <p>Stores stable record IDs. Referenced records cannot be deleted until their active links are removed.</p>
  {:else}
    <label>Relationship<select value={String(config.relationship_field||'')} onchange={event=>config={...config,relationship_field:event.currentTarget.value,target_field:''}}><option value="">Choose a relationship…</option>{#each relationships as field}<option value={field.key}>{field.label}</option>{/each}</select></label>
    {#if kind==='rollup'}<label>Calculation<select value={String(config.function||'sum')} onchange={event=>config={...config,function:event.currentTarget.value}}>{#each ['sum','count','average','min','max'] as fn}<option value={fn}>{fn==='average'?'Average':fn==='count'?'Count linked records':fn[0].toUpperCase()+fn.slice(1)}</option>{/each}</select></label>{/if}
    {#if kind==='lookup'||config.function!=='count'}<label>Linked field<select value={String(config.target_field||'')} onchange={event=>config={...config,target_field:event.currentTarget.value}}><option value="">Choose a field…</option>{#each targetFields.filter(f=>kind==='lookup'||['integer','decimal','auto_number','formula'].includes(f.field_type)) as field}<option value={field.key}>{field.label}</option>{/each}</select></label>{/if}
    <p>Read-only and calculated from the linked records you are allowed to access.</p>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</div>
<style>.relationship-config{display:grid;gap:.65rem;grid-column:1/-1;min-width:0}.relationship-config label{min-width:0}p{color:var(--muted);font-size:.8rem;margin:0}p[role=alert]{color:var(--danger)}</style>
