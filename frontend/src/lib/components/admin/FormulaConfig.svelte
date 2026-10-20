<script lang="ts">
  import type {FieldDefinition,JsonValue} from '../../api/domain';
  let {config=$bindable({}),fields,fieldKey=''}:{config:Record<string,JsonValue>;fields:FieldDefinition[];fieldKey?:string}=$props();
  const parts=$derived((Array.isArray(config.parts)?config.parts:[]) as string[]);
  const candidates=$derived(fields.filter(f=>f.active&&f.key!==fieldKey&&!['attachments','relationship','lookup','rollup'].includes(f.field_type)));
  const numericCandidates=$derived(fields.filter(f=>f.active&&f.key!==fieldKey&&(['integer','decimal','auto_number','rollup','boolean'].includes(f.field_type)||(f.field_type==='formula'&&f.config.result_type!=='text')||(f.field_type==='select'&&Array.isArray(f.config.option_details)&&f.config.option_details.some(option=>option&&typeof option==='object'&&!Array.isArray(option)&&typeof option.number==='number')))));
  function changePart(index:number,key:string){config={...config,parts:parts.map((p,i)=>i===index?key:p)};}
  function mode(value:string){config=value==='text'?{...config,result_type:'text',parts:config.parts||[],separator:config.separator??' ',skip_empty:true}:{...config,result_type:'number',expression:config.expression||'',decimals:config.decimals??2};}
</script>
<fieldset class="formula-config"><legend>Calculated value</legend>
  <label>Result type<select aria-label="Formula result type" value={String(config.result_type||'number')} onchange={event=>mode(event.currentTarget.value)}><option value="number">Number / arithmetic</option><option value="text">Text / combine fields</option></select></label>
  {#if config.result_type==='text'}
    <p>Combine fields in this order. For a full name, choose First name, Middle name and Last name. Blank middle names can be skipped.</p>
    {#each parts as part,index}<div class="part"><label>Part {index+1}<select aria-label={`Combined text field ${index+1}`} value={part} onchange={event=>changePart(index,event.currentTarget.value)}><option value="">Choose a field…</option>{#each candidates as field}<option value={field.key}>{field.label}</option>{/each}</select></label><button class="button small" type="button" aria-label={`Remove text part ${index+1}`} onclick={()=>config={...config,parts:parts.filter((_,i)=>i!==index)}}>Remove</button></div>{/each}
    <button class="button small" type="button" disabled={!candidates.length||parts.length>=64} onclick={()=>config={...config,parts:[...parts,'']}}>Add text part</button>
    <label>Between fields<input aria-label="Combined text separator" value={String(config.separator??' ')} maxlength="64" oninput={event=>config={...config,separator:event.currentTarget.value}}/><small>One space gives a normal full name. Use a comma, dash or another separator if needed.</small></label>
    <label class="checkbox"><input type="checkbox" checked={config.skip_empty!==false} onchange={event=>config={...config,skip_empty:event.currentTarget.checked}}/>Skip empty fields</label>
  {:else}
    <label>Formula expression<input aria-label="Formula expression" value={String(config.expression||'')} placeholder="allowance - used" oninput={event=>config={...config,expression:event.currentTarget.value}}/></label>
    <label>Decimal places<input type="number" min="0" max="12" value={Number(config.decimals??2)} oninput={event=>config={...config,decimals:Number(event.currentTarget.value)}}/></label>
    <p>Use field keys with +, −, *, /, %, ^ and functions such as SUM, AVERAGE, MIN, MAX and ROUND. Choices use the numeric value configured under their Form rules. Linked totals can be used, but these live formulas are not sortable or searchable.</p>
    <div class="keys" aria-label="Available field keys">{#each numericCandidates as field}<span>{field.label}: <code>{field.key}</code></span>{/each}</div>
  {/if}
</fieldset>
<style>.formula-config{grid-column:1/-1;display:grid;gap:.75rem;border:1px solid var(--line);border-radius:.5rem;padding:1rem;min-width:0}.formula-config legend{font-weight:650}.formula-config label:not(.checkbox){display:grid;gap:.35rem;min-width:0}.formula-config input:not([type=checkbox]),select{width:100%;min-width:0}.formula-config p,.formula-config small{font-size:.8rem;color:var(--muted);line-height:1.5;margin:0}.part{display:grid;grid-template-columns:minmax(0,1fr) auto;align-items:end;gap:.5rem}.keys{display:flex;flex-wrap:wrap;gap:.5rem;font-size:.75rem;color:var(--muted)}.keys>span{background:var(--surface-2);padding:.3rem .5rem;border-radius:.3rem;overflow-wrap:anywhere}.checkbox{display:flex;gap:.5rem;align-items:center}@media(max-width:480px){.part{grid-template-columns:1fr}.part button{justify-self:start}}</style>
