<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FileText from '@lucide/svelte/icons/file-text';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Minus from '@lucide/svelte/icons/minus';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import {focusDialog} from '../util/focusDialog';

  let { fileName, load, navigationItems = [], activeIndex = 0, onNavigate = (_index: number) => undefined, externalNavigation, navigationNoun = 'attachment', onStep = (_direction: -1|1) => undefined, onClose }: {
    fileName: string;
    load: () => Promise<{ blob?: Blob; url?: string; fileName: string; mimeType: string }>;
    navigationItems?: { key: string; name: string; detail?: string }[];
    activeIndex?: number;
    onNavigate?: (index: number) => void;
    externalNavigation?: {previous: {name:string}|null;next:{name:string}|null;position:number;total:number;busy:boolean;error?:string};
    navigationNoun?: string;
    onStep?: (direction:-1|1)=>void;
    onClose: () => void;
  } = $props();

  let stage = $state<'loading' | 'ready' | 'error'>('loading');
  let status = $state('Loading preview…');
  let error = $state('');
  let previewUrl = $state('');
  let previewName = $state('');
  let previewMime = $state('');
  let previewText = $state('');
  let previewKind = $state<'image' | 'pdf' | 'video' | 'audio' | 'text' | 'unknown'>('unknown');
  let imageViewport = $state<HTMLDivElement>();
  let imageNaturalWidth = $state(0);
  let imageNaturalHeight = $state(0);
  let imageZoom = $state(1);
  let imageFitted = $state(true);
  let imagePanning = $state(false);
  let panPointerId = $state<number | null>(null);
  let panStartX = 0;
  let panStartY = 0;
  let panScrollLeft = 0;
  let panScrollTop = 0;
  let pdfZoom = $state<number | 'page-width'>('page-width');
  let textZoom = $state(1);
  const imageZoomLabel = $derived(`${Math.round(imageZoom * 100)}%`);
  const pdfZoomLabel = $derived(pdfZoom === 'page-width' ? 'Fit width' : `${pdfZoom}%`);
  const pdfSource = $derived(previewUrl ? `${previewUrl}#toolbar=1&navpanes=0&zoom=${pdfZoom}` : '');
  const hasNavigation = $derived(!!externalNavigation || navigationItems.length > 1);
  const showNavigator = $derived(!externalNavigation && navigationItems.length > 1);
  let textTruncated=$state(false);
  const previewController=new AbortController();let disposed=false;

  onMount(() => {
    previewName = fileName;
    if (/\.(docx?|xlsx?|pptx?|odt|ods|odp)$/i.test(fileName)) status = 'Converting document to a browser preview…';
    void loadPreview();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); onClose(); return; }
      if (event.target instanceof Element && event.target.closest('input,textarea,select,[contenteditable="true"],video,audio')) return;
      if (hasNavigation && event.key === 'ArrowLeft' && !event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        step(-1);
        return;
      }
      if (hasNavigation && event.key === 'ArrowRight' && !event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        step(1);
        return;
      }
      if (stage !== 'ready' || event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.key === '+' || event.key === '=') {
        event.preventDefault();
        if (previewKind === 'image') void setImageZoom(imageZoom + .25);
        else if (previewKind === 'pdf') changePdfZoom(25);
        else if (previewKind === 'text') changeTextZoom(.1);
      } else if (event.key === '-') {
        event.preventDefault();
        if (previewKind === 'image') void setImageZoom(imageZoom - .25);
        else if (previewKind === 'pdf') changePdfZoom(-25);
        else if (previewKind === 'text') changeTextZoom(-.1);
      } else if (event.key === '0') {
        event.preventDefault();
        if (previewKind === 'image') void fitImage();
        else if (previewKind === 'pdf') pdfZoom = 'page-width';
        else if (previewKind === 'text') textZoom = 1;
      } else if (event.key === '1' && previewKind === 'image') {
        event.preventDefault();
        void setImageZoom(1);
      }
    };
    const resized = () => { if (previewKind === 'image' && imageFitted) void fitImage(); };
    window.addEventListener('keydown', keydown);
    window.addEventListener('resize', resized);
    return () => {
      window.removeEventListener('keydown', keydown);
      window.removeEventListener('resize', resized);
    };
  });
  let ownsPreviewUrl = false;
  onDestroy(() => { disposed=true;previewController.abort();if (previewUrl && ownsPreviewUrl) URL.revokeObjectURL(previewUrl); });

  function previewType(mimeType: string, name: string): typeof previewKind {
    const type = mimeType.toLowerCase(); const lowerName = name.toLowerCase();
    if (type.startsWith('image/')) return 'image';
    if (type === 'application/pdf' || lowerName.endsWith('.pdf')) return 'pdf';
    if (type.startsWith('video/')) return 'video';
    if (type.startsWith('audio/')) return 'audio';
    if (type.startsWith('text/') || /(json|xml|javascript)/.test(type) || /\.(txt|csv|json|xml|md|log|ini|conf|yaml|yml|toml|rs|js|ts|css|html)$/i.test(lowerName)) return 'text';
    return 'unknown';
  }
  async function loadPreview() {
    try {
      const result = await load();
      if(disposed)return;
      previewName = result.fileName || fileName;
      previewMime = result.mimeType || result.blob?.type || 'application/octet-stream';
      previewKind = previewType(previewMime, previewName);
      if (result.url) {
        previewUrl = result.url;
        ownsPreviewUrl = false;
      } else if (result.blob) {
        previewUrl = URL.createObjectURL(result.blob);
        ownsPreviewUrl = true;
      } else {
        throw new Error('MX did not return a preview resource.');
      }
      if (previewKind === 'text') {
        const maximum=256*1024;
        if(result.blob){textTruncated=result.blob.size>maximum;previewText=await result.blob.slice(0,maximum).text();}
        else {
          const response=await fetch(previewUrl,{headers:{Range:`bytes=0-${maximum-1}`},signal:previewController.signal});
          if(!response.ok)throw new Error('The text preview could not be loaded.');
          const range=response.headers.get('content-range');const total=Number(range?.split('/')[1]||response.headers.get('content-length'));
          textTruncated=total>maximum;
          const reader=response.body?.getReader();const chunks:Uint8Array[]=[];let size=0;
          if(reader)try{while(size<maximum){const next=await reader.read();if(next.done)break;const chunk=next.value.slice(0,maximum-size);chunks.push(chunk);size+=chunk.length;if(next.value.length>chunk.length)textTruncated=true;}if(size===maximum&&!total)textTruncated=true;}finally{await reader.cancel().catch(()=>undefined);}
          const bytes=new Uint8Array(size);let cursor=0;for(const chunk of chunks){bytes.set(chunk,cursor);cursor+=chunk.length;}previewText=new TextDecoder().decode(bytes);
        }
      }
      if(disposed)return;
      stage = 'ready';
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'Preview failed.';
      stage = 'error';
    }
  }
  function openPreview() { if (previewUrl) window.open(previewUrl, '_blank', 'noopener,noreferrer'); }
  function downloadPreview() {
    if (!previewUrl) return;
    const link = document.createElement('a'); link.href = previewUrl; link.download = previewName || fileName; link.click();
  }
  function clamp(value: number, minimum: number, maximum: number) { return Math.min(maximum, Math.max(minimum, value)); }
  async function imageLoaded(event: Event) {
    const image = event.currentTarget as HTMLImageElement;
    imageNaturalWidth = image.naturalWidth;
    imageNaturalHeight = image.naturalHeight;
    await fitImage();
  }
  async function setImageZoom(next: number, fitted = false) {
    const viewport = imageViewport;
    const oldWidth = viewport?.scrollWidth || 1;
    const oldHeight = viewport?.scrollHeight || 1;
    const centerX = viewport ? (viewport.scrollLeft + viewport.clientWidth / 2) / oldWidth : .5;
    const centerY = viewport ? (viewport.scrollTop + viewport.clientHeight / 2) / oldHeight : .5;
    imageZoom = clamp(next, .1, 4);
    imageFitted = fitted;
    await tick();
    if (viewport) {
      viewport.scrollLeft = Math.max(0, centerX * viewport.scrollWidth - viewport.clientWidth / 2);
      viewport.scrollTop = Math.max(0, centerY * viewport.scrollHeight - viewport.clientHeight / 2);
    }
  }
  async function fitImage() {
    if (!imageViewport || !imageNaturalWidth || !imageNaturalHeight) return;
    const widthScale = Math.max(1, imageViewport.clientWidth - 48) / imageNaturalWidth;
    const heightScale = Math.max(1, imageViewport.clientHeight - 48) / imageNaturalHeight;
    await setImageZoom(Math.min(widthScale, heightScale, 1), true);
    imageViewport.scrollLeft = 0;
    imageViewport.scrollTop = 0;
  }
  function imageWheel(event: WheelEvent) {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    void setImageZoom(imageZoom + (event.deltaY < 0 ? .1 : -.1));
  }
  function startImagePan(event: PointerEvent) {
    if (!imageViewport || event.button !== 0) return;
    imagePanning = true;
    panPointerId = event.pointerId;
    panStartX = event.clientX;
    panStartY = event.clientY;
    panScrollLeft = imageViewport.scrollLeft;
    panScrollTop = imageViewport.scrollTop;
    imageViewport.setPointerCapture(event.pointerId);
  }
  function moveImagePan(event: PointerEvent) {
    if (!imageViewport || !imagePanning || panPointerId !== event.pointerId) return;
    imageViewport.scrollLeft = panScrollLeft - (event.clientX - panStartX);
    imageViewport.scrollTop = panScrollTop - (event.clientY - panStartY);
  }
  function stopImagePan(event: PointerEvent) {
    if (!imagePanning || panPointerId !== event.pointerId) return;
    imagePanning = false;
    panPointerId = null;
    imageViewport?.releasePointerCapture(event.pointerId);
  }
  function changePdfZoom(change: number) { pdfZoom = clamp(typeof pdfZoom === 'number' ? pdfZoom + change : 100 + change, 50, 300); }
  function changeTextZoom(change: number) { textZoom = clamp(Number((textZoom + change).toFixed(1)), .7, 2); }
  function navigate(index: number) {
    if (index < 0 || index >= navigationItems.length || index === activeIndex) return;
    onNavigate(index);
  }
  function step(direction:-1|1){if(externalNavigation){if(!externalNavigation.busy)onStep(direction);}else navigate(activeIndex+direction);}
</script>

<div class="overlay preview-overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <div class="dialog preview-dialog" use:focusDialog role="dialog" aria-modal="true" aria-labelledby="collaboration-preview-title" tabindex="-1">
    <header class:has-navigation={hasNavigation} class="dialog-head preview-head">
      <div class="preview-heading">
        <h2 id="collaboration-preview-title">{fileName}</h2>
        <p class="muted preview-meta">{stage === 'loading' ? status : previewMime}{stage === 'ready' && previewName !== fileName ? ` · generated preview: ${previewName}` : ''}</p>
      </div>
      {#if hasNavigation}
        <div class="preview-navigation" role="toolbar" aria-label="Attachment navigation">
          <button type="button" onclick={() => step(-1)} disabled={externalNavigation ? externalNavigation.busy||!externalNavigation.previous : activeIndex<=0} aria-label={`Previous ${navigationNoun}`} title={`Previous ${navigationNoun} (Left arrow)`}><ChevronLeft size={16} /></button>
          <output aria-live="polite">{externalNavigation?.busy ? 'Loading…' : externalNavigation ? `${externalNavigation.position} of ${externalNavigation.total}` : `${activeIndex+1} of ${navigationItems.length}`}</output>
          <button type="button" onclick={() => step(1)} disabled={externalNavigation ? externalNavigation.busy||!externalNavigation.next : activeIndex>=navigationItems.length-1} aria-label={`Next ${navigationNoun}`} title={`Next ${navigationNoun} (Right arrow)`}><ChevronRight size={16} /></button>
        </div>
      {/if}
      {#if stage === 'ready' && (previewKind === 'image' || previewKind === 'pdf' || previewKind === 'text')}
        <div class="preview-view-controls" role="toolbar" aria-label={`${previewKind} preview controls`}>
          {#if previewKind === 'image'}
            <button type="button" onclick={() => void setImageZoom(imageZoom - .25)} disabled={imageZoom <= .1} aria-label="Zoom out" title="Zoom out (-)"><Minus size={15} /></button>
            <output aria-label="Current zoom">{imageZoomLabel}</output>
            <button type="button" onclick={() => void setImageZoom(imageZoom + .25)} disabled={imageZoom >= 4} aria-label="Zoom in" title="Zoom in (+)"><Plus size={15} /></button>
            <i></i>
            <button class:active={imageFitted} type="button" onclick={() => void fitImage()} aria-label="Fit image to window" title="Fit to window (0)"><Maximize2 size={14} /><span>Fit</span></button>
            <button class:active={!imageFitted && imageZoom === 1} type="button" onclick={() => void setImageZoom(1)} aria-label="View image at actual size" title="Actual size (1)"><span>1:1</span></button>
          {:else if previewKind === 'pdf'}
            <button type="button" onclick={() => changePdfZoom(-25)} disabled={pdfZoom === 50} aria-label="Zoom PDF out" title="Zoom out (-)"><Minus size={15} /></button>
            <button class="preview-zoom-label" type="button" onclick={() => pdfZoom = 'page-width'} title="Fit PDF to width (0)">{pdfZoomLabel}</button>
            <button type="button" onclick={() => changePdfZoom(25)} disabled={pdfZoom === 300} aria-label="Zoom PDF in" title="Zoom in (+)"><Plus size={15} /></button>
          {:else}
            <button type="button" onclick={() => changeTextZoom(-.1)} disabled={textZoom <= .7} aria-label="Decrease text size" title="Decrease text size (-)"><Minus size={15} /></button>
            <button class="preview-zoom-label" type="button" onclick={() => textZoom = 1} title="Reset text size (0)">{Math.round(textZoom * 100)}%</button>
            <button type="button" onclick={() => changeTextZoom(.1)} disabled={textZoom >= 2} aria-label="Increase text size" title="Increase text size (+)"><Plus size={15} /></button>
          {/if}
        </div>
      {/if}
      {#if stage === 'ready'}
        <div class="inline-actions preview-file-actions">
          <button class="button small" type="button" onclick={openPreview}>Open in new tab</button>
          <button class="button small" type="button" onclick={downloadPreview}>{previewName !== fileName ? 'Download preview' : 'Download'}</button>
        </div>
      {/if}
      <button class="icon-button preview-close" type="button" aria-label="Close preview" onclick={onClose}><X size={18} /></button>
      {#if externalNavigation?.error}<p role="alert" class="preview-navigation-error" style="grid-column:1/-1">{externalNavigation.error}</p>{/if}
      {#if textTruncated}<p role="status" class="preview-navigation-error" style="grid-column:1/-1">Showing the first 256 KiB. Download or open the file separately to read the rest.</p>{/if}
    </header>
    <div class:with-navigator={showNavigator} class="preview-workspace">
      {#if showNavigator}
        <aside class="preview-navigator" aria-label="Record attachments">
          <header><strong>Attachments</strong><span>{navigationItems.length}</span></header>
          <div>{#each navigationItems as item, index (item.key)}<button class:active={index === activeIndex} type="button" aria-current={index === activeIndex ? 'true' : undefined} onclick={() => navigate(index)}><span><FileText size={15} /></span><span><strong>{item.name}</strong>{#if item.detail}<small>{item.detail}</small>{/if}</span><i>{index + 1}</i></button>{/each}</div>
        </aside>
      {/if}
      <div class:media-preview={stage === 'ready' && ['image', 'pdf', 'video'].includes(previewKind)} class:image-preview={previewKind === 'image'} class="preview-content">
      {#if stage === 'loading'}<div class="ghost-document" aria-live="polite"><aside><i></i><i></i><i></i><i></i></aside><main><div class="ghost-page"><b></b><span></span><span></span><span></span><em></em><span></span><span></span></div><p><strong>{status}</strong><small>MX is preparing the shared file inside this preview. You can close it without interrupting the conversation.</small></p></main></div>
      {:else if stage === 'error'}<div class="preview-fallback"><h3>Preview unavailable</h3><p class="muted">{error}</p><button class="button" type="button" onclick={onClose}>Close</button></div>
      {:else if previewKind === 'image'}
        <div class="preview-image-viewer">
          <div class:panning={imagePanning} class="preview-image-viewport" bind:this={imageViewport} role="region" aria-label="Zoomable image preview" onwheel={imageWheel} onpointerdown={startImagePan} onpointermove={moveImagePan} onpointerup={stopImagePan} onpointercancel={stopImagePan}>
            <div class="preview-image-canvas">
              <img src={previewUrl} alt={fileName} draggable={false} onload={imageLoaded} style={imageNaturalWidth ? `width:${imageNaturalWidth * imageZoom}px;height:${imageNaturalHeight * imageZoom}px` : ''} />
            </div>
          </div>
          <p class="preview-image-hint">Ctrl/⌘ + wheel to zoom · Drag to pan</p>
        </div>
      {:else if previewKind === 'pdf'}<iframe title={`Preview of ${fileName}`} src={pdfSource}></iframe>
      {:else if previewKind === 'video'}<video src={previewUrl} controls><track kind="captions" /></video>
      {:else if previewKind === 'audio'}<audio src={previewUrl} controls><track kind="captions" /></audio>
      {:else if previewKind === 'text'}<pre style={`font-size:${.76 * textZoom}rem`}>{previewText}</pre>
      {:else}<div class="preview-fallback"><h3>Browser preview unavailable</h3><p class="muted">This file type cannot be rendered safely in the browser. Use Download to open the original file with the appropriate application.</p></div>{/if}
      </div>
    </div>
  </div>
</div>
