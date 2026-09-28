<script lang="ts">
  import { onMount } from 'svelte';
  import Eye from '@lucide/svelte/icons/eye';
  import FileText from '@lucide/svelte/icons/file-text';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import X from '@lucide/svelte/icons/x';
  import type { TrashRecord, TrashRecordDetail } from '../../api/domain';
  import { getTrashRecord, listTrash, restoreTrashRecord } from '../../api/workspace';

  let records = $state<TrashRecord[]>([]); let detail = $state<TrashRecordDetail | null>(null);
  let loading = $state(true); let inspecting = $state(''); let busy = $state(''); let error = $state(''); let notice = $state('');

  onMount(() => void refresh());

  async function refresh() {
    loading = true; error = '';
    try { records = (await listTrash()).records; }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Trash could not be loaded.'; }
    finally { loading = false; }
  }

  async function inspect(item: TrashRecord) {
    inspecting = item.uid; error = '';
    try { detail = (await getTrashRecord(item.uid)).record; }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Deleted record details could not be loaded.'; }
    finally { inspecting = ''; }
  }

  async function restore(item: TrashRecord) {
    const identity = item.summary_fields[0]?.value || item.singular_name;
    if (!confirm(`Restore ${identity} to ${item.module_name}?`)) return;
    busy = item.uid; error = '';
    try {
      await restoreTrashRecord(item.uid);
      notice = `${item.singular_name} restored to ${item.module_name}.`;
      if (detail?.uid === item.uid) detail = null;
      await refresh();
    } catch (reason) { error = reason instanceof Error ? reason.message : 'Record could not be restored.'; }
    finally { busy = ''; }
  }
  function restoreInspected() { if (detail) void restore(detail); }

  function title(item: TrashRecord) {
    const first = item.summary_fields[0];
    return first ? `${first.label}: ${first.value}` : `${item.singular_name} with no populated fields`;
  }
  function secondarySummary(item: TrashRecord) { return item.summary_fields.slice(1).map((field) => `${field.label}: ${field.value}`).join(' · '); }
  function formatBytes(size: number) {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(size < 10240 ? 1 : 0)} KB`;
    if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`;
    return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }
</script>

{#if notice}<div class="notice success">{notice}</div>{/if}
{#if error}<div class="notice error">{error}</div>{/if}

<section class="panel section-panel trash-panel">
  <div class="panel-heading">
    <div><h2>Record trash</h2><p class="muted">Inspect deleted records and their files before restoring them. Record IDs remain available under technical details.</p></div>
    <button class="button icon-label" onclick={refresh} disabled={loading}><RotateCcw size={16} /> Refresh</button>
  </div>
  <div class="table-wrap">
    <table class="trash-table">
      <thead><tr><th>Module</th><th>Record</th><th>Deleted by</th><th>Deleted</th><th>Files</th><th><span class="sr-only">Actions</span></th></tr></thead>
      <tbody>
        {#if loading}<tr><td colspan="6" class="trash-state">Loading deleted records…</td></tr>
        {:else if !records.length}<tr><td colspan="6" class="trash-state">Trash is empty.</td></tr>{/if}
        {#each records as item}
          <tr>
            <td><strong>{item.module_name}</strong><small class="block">{item.singular_name}</small></td>
            <td class="trash-record-cell"><button class="trash-record-link" onclick={() => inspect(item)} disabled={inspecting === item.uid}><strong>{title(item)}</strong>{#if secondarySummary(item)}<small>{secondarySummary(item)}</small>{/if}</button></td>
            <td>{item.deleted_by_name}</td>
            <td><time datetime={new Date(item.deleted_at).toISOString()}>{new Date(item.deleted_at).toLocaleString()}</time></td>
            <td>{item.attachment_count}</td>
            <td><div class="inline-actions trash-actions"><button class="button small icon-label" disabled={inspecting === item.uid} onclick={() => inspect(item)}><Eye size={15} />{inspecting === item.uid ? 'Opening…' : 'Inspect'}</button><button class="button primary small icon-label" disabled={busy === item.uid} onclick={() => restore(item)}><RotateCcw size={15} />{busy === item.uid ? 'Restoring…' : 'Restore'}</button></div></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

{#if detail}
  <div class="overlay trash-overlay" role="presentation" onclick={(event) => event.currentTarget === event.target && (detail = null)}>
    <div class="dialog trash-dialog" role="dialog" aria-modal="true" aria-labelledby="trash-detail-title">
      <header class="dialog-head">
        <div><p class="eyebrow">Deleted {detail.singular_name}</p><h2 id="trash-detail-title">{title(detail)}</h2>{#if secondarySummary(detail)}<p class="muted">{secondarySummary(detail)}</p>{/if}</div>
        <button class="icon-button" aria-label="Close deleted record details" onclick={() => detail = null}><X size={19} /></button>
      </header>

      <div class="trash-meta" aria-label="Deletion details">
        <div><span>Module</span><strong>{detail.module_name}</strong></div>
        <div><span>Deleted by</span><strong>{detail.deleted_by_name}</strong></div>
        <div><span>Deleted</span><strong>{new Date(detail.deleted_at).toLocaleString()}</strong></div>
        <div><span>History</span><strong>{detail.version_count} version{detail.version_count === 1 ? '' : 's'}</strong></div>
      </div>

      <section class="trash-detail-section">
        <div class="trash-section-title"><FileText size={17} /><div><h3>Record data</h3><p>Read-only values retained with this deleted record.</p></div></div>
        <dl class="trash-field-grid">
          {#each detail.fields as field}
            <div class:archived={!field.active}><dt>{field.label}{#if !field.active}<small>Archived field</small>{/if}</dt><dd class:empty={!field.display_value}>{field.display_value || 'Not set'}</dd></div>
          {/each}
        </dl>
      </section>

      <section class="trash-detail-section">
        <div class="trash-section-title"><Paperclip size={17} /><div><h3>Attachments</h3><p>File references are retained and will return with the record.</p></div><span class="trash-file-count">{detail.attachments.length}</span></div>
        {#if detail.attachments.length}
          <div class="trash-files">{#each detail.attachments as file}<article><span><strong>{file.file_name}</strong><small>{file.field_label} · {file.mime_type} · {formatBytes(file.size)}</small></span></article>{/each}</div>
        {:else}<p class="muted trash-empty">No files were attached.</p>{/if}
      </section>

      <details class="trash-technical"><summary>Technical details</summary><dl><div><dt>Record ID</dt><dd><code>{detail.uid}</code></dd></div><div><dt>Module ID</dt><dd><code>{detail.module_uid}</code></dd></div></dl></details>
      <footer class="dialog-actions"><button class="button" onclick={() => detail = null}>Close</button><span class="spacer"></span><button class="button primary icon-label" disabled={busy === detail.uid} onclick={restoreInspected}><RotateCcw size={16} />{busy === detail.uid ? 'Restoring…' : `Restore ${detail.singular_name}`}</button></footer>
    </div>
  </div>
{/if}
