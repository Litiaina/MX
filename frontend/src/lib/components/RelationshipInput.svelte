<script lang="ts">
  import {relationshipOptions,type LinkedRecord} from '../api/relationships';
  import type {FieldDefinition,JsonValue,RelationshipState} from '../api/domain';
  let {field,value=null,labels,disabled=false,onChange}:{field:FieldDefinition;value?:JsonValue;labels?:RelationshipState;disabled?:boolean;onChange:(value:JsonValue)=>void}=$props();
  let query=$state('');let page=$state(1);let items=$state<LinkedRecord[]>([]);let known=$state<Record<string,string>>({});let more=$state(false);let busy=$state(false);let error=$state('');
  let retry=$state(0);
  const multiple=$derived(field.config.multiple===true);
  const selected=$derived(Array.isArray(value)?value.map(String):typeof value==='string'&&value?[value]:[]);
  $effect(()=>{for(const item of labels?.items||[])known[item.uid]=item.label;});
  $effect(()=>{const id=field.uid;const search=query;const number=page;void retry;if(disabled||labels?.restricted)return;const controller=new AbortController();const timer=setTimeout(()=>{busy=true;error='';void relationshipOptions(id,search,number,controller.signal).then(result=>{if(controller.signal.aborted)return;items=result.items;more=result.has_next;for(const item of items)known[item.uid]=item.label;}).catch(reason=>{if(!controller.signal.aborted)error=reason instanceof Error?reason.message:'Could not load related records.';}).finally(()=>{if(!controller.signal.aborted)busy=false;});},200);return()=>{clearTimeout(timer);controller.abort();};});
  function choose(uid:string){if(!uid)return;onChange(multiple?[...new Set([...selected,uid])].sort():uid);}
  function remove(uid:string){const next=selected.filter(id=>id!==uid);onChange(multiple?next:next[0]||null);}
</script>
<div class="relationship-input" role="group" aria-label={field.label}>
  <span class="label">{field.label}{field.required?' *':''}</span>
  {#if labels?.restricted}<p class="hint">Linked module access is restricted.</p>{:else}
    <div class="linked-items">{#each selected as uid}<span>{known[uid]||'Linked record'}{#if !disabled}<button type="button" aria-label={`Remove ${known[uid]||'linked record'}`} onclick={()=>remove(uid)}>×</button>{/if}</span>{:else}{#if disabled}<span class="hint">No linked record</span>{/if}{/each}</div>
    {#if !disabled}<input type="search" aria-label={`Search ${field.label}`} placeholder="Search linked records…" value={query} oninput={event=>{query=event.currentTarget.value;page=1;}} />
      <select aria-label={`Choose ${field.label}`} value="" onchange={event=>{choose(event.currentTarget.value);event.currentTarget.value='';}} disabled={busy||selected.length>=100}>
        <option value="">{busy?'Loading…':multiple?'Link another record…':'Choose a record…'}</option>{#each items.filter(item=>!selected.includes(item.uid)) as item}<option value={item.uid}>{item.label}</option>{/each}
      </select>
      {#if !busy&&!items.length&&!error}<small>No matching records.</small>{/if}
      {#if page>1||more}<div class="pages"><button type="button" disabled={busy||page===1} onclick={()=>page--}>Previous</button><small>Page {page}</small><button type="button" disabled={busy||!more} onclick={()=>page++}>Next</button></div>{/if}
      {#if error}<p role="alert">{error}</p><button type="button" onclick={()=>retry++}>Retry search</button>{/if}
    {/if}
  {/if}
</div>
<style>
  .relationship-input{display:flex;flex-direction:column;gap:.4rem;min-width:0}.label{font-size:.85rem;font-weight:600}.hint,small{color:var(--muted);font-size:.8rem;margin:0}.linked-items{display:flex;gap:.35rem;flex-wrap:wrap;min-width:0}.linked-items>span{background:var(--surface-2);border:1px solid var(--line);border-radius:.35rem;padding:.3rem .45rem;overflow-wrap:anywhere;display:flex;align-items:center;gap:.4rem}.linked-items button{border:0;background:transparent;color:var(--text);font-size:1.1rem;cursor:pointer}.pages{display:flex;align-items:center;justify-content:space-between;gap:.4rem}.pages button{border:1px solid var(--line);border-radius:.3rem;padding:.35rem;background:var(--surface);color:var(--text)}p[role=alert]{color:var(--danger);font-size:.8rem}input,select{width:100%;min-width:0}
</style>
