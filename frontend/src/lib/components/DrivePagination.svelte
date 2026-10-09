<script lang="ts">
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  let { total, offset, limit, loading = false, onPage, onLimit }: {total:number;offset:number;limit:number;loading?:boolean;onPage:(offset:number)=>void;onLimit?:(limit:number)=>void} = $props();
  const pages=$derived(Math.max(1,Math.ceil(total/limit)));
  const current=$derived(Math.min(pages,Math.floor(offset/limit)+1));
  const numbers=$derived([...new Set([1,current-1,current,current+1,pages])].filter(n=>n>=1&&n<=pages).sort((a,b)=>a-b));
  let jump=$state(1);$effect(()=>{jump=current;});
  function go(n:number){if(Number.isFinite(n))onPage((Math.max(1,Math.min(pages,Math.trunc(n)))-1)*limit);}
</script>
<nav class="drive-pagination" aria-label="Drive pagination">
  <span class="range" aria-live="polite">{total ? `${offset+1}–${Math.min(offset+limit,total)} of ${total.toLocaleString()}` : '0 items'}</span>
  {#if onLimit}<label>Per page<select aria-label="Drive items per page" value={limit} disabled={loading} onchange={(e)=>onLimit?.(Number(e.currentTarget.value))}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option></select></label>{/if}
  <div class="page-controls"><button aria-label="Previous page" title="Previous page" disabled={loading||current===1} onclick={()=>go(current-1)}><ChevronLeft size={16}/></button>
  {#each numbers as n,index}{#if index&&n>numbers[index-1]+1}<span aria-hidden="true">…</span>{/if}<button class:current={n===current} aria-current={n===current?'page':undefined} aria-label={`Page ${n}`} disabled={loading} onclick={()=>go(n)}>{n}</button>{/each}
  <button aria-label="Next page" title="Next page" disabled={loading||current===pages} onclick={()=>go(current+1)}><ChevronRight size={16}/></button></div>
  {#if pages>5}<form onsubmit={(e)=>{e.preventDefault();go(jump);}}><label>Go to<input aria-label="Go to Drive page" type="number" min="1" max={pages} bind:value={jump} disabled={loading}/></label><button disabled={loading}>Go</button></form>{/if}
</nav>
<style>
  .drive-pagination {display:flex;flex-wrap:wrap;align-items:center;gap:.65rem;padding:.75rem 0;color:var(--collab-muted);font-size:.78rem;min-width:0;} .range {margin-right:auto;font-variant-numeric:tabular-nums;white-space:nowrap;} label,form,.page-controls {display:flex;align-items:center;gap:.4rem;} label {white-space:nowrap;} button,input,select {font:inherit;color:var(--collab-text);background:var(--collab-bg);border:1px solid var(--collab-line);border-radius:.4rem;min-height:2rem;} button {padding:.35rem .55rem;display:inline-flex;align-items:center;justify-content:center;cursor:pointer;} button:hover:not(:disabled){background:var(--collab-surface);} button:disabled {opacity:.4;cursor:default;} button.current {background:var(--collab-selected);color:var(--collab-selected-text);border-color:var(--collab-selected-border);} select {width:auto;padding:.3rem .4rem;} input {width:3.8rem;min-width:0;padding:.3rem;} button:focus-visible,input:focus-visible,select:focus-visible {outline:2px solid var(--collab-selected-border);outline-offset:2px;} @media(max-width:600px){.range {width:100%;} .page-controls {margin-left:auto;}form{margin-left:auto;}}
</style>
