<script lang="ts">
  import { onMount } from 'svelte';
  import ImageIcon from '@lucide/svelte/icons/image';
  import type { MessageFile } from '../api/domain';
  import { issueMessageFilePreviewTicket } from '../api/workspace';

  let { file, onOpen }: { file: MessageFile; onOpen: () => void } = $props();
  let imageUrl = $state('');
  let ready = $state(false);
  let failed = $state(false);

  onMount(() => {
    let active = true;
    void issueMessageFilePreviewTicket(file.uid)
      .then(({ url }) => {
        if (!active) return;
        imageUrl = url;
      })
      .catch(() => { if (active) failed = true; });
    return () => { active = false; };
  });

  function imageFailed() {
    failed = true;
    ready = false;
    imageUrl = '';
  }
</script>

<button class:failed class="shared-image-thumbnail" type="button" onclick={onOpen} aria-label={`Preview image ${file.file_name}`}>
  {#if failed}
    <span class="shared-image-fallback"><ImageIcon size={24} /><strong>{file.file_name}</strong><small>Open image preview</small></span>
  {:else}
    {#if imageUrl}<img class:ready src={imageUrl} alt={file.file_name} onload={() => ready = true} onerror={imageFailed} />{/if}
    {#if !ready}<span class="shared-image-ghost" aria-label={`Loading thumbnail for ${file.file_name}`}><i></i><i></i><i></i></span>{/if}
    <span class="shared-image-caption"><strong>{file.file_name}</strong><small>{ready ? 'Image · Open preview' : 'Loading image…'}</small></span>
  {/if}
</button>
