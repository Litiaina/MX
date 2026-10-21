<script lang="ts">
  import { onMount } from 'svelte';
  import { loadUploadLimits, saveUploadLimits } from '../../api/workspace';
  import type { UploadLimits, UploadSettings } from '../../api/domain';
  let current = $state<UploadLimits | null>(null);
  type LimitMode = 'default' | 'custom' | 'unlimited';
  let recordMode = $state<LimitMode>('default'), chatMode = $state<LimitMode>('default');
  let recordMiB = $state<number | undefined>(100), chatMiB = $state<number | undefined>(100);
  let loading = $state(false), saving = $state(false), problem = $state(''), notice = $state('');
  let alert: HTMLDivElement;
  let alive = true;
  onMount(() => { void refresh(true); return () => { alive = false; }; });
  function useSaved(value: UploadLimits) {
    current = value;
    recordMode = mode(value.settings.record_max_size_mb);
    chatMode = mode(value.settings.collaboration_max_size_mb);
    recordMiB = value.settings.record_max_size_mb || value.defaults.record_max_size_mb || 100;
    chatMiB = value.settings.collaboration_max_size_mb || value.defaults.collaboration_max_size_mb || 100;
  }
  function mode(value: number | null): LimitMode { return value === null ? 'default' : value === 0 ? 'unlimited' : 'custom'; }
  function label(mib: number) { return mib === 0 ? 'No file-size limit' : `${mib} MiB per file`; }
  async function refresh(replaceDraft = false) {
    loading = true;
    try { const value = await loadUploadLimits(); if (!alive) return; if (replaceDraft || !current) useSaved(value); else current = value; problem = ''; }
    catch (reason) { if (alive) problem = reason instanceof Error ? reason.message : 'Could not load upload limits.'; }
    finally { if (alive) loading = false; }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault(); if (!current || saving || loading) return;
    problem = ''; notice = '';
    if ((recordMode === 'custom' && (!recordMiB || recordMiB < 1)) || (chatMode === 'custom' && (!chatMiB || chatMiB < 1))) {
      problem = 'Enter a positive whole number of MiB, or choose No file-size limit.'; alert?.focus(); return;
    }
    const settings: UploadSettings = { record_max_size_mb:recordMode === 'default'?null:recordMode === 'unlimited'?0:recordMiB!, collaboration_max_size_mb:chatMode === 'default'?null:chatMode === 'unlimited'?0:chatMiB! };
    if (Object.values(settings).some(value => value !== null && (!Number.isSafeInteger(value) || !Number.isSafeInteger(value * 1024 ** 2)))) {
      problem = 'Enter a positive whole number of MiB, or choose No file-size limit.'; alert?.focus(); return;
    }
    saving = true;
    try { const value = await saveUploadLimits(current.revision, settings); if (alive) { useSaved(value); notice = 'Upload limits saved. New uploads use these values immediately; no restart is needed.'; } }
    catch (reason) { if (alive) { problem = reason instanceof Error ? reason.message : 'Upload limits were not saved.'; alert?.focus(); } }
    finally { if (alive) saving = false; }
  }
</script>

<section class="panel upload-limits-panel" aria-label="Upload limits">
  <div class="panel-heading"><div><h2>Upload limits</h2><p class="muted">Maximum size of each file attached to a module record or sent in Collaboration.</p></div></div>
  <form onsubmit={save} novalidate>
    <div bind:this={alert} tabindex="-1" class="upload-policy-feedback" role={problem?'alert':'status'} aria-live="polite">
      {#if problem}<p class="notice error">{problem}</p>{/if}{#if notice}<p class="notice success">{notice}</p>{/if}
    </div>
    {#if current}
      <div class="upload-limit-fields">
        <fieldset><legend>Module record attachments</legend>
          <label>Record attachment limit<select bind:value={recordMode} disabled={saving}><option value="default">Server default ({label(current.defaults.record_max_size_mb)})</option><option value="custom">Custom limit</option><option value="unlimited">No file-size limit</option></select></label>
          {#if recordMode === 'custom'}<label>Record maximum (MiB)<input type="number" min="1" step="1" bind:value={recordMiB} disabled={saving} /></label>{/if}
          <p class="muted">Currently enforced: <strong>{label(current.record_max_size_bytes / 1024 ** 2)}</strong>.</p>
        </fieldset>
        <fieldset><legend>Collaboration attachments</legend>
          <label>Collaboration file limit<select bind:value={chatMode} disabled={saving}><option value="default">Server default ({label(current.defaults.collaboration_max_size_mb)})</option><option value="custom">Custom limit</option><option value="unlimited">No file-size limit</option></select></label>
          {#if chatMode === 'custom'}<label>Collaboration maximum (MiB)<input type="number" min="1" step="1" bind:value={chatMiB} disabled={saving} /></label>{/if}
          <p class="muted">Currently enforced: <strong>{label(current.collaboration_max_size_bytes / 1024 ** 2)}</strong>.</p>
        </fieldset>
      </div>
      <p class="muted">Applies to new uploads immediately. Existing files and Drive quotas do not change.</p>
      {#if recordMode === 'unlimited' || chatMode === 'unlimited'}<p class="notice">No file-size limit removes MX’s per-file cap, not disk capacity or reverse-proxy limits. MX needs temporary disk space for each file; use Drive for resumable large transfers.</p>{/if}
      <details><summary>Server defaults and large files</summary><p class="muted">Server default uses <code>mx.config [n1]</code>; editing that file requires an MX restart. Custom limits and No file-size limit are saved in MX and take precedence until you select Server default again. A reverse proxy must also allow the request size.</p><p class="muted">Record/chat files are staged on the MX temporary disk and uploaded to N1 in bounded parts. These settings do not change file-count rules, permissions, storage quotas, spreadsheet-import or Office viewing budgets.</p></details>
    {:else if loading}<p role="status">Loading the current server limits…</p>{/if}
    <div class="inline-actions"><button class="button" type="button" onclick={() => void refresh()} disabled={loading || saving}>Refresh current limits</button><button class="button primary" disabled={!current || loading || saving}>{saving?'Saving limits…':'Save upload limits'}</button></div>
  </form>
</section>

<style>
  .upload-limits-panel{margin:.75rem 0 1rem;min-width:0;}.upload-limit-fields{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:1rem;}
  fieldset{min-width:0;margin:0;padding:1rem;border:1px solid var(--line);border-radius:var(--corner-radius);}
  legend{font-weight:600;padding:0 .3rem;}label{display:grid;gap:.4rem;margin-bottom:.75rem;}input,select{width:100%;min-width:0;box-sizing:border-box;}
  p{font-size:calc(.78rem * var(--font-scale));line-height:1.5;overflow-wrap:anywhere;}details{margin:0 0 .85rem;font-size:calc(.78rem * var(--font-scale));}summary{cursor:pointer;}.upload-policy-feedback:focus{outline:2px solid var(--primary);outline-offset:3px;}
  @media(max-width:700px){.upload-limit-fields{grid-template-columns:1fr;}}
</style>
