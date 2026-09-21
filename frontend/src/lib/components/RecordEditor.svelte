<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { ApiError } from '../api/client';
  import type { FieldConflict, FieldDefinition, JsonValue, MxRecord, SchemaResponse } from '../api/domain';
  import type { LiveMessage } from '../live/client';
  import { cloneJson } from '../util/json';
  import { createRecord, deleteAttachment, deleteRecord, download, getRecord, patchRecord, previewAttachment, uploadAttachments } from '../api/workspace';

  let { schema, record, canWrite, canDelete, liveMessage = null, focusAttachments = false, recordLabel = 'Record', onClose, onSaved }:
    { schema: SchemaResponse; record: MxRecord | null; canWrite: boolean; canDelete: boolean; liveMessage?: LiveMessage | null; focusAttachments?: boolean; recordLabel?: string; onClose: () => void; onSaved: () => Promise<void> } = $props();

  let values = $state<Record<string, JsonValue>>({});
  let original = $state<Record<string, JsonValue>>({});
  let pending = $state<Record<string, File[]>>({});
  let busy = $state(false);
  let error = $state('');
  let previewUrl = $state('');
  let previewName = $state('');
  let previewDownloadName = $state('');
  let previewMime = $state('');
  let previewText = $state('');
  let previewKind = $state<'image' | 'pdf' | 'video' | 'audio' | 'text' | 'unknown'>('unknown');
  let persisted = $state<MxRecord | null>(null);
  let conflicts = $state<FieldConflict[]>([]);
  let liveNotice = $state('');
  let deletedRemotely = $state(false);
  let handledSequence = $state(0);
  let uploadProgress = $state(0);
  let uploadLabel = $state('');
  let fileProgress = $state<Record<string, { percent: number; label: string }>>({});
  let dragAttachmentField = $state('');
  let dialogElement: HTMLDivElement;
  let focusedAttachments = false;
  const current = $derived(persisted || record);
  const attachmentCount = $derived(current?.attached_files.length || 0);
  const MAX_ATTACHMENT_SIZE = 50 * 1024 * 1024;

  onDestroy(() => { if (previewUrl) URL.revokeObjectURL(previewUrl); });

  $effect(() => {
    const source = record?.values || {};
    const initial = cloneJson(source);
    if (!record && schema.fields.some((field) => field.active && field.key === 'date' && field.field_type === 'date')) {
      const now = new Date();
      initial.date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    }
    values = initial;
    original = cloneJson(initial);
    pending = {};
    persisted = null;
    conflicts = [];
    liveNotice = '';
    deletedRemotely = false;
    uploadProgress = 0;
    uploadLabel = '';
    fileProgress = {};
    focusedAttachments = false;
  });

  $effect(() => {
    if (!focusAttachments || focusedAttachments || !dialogElement) return;
    focusedAttachments = true;
    void tick().then(() => jumpToAttachments());
  });

  $effect(() => {
    const message = liveMessage;
    if (!message || !current || !message.sequence || message.sequence <= handledSequence || busy) return;
    handledSequence = message.sequence;
    void applyLiveMessage(message);
  });

  const fields = $derived(schema.fields.filter((field) => field.active).sort((a, b) => a.position - b.position));
  const unassignedFiles = $derived(current?.attached_files.filter((file) => !file.attachment_field_uid) || []);

  function fieldValue(field: FieldDefinition): string | number | boolean {
    const value = values[field.key];
    if (field.field_type === 'boolean') return value === true;
    if (typeof value === 'number' || typeof value === 'string') return value;
    return '';
  }

  function autoNumberValue(field: FieldDefinition) {
    const value = values[field.key];
    if (value === null || value === undefined || value === '') return '';
    const padding = Math.max(0, Number(field.config.padding || 0));
    return `${String(field.config.prefix || '')}${padding ? String(value).padStart(padding, '0') : String(value)}`;
  }

  function bytes(value: number) {
    if (value < 1024) return `${value} B`;
    if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
    return `${(value / 1024 ** 2).toFixed(1)} MB`;
  }

  function setValue(field: FieldDefinition, target: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement) {
    if (field.field_type === 'boolean') values[field.key] = (target as HTMLInputElement).checked;
    else if (field.field_type === 'integer') values[field.key] = target.value === '' ? null : Number.parseInt(target.value, 10);
    else if (field.field_type === 'decimal') values[field.key] = target.value === '' ? null : Number(target.value);
    else values[field.key] = target.value || null;
  }

  function filesFor(field: FieldDefinition) {
    return current?.attached_files.filter((file) => file.attachment_field_uid === field.uid) || [];
  }

  function attachmentLimit(field: FieldDefinition) { return Number(field.config.max_files || (field.config.multiple === false ? 1 : 0)); }

  function recordIdentity(item: MxRecord) {
    for (const field of fields) {
      if (field.field_type === 'attachments') continue;
      const value = item.values[field.key];
      if (value !== null && value !== undefined && value !== '') return autoNumberOrText(field, value);
    }
    return item.uid;
  }

  function autoNumberOrText(field: FieldDefinition, value: JsonValue) {
    if (field.field_type !== 'auto_number') return String(value);
    const padding = Math.max(0, Number(field.config.padding || 0));
    return `${String(field.config.prefix || '')}${padding ? String(value).padStart(padding, '0') : String(value)}`;
  }

  function isDirty(key: string) { return JSON.stringify(values[key] ?? null) !== JSON.stringify(original[key] ?? null); }
  function upsertConflict(conflict: FieldConflict) { conflicts = [...conflicts.filter((item) => item.field_key !== conflict.field_key), conflict]; }
  function updateCurrentRevision(key: string, revision: number) {
    if (!current) return;
    const next = cloneJson(persisted || current);
    next.field_revisions[key] = revision;
    persisted = next;
  }
  async function applyLiveMessage(message: LiveMessage) {
    if (!current) return;
    if (message.type === 'sync.required') { await refreshOpenRecord('This record was resynchronized after the live connection recovered.'); return; }
    if (!message.payload || typeof message.payload !== 'object') return;
    const payload = message.payload as Record<string, unknown>;
    if (payload.record_uid !== current.uid) return;
    if (message.type === 'record.deleted') { deletedRemotely = true; onClose(); return; }
    if (message.type === 'attachment.created' || message.type === 'attachment.deleted') {
      await refreshOpenRecord('The attachment list was refreshed after another user changed it.');
      return;
    }
    if (message.type !== 'record.fields.updated') { if (message.type === 'record.updated') await refreshOpenRecord('This record was refreshed after another user updated it.'); return; }
    const changes = payload.changes && typeof payload.changes === 'object' ? payload.changes as Record<string, JsonValue> : {};
    const revisions = payload.field_revisions && typeof payload.field_revisions === 'object' ? payload.field_revisions as Record<string, number> : {};
    for (const [key, latestValue] of Object.entries(changes)) {
      const field = fields.find((item) => item.key === key);
      if (!field) continue;
      const latestRevision = Number(revisions[key] || current.field_revisions[key] || 0);
      if (isDirty(key)) upsertConflict({ field_uid: field.uid, field_key: key, label: field.label, base_revision: current.field_revisions[key] || 0, current_revision: latestRevision, current_value: latestValue, your_value: values[key] ?? null });
      else { values[key] = latestValue; original[key] = cloneJson(latestValue); updateCurrentRevision(key, latestRevision); }
    }
    liveNotice = conflicts.length ? 'Another user changed fields you are editing. Choose which values to keep.' : 'This open record was updated with the latest saved values.';
  }

  async function refreshOpenRecord(message: string) {
    if (!current) return;
    const opened = current;
    try {
      const latest = await getRecord(opened.uid);
      if (!current || current.uid !== opened.uid) return;
      const next = cloneJson(latest);
      let conflictCount = 0;
      for (const field of fields.filter((item) => item.field_type !== 'attachments')) {
        const latestValue = latest.values[field.key] ?? null;
        const latestRevision = latest.field_revisions[field.key] || 0;
        const baseRevision = opened.field_revisions[field.key] || 0;
        if (isDirty(field.key) && latestRevision !== baseRevision) {
          upsertConflict({ field_uid: field.uid, field_key: field.key, label: field.label, base_revision: baseRevision, current_revision: latestRevision, current_value: latestValue, your_value: values[field.key] ?? null });
          next.values[field.key] = cloneJson(original[field.key] ?? null);
          next.field_revisions[field.key] = baseRevision;
          conflictCount += 1;
        } else if (!isDirty(field.key)) {
          values[field.key] = cloneJson(latestValue);
          original[field.key] = cloneJson(latestValue);
        }
      }
      persisted = next;
      liveNotice = conflictCount ? `${conflictCount} field${conflictCount === 1 ? '' : 's'} changed elsewhere. Your unsaved input was preserved.` : message;
    } catch { liveNotice = 'Live data changed, but this open record could not be refreshed yet. Saving will still check for conflicts.'; }
  }

  function useLatest(conflict: FieldConflict) { values[conflict.field_key] = cloneJson(conflict.current_value); original[conflict.field_key] = cloneJson(conflict.current_value); updateCurrentRevision(conflict.field_key, conflict.current_revision); conflicts = conflicts.filter((item) => item.field_key !== conflict.field_key); }
  function keepMine(conflict: FieldConflict) { updateCurrentRevision(conflict.field_key, conflict.current_revision); conflicts = conflicts.filter((item) => item.field_key !== conflict.field_key); liveNotice = 'Your value is ready to save over the latest revision.'; }
  function queueFiles(field: FieldDefinition, list: FileList | File[]) {
    const queued = [...(pending[field.uid] || [])];
    const storedCount = filesFor(field).length;
    const maximum = attachmentLimit(field);
    for (const file of Array.from(list)) {
      if (file.size > MAX_ATTACHMENT_SIZE) { error = `${file.name} exceeds the 50 MiB attachment limit.`; continue; }
      if (queued.some((item) => item.name === file.name && item.size === file.size && item.lastModified === file.lastModified)) continue;
      if (maximum > 0 && storedCount + queued.length >= maximum) { error = `${field.label} allows at most ${maximum} file${maximum === 1 ? '' : 's'}.`; break; }
      queued.push(file);
      fileProgress[fileKey(file)] = { percent: 0, label: 'Ready — uploads when you save' };
    }
    pending[field.uid] = queued;
  }
  function fileKey(file: File) { return `${file.name}\u0000${file.size}\u0000${file.lastModified}`; }
  function removePending(fieldUid: string, index: number) { const file = pending[fieldUid]?.[index]; if (file) delete fileProgress[fileKey(file)]; pending[fieldUid] = (pending[fieldUid] || []).filter((_, item) => item !== index); }
  function resetForm() { values = cloneJson(original); pending = {}; fileProgress = {}; uploadProgress = 0; uploadLabel = ''; conflicts = []; error = ''; liveNotice = ''; }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!canWrite || deletedRemotely) return;
    if (conflicts.length) { error = 'Resolve the live field conflicts before saving.'; return; }
    for (const field of fields.filter((item) => item.field_type === 'attachments')) {
      const totalFiles = filesFor(field).length + (pending[field.uid]?.length || 0);
      const maximum = attachmentLimit(field);
      if (field.required && totalFiles === 0) { error = `${field.label} requires at least one file.`; return; }
      if (maximum > 0 && totalFiles > maximum) { error = `${field.label} allows at most ${maximum} file${maximum === 1 ? '' : 's'}.`; return; }
    }
    busy = true; error = ''; uploadProgress = 0; uploadLabel = '';
    try {
      let saved: MxRecord;
      if (current) {
        const changes: Record<string, JsonValue> = {};
        const base: Record<string, number> = {};
        for (const field of fields) {
          if (['attachments', 'auto_number'].includes(field.field_type)) continue;
          if (JSON.stringify(values[field.key] ?? null) !== JSON.stringify(original[field.key] ?? null)) {
            changes[field.key] = values[field.key] ?? null;
            base[field.key] = current.field_revisions[field.key] || 0;
          }
        }
        const hasPendingAttachments = fields.some((field) => field.field_type === 'attachments' && (pending[field.uid]?.length || 0) > 0);
        if (!Object.keys(changes).length && !hasPendingAttachments) { liveNotice = 'There are no changes to save.'; return; }
        saved = Object.keys(changes).length ? await patchRecord(current.uid, changes, base) : cloneJson(current);
      } else {
        const payload: Record<string, JsonValue> = {};
        for (const field of fields) if (!['attachments', 'auto_number'].includes(field.field_type)) payload[field.key] = values[field.key] ?? null;
        saved = await createRecord(payload);
      }
      persisted = saved;
      original = cloneJson(saved.values);
      const uploadFields = fields.filter((item) => item.field_type === 'attachments' && pending[item.uid]?.length);
      const uploadBytes = uploadFields.reduce((sum, field) => sum + pending[field.uid].reduce((fieldSum, file) => fieldSum + file.size, 0), 0);
      let completedBytes = 0;
      for (const field of uploadFields) {
        for (const file of [...pending[field.uid]]) {
          const key = fileKey(file);
          uploadLabel = `Uploading ${file.name}…`;
          fileProgress[key] = { percent: 0, label: `Uploading to ${field.label}…` };
          try {
            await uploadAttachments(saved.uid, field.uid, [file], (loaded, total) => {
              const ratio = total > 0 ? Math.min(1, loaded / total) : 0;
              const percent = Math.round(ratio * 100);
              fileProgress[key] = { percent, label: percent >= 100 ? 'Processing…' : `Uploading · ${percent}%` };
              uploadProgress = uploadBytes ? Math.round((completedBytes + file.size * ratio) / uploadBytes * 100) : 100;
            });
          } catch (reason) {
            fileProgress[key] = { percent: fileProgress[key]?.percent || 0, label: 'Upload failed — save again to retry' };
            throw reason;
          }
          completedBytes += file.size;
          uploadProgress = uploadBytes ? Math.round(completedBytes / uploadBytes * 100) : 100;
          pending[field.uid] = pending[field.uid].filter((item) => item !== file);
          delete fileProgress[key];
        }
      }
      await onSaved();
      onClose();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'The record could not be saved.';
      if (reason instanceof ApiError && reason.status === 409 && reason.payload && typeof reason.payload === 'object') {
        const payload = reason.payload as { conflicts?: FieldConflict[] };
        if (Array.isArray(payload.conflicts)) conflicts = payload.conflicts;
      }
      if (persisted) {
        try { persisted = await getRecord(persisted.uid); } catch { /* The save error remains authoritative. */ }
      }
    } finally { busy = false; }
  }

  async function removeRecord() {
    if (!current || !confirm('Delete this record and its attachments?')) return;
    busy = true;
    try { await deleteRecord(current.uid); await onSaved(); onClose(); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Delete failed.'; }
    finally { busy = false; }
  }

  async function removeFile(uid: string) {
    if (!current || !confirm('Delete this attachment?')) return;
    busy = true;
    try { await deleteAttachment(current.uid, uid); persisted = await getRecord(current.uid); await onSaved(); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Delete failed.'; }
    finally { busy = false; }
  }

  function previewType(mimeType: string, fileName: string): typeof previewKind {
    const type = mimeType.toLowerCase(); const name = fileName.toLowerCase();
    if (type.startsWith('image/')) return 'image';
    if (type === 'application/pdf' || name.endsWith('.pdf')) return 'pdf';
    if (type.startsWith('video/')) return 'video';
    if (type.startsWith('audio/')) return 'audio';
    if (type.startsWith('text/') || /(json|xml|javascript)/.test(type) || /\.(txt|csv|json|xml|md|log|ini|conf|yaml|yml|toml|rs|js|ts|css|html)$/i.test(name)) return 'text';
    return 'unknown';
  }

  async function presentPreview(blob: Blob, title: string, downloadName: string, mimeType: string) {
    closePreview();
    previewName = title;
    previewDownloadName = downloadName || title;
    previewMime = mimeType || blob.type || 'application/octet-stream';
    previewKind = previewType(previewMime, previewDownloadName);
    previewUrl = URL.createObjectURL(blob);
    if (previewKind === 'text') {
      try { previewText = await blob.text(); }
      catch { previewText = 'Unable to decode this file as text.'; }
    }
  }

  async function showPreview(uid: string, name: string) {
    if (!current) return;
    try { const result = await previewAttachment(current.uid, uid); await presentPreview(result.blob, name, result.fileName || name, result.mimeType); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Preview failed.'; }
  }

  function showPending(file: File) { void presentPreview(file, file.name, file.name, file.type); }
  function closePreview() { if (previewUrl) URL.revokeObjectURL(previewUrl); previewUrl = ''; previewName = ''; previewDownloadName = ''; previewMime = ''; previewText = ''; previewKind = 'unknown'; }
  function openPreview() { if (previewUrl) window.open(previewUrl, '_blank', 'noopener,noreferrer'); }
  function downloadPreview() { if (!previewUrl) return; const link = document.createElement('a'); link.href = previewUrl; link.download = previewDownloadName || previewName; link.click(); }
  function jumpToAttachments() { dialogElement?.querySelector<HTMLElement>('.attachment-section')?.scrollIntoView({ behavior: 'smooth', block: 'start' }); }
  function attemptClose() { if (busy) { error = 'Wait for the record save and attachment upload to finish.'; return; } onClose(); }
</script>

<div class="overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && attemptClose()}>
  <div bind:this={dialogElement} class="dialog record-dialog" role="dialog" aria-modal="true" aria-labelledby="record-title">
    <header class="dialog-head">
      <div><p class="eyebrow">{current ? (canWrite ? 'Edit existing' : 'View existing') : 'Create new'}</p><h2 id="record-title">{current ? `${canWrite ? 'Edit' : 'View'} ${recordLabel}` : `New ${recordLabel}`}</h2>{#if current}<p class="muted record-dialog-context">{canWrite ? 'Editing' : 'Viewing'} {recordIdentity(current)}</p>{/if}</div>
      <div class="inline-actions">{#if fields.some((field) => field.field_type === 'attachments') || unassignedFiles.length}<button class="button small attachment-shortcut" type="button" onclick={jumpToAttachments}>Attachments{current ? ` · ${attachmentCount}` : ''}</button>{/if}<button class="icon-button" type="button" onclick={attemptClose} aria-label="Close">×</button></div>
    </header>
    {#if error}<div class="notice error">{error}</div>{/if}
    {#if liveNotice}<div class="notice live-notice">{liveNotice}</div>{/if}
    {#if conflicts.length}<section class="conflict-panel"><strong>Resolve concurrent changes</strong><p>Another user saved these fields after you opened the record.</p>{#each conflicts as conflict}<div class="conflict-row"><div><strong>{conflict.label}</strong><small>Latest: {String(conflict.current_value ?? 'Empty')} · Yours: {String(conflict.your_value ?? 'Empty')}</small></div><div class="inline-actions"><button class="button small" type="button" onclick={() => useLatest(conflict)}>Use latest</button><button class="button primary small" type="button" onclick={() => keepMine(conflict)}>Keep mine</button></div></div>{/each}</section>{/if}
    <form class="record-form" onsubmit={submit}>
      {#if !fields.length}<div class="empty-state compact full"><strong>This deployment has no record fields yet.</strong><p>Open Administration → Record structure and add the first field.</p></div>{/if}
      {#each fields as field}
        {#if field.field_type === 'attachments'}
          <fieldset class="field-block full attachment-drop attachment-section"><legend>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</legend>
            {#if field.config.description}<p class="muted small">{String(field.config.description)}</p>{/if}
            <p class="muted attachment-field-meta">N1: <code>records/&lt;configured folders&gt;/{String(field.config.storage_name || field.label)}/file.ext</code>{#if attachmentLimit(field) > 0} · maximum {attachmentLimit(field)} file{attachmentLimit(field) === 1 ? '' : 's'}{/if}</p>
            {#if !filesFor(field).length && !(pending[field.uid]?.length)}<p class="muted attachment-empty">No stored files in this attachment field.</p>{/if}
            {#each filesFor(field) as file}
              <div class="file-row"><span>{file.file_name} <small>{file.mime_type || 'application/octet-stream'} · {bytes(file.size)}</small></span><div class="inline-actions">
                <button class="link-button" type="button" onclick={() => showPreview(file.uid, file.file_name)}>Preview</button>
                <button class="link-button" type="button" onclick={() => download(`/mx/v1/records/${current?.uid}/attachments/${file.uid}/download`, file.file_name)}>Download</button>
                {#if canDelete}<button class="link-button danger-text" type="button" onclick={() => removeFile(file.uid)}>Delete</button>{/if}
              </div></div>
            {/each}
            {#each pending[field.uid] || [] as file, index}{@const progress = fileProgress[fileKey(file)] || { percent: 0, label: 'Ready — uploads when you save' }}<div class="file-row pending-file"><span>{file.name} <small>{file.type || 'application/octet-stream'} · {bytes(file.size)}</small><span class="file-progress"><i><b style={`width:${progress.percent}%`}></b></i><em>{progress.label}</em></span></span><div class="inline-actions"><button class="link-button" type="button" onclick={() => showPending(file)}>Preview</button><button class="link-button danger-text" type="button" disabled={busy} onclick={() => removePending(field.uid, index)}>Remove</button></div></div>{/each}
            {#if canWrite}<label class:drag-active={dragAttachmentField === field.uid} class="file-picker" ondragenter={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragover={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragleave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) dragAttachmentField = ''; }} ondrop={(event) => { event.preventDefault(); dragAttachmentField = ''; if (event.dataTransfer?.files) queueFiles(field, event.dataTransfer.files); }}><span class="file-picker-icon" aria-hidden="true">⇧</span><span class="file-picker-copy"><strong>Drop {field.config.multiple === false ? 'a file' : 'files'} here or browse</strong><small>Up to 50 MiB per file{attachmentLimit(field) > 0 ? ` · ${attachmentLimit(field)} maximum` : ''}</small></span><input type="file" multiple={field.config.multiple !== false} onchange={(event) => { if (event.currentTarget.files) queueFiles(field, event.currentTarget.files); event.currentTarget.value = ''; }} /></label>{/if}
          </fieldset>
        {:else if field.field_type === 'auto_number'}
          <label>{field.label}<input value={autoNumberValue(field)} disabled placeholder="Assigned automatically when saved" /></label>
        {:else if field.field_type === 'long_text'}
          <label class="full"><span>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</span><textarea rows="4" required={field.required} disabled={!canWrite} value={String(fieldValue(field))} oninput={(event) => setValue(field, event.currentTarget)}></textarea></label>
        {:else if field.field_type === 'select'}
          <label><span>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</span><select required={field.required} disabled={!canWrite} value={String(fieldValue(field))} onchange={(event) => setValue(field, event.currentTarget)}><option value="">Select…</option>{#each (field.config.options as JsonValue[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select></label>
        {:else if field.field_type === 'boolean'}
          <label class="checkbox field-checkbox"><input type="checkbox" checked={Boolean(fieldValue(field))} disabled={!canWrite} onchange={(event) => setValue(field, event.currentTarget)} /><span>{field.label}</span></label>
        {:else}
          <label><span>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</span><input type={field.field_type === 'date' ? 'date' : field.field_type === 'integer' || field.field_type === 'decimal' ? 'number' : 'text'} step={field.field_type === 'decimal' ? 'any' : field.field_type === 'integer' ? 1 : undefined} required={field.required} disabled={!canWrite} value={String(fieldValue(field))} oninput={(event) => setValue(field, event.currentTarget)} /></label>
        {/if}
      {/each}
      {#if unassignedFiles.length}<fieldset class="field-block full attachment-section"><legend>Unassigned legacy attachments</legend><p class="muted small">These files predate File Attachment fields. They remain available for preview and download.</p>{#each unassignedFiles as file}<div class="file-row"><span>{file.file_name} <small>{file.mime_type || 'application/octet-stream'} · {bytes(file.size)}</small></span><div class="inline-actions"><button class="link-button" type="button" onclick={() => showPreview(file.uid, file.file_name)}>Preview</button><button class="link-button" type="button" onclick={() => download(`/mx/v1/records/${current?.uid}/attachments/${file.uid}/download`, file.file_name)}>Download</button>{#if canDelete}<button class="link-button danger-text" type="button" onclick={() => removeFile(file.uid)}>Delete</button>{/if}</div></div>{/each}</fieldset>{/if}
      {#if uploadLabel}<section class="upload-progress-panel full" aria-live="polite"><div><strong>{uploadLabel}</strong><span>{uploadProgress}%</span></div><div class="upload-progress-track"><i style={`width:${uploadProgress}%`}></i></div><small>Keep this window open until every queued file finishes.</small></section>{/if}
      <footer class="dialog-actions">
        {#if current && canDelete}<button class="button danger" type="button" onclick={removeRecord} disabled={busy}>Delete {recordLabel.toLowerCase()}</button>{/if}
        <span class="spacer"></span><button class="button" type="button" onclick={resetForm} disabled={busy}>{current ? 'Reset changes' : 'Clear form'}</button><button class="button" type="button" onclick={attemptClose} disabled={busy}>Cancel</button>
        {#if canWrite}<button class="button primary" type="submit" disabled={busy || deletedRemotely}>{busy ? 'Saving and uploading…' : current ? `Update ${recordLabel}` : `Save ${recordLabel}`}</button>{/if}
      </footer>
    </form>
  </div>
</div>

{#if previewUrl}
  <div class="overlay preview-overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && closePreview()}><div class="dialog preview-dialog" role="dialog" aria-modal="true" aria-labelledby="preview-title"><header class="dialog-head"><div><h2 id="preview-title">{previewName}</h2><p class="muted preview-meta">{previewMime}{previewDownloadName !== previewName ? ` · generated preview: ${previewDownloadName}` : ''}</p></div><div class="inline-actions"><button class="button small" onclick={openPreview}>Open in new tab</button><button class="button small" onclick={downloadPreview}>{previewDownloadName !== previewName ? 'Download preview' : 'Download'}</button><button class="icon-button" aria-label="Close preview" onclick={closePreview}>×</button></div></header><div class="preview-content">{#if previewKind === 'image'}<img src={previewUrl} alt={previewName} />{:else if previewKind === 'pdf'}<iframe title={`Preview of ${previewName}`} src={previewUrl}></iframe>{:else if previewKind === 'video'}<video src={previewUrl} controls><track kind="captions" /></video>{:else if previewKind === 'audio'}<audio src={previewUrl} controls><track kind="captions" /></audio>{:else if previewKind === 'text'}<pre>{previewText}</pre>{:else}<div class="preview-fallback"><h3>Browser preview unavailable</h3><p class="muted">This file type cannot be rendered safely in the browser. Download it to open it with the appropriate application.</p><button class="button primary" onclick={downloadPreview}>Download {previewName}</button></div>{/if}</div></div></div>
{/if}
