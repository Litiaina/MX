<script lang="ts">
  import { tick } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import File from '@lucide/svelte/icons/file';
  import { driveSize } from '../api/drive';
  import { uploadEta } from '../util/uploadProgress';
  import type { DriveUploads } from '../drive/uploads.svelte';
  let { uploads, avoidComposer = false }: { uploads: DriveUploads; avoidComposer?: boolean } = $props();
  let composerClearance = $state(0);
  // Keep messaging usable while transfers continue: float above its actual
  // composer, including attachments, mobile sizing and font preferences.
  $effect(() => {
    if (!avoidComposer) { composerClearance = 0; return; }
    let stopped = false; let resize: ResizeObserver | undefined; let mutations: MutationObserver | undefined; let observed: Element | null = null;
    const update = () => {
      const composer = document.querySelector('.message-composer');
      composerClearance = composer ? Math.max(0, innerHeight - composer.getBoundingClientRect().top + 12) : 0;
    };
    const attach = () => {
      const composer = document.querySelector('.message-composer'); if (stopped || composer === observed) return;
      resize?.disconnect(); observed = composer;
      if (composer) { resize = new ResizeObserver(update); resize.observe(composer); }
      update();
    };
    void tick().then(() => { if (stopped) return; mutations = new MutationObserver(attach); mutations.observe(document.body, { childList: true, subtree: true }); attach(); });
    window.addEventListener('resize', update);
    return () => { stopped = true; mutations?.disconnect(); resize?.disconnect(); window.removeEventListener('resize', update); };
  });
  const complete = $derived(uploads.transfers.filter((item) => item.status === 'done').length);
  const running = $derived(uploads.transfers.filter((item) => item.status === 'uploading' || item.status === 'queued').length);
</script>

{#if uploads.transfers.length || uploads.preparing || uploads.preparationError}
  <section class="upload-panel" aria-label="Upload queue" style:bottom={composerClearance ? `${composerClearance}px` : undefined} style:--composer-clearance={composerClearance ? `${composerClearance}px` : '1rem'}>
    <header><button class="heading" aria-label="Toggle upload details" aria-expanded={!uploads.collapsed} onclick={() => uploads.toggle()}><strong>{running ? `Uploading ${running} ${running === 1 ? 'file' : 'files'}` : uploads.preparing ? 'Preparing uploads…' : 'Uploads'}</strong><small>{complete} of {uploads.transfers.length} complete</small></button><button class="icon" aria-label={uploads.collapsed ? 'Expand uploads' : 'Minimize uploads'} onclick={() => uploads.toggle()}>{#if uploads.collapsed}<ChevronUp size={18} />{:else}<ChevronDown size={18} />{/if}</button><button class="icon" aria-label="Clear completed uploads" title="Clear completed uploads" disabled={!complete} onclick={() => uploads.clearCompleted()}><X size={18} /></button></header>
    <div class="details" hidden={uploads.collapsed}>
      {#if uploads.preparationError}<div class="problem" role="alert">{uploads.preparationError}<button onclick={() => uploads.clearError()} aria-label="Dismiss upload error"><X size={16} /></button></div>{/if}
      {#if uploads.preparing}<p>Preparing folder structure…</p>{/if}
      {#each uploads.transfers as transfer (transfer.uid)}
        <div class="transfer" data-status={transfer.status}>
          <div class="progress-icon" role="progressbar" aria-label={`Upload progress for ${transfer.file.name}`} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(transfer.status === 'done' ? 100 : Math.min(100,transfer.progress.loaded / Math.max(1,transfer.file.size)*100))}>
            <svg class="progress-ring" viewBox="0 0 40 40" aria-hidden="true"><circle class="track" cx="20" cy="20" r="18" /><circle class="value" cx="20" cy="20" r="18" pathLength="100" stroke-dasharray="100" stroke-dashoffset={transfer.status === 'done' ? 0 : 100-Math.min(100,transfer.progress.loaded / Math.max(1,transfer.file.size)*100)} /></svg><File size={18} aria-hidden="true" />
          </div>
          <div class="transfer-info"><strong title={transfer.relativePath || transfer.file.name}>{transfer.relativePath || transfer.file.name}</strong>
            <small>{transfer.status === 'done' ? 'Uploaded' : transfer.status === 'queued' ? 'Queued' : transfer.status === 'paused' ? 'Paused · completed parts retained' : transfer.status === 'failed' ? transfer.error : transfer.progress.phase === 'initializing' ? 'Starting upload…' : transfer.progress.phase === 'finalizing' ? 'Finalizing with N1…' : `${driveSize(transfer.progress.loaded)} / ${driveSize(transfer.file.size)}`}</small>
            {#if transfer.status === 'uploading' && transfer.progress.phase === 'uploading'}<small class="telemetry"><span>{(transfer.progress.bytesPerSecond / 1_000_000).toFixed(2)} MB/s</span><span>{uploadEta(transfer.progress.etaSeconds)}</span></small>{/if}
          </div>
          {#if transfer.status === 'done'}<Check size={19} aria-label="Upload complete" />{:else if transfer.status === 'failed' || transfer.status === 'paused'}<button class="icon" aria-label={`Resume ${transfer.file.name}`} title="Resume upload" onclick={() => uploads.resume(transfer)}><Play size={17} /></button>{:else}<button class="icon" aria-label={`Pause ${transfer.file.name}`} title="Pause upload" onclick={() => uploads.pause(transfer)}><Pause size={17} /></button>{/if}
        </div>
      {/each}
    </div>
    {#if !uploads.collapsed && running}<footer>Uploads continue while you work in other MX modules.</footer>{/if}
  </section>
{/if}

<style>
  .upload-panel { --surface:var(--collab-bg); --surface-2:var(--collab-surface); --text:var(--collab-text); --muted:var(--collab-muted); --line:var(--collab-line); }
  .upload-panel { position: fixed; right: 1.25rem; bottom: 1rem; width: min(25rem,calc(100vw - 2rem)); max-height:calc(100dvh - var(--composer-clearance) - .5rem); display:flex; flex-direction:column; z-index: 65; color: var(--text); background: var(--surface); border: 1px solid var(--line); border-radius: .8rem; box-shadow: 0 .5rem 2rem #0003; overflow: hidden; font-size: .85rem; }
  header { display:flex; flex-shrink:0; align-items:center; gap:.3rem; padding:.6rem .7rem; background:var(--surface-2); border-bottom:1px solid var(--line); }
  button { font:inherit; color:inherit; background:transparent; border:0; cursor:pointer; }
  button:disabled { opacity:.4; cursor:default; } button:focus-visible { outline:2px solid var(--collab-icon); outline-offset:-2px; } .icon { width:2rem; height:2rem; display:grid; place-items:center; border-radius:.4rem; flex-shrink:0; } .icon:hover:not(:disabled) { background:var(--line); }
  .heading { flex:1; min-width:0; text-align:left; padding:.2rem; } strong { display:block; font-weight:600; } small { display:block; color:var(--muted); font-size:.75rem; line-height:1.5; }
  .details { min-height:0; max-height:min(24rem,55dvh); overflow-y:auto; overscroll-behavior:contain; } .details[hidden] { display:none; }
  .transfer { display:flex; gap:.65rem; align-items:center; padding:.8rem; border-bottom:1px solid var(--line); } .transfer-info { flex:1; min-width:0; } .transfer-info strong { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; } .transfer-info small { overflow-wrap:anywhere; } .telemetry { display:flex; justify-content:space-between; gap:.5rem; flex-wrap:wrap; font-variant-numeric:tabular-nums; }
  .progress-icon { position:relative; width:2.5rem; height:2.5rem; flex-shrink:0; display:grid; place-items:center; color:var(--collab-icon); } .progress-ring { position:absolute; inset:0; width:100%; height:100%; transform:rotate(-90deg); fill:none; stroke-width:2; } .track { stroke:var(--line); } .value { stroke:currentColor; stroke-linecap:round; transition:stroke-dashoffset .2s linear; } [data-status="paused"] .progress-icon { color:var(--muted); }
  footer, p { margin:0; padding:.6rem .8rem; color:var(--muted); font-size:.72rem; } .problem { padding:.7rem; color:var(--danger); display:flex; align-items:center; justify-content:space-between; }
  @media(max-width:600px) { .upload-panel { right:.5rem; bottom:.5rem; width:calc(100vw - 1rem); } .details { max-height:40dvh; } }
</style>
