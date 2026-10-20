<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FileLock from '@lucide/svelte/icons/file-lock';
  import DriveFileIcon from './DriveFileIcon.svelte';
  import {driveSize, type MessageDriveLink} from '../api/drive';
  import {canOpenOffice, openOffice} from '../office/launch';
  let {link}: {link:MessageDriveLink} = $props();
  let officeError = $state('');
</script>

{#if link.item && !link.unavailable}
  <a class="drive-share-card" href={`#drive?item=${encodeURIComponent(link.uid)}`} aria-label={`Open ${link.item.name} in MX Drive`}>
    <DriveFileIcon item={link.item}/>
    <span><strong>{link.item.name}</strong><small>MX Drive · {link.item.kind === 'folder' ? 'Folder' : driveSize(link.item.size)} · {link.item.permission === 'owner' ? 'Your file' : `${link.item.permission} access`}</small></span>
    <ChevronRight size={16}/>
  </a>
  {#if canOpenOffice(link.item)}<button class="shared-office-open" type="button" aria-label={`Open ${link.item.name} in MX Office`} onclick={() => { officeError = openOffice(link.item!) ? '' : 'Allow popups for MX to open this document.'; }}>Open in MX Office</button>{/if}
  {#if officeError}<p role="alert">{officeError}</p>{/if}
{:else}
  <div class="drive-share-card unavailable"><FileLock size={20}/><span><strong>Shared item unavailable</strong><small>Access was removed or the item is in trash.</small></span></div>
{/if}

<style>
  .drive-share-card{display:flex;align-items:center;gap:12px;min-width:0;max-width:100%;padding:12px;border:1px solid var(--line);border-radius:10px;color:var(--text);background:var(--surface);text-decoration:none}
  .drive-share-card:hover{border-color:var(--primary);background:var(--surface-raised,var(--surface))}
  .shared-office-open{font:inherit;font-size:.8rem;margin-top:6px;padding:6px 10px;border:1px solid var(--line);border-radius:6px;background:var(--surface);color:var(--text);cursor:pointer;}.shared-office-open:hover{border-color:var(--primary);}
  span{flex:1;min-width:0}strong,small{display:block}strong{font-size:13px;overflow-wrap:anywhere}small{font-size:11px;color:var(--muted);margin-top:4px}.unavailable{color:var(--muted)}.unavailable:hover{border-color:var(--line)}
</style>
