<script lang="ts">
  import { onMount } from 'svelte';
  import FileText from '@lucide/svelte/icons/file-text';
  import ImageIcon from '@lucide/svelte/icons/image';
  import type { FileAttachment } from '../api/domain';
  import { issueRecordAttachmentPreviewTicket } from '../api/workspace';

  let { recordUid, file, onOpen }: { recordUid: string; file: FileAttachment; onOpen: () => void } = $props();
  let thumbnailUrl = $state('');
  let ready = $state(false);
  let failed = $state(false);
  const image = $derived(file.mime_type.startsWith('image/'));
  const extension = $derived(file.file_name.includes('.') ? file.file_name.split('.').pop()?.slice(0, 5).toUpperCase() || 'FILE' : 'FILE');

  onMount(() => {
    if (!image) return;
    let active = true;
    void issueRecordAttachmentPreviewTicket(recordUid, file.uid)
      .then((ticket) => { if (active) thumbnailUrl = ticket.url; })
      .catch(() => { if (active) failed = true; });
    return () => { active = false; };
  });
</script>

<button class:image class:ready class="record-attachment-thumbnail" type="button" onclick={onOpen} aria-label={`Preview ${file.file_name}`} title={`Preview ${file.file_name}`}>
  {#if image && thumbnailUrl && !failed}
    <img src={thumbnailUrl} alt="" onload={() => ready = true} onerror={() => { failed = true; ready = false; }} />
  {/if}
  {#if !image || failed}
    <span>{#if image}<ImageIcon size={18} />{:else}<FileText size={18} />{/if}<small>{extension}</small></span>
  {:else if !ready}
    <span class="record-thumbnail-loading"><i></i></span>
  {/if}
</button>
