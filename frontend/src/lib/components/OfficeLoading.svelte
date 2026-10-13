<script lang="ts">
  import { onMount } from 'svelte';
  import FileText from '@lucide/svelte/icons/file-text';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import { officeLoadingSteps, loadingBytes, loadingPercent, type OfficeLoadingProgress } from '../office/loading';
  let { progress, fileName = '', failed = false }: { progress: OfficeLoadingProgress; fileName?: string; failed?: boolean } = $props();
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
      <p>{failed ? 'See the error details. Use Document options to download a loaded copy or return to Drive.' : progress.detail}</p>
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
  </div>
</section>

<style>
  .office-loading{position:absolute;inset:0;display:grid;place-items:center;overflow:auto;padding:20px;box-sizing:border-box;background:#f7f8fa;color:#20242b;text-align:left;}
  .loading-card{width:100%;max-width:460px;margin:auto;box-sizing:border-box;padding:26px;background:#fff;border:1px solid #d9dee6;border-radius:14px;box-shadow:0 8px 32px #14284408;}
  .document-icon{width:46px;height:46px;display:grid;place-items:center;background:#eef3ff;color:#1d4ed8;border-radius:12px;margin-bottom:15px;}
  h1{font-size:20px;font-weight:600;line-height:1.3;margin:0 0 8px;}.file-name{margin:0 0 18px;color:#657285;font-size:13px;overflow-wrap:anywhere;}.current-status strong{font-size:13px;font-weight:600;}.current-status p{margin:7px 0 13px;color:#657285;line-height:1.5;font-size:12px;}
  progress{display:block;width:100%;height:5px;border:0;border-radius:8px;overflow:hidden;accent-color:#1d4ed8;background:#e7edf7;}progress::-webkit-progress-bar{background:#e7edf7;border-radius:8px;}progress::-webkit-progress-value{background:#1d4ed8;border-radius:8px;}progress::-moz-progress-bar{background:#1d4ed8;border-radius:8px;}
  .loading-track{height:5px;border-radius:8px;overflow:hidden;background:#e7edf7;}.loading-track>span{display:block;height:100%;width:25%;border-radius:8px;background:#5780da;animation:loading 1.7s linear infinite;}
  .transfer-detail{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;margin-top:8px;color:#657285;font-size:11px;font-variant-numeric:tabular-nums;}ol{padding:0;list-style:none;margin:19px 0 0;border-top:1px solid #e6eaf0;}li{display:flex;align-items:center;gap:10px;font-size:12px;min-height:34px;color:#657285;}li:first-child{padding-top:5px;}li>span:nth-child(2){flex:1;}li small{font-size:10px;white-space:nowrap;}.step-icon{display:grid;place-items:center;width:20px;height:20px;flex-shrink:0;border:1px solid #d9dee6;border-radius:50%;font-size:10px;font-variant-numeric:tabular-nums;}.complete .step-icon{background:#eef8f1;color:#24633b;border-color:#c4e3ce;}.current{color:#1d4ed8;font-weight:600;}.current .step-icon{background:#eef3ff;border-color:#7697de;}.stopped{color:#a32828;}.stopped .step-icon{border-color:#e1a2a2;}.loading-note{font-size:11px;color:#657285;line-height:1.5;margin:16px 0 0;}
  :global(html[data-theme='dark']) .office-loading{background:#16191d;color:#e5e7eb;}:global(html[data-theme='dark']) .loading-card{background:#20242a;border-color:#3e4754;box-shadow:none;}:global(html[data-theme='dark']) .document-icon,:global(html[data-theme='dark']) .current .step-icon{background:#23314b;color:#acc5ff;}:global(html[data-theme='dark']) .file-name,:global(html[data-theme='dark']) .current-status p,:global(html[data-theme='dark']) .transfer-detail,:global(html[data-theme='dark']) li,:global(html[data-theme='dark']) .loading-note{color:#a5b0bf;}:global(html[data-theme='dark']) ol,:global(html[data-theme='dark']) .step-icon{border-color:#3e4754;}:global(html[data-theme='dark']) li.current{color:#acc5ff;}:global(html[data-theme='dark']) .complete .step-icon{background:#1d3026;color:#a4d4b7;}:global(html[data-theme='dark']) .stopped{color:#ffb6b6;}
  :global(html[data-theme='dark']) .loading-track{background:#354052;}:global(html[data-theme='dark']) .loading-track>span{background:#92b5ff;}
  @keyframes loading{from{transform:translateX(-100%);}to{transform:translateX(400%);}}
  @media(prefers-reduced-motion:reduce){.loading-track>span{animation:none;margin-left:35%;}}
  @media(max-width:480px){.office-loading{padding:12px;}.loading-card{padding:20px;}h1{font-size:18px;}}
</style>
