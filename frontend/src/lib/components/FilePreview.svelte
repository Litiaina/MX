<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import X from '@lucide/svelte/icons/x';

  let { fileName, load, onClose }: {
    fileName: string;
    load: () => Promise<{ blob: Blob; fileName: string; mimeType: string }>;
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

  onMount(() => {
    previewName = fileName;
    if (/\.(docx?|xlsx?|pptx?|odt|ods|odp)$/i.test(fileName)) status = 'Converting document to a browser preview…';
    void loadPreview();
  });
  onDestroy(() => { if (previewUrl) URL.revokeObjectURL(previewUrl); });

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
      previewName = result.fileName || fileName;
      previewMime = result.mimeType || result.blob.type || 'application/octet-stream';
      previewKind = previewType(previewMime, previewName);
      previewUrl = URL.createObjectURL(result.blob);
      if (previewKind === 'text') previewText = await result.blob.text();
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
</script>

<div class="overlay preview-overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <div class="dialog preview-dialog" role="dialog" aria-modal="true" aria-labelledby="collaboration-preview-title">
    <header class="dialog-head preview-head"><div class="preview-heading"><h2 id="collaboration-preview-title">{fileName}</h2><p class="muted preview-meta">{stage === 'loading' ? status : previewMime}{stage === 'ready' && previewName !== fileName ? ` · generated preview: ${previewName}` : ''}</p></div>{#if stage === 'ready'}<div class="inline-actions preview-file-actions"><button class="button small" type="button" onclick={openPreview}>Open in new tab</button><button class="button small" type="button" onclick={downloadPreview}>{previewName !== fileName ? 'Download preview' : 'Download'}</button></div>{/if}<button class="icon-button preview-close" type="button" aria-label="Close preview" onclick={onClose}><X size={18} /></button></header>
    <div class="preview-content">
      {#if stage === 'loading'}<div class="ghost-document" aria-live="polite"><aside><i></i><i></i><i></i><i></i></aside><main><div class="ghost-page"><b></b><span></span><span></span><span></span><em></em><span></span><span></span></div><p><strong>{status}</strong><small>MX is preparing the shared file inside this preview. You can close it without interrupting the conversation.</small></p></main></div>
      {:else if stage === 'error'}<div class="preview-fallback"><h3>Preview unavailable</h3><p class="muted">{error}</p><button class="button" type="button" onclick={onClose}>Close</button></div>
      {:else if previewKind === 'image'}<img src={previewUrl} alt={fileName} />
      {:else if previewKind === 'pdf'}<iframe title={`Preview of ${fileName}`} src={previewUrl}></iframe>
      {:else if previewKind === 'video'}<video src={previewUrl} controls><track kind="captions" /></video>
      {:else if previewKind === 'audio'}<audio src={previewUrl} controls><track kind="captions" /></audio>
      {:else if previewKind === 'text'}<pre>{previewText}</pre>
      {:else}<div class="preview-fallback"><h3>Browser preview unavailable</h3><p class="muted">This file type cannot be rendered safely in the browser. Use Download to open the original file with the appropriate application.</p></div>{/if}
    </div>
  </div>
</div>
