<script lang="ts">
  import { onDestroy } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import ModuleIcon from './ModuleIcon.svelte';
  import type { GlobalSearchResult } from '../api/domain';
  import { globalSearch } from '../api/workspace';

  let query = $state(''); let results = $state<GlobalSearchResult[]>([]);
  let open = $state(false); let loading = $state(false); let error = $state('');
  let timer: number | undefined; let request = 0;

  onDestroy(() => window.clearTimeout(timer));

  function schedule() {
    window.clearTimeout(timer); error = ''; open = query.trim().length >= 2;
    if (!open) { results = []; loading = false; return; }
    loading = true;
    timer = window.setTimeout(() => void search(), 280);
  }
  async function search() {
    const current = ++request; const value = query.trim();
    try { const response = await globalSearch(value); if (current === request) results = response.results; }
    catch (reason) { if (current === request) error = reason instanceof Error ? reason.message : 'Search failed.'; }
    finally { if (current === request) loading = false; }
  }
  function choose(result: GlobalSearchResult) {
    query = ''; results = []; open = false;
    location.hash = `module/${encodeURIComponent(result.module_uid)}?record=${encodeURIComponent(result.uid)}`;
  }
</script>

<div class="global-search" class:open>
  <span aria-hidden="true"><Search size={15} /></span>
  <input bind:value={query} oninput={schedule} onfocus={() => open = query.trim().length >= 2} onblur={() => window.setTimeout(() => open = false, 160)} onkeydown={(event) => { if (event.key === 'Escape') { open = false; event.currentTarget.blur(); } }} type="search" placeholder="Search all modules…" aria-label="Search all modules" autocomplete="off" />
  {#if open}<div class="global-search-results" role="listbox">
    <header><strong>Search across modules</strong><small>Only records you may view appear</small></header>
    {#if loading}<div class="search-skeleton"><i></i><i></i><i></i></div>
    {:else if error}<p class="error-text">{error}</p>
    {:else if !results.length}<p class="muted">No accessible records match “{query.trim()}”.</p>
    {:else}{#each results as result}<button type="button" role="option" aria-selected="false" onclick={() => choose(result)}><span style={`color:${result.color}`}><ModuleIcon name={result.icon} size={16} /></span><div><strong>{result.label}</strong><small>{result.module_name} · {result.context}</small></div></button>{/each}{/if}
  </div>{/if}
</div>
