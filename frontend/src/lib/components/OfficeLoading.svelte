<script lang="ts">
  import { onMount } from 'svelte';
  import FileText from '@lucide/svelte/icons/file-text';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import { officeLoadingSteps, loadingBytes, loadingPercent, type OfficeLoadingProgress } from '../office/loading';
  let { progress, fileName = '', failed = false, onCancel, onRetry, onDownload, onReturn, downloadLabel='Download loaded copy', returnLabel='Return to Drive', failureDetail='See the error details. Use Document options to download a loaded copy or return to Drive.' }: { progress: OfficeLoadingProgress; fileName?: string; failed?: boolean; onCancel?:()=>void;onRetry?:()=>void;onDownload?:()=>void;onReturn?:()=>void;downloadLabel?:string;returnLabel?:string;failureDetail?:string } = $props();
  let elapsed = $state(0);
  const index = $derived(officeLoadingSteps.findIndex(step => step.phase === progress.phase));
  const percent = $derived(loadingPercent(progress.loaded, progress.total));
  onMount(() => {
    const started = performance.now();
    const timer = setInterval(() => { if (!failed) elapsed = Math.floor((performance.now() - started) / 1000); }, 1000);
    return () => clearInterval(timer);
  });
</script>

<section class="office-loading" class:failed aria-label="Office loading progress">
  <div class="loading-card">
    <div class="document-icon"><FileText size={24}/></div>
    <h1>{failed ? 'The editor could not finish opening' : 'Opening your document'}</h1>
    {#if fileName}<p class="file-name" title={fileName}>{fileName}</p>{/if}
    <div class="current-status" role="status" aria-live="polite" aria-atomic="true">
      <strong>{failed ? 'Stopped at' : `Step ${index + 1} of ${officeLoadingSteps.length} ·`} {officeLoadingSteps[index].title}</strong>
      <p>{failed ? failureDetail : progress.detail}</p>
    </div>
    {#if !failed}
      {#if percent === null}<div class="loading-track" role="progressbar" aria-label="Current loading step"><span></span></div>{:else}<progress aria-label={progress.phase === 'document' ? 'Document download progress' : 'Editor resources download progress'} max="100" value={percent}></progress>{/if}
      <div class="transfer-detail">
        <span>{progress.loaded !== undefined ? `${loadingBytes(progress.loaded)}${progress.total ? ` of ${loadingBytes(progress.total)}` : ''} · ${progress.phase === 'engine' ? 'editor resources' : 'document'}` : 'Please keep this tab open'}</span>
        <span>{percent !== null ? `${percent}% · ` : ''}{elapsed}s elapsed</span>
      </div>
    {/if}
    <ol aria-label="Loading stages">
      {#each officeLoadingSteps as step, number}
        {@const state = number < index ? 'complete' : number === index ? failed ? 'failed' : 'current' : 'waiting'}
        <li class:complete={state === 'complete'} class:current={state === 'current'} class:stopped={state === 'failed'} aria-current={number === index ? 'step' : undefined}>
          <span class="step-icon" aria-hidden="true">{#if state === 'complete'}<Check size={13}/>{:else if state === 'failed'}<X size={13}/>{:else}{number + 1}{/if}</span>
          <span>{step.title}</span><small>{state === 'complete' ? 'Done' : state === 'current' ? 'In progress' : state === 'failed' ? 'Stopped' : 'Waiting'}</small>
        </li>
      {/each}
    </ol>
    <p class="loading-note">{failed ? 'No save is performed by opening this file.' : elapsed >= 20 ? 'First launches and large documents can take longer. Download progress and device initialization are separate steps.' : 'The editor runs on your device. Resources come from MX or its local browser cache; no Internet service is required.'}</p>
    <div class="recovery-actions">
      {#if !failed && onCancel}<button onclick={onCancel}>Cancel opening</button>{/if}
      {#if failed && onRetry}<button onclick={onRetry}>Retry opening</button>{/if}
      {#if failed && onDownload}<button onclick={onDownload}>{downloadLabel}</button>{/if}
      {#if failed && onReturn}<button onclick={onReturn}>{returnLabel}</button>{/if}
    </div>
  </div>
</section>

<style>
  .office-loading{position:absolute;inset:0;display:grid;place-items:center;overflow:auto;padding:20px;box-sizing:border-box;background:var(--bg);color:var(--text);text-align:left;}
  .loading-card{width:100%;max-width:460px;margin:auto;box-sizing:border-box;padding:26px;background:var(--surface);border:1px solid var(--line);border-radius:var(--card-radius);box-shadow:var(--shadow-popover);}
  .document-icon{width:46px;height:46px;display:grid;place-items:center;background:color-mix(in srgb, var(--primary) 10%, var(--surface));color:var(--primary-text);border-radius:12px;margin-bottom:15px;}
  h1{font-size:1.25rem;font-weight:600;line-height:1.3;margin:0 0 8px;}.file-name{margin:0 0 18px;color:var(--muted);font-size:0.8125rem;overflow-wrap:anywhere;}.current-status strong{font-size:0.8125rem;font-weight:600;}.current-status p{margin:7px 0 13px;color:var(--muted);line-height:1.5;font-size:0.75rem;}
  progress{display:block;width:100%;height:5px;border:0;border-radius:8px;overflow:hidden;accent-color:var(--primary-text);background:var(--surface-3);}progress::-webkit-progress-bar{background:var(--surface-3);border-radius:8px;}progress::-webkit-progress-value{background:var(--primary);border-radius:8px;}progress::-moz-progress-bar{background:var(--primary);border-radius:8px;}
  .loading-track{height:5px;border-radius:8px;overflow:hidden;background:var(--surface-3);}.loading-track>span{display:block;height:100%;width:25%;border-radius:8px;background:var(--primary);animation:loading 1.7s linear infinite;}
  .transfer-detail{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;margin-top:8px;color:var(--muted);font-size:0.6875rem;font-variant-numeric:tabular-nums;}ol{padding:0;list-style:none;margin:19px 0 0;border-top:1px solid var(--line);}li{display:flex;align-items:center;gap:10px;font-size:0.75rem;min-height:34px;color:var(--muted);}li:first-child{padding-top:5px;}li>span:nth-child(2){flex:1;}li small{font-size:0.625rem;white-space:nowrap;}.step-icon{display:grid;place-items:center;width:20px;height:20px;flex-shrink:0;border:1px solid var(--line);border-radius:50%;font-size:0.625rem;font-variant-numeric:tabular-nums;}.complete .step-icon{background:var(--success-surface);color:var(--success);border-color:color-mix(in srgb, var(--success) 35%, var(--line));}.current{color:var(--primary-text);font-weight:600;}.current .step-icon{background:color-mix(in srgb, var(--primary) 10%, var(--surface));border-color:var(--primary-text);}.stopped{color:var(--danger);}.stopped .step-icon{border-color:var(--danger);}.loading-note{font-size:0.6875rem;color:var(--muted);line-height:1.5;margin:16px 0 0;}
  @keyframes loading{from{transform:translateX(-100%);}to{transform:translateX(400%);}}
  .recovery-actions{display:flex;gap:8px;flex-wrap:wrap;margin-top:12px;}.recovery-actions button{font:inherit;font-size:0.75rem;padding:7px 10px;border:1px solid var(--line-strong);border-radius:6px;background:transparent;color:inherit;cursor:pointer;}.recovery-actions button:focus-visible{outline:2px solid var(--primary);outline-offset:2px;}
  :global(html[data-reduced-motion='true']) .loading-track>span{animation:none;margin-left:35%;}
  @media(prefers-reduced-motion:reduce){.loading-track>span{animation:none;margin-left:35%;}}
  @media(max-width:480px){.office-loading{padding:12px;}.loading-card{padding:20px;}h1{font-size:1.125rem;}}
</style>
