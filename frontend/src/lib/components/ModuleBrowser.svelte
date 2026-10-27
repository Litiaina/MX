<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import X from '@lucide/svelte/icons/x';
  import type { ModuleDefinition } from '../api/domain';
  import { focusDialog } from '../util/focusDialog';
  import { dismissBackdrop } from '../util/dismissBackdrop';
  import { matchingModules, moduleGroup, orderedModules } from '../util/moduleNavigation';
  import ModuleIcon from './ModuleIcon.svelte';

  let { modules, selectedModuleUid, onChoose, onClose }: {
    modules: ModuleDefinition[]; selectedModuleUid: string;
    onChoose: (uid: string) => void; onClose: () => void;
  } = $props();
  const pageSize = 40;
  let query = $state(''), group = $state(''), page = $state(0);
  const available = $derived(orderedModules(modules));
  const groups = $derived([...new Set(available.map(moduleGroup).filter(Boolean))].sort((a,b) => a.localeCompare(b)));
  const results = $derived(matchingModules(available, query, group));
  const lastPage = $derived(Math.max(0, Math.ceil(results.length / pageSize) - 1));
  const currentPage = $derived(Math.min(page, lastPage));
  const visible = $derived(results.slice(currentPage * pageSize, (currentPage + 1) * pageSize));
  let list: HTMLDivElement;
  let searchInput: HTMLInputElement;
  function changePage(next: number) { page = next; list?.scrollTo({top: 0}); }
</script>

<div class="module-browser-backdrop" use:dismissBackdrop={onClose}>
  <div class="module-browser" role="dialog" aria-modal="true" aria-label="Browse modules" tabindex="-1" use:focusDialog onkeydown={event => { if (event.key === 'Escape') { event.stopPropagation(); onClose(); } }}>
    <header><div><h2>Modules</h2><p>Find a workspace by name, description or group.</p></div><button class="button icon-only" type="button" aria-label="Close module browser" onclick={onClose}><X size={18}/></button></header>
    <div class="module-browser-filters">
      <label class="module-browser-search"><span class="sr-only">Search modules</span><Search size={17}/><input type="search" aria-label="Search modules" placeholder="Search modules…" bind:this={searchInput} bind:value={query} oninput={() => page = 0}/></label>
      {#if groups.length}<label>Group<select aria-label="Filter modules by group" bind:value={group} onchange={() => page = 0}><option value="">All groups</option>{#each groups as value}<option value={value}>{value}</option>{/each}</select></label>{/if}
    </div>
    <p class="module-browser-count" role="status">{results.length} module{results.length === 1 ? '' : 's'}{query || group ? ' matching' : ' available'}</p>
    <div class="module-browser-results" bind:this={list}>
      {#each visible as module (module.uid)}
        <button class:current={module.uid === selectedModuleUid} type="button" aria-label={`Open ${module.name}`} onclick={() => onChoose(module.uid)}>
          <i style={`color:${module.color}`}><ModuleIcon name={module.icon} size={22}/></i>
          <span><strong>{module.name}</strong><small>{module.description || 'Open workspace'}</small><em>{moduleGroup(module) || 'Ungrouped'}{module.uid === selectedModuleUid ? ' · Current module' : ''}</em></span>
        </button>
      {:else}
        <div class="module-browser-empty"><h3>{available.length ? 'No matching modules' : 'No modules available'}</h3><p>{available.length ? 'Try a different name or group.' : 'An administrator can create modules and assign your access.'}</p>{#if query || group}<button class="button" type="button" onclick={() => { query = ''; group = ''; page = 0; searchInput?.focus(); }}>Clear filters</button>{/if}</div>
      {/each}
    </div>
    {#if results.length > pageSize}<footer aria-label="Module pages"><span>{currentPage * pageSize + 1}–{Math.min((currentPage + 1) * pageSize, results.length)} of {results.length}</span><button class="button small" type="button" disabled={currentPage === 0} onclick={() => changePage(currentPage - 1)}>Previous</button><button class="button small" type="button" disabled={currentPage === lastPage} onclick={() => changePage(currentPage + 1)}>Next</button></footer>{/if}
  </div>
</div>

<style>
  .module-browser-backdrop{position:fixed;inset:0;z-index:150;display:grid;place-items:center;padding:1rem;background:rgba(0,0,0,.45);}
  .module-browser{width:min(58rem,100%);max-height:calc(100dvh - 2rem);min-height:0;display:flex;flex-direction:column;border:1px solid var(--line);border-radius:var(--corner-radius,.75rem);background:var(--surface);color:var(--text);box-shadow:0 1.5rem 4rem #0004;padding:1.1rem;box-sizing:border-box;}
  header{display:flex;align-items:flex-start;justify-content:space-between;gap:1rem;}h2{margin:0;font-size:1.2rem;}p{margin:.3rem 0 .8rem;font-size:.85rem;color:var(--muted);}header .button{flex-shrink:0;}
  .module-browser-filters{display:flex;align-items:end;gap:.75rem;flex-wrap:wrap;}.module-browser-filters label{display:grid;gap:.25rem;min-width:0;font-size:.8rem;}.module-browser-filters .module-browser-search{display:flex;align-items:center;flex:1 1 15rem;gap:.5rem;border:1px solid var(--line);border-radius:var(--corner-radius,.4rem);padding:0 .7rem;}.module-browser-search:focus-within{outline:2px solid var(--primary);outline-offset:1px;}.module-browser-search input{border:0;box-shadow:none;background:transparent;width:100%;min-width:0;padding-inline:0;outline:none;}.module-browser-search input:focus{box-shadow:none;outline:none;}select{max-width:100%;}.module-browser-count{margin:.75rem 0;}
  .module-browser-results{min-height:0;overflow:auto;overscroll-behavior:contain;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:.65rem;padding:.2rem;}.module-browser-results > button{display:flex;align-items:flex-start;gap:.75rem;min-width:0;text-align:left;border:1px solid var(--line);border-radius:var(--corner-radius,.5rem);background:var(--surface);color:var(--text);padding:.85rem;}.module-browser-results > button:hover,.module-browser-results > button.current{border-color:var(--primary);background:color-mix(in srgb,var(--primary) 6%,var(--surface));}.module-browser-results i{display:grid;place-items:center;flex:0 0 2rem;min-height:2rem;font-style:normal;}.module-browser-results span{display:grid;gap:.3rem;min-width:0;overflow-wrap:anywhere;}.module-browser-results strong{font-size:.95rem;}.module-browser-results small,.module-browser-results em{font-size:.8rem;font-style:normal;color:var(--muted);}.module-browser-empty{grid-column:1/-1;padding:1rem;text-align:center;}.module-browser-empty h3{margin:.5rem 0;font-size:1rem;}
  footer{display:flex;align-items:center;justify-content:flex-end;gap:.5rem;padding-top:.85rem;margin-top:.85rem;border-top:1px solid var(--line);font-size:.8rem;}footer span{margin-right:auto;}
  @media(max-width:600px){.module-browser-backdrop{padding:.5rem;}.module-browser{max-height:calc(100dvh - 1rem);padding:.8rem;}.module-browser-results{grid-template-columns:minmax(0,1fr);}}
  @media(max-width:400px){header p{display:none;}.module-browser-filters{margin-top:.75rem;}.module-browser-results > button{flex-direction:column;gap:.4rem;}.module-browser-results i{flex-basis:auto;}.module-browser-results span{width:100%;}}
</style>
