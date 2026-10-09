<script lang="ts">
  import { onMount } from 'svelte';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import X from '@lucide/svelte/icons/x';
  import { driveQuota, saveDriveQuota, driveSize, type DriveQuota } from '../../api/drive';
  import { focusDialog } from '../../util/focusDialog';
  let { userUid, userName, onClose }: { userUid: string; userName: string; onClose: () => void } = $props();
  let data = $state<DriveQuota | null>(null); let gigabytes = $state<number | undefined>(10);
  let useDefault = $state(true); let busy = $state(false); let error = $state(''); let notice = $state('');
  const percent = $derived(data ? Math.min(100, data.quota_bytes ? data.used_bytes / data.quota_bytes * 100 : data.used_bytes ? 100 : 0) : 0);
  onMount(() => void load());
  async function load() { busy = true; error = ''; try { data = await driveQuota(userUid); useDefault = !data.assigned; gigabytes = data.quota_bytes / 1024 ** 3; } catch (reason) { error = reason instanceof Error ? reason.message : 'Could not load quota.'; } finally { busy = false; } }
  async function save(event: SubmitEvent) {
    event.preventDefault(); if (!data) return;
    const bytes = useDefault ? null : Math.round(Number(gigabytes) * 1024 ** 3);
    if (bytes !== null && (!Number.isSafeInteger(bytes) || bytes < 0 || gigabytes === undefined)) { error = 'Enter a valid storage allowance.'; return; }
    busy = true; error = ''; notice = '';
    try { data = await saveDriveQuota(userUid, data.revision, bytes); gigabytes = data.quota_bytes / 1024 ** 3; useDefault = !data.assigned; notice = `Storage allowance for ${userName} saved.`; }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Could not save quota.'; }
    finally { busy = false; }
  }
</script>

<div class="quota-backdrop"><div class="quota-dialog" role="dialog" aria-modal="true" aria-label="Drive storage allowance" tabindex="-1" use:focusDialog onkeydown={(event) => { if (event.key === 'Escape' && !busy) onClose(); }}>
  <header><HardDrive size={22} /><div><h2>Drive storage</h2><p>{userName}</p></div><button aria-label="Close storage settings" disabled={busy} onclick={onClose}><X size={18} /></button></header>
  {#if data}
    <div class="usage"><strong>{driveSize(data.used_bytes)} <span>of {driveSize(data.quota_bytes)} used</span></strong><progress aria-label="Current storage usage" max="100" value={percent}></progress><small>Includes all file versions and trash.{data.reserved_bytes ? ` ${driveSize(data.reserved_bytes)} reserved by pending uploads.` : ''}</small></div>
    <form onsubmit={save}><label class="default"><input type="checkbox" bind:checked={useDefault} disabled={busy} />Use default allowance ({driveSize(data.default_quota_bytes)})</label>
      <label>Storage allowance (GB)<input type="number" min="0" step="any" bind:value={gigabytes} disabled={useDefault || busy} required={!useDefault} /></label>
      <p class="hint">1 GB = 1,024 MB. Set 0 to stop new uploads. Reducing the allowance never deletes existing files; users can still preview, download, and empty trash. There is no separate per-file size cap.</p>
      {#if notice}<p class="notice" role="status">{notice}</p>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <footer><button type="button" disabled={busy} onclick={() => void load()}>Reload settings</button><button class="save" disabled={busy}>{busy ? 'Saving…' : 'Save storage allowance'}</button></footer>
    </form>
  {:else}<p class="loading">{error || 'Loading storage…'}</p>{#if error}<button onclick={() => void load()}>Retry</button>{/if}{/if}
</div></div>

<style>
  .quota-backdrop { position: fixed; inset: 0; z-index: 130; display: grid; place-items: center; padding: 1rem; background: #0007; }
  .quota-dialog { width: min(32rem,100%); max-height: 90dvh; overflow: auto; padding: 1.4rem; border: 1px solid var(--collab-line); border-radius: 1rem; background: var(--collab-bg); color: var(--collab-text); box-shadow: 0 1rem 4rem #0003; }
  header { display: flex; align-items: center; gap: .8rem; padding-bottom: 1rem; border-bottom: 1px solid var(--collab-line); } header div { flex: 1; min-width: 0; } h2 { margin: 0; font-size: 1.15rem; } p { margin: .3rem 0; overflow-wrap: anywhere; } header p, small, .hint, .usage span { color: var(--collab-muted); } .usage { margin: 1.4rem 0; } .usage strong { font-size: 1rem; } .usage span { font-weight: 400; } progress { width: 100%; height: .4rem; margin: .8rem 0; accent-color: var(--collab-icon); } small, .hint { display: block; font-size: .8rem; line-height: 1.6; }
  label { display: grid; gap: .5rem; font-size: .9rem; margin: 1.2rem 0; } label.default { display: flex; align-items: center; } input[type=checkbox] { width: 1rem; height: 1rem; accent-color: var(--collab-icon); } input[type=number] { width: 100%; box-sizing: border-box; padding: .7rem; border: 1px solid var(--collab-line); border-radius: .5rem; background: var(--collab-input); color: var(--collab-text); font: inherit; } input:disabled { opacity: .6; } footer { display: flex; justify-content: flex-end; gap: .5rem; flex-wrap: wrap; margin-top: 1.4rem; } button { font: inherit; font-size: .85rem; padding: .6rem .8rem; border: 1px solid var(--collab-line); border-radius: .5rem; background: var(--collab-surface); color: var(--collab-text); cursor: pointer; } button:disabled { opacity: .5; cursor: default; } button.save { background: var(--collab-selected); color: var(--collab-selected-text); border-color: var(--collab-selected-border); } .error { color: var(--danger); font-size: .85rem; } .notice { color: var(--success); font-size: .85rem; } .loading { margin-top: 1rem; }
</style>
