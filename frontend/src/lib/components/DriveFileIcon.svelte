<script lang="ts">
  import { untrack } from 'svelte';
  import Folder from '@lucide/svelte/icons/folder';
  import File from '@lucide/svelte/icons/file';
  import FileText from '@lucide/svelte/icons/file-text';
  import Film from '@lucide/svelte/icons/film';
  import Music from '@lucide/svelte/icons/music';
  import Image from '@lucide/svelte/icons/image';
  import Sheet from '@lucide/svelte/icons/sheet';
  import { drivePreview, guestPreview, type DriveItem } from '../api/drive';
  let { item, guestToken = '', large = false }: { item: DriveItem; guestToken?: string; large?: boolean } = $props();
  let target: HTMLSpanElement; let url = $state(''); let failed = $state(false);
  const image = $derived(item.mime_type.startsWith('image/') && item.mime_type !== 'image/svg+xml');
  const identity = $derived(`${item.uid}:${item.revision}:${guestToken}`);
  $effect(() => {
    void identity;
    const current=untrack(()=>({...item}));const token=untrack(()=>guestToken);
    let destroyed = false;
    failed=false;
    if (!current.mime_type.startsWith('image/') || current.mime_type==='image/svg+xml' || current.size>10*1024*1024) {url='';return;}
    const load = async () => { try { const preview = token ? guestPreview(token, current) : await drivePreview(current); if (!destroyed) url = preview.url; } catch { if(!destroyed)failed = true; } };
    if (!('IntersectionObserver' in window)) { void load(); return () => { destroyed = true; }; }
    const observer = new IntersectionObserver((entries) => { if (entries.some((entry) => entry.isIntersecting)) { observer.disconnect(); void load(); } }, { rootMargin: '100px' });
    observer.observe(target); return () => { destroyed = true; observer.disconnect(); };
  });
</script>
<span class="drive-file-icon" class:large bind:this={target}>
  {#if url && !failed}<img src={url} alt="" loading="lazy" onerror={() => failed = true} />
  {:else if item.kind === 'folder'}<Folder size={24} />
  {:else if image}<Image size={24} />
  {:else if item.mime_type.startsWith('video/')}<Film size={24} />
  {:else if item.mime_type.startsWith('audio/')}<Music size={24} />
  {:else if /\.(xlsx?|csv|ods)$/i.test(item.name)}<Sheet size={24} />
  {:else if /\.(pdf|docx?|txt|md|rtf|odt)$/i.test(item.name)}<FileText size={24} />
  {:else}<File size={24} />{/if}
</span>
<style>
  .drive-file-icon { display: inline-flex; align-items: center; justify-content: center; flex: 0 0 2.6rem; width: 2.6rem; height: 2.6rem; border-radius: .45rem; background: var(--collab-surface); color: var(--collab-icon); overflow: hidden; }
  img { width: 100%; height: 100%; object-fit: cover; }
  .large { flex: none; width: 100%; height: 9rem; border-radius: .5rem; } .large img { object-fit: contain; } .large :global(svg) { width: 3rem; height: 3rem; }
</style>
