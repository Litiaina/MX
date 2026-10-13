<script lang="ts">
  import type {FieldDefinition,JsonValue} from '../api/domain';
  import {availableOptions,optionLabel,equal} from '../util/formRules';
  let {field,value=null,values={},disabled=false,required=false,onChange}:{field:FieldDefinition;value?:JsonValue;values?:Record<string,JsonValue>;disabled?:boolean;required?:boolean;onChange:(value:JsonValue)=>void}=$props();
  const id=$props.id();const options=$derived(availableOptions(field,values));
  const presentation=$derived(String(field.config.presentation||'select'));
  function choose(event:Event,option:string){const control=event.currentTarget as HTMLInputElement;const next=control.checked?option:null;for(const input of control.closest('fieldset')?.querySelectorAll<HTMLInputElement>('input')||[])input.checked=equal(input.value,value);onChange(next);}
</script>
{#if presentation==='select'}
  <label><span>{field.label}{#if required}<b class="required-mark"> *</b>{/if}</span><select {disabled} {required} value={String(value??'')} onchange={event=>{const next=event.currentTarget.value;event.currentTarget.value=String(value??'');onChange(next||null);}}><option value="">Select…</option>{#if value&&!options.includes(String(value))}<option value={String(value)} disabled>Unavailable — {optionLabel(field,value)}</option>{/if}{#each options as option}<option value={option}>{optionLabel(field,option)}</option>{/each}</select></label>
{:else}
  <fieldset class="choice-field" role={presentation==='radio'?'radiogroup':'group'} aria-label={field.label}><legend>{field.label}{#if required}<b class="required-mark"> *</b>{/if}</legend><div>{#each options as option}<label class="choice" class:selected={equal(value,option)}><input type={presentation==='radio'?'radio':'checkbox'} name={id} value={option} checked={equal(value,option)} {disabled} required={presentation==='radio'&&required} onchange={event=>choose(event,option)}/><span>{optionLabel(field,option)}</span></label>{/each}</div>{#if !options.length}<small class="muted">Choose the controlling fields first; no options are currently available.</small>{/if}</fieldset>
{/if}
<style>label{display:grid;gap:.3rem;min-width:0}select{width:100%;min-width:0}.choice-field{min-width:0;margin:0;padding:.7rem;border:1px solid var(--line);border-radius:.5rem}.choice-field legend{font-size:.85rem;font-weight:600}.choice-field>div{display:flex;flex-wrap:wrap;gap:.5rem}.choice{display:flex;align-items:center;gap:.4rem;border:1px solid var(--line);border-radius:.4rem;background:var(--surface-2);padding:.5rem .65rem;cursor:pointer}.choice.selected{border-color:var(--primary);background:color-mix(in srgb,var(--primary) 10%,var(--surface))}.choice input{width:1rem;height:1rem;margin:0;flex-shrink:0}.choice:has(input:disabled){cursor:default;opacity:.7}</style>
