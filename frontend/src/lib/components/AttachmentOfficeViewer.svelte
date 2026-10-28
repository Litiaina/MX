<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Download from '@lucide/svelte/icons/download';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import OfficeLoading from './OfficeLoading.svelte';
  import { apiFile } from '../api/client';
  import { download, issueMessageFilePreviewTicket, issueRecordAttachmentPreviewTicket } from '../api/workspace';
  import { OfficeEngine } from '../office/engine';
  import { officeExtension } from '../office/formats';
  import { openingDetail, type OfficeLoadingProgress } from '../office/loading';
  import { checkOfficeInputSize, officeResources } from '../office/resources';

  // Attachments are immutable copies, not Drive items. Never pretend that
  // their UIDs/revisions are writable Drive versions or persist private drafts.
  const params = new URLSearchParams(location.hash.slice(1));
  const recordSource = params.has('record') || params.has('attachment');
  const fileUid = params.get('message_file') || '';
  const recordUid = params.get('record') || '', attachmentUid = params.get('attachment') || '';
  const moduleUid = params.get('module') || '';
  const returnHref = recordSource ? moduleUid ? `/#module/${encodeURIComponent(moduleUid)}?record=${encodeURIComponent(recordUid)}` : `/#records?record=${encodeURIComponent(recordUid)}` : '/#collaboration';
  const returnLabel = recordSource ? 'Return to record' : 'Return to Collaboration';
  const sourceName = recordSource ? 'record' : 'chat';
  let canvas: HTMLCanvasElement;
  const canvasWidth = Math.max(960, innerWidth);
  let fileName = $state('Shared Office document');
  let ready = $state(false), failed = $state(false), nativeFocused = $state(false);
  let error = $state(''), notice = $state(''), downloading = $state(false);
  let original = $state<Blob | undefined>();
  let progress = $state<OfficeLoadingProgress>({ phase: 'access', detail: `Checking your access to this ${sourceName} attachment.` });
  const controller = new AbortController();
  let engine: OfficeEngine, disposed = false;

  onMount(() => {
    const readOnlyNotice = () => { notice = `This ${sourceName} attachment is read-only. Download the original to edit a local copy, or use a shared MX Drive file for editable versions.`; };
    engine = new OfficeEngine(() => {}, message => { if (!disposed) { error = message; ready = false; failed = true; } }, readOnlyNotice, message => { notice = message; }, () => {}, value => {
      if (!disposed && !ready && !failed && ['engine', 'runtime'].includes(progress.phase)) progress = value;
    });
    void openDocument();
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && ['s', 'o'].includes(event.key.toLowerCase())) { event.preventDefault(); readOnlyNotice(); }
    };
    window.addEventListener('keydown', key);
    return () => { disposed = true; controller.abort(); engine.dispose(); window.removeEventListener('keydown', key); };
  });

  async function openDocument() {
    try {
      if (recordSource ? !recordUid || !attachmentUid : !fileUid) throw new Error('Open an Office attachment from an MX record or conversation.');
      // Ticket issuance and streaming both enforce current access, including
      // record/field privacy. A return-module URL is never authorization.
      const ticket = recordSource ? await issueRecordAttachmentPreviewTicket(recordUid, attachmentUid, controller.signal) : await issueMessageFilePreviewTicket(fileUid, controller.signal);
      if (disposed) return;
      fileName = ticket.file_name;
      const extension = officeExtension(fileName);
      if (!extension) throw new Error('This Office format cannot be viewed in the browser. Download the original instead.');
      document.title = `${fileName} · MX Office`;
      const boot = engine.start(canvas); void boot.catch(() => {});
      progress = { phase: 'document', detail: `Loading the original ${sourceName} attachment from MX. No conversion or save is running on the server.` };
      let displayedAt = 0;
      const file = await apiFile(ticket.url, { signal: controller.signal, maxAttempts: 1, onProgress: (loaded, total) => {
        try { checkOfficeInputSize(Math.max(loaded, total || 0), sourceName); }
        catch (reason) { controller.abort(reason); throw reason; }
        const now = performance.now();
        if (!disposed && progress.phase === 'document' && (loaded === 0 || loaded === total || now - displayedAt >= 100)) { displayedAt = now; progress = { ...progress, loaded, total }; }
      } });
      if (disposed) return;
      original = file.blob;
      checkOfficeInputSize(original.size, sourceName);
      await officeResources(original, extension);
      progress = { phase: 'engine', detail: 'Document received. Starting the locally bundled Office viewer on your device.' };
      await boot;
      controller.signal.throwIfAborted();
      progress = { phase: 'opening', detail: openingDetail(extension) + ' Opening this attachment read-only; the shared original is unchanged.' };
      await engine.open(original, extension, true);
      if (disposed) return;
      ready = true;
      await tick(); window.dispatchEvent(new Event('resize'));
      await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
      canvas.focus(); await focusDocument(true);
    } catch (reason) {
      if (disposed) return;
      error = reason instanceof Error ? reason.message : String(reason);
      failed = true; ready = false; engine.dispose();
    }
  }
  async function focusDocument(initializeInput = false) {
    if (!ready || disposed) return;
    try { await engine.focus(initializeInput); if (!disposed) nativeFocused = true; }
    catch (reason) { if (!disposed) error = reason instanceof Error ? reason.message : String(reason); }
  }
  function cancelOpening() { controller.abort(new Error('Opening cancelled. The original attachment is unchanged.')); engine.dispose(); ready = false; failed = true; error = 'Opening cancelled. The original attachment is unchanged.'; }
  async function downloadOriginal() {
    if (downloading) return;
    downloading = true;
    try {
      if (original) {
        const url = URL.createObjectURL(original), link = document.createElement('a');
        link.href = url; link.download = fileName; link.click(); setTimeout(() => URL.revokeObjectURL(url), 30000);
      } else await download(recordSource ? `/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}/download` : `/mx/v1/collaboration/files/${encodeURIComponent(fileUid)}/download`, fileName);
    } catch (reason) { error = reason instanceof Error ? reason.message : String(reason); }
    finally { downloading = false; }
  }
</script>

<main data-office-source={recordSource ? 'record' : 'collaboration'}>
  <header>
    <a href={returnHref} title={returnLabel} aria-label={returnLabel}><ArrowLeft size={17}/></a>
    <strong title={fileName}>{fileName}</strong>
    <span class="read-only" role="status">Read-only attachment</span>
    <button disabled={downloading} onclick={() => void downloadOriginal()}><Download size={16}/><span>{downloading ? 'Downloading…' : 'Download original'}</span></button>
  </header>
  {#if error && ready}<p class="notice error" role="alert">{error}</p>{:else if notice}<p class="notice" role="status">{notice}<button aria-label="Dismiss document notice" onclick={() => notice = ''}>×</button></p>{/if}
  <div class="viewer" class:inactive={!ready} role="region" aria-label="Scrollable Office document">
    <canvas id="qtcanvas" bind:this={canvas} style:width={`${canvasWidth}px`} contenteditable="true" aria-label="Office document (read-only)" data-native-focused={nativeFocused} onfocus={() => void focusDocument()} oncontextmenu={event => event.preventDefault()} onkeydown={event => event.preventDefault()} onwheel={event => event.preventDefault()}></canvas>
    {#if !ready}<OfficeLoading {progress} {fileName} {failed} failureDetail={error} onCancel={cancelOpening} onRetry={() => location.reload()} onDownload={() => void downloadOriginal()} downloadLabel="Download original" {returnLabel} onReturn={() => location.assign(returnHref)}/>{/if}
  </div>
</main>

<style>
  :global(html),:global(body),:global(#office-app){height:100%;margin:0;overflow:hidden;}
  main{height:100dvh;display:flex;flex-direction:column;color:var(--text);background:var(--surface);}
  header{height:max(40px,2.5rem);box-sizing:border-box;padding:5px 8px;display:flex;align-items:center;gap:10px;border-bottom:1px solid var(--line-strong);font-size:0.8125rem;}
  header strong{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}header a{display:inline-flex;color:inherit;}header button{flex:none;}
  .read-only{font-size:0.6875rem;color:var(--muted);}button{display:inline-flex;align-items:center;gap:6px;padding:5px 9px;border:1px solid var(--line-strong);border-radius:6px;background:transparent;color:inherit;font:inherit;cursor:pointer;}button:disabled{opacity:.5;cursor:default;}
  a:focus-visible,button:focus-visible{outline:2px solid var(--primary);outline-offset:2px;}
  .viewer{position:relative;flex:1;min-height:0;overflow-x:auto;overflow-y:hidden;background:#e7e7e7;}canvas{display:block;height:100%;box-sizing:border-box;border:0;padding:0;outline:0;}.inactive canvas{visibility:hidden;}
  .notice{position:absolute;z-index:30;bottom:20px;left:50%;transform:translateX(-50%);box-sizing:border-box;width:min(760px,calc(100% - 24px));display:flex;align-items:center;gap:10px;padding:10px 12px;border:1px solid var(--line-strong);border-radius:8px;background:var(--success-surface);font-size:0.8125rem;overflow-wrap:anywhere;}.notice button{margin-left:auto;}.error{background:var(--danger-surface);color:var(--danger);}
  @media(max-width:600px){header{gap:7px;}.read-only{font-size:0.625rem;}header button span{display:none;}}
</style>
