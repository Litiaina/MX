<script lang="ts">
  import { onMount } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Search from '@lucide/svelte/icons/search';
  import DriveFileIcon from './DriveFileIcon.svelte';
  import DrivePagination from './DrivePagination.svelte';
  import { focusDialog } from '../util/focusDialog';
  import { driveList, driveSize, type DriveItem, type DriveTab, type DrivePage } from '../api/drive';
  let { foldersOnly = false, excludeUid = '', excludeUids = [], confirmLabel = 'Move here', onChoose = (_item: DriveItem | null) => undefined, onChooseFolder = (_uid: string | null) => undefined, onClose }: { foldersOnly?: boolean; excludeUid?: string; excludeUids?: string[]; confirmLabel?: string; onChoose?: (item: DriveItem | null) => void; onChooseFolder?: (uid: string | null) => void; onClose: () => void } = $props();
  let tab = $state<DriveTab>('mine'); let folder = $state<string | null>(null); let query = $state(''); let page = $state<DrivePage | null>(null); let loading = $state(false); let error = $state(''); let offset = $state(0); let generation = 0;
  onMount(() => { void load(); });
  async function load() { const token = ++generation; loading = true; error = ''; try { const result = await driveList(tab, folder, query, offset, {kind: foldersOnly ? 'folder' : '', sort: 'name_asc'}); if (token === generation) {page = result; offset = result.offset;} } catch (reason) { if (token === generation) error = reason instanceof Error ? reason.message : 'Cannot load Drive.'; } finally { if (token === generation) loading = false; } }
  function navigate(uid: string | null) { folder = uid; offset = 0; query = ''; void load(); }
</script>
<div class="drive-picker-backdrop">
  <div class="drive-picker" use:focusDialog role="dialog" aria-modal="true" aria-label={foldersOnly ? 'Move to folder' : 'Choose from MX Drive'} tabindex="-1" onkeydown={(event) => { if (event.key === 'Escape') onClose(); }}>
    <header><div><h2>{foldersOnly ? 'Move to folder' : 'Choose from MX Drive'}</h2><p>{foldersOnly ? 'Choose the destination in your Drive.' : 'A separate copy will be attached. The original stays in Drive.'}</p></div><button type="button" aria-label="Close Drive picker" onclick={onClose}><X size={18} /></button></header>
    {#if !foldersOnly}<nav aria-label="Drive source"><button class:active={tab === 'mine'} onclick={() => { tab = 'mine'; navigate(null); }}>My Drive</button><button class:active={tab === 'shared'} onclick={() => { tab = 'shared'; navigate(null); }}>Shared with me</button></nav>{/if}
    <form onsubmit={(event) => { event.preventDefault(); offset = 0; void load(); }}><Search size={16} /><input aria-label="Search Drive files" placeholder="Search files…" bind:value={query} /><button type="submit">Search</button></form>
    <div class="breadcrumbs"><button onclick={() => navigate(null)}>My Drive</button>{#each page?.breadcrumbs || [] as crumb}<ChevronRight size={12} /><button onclick={() => navigate(crumb.uid)}>{crumb.name}</button>{/each}</div>
    {#if error}<p role="alert">{error}</p><button onclick={() => void load()}>Retry</button>{/if}
    <div class="picker-files" aria-busy={loading}>
      {#each (page?.items || []).filter((item) => item.uid !== excludeUid && !excludeUids.includes(item.uid) && (!foldersOnly || item.kind === 'folder')) as item (item.uid)}
        <button class="picker-file" onclick={() => item.kind === 'folder' ? navigate(item.uid) : onChoose(item)}><DriveFileIcon {item} /><span><strong>{item.name}</strong><small>{item.kind === 'folder' ? 'Folder' : driveSize(item.size)}</small></span>{#if item.kind === 'folder'}<ChevronRight size={16} />{/if}</button>
      {:else}<p class="empty">{loading ? 'Loading files…' : 'No files or folders here.'}</p>{/each}
    </div>
    {#if page}<div class="picker-pagination"><DrivePagination total={page.total} {offset} limit={page.limit} {loading} onPage={(next)=>{offset=next;void load();}}/></div>{/if}
    <footer>{#if foldersOnly}<button class="choose" disabled={loading} onclick={() => onChooseFolder(folder)}>{confirmLabel}</button>{:else}<button onclick={onClose}>Cancel</button>{/if}</footer>
  </div>
</div>
<style>
  .drive-picker-backdrop { position: fixed; inset: 0; z-index: 115; background: #0008; display: grid; place-items: center; padding: 1rem; }
  .drive-picker { width: min(40rem, 100%); max-height: 90dvh; display: flex; flex-direction: column; background: var(--surface); color: var(--text); border: 1px solid var(--line); border-radius: .8rem; box-shadow: 0 1rem 4rem #0004; overflow: hidden; }
  header, footer { display: flex; align-items: center; justify-content: space-between; gap: .6rem; padding: 1rem; flex-shrink: 0; } h2 { margin: 0; font-size: 1.15rem; } p { color: var(--muted); font-size: .85rem; margin: .4rem 0 0; }
  nav { display: flex; padding: 0 1rem; gap: .5rem; } button { border: 1px solid var(--line); border-radius: .4rem; background: var(--surface); color: var(--text); padding: .5rem .7rem; display: inline-flex; align-items: center; gap: .5rem; cursor: pointer; } button:hover { background: var(--surface-2); } button.active, button.choose { background: var(--collab-selected); color: var(--collab-selected-text); border-color: var(--collab-selected-border); } button:disabled { opacity: .45; cursor: default; }
  form { display: flex; align-items: center; gap: .5rem; padding: .8rem 1rem; } input { min-width: 0; width: 100%; background: var(--surface-2); color: var(--text); border: 1px solid var(--line); padding: .5rem; border-radius: .4rem; }
  .breadcrumbs { display: flex; flex-wrap: wrap; align-items: center; gap: .15rem; padding: 0 1rem .5rem; font-size: .8rem; } .breadcrumbs button { border: 0; padding: .3rem; }
  .picker-files { overflow: auto; min-height: 10rem; padding: .5rem 1rem; } .picker-file { width: 100%; text-align: left; margin: .2rem 0; } .picker-file span { flex: 1; min-width: 0; } strong { display: block; overflow-wrap: anywhere; } small { display: block; color: var(--muted); margin-top: .2rem; } .empty { padding: 2rem; text-align: center; }
  footer { border-top: 1px solid var(--line); }
  .picker-pagination {padding:0 1rem;flex-shrink:0;}
</style>
