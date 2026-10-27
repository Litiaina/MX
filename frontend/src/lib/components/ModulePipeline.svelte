<script lang="ts">
 import {onDestroy,untrack} from 'svelte';
 import {loadWorkflow,loadWorkQueue,type WorkflowConfiguration,type WorkQueue} from '../api/workflows';
 let {moduleUid,revision=0,onOpen}:{moduleUid:string;revision?:number;onOpen:(uid:string)=>void}=$props();
 type Lane={key:string;name:string;total:number;data:WorkQueue|null;after:string;previous:string[];loading:boolean;error:string};
 let lanes=$state<Lane[]>([]);let config=$state<WorkflowConfiguration|null>(null);let search=$state('');let applied=$state('');let actionable=$state(false);let loading=$state(true);let error=$state('');let changed=$state(false);
 let controller:AbortController|undefined;let generation=0;let dead=false;let seenRevision:number|undefined;
 $effect(()=>{const uid=moduleUid;untrack(()=>void initialize(uid));});
 $effect(()=>{const next=revision;untrack(()=>{if(seenRevision!==undefined&&next!==seenRevision)changed=true;seenRevision=next;});});
 onDestroy(()=>{dead=true;generation++;controller?.abort();});
 async function initialize(uid=moduleUid){controller?.abort();const request=new AbortController();controller=request;const current=++generation;loading=true;error='';changed=false;
  try{const result=await loadWorkflow(uid,request.signal);if(dead||current!==generation)return;config=result;const counts=result.definition.enabled?await loadWorkQueue(uid,{q:applied,actionable,counts_only:true},request.signal):null;if(dead||current!==generation)return;lanes=result.definition.enabled?result.definition.states.map(s=>({...s,total:counts?.stage_totals?.find(c=>c.state===s.key)?.total||0,data:null,after:'',previous:[],loading:true,error:''})):[];
   // Two bounded stage requests at a time; every lane replaces its 40-row page.
   const jobs=[...lanes];await Promise.all(Array.from({length:Math.min(2,jobs.length)},async()=>{while(jobs.length&&!request.signal.aborted){const lane=jobs.shift()!;await loadLane(lane,request.signal,current);}}));
  }catch(e){if(!request.signal.aborted&&!dead)error=e instanceof Error?e.message:String(e);}finally{if(current===generation)loading=false;}}
 async function loadLane(lane:Lane,signal=controller?.signal,current=generation){lane.loading=true;lane.error='';try{const result=await loadWorkQueue(moduleUid,{state:lane.key,after:lane.after,q:applied,actionable},signal);if(dead||current!==generation)return;lane.data=result;if(result.configuration_revision!==config?.revision)changed=true;}catch(e){if(!signal?.aborted&&!dead)lane.error=e instanceof Error?e.message:String(e);}finally{if(!dead&&current===generation)lane.loading=false;}}
 function next(lane:Lane){if(!lane.data?.next_cursor)return;lane.previous.push(lane.after);lane.after=lane.data.next_cursor;void loadLane(lane);}
 function previous(lane:Lane){if(!lane.previous.length)return;lane.after=lane.previous.pop()!;void loadLane(lane);}
 function filter(event?:SubmitEvent){event?.preventDefault();applied=search.trim();void initialize();}
</script>
<div class="pipeline-tools"><form role="search" onsubmit={filter}><label class="sr-only" for="pipeline-search">Search pipeline titles</label><input id="pipeline-search" type="search" maxlength="200" bind:value={search} placeholder="Find an opportunity or request…"/><button class="button" type="submit">Search</button></form><label class="checkbox"><input type="checkbox" bind:checked={actionable} onchange={()=>filter()}/>Needs my action</label><button class="button" type="button" disabled={loading} onclick={()=>void initialize()}>Refresh pipeline</button></div>
{#if error}<p class="notice error" role="alert">{error}</p>{/if}
{#if changed}<div class="notice" role="status">This process changed. Your position is preserved.<button class="button small" onclick={()=>void initialize()}>Load latest</button></div>{/if}
{#if !config&&loading}<p role="status">Loading process stages…</p>{:else if config&&!config.definition.enabled}<div class="empty-state"><h2>Pipeline unavailable</h2><p>An administrator must enable this module’s Process & actions. Records and their history have not been changed.</p></div>{:else}
<div class="pipeline-board" aria-label={config?.definition.name||'Pipeline board'} role="region">
 {#each lanes as lane (lane.key)}<section class="pipeline-lane" aria-label={lane.name}>
  <header><h2>{lane.name}</h2><span class="stage-count" aria-label={`${lane.name} count`}>{lane.total}</span></header>
  <div class="lane-cards" aria-busy={lane.loading}>{#if lane.error}<p class="notice error" role="alert">{lane.error}<button class="button small" onclick={()=>void loadLane(lane)}>Retry stage</button></p>{/if}
   {#each lane.data?.items||[] as item (item.uid)}<article class="pipeline-card" data-record-uid={item.uid}><button class="card-title" onclick={()=>onOpen(item.uid)}>{item.title}</button>{#if item.restarted}<p class="review-badge">Changed · review required</p>{/if}
    <div class="card-process"><span>{item.actions?.length?item.actions.map(a=>a.name).join(' · '):'View details'}</span>{#if item.actions?.some(a=>a.missing_fields.length)}<span class="missing">Information needed</span>{/if}</div>
    <button class="button small review-button" onclick={()=>onOpen(item.uid)}>Open & review</button>
   </article>{/each}
   {#if lane.loading&&!lane.data}<p class="lane-empty" role="status">Loading…</p>{:else if lane.data&&!lane.data.items.length}<p class="lane-empty">No matching items in this stage.</p>{/if}
  </div>
  {#if lane.previous.length||lane.data?.has_next}<footer><button class="button small" disabled={lane.loading||!lane.previous.length} onclick={()=>previous(lane)}>Previous</button><small>40 per page</small><button class="button small" disabled={lane.loading||!lane.data?.has_next} onclick={()=>next(lane)}>Next</button></footer>{/if}
 </section>{/each}
</div><p class="pipeline-hint">Open a card to review saved information and take a permitted action. Stage changes are verified by MX—not inferred from dragging a card.</p>{/if}
<style>
 .pipeline-tools{display:flex;align-items:center;gap:.65rem;flex-wrap:wrap;margin-bottom:.65rem}
 .pipeline-tools form{display:flex;gap:.4rem;flex:1;min-width:min(100%,14rem);max-width:36rem}.pipeline-tools input[type=search]{width:100%;min-width:0;font-size:.8125rem}
 .checkbox{display:flex;align-items:center;gap:.4rem;font-size:.8125rem}
 .pipeline-board{display:flex;gap:.65rem;overflow:auto;align-items:stretch;min-height:26rem;padding:.1rem .1rem .65rem;scrollbar-gutter:stable;scrollbar-width:thin;scrollbar-color:var(--line-strong) transparent}
 .pipeline-lane{flex:1 0 18rem;min-width:18rem;max-width:27rem;background:var(--surface-2);border:1px solid var(--line);border-radius:var(--card-radius);display:flex;flex-direction:column;max-height:max(26rem,calc(100dvh - 14rem))}
 .pipeline-lane>header{display:flex;align-items:center;justify-content:space-between;gap:.6rem;padding:.7rem .75rem}
 .pipeline-lane h2{font-size:.8125rem;font-weight:650;margin:0;overflow-wrap:anywhere}
 .stage-count{padding:0 .15rem;font-size:.75rem;font-weight:650;font-variant-numeric:tabular-nums;color:var(--muted)}
 .lane-cards{padding:0 .5rem .5rem;overflow:auto;flex:1;scrollbar-width:thin}
 .pipeline-card{background:var(--surface);border:1px solid var(--line);border-radius:var(--card-radius);padding:.85rem .9rem;margin-bottom:.5rem;box-shadow:0 1px 2px #00000003}
 .pipeline-card:last-child{margin-bottom:0}.pipeline-card:focus-within{border-color:var(--line-strong)}
 .card-title{display:block;font:inherit;font-size:.875rem;font-weight:650;line-height:1.45;text-align:left;color:var(--text);background:none;border:0;padding:0;cursor:pointer;overflow-wrap:anywhere}.card-title:hover{text-decoration:underline}
 .card-process{display:grid;gap:.3rem;margin:.55rem 0;color:var(--muted);font-size:.8125rem;line-height:1.5}
 .missing,.review-badge{color:var(--warning);font-size:.8125rem}
 .review-button{min-height:1.8rem;justify-content:start;padding:.25rem 0;border-color:transparent;background:transparent;color:var(--primary-text);font-size:.75rem}.review-button:hover{background:var(--surface-2)}
 .lane-empty{padding:1rem .25rem;color:var(--muted);font-size:.8125rem}.pipeline-hint{font-size:.75rem;color:var(--muted);margin:.25rem 0;line-height:1.5}
 footer{padding:.5rem;display:flex;align-items:center;justify-content:space-between;gap:.3rem;border-top:1px solid var(--line)}footer small{font-size:.6875rem;color:var(--muted)}
 @media(max-width:600px){.pipeline-lane{max-height:70dvh;flex-basis:85vw;min-width:min(16rem,85vw)}.pipeline-tools form{max-width:none}}
</style>
