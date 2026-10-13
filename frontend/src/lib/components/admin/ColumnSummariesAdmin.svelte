<script lang="ts">
  import { untrack } from 'svelte';
  import type { FieldDefinition } from '../../api/domain';
  import { updateField } from '../../api/workspace';
  import { canSummarize } from '../../util/columnSummaries';
  let { fields, moduleName, onSaved }: { fields: FieldDefinition[]; moduleName: string; onSaved: () => Promise<void> } = $props();
  let choices = $state<Record<string, { method: string; decimals: number }>>({});
  let busy = $state(''); let error = $state(''); let notice = $state('');
  const eligible = $derived(fields.filter(field => field.active && canSummarize(field)));
  $effect(() => {
    const values=eligible.map(field => [field.uid, { method: String(field.config.table_summary || 'none'), decimals: Number(field.config.summary_decimals ?? (field.field_type === 'integer' || field.field_type === 'auto_number' ? 0 : field.config.decimals ?? 2)) }] as const);
    untrack(() => { choices=Object.fromEntries(values); });
  });
  async function save(field: FieldDefinition) {
    busy=field.uid; error=''; notice='';
    try { const choice=choices[field.uid]; await updateField(field.uid,{config:{...field.config,table_summary:choice.method,summary_decimals:Number(choice.decimals)}}); await onSaved(); notice=`${field.label} summary saved.`; }
    catch(reason){error=reason instanceof Error?reason.message:'Could not save the column summary.';}
    finally{busy='';}
  }
</script>
<section class="panel column-summaries-admin" aria-label="Column summaries configuration">
  <div class="panel-heading"><div><p class="eyebrow">Module · {moduleName}</p><h2>Column summaries</h2><p class="muted">Optional totals below numeric columns. Calculations cover all matching records across every page, follow search and filters, and exclude trashed records. Count includes non-empty numeric values.</p></div></div>
  {#if error}<p role="alert" class="notice error">{error}</p>{/if}{#if notice}<p role="status" class="notice success">{notice}</p>{/if}
  {#each eligible as field}
    {#if choices[field.uid]}<div class="summary-setting"><strong>{field.label}</strong><label>Calculation<select aria-label={`Summary for ${field.label}`} bind:value={choices[field.uid].method} disabled={!!busy}><option value="none">None</option>{#if field.field_type!=='auto_number'}<option value="sum">Sum / total</option><option value="average">Average</option><option value="min">Minimum</option><option value="max">Maximum</option>{/if}<option value="count">Count values</option></select></label><label>Decimal places<input aria-label={`Summary decimals for ${field.label}`} type="number" min="0" max="12" bind:value={choices[field.uid].decimals} disabled={!!busy||['none','count'].includes(choices[field.uid].method)} /></label><button class="button primary small" disabled={!!busy} aria-label={`Save summary for ${field.label}`} onclick={()=>void save(field)}>{busy===field.uid?'Saving…':'Save summary'}</button></div>{/if}
  {:else}<p class="muted">Add an integer, decimal or formula field to configure column summaries.</p>{/each}
</section>
<style>.column-summaries-admin{margin-bottom:1rem}.summary-setting{display:grid;grid-template-columns:minmax(120px,1fr) minmax(140px,1fr) 120px auto;gap:.75rem;align-items:end;padding:.75rem 0;border-top:1px solid var(--line)}.summary-setting strong{align-self:center}.summary-setting label{min-width:0;display:grid;gap:.3rem;font-size:.8rem}.summary-setting select,.summary-setting input{width:100%;min-width:0}@media(max-width:700px){.summary-setting{grid-template-columns:minmax(0,1fr) minmax(0,1fr)}.summary-setting strong{grid-column:1/-1}.summary-setting button{grid-column:1/-1;justify-self:start}}</style>
