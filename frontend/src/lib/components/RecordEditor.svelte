<script lang="ts">
  import { tick } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Eye from '@lucide/svelte/icons/eye';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Upload from '@lucide/svelte/icons/upload';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { ApiError } from '../api/client';
  import { operationUid } from '../api/operation';
  import type { FieldConflict, FieldDefinition, JsonValue, MxRecord, RecordVersionDetail, RecordVersionSummary, SchemaResponse } from '../api/domain';
  import type { LiveMessage } from '../live/client';
  import { requestConfirmation } from '../confirmation';
  import { cloneJson } from '../util/json';
  import { createRecord, deleteAttachment, deleteRecord, download, getRecord, getRecordVersion, listRecordVersions, patchRecord, previewAttachment, restoreRecordVersion, uploadAttachments } from '../api/workspace';
  import FilePreview from './FilePreview.svelte';
  import DrivePicker from './DrivePicker.svelte';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import { driveFile, type DriveItem } from '../api/drive';
  import RecordAttachmentThumbnail from './RecordAttachmentThumbnail.svelte';

  let { schema, moduleUid = '', record, canWrite, canDelete, canAttachments = true, allowEdit = false, liveMessage = null, focusAttachments = false, recordLabel = 'Record', onEdit = () => undefined, onClose, onSaved }:
    { schema: SchemaResponse; moduleUid?: string; record: MxRecord | null; canWrite: boolean; canDelete: boolean; canAttachments?: boolean; allowEdit?: boolean; liveMessage?: LiveMessage | null; focusAttachments?: boolean; recordLabel?: string; onEdit?: () => void; onClose: () => void; onSaved: () => Promise<void> } = $props();

  type PreviewEntry = { key: string; name: string; mimeType: string; detail: string; storedUid?: string; pendingFile?: File };

  let values = $state<Record<string, JsonValue>>({});
  let original = $state<Record<string, JsonValue>>({});
  let pending = $state<Record<string, File[]>>({});
  let busy = $state(false);
  let error = $state('');
  let previewOpen = $state(false);
  let previewKey = $state('');
  let persisted = $state<MxRecord | null>(null);
  let conflicts = $state<FieldConflict[]>([]);
  let liveNotice = $state('');
  let deletedRemotely = $state(false);
  let handledSequence = $state(0);
  let uploadProgress = $state(0);
  let uploadLabel = $state('');
  let fileProgress = $state<Record<string, { percent: number; label: string }>>({});
  let dragAttachmentField = $state('');
  let driveAttachmentField = $state<FieldDefinition | null>(null);
  let driveCopying = $state(false);
  let workingRevision = $state(1);
  let createSubmission = $state<{ uid: string; values: Record<string, JsonValue> } | null>(null);
  let historyOpen = $state(false);
  let historyLoading = $state(false);
  let historyError = $state('');
  let versions = $state<RecordVersionSummary[]>([]);
  let selectedVersion = $state<RecordVersionDetail | null>(null);
  let dialogElement: HTMLDivElement;
  let historyDialogElement = $state<HTMLDivElement>();
  let historyButtonElement = $state<HTMLButtonElement>();
  let focusedAttachments = false;
  const current = $derived(persisted || record);
  const attachmentCount = $derived(current?.attached_files.length || 0);
  const previewEntries = $derived.by(() => {
    const entries: PreviewEntry[] = [];
    for (const file of current?.attached_files || []) entries.push({ key: `stored:${file.uid}`, name: file.file_name, mimeType: file.mime_type || 'application/octet-stream', detail: `${file.attachment_field_label || 'Attachment'} · ${file.mime_type || 'application/octet-stream'} · ${bytes(file.size)}`, storedUid: file.uid });
    for (const files of Object.values(pending)) for (const file of files) entries.push({ key: `pending:${fileKey(file)}`, name: file.name, mimeType: file.type || 'application/octet-stream', detail: `Queued · ${file.type || 'application/octet-stream'} · ${bytes(file.size)}`, pendingFile: file });
    return entries;
  });
  const previewIndex = $derived(Math.max(0, previewEntries.findIndex((item) => item.key === previewKey)));
  const activePreview = $derived(previewEntries[previewIndex] || null);
  const MAX_ATTACHMENT_SIZE = 50 * 1024 * 1024;

  $effect(() => {
    const source = record?.values || {};
    const base = cloneJson(source);
    if (!record && schema.fields.some((field) => field.active && field.key === 'date' && field.field_type === 'date')) {
      const now = new Date();
      base.date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    }
    const initial = cloneJson(base);
    let restoredDraft = false;
    let restoredRevision = record?.revision || 1;
    let restoredSubmission: typeof createSubmission = null;
    try {
      const saved = canWrite ? JSON.parse(sessionStorage.getItem(draftStorageKey()) || 'null') as { values?: Record<string, JsonValue>; original?: Record<string, JsonValue>; base_revision?: number; create_submission?: {uid:string;values:Record<string,JsonValue>} } | null : null;
      if (!record && saved?.create_submission?.uid) restoredSubmission = saved.create_submission;
      if (saved?.values && typeof saved.values === 'object') {
        const editable = new Set(schema.fields.filter((field) => field.active && !['attachments', 'auto_number', 'formula'].includes(field.field_type)).map((field) => field.key));
        for (const [key, value] of Object.entries(saved.values)) {
          if (!editable.has(key)) continue;
          const savedOriginal = saved.original?.[key] ?? null;
          if (JSON.stringify(value ?? null) === JSON.stringify(savedOriginal)) continue;
          initial[key] = value;
          base[key] = cloneJson(savedOriginal);
        }
        restoredRevision = Number(saved.base_revision) > 0 && Number(saved.base_revision) <= restoredRevision ? Number(saved.base_revision) : 1;
        restoredDraft = JSON.stringify(initial) !== JSON.stringify(base);
      }
    } catch { /* A malformed or unavailable session store should not block the editor. */ }
    values = initial;
    createSubmission = restoredSubmission;
    original = cloneJson(base);
    pending = {};
    persisted = null;
    workingRevision = restoredRevision;
    conflicts = [];
    liveNotice = restoredDraft ? 'Your unsaved entries from this tab were restored.' : '';
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
    if (!message || !message.sequence || message.sequence <= handledSequence) return;
    if (!current || busy) return;
    handledSequence = message.sequence;
    void applyLiveMessage(message);
  });

  const fields = $derived(schema.fields.filter((field) => field.active && (canAttachments || field.field_type !== 'attachments')).sort((a, b) => a.position - b.position));
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
    persistDraft();
  }

  function draftStorageKey() {
    return `mx-record-draft:${moduleUid || 'mx-default-records'}:${record?.uid || 'new'}`;
  }

  function clearDraft() {
    try { sessionStorage.removeItem(draftStorageKey()); } catch { /* Ignore restricted storage. */ }
  }

  function persistDraft() {
    if (!canWrite) return;
    const changed = schema.fields.some((field) => !['attachments', 'auto_number', 'formula'].includes(field.field_type) && JSON.stringify(values[field.key] ?? null) !== JSON.stringify(original[field.key] ?? null));
    try {
      if (changed || createSubmission) sessionStorage.setItem(draftStorageKey(), JSON.stringify({ values: cloneJson(values), original: cloneJson(original), base_revision: workingRevision, create_submission: createSubmission, saved_at: Date.now() }));
      else sessionStorage.removeItem(draftStorageKey());
    } catch { /* The editor continues even when private browsing blocks storage. */ }
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
    const hasDirtyFields = fields.some((field) => isDirty(field.key));
    for (const [key, latestValue] of Object.entries(changes)) {
      if (!isDirty(key)) { values[key] = latestValue; original[key] = cloneJson(latestValue); }
    }
    const nextRevision = Number(payload.record_revision || 0);
    if (!hasDirtyFields && nextRevision > 0) workingRevision = nextRevision;
    liveNotice = hasDirtyFields ? 'Another user saved this record. Your unsaved input is preserved and will be merged safely when you save.' : 'This open record was updated with the latest saved values.';
  }

  async function refreshOpenRecord(message: string) {
    if (!current) return;
    const opened = current;
    try {
      const latest = await getRecord(opened.uid, moduleUid || undefined);
      if (!current || current.uid !== opened.uid) return;
      const next = cloneJson(latest);
      let dirtyCount = 0;
      for (const field of fields.filter((item) => item.field_type !== 'attachments')) {
        const latestValue = latest.values[field.key] ?? null;
        if (isDirty(field.key)) {
          next.values[field.key] = cloneJson(original[field.key] ?? null);
          dirtyCount += 1;
        } else {
          values[field.key] = cloneJson(latestValue);
          original[field.key] = cloneJson(latestValue);
        }
      }
      persisted = next;
      if (!dirtyCount) workingRevision = latest.revision;
      liveNotice = dirtyCount ? `Saved data changed elsewhere. Your ${dirtyCount} unsaved field${dirtyCount === 1 ? '' : 's'} remain local and will be checked when you save.` : message;
    } catch { liveNotice = 'Live data changed, but this open record could not be refreshed yet. Saving will still check for conflicts.'; }
  }

  function useLatest(conflict: FieldConflict) { values[conflict.field_key] = cloneJson(conflict.current_value); original[conflict.field_key] = cloneJson(conflict.current_value); conflicts = conflicts.filter((item) => item.field_key !== conflict.field_key); if (!conflicts.length) error = ''; }
  function keepMine(conflict: FieldConflict) { conflicts = conflicts.filter((item) => item.field_key !== conflict.field_key); if (!conflicts.length) error = ''; liveNotice = 'Your value is ready to save over the latest stored revision.'; }
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
  async function attachDriveFile(item: DriveItem | null) {
    const field = driveAttachmentField;
    if (!item || !field || !canWrite || busy) return;
    driveAttachmentField = null;
    if (item.size > MAX_ATTACHMENT_SIZE) { error = 'This file exceeds the record attachment limit of 50 MiB.'; return; }
    driveCopying = true;
    try { const file = await driveFile(item); if (canWrite && !busy && !deletedRemotely) queueFiles(field, [file]); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Could not copy this Drive file.'; }
    finally { driveCopying = false; }
  }
  function fileKey(file: File) { return `${file.name}\u0000${file.size}\u0000${file.lastModified}`; }
  function removePending(fieldUid: string, index: number) { const file = pending[fieldUid]?.[index]; if (file) delete fileProgress[fileKey(file)]; pending[fieldUid] = (pending[fieldUid] || []).filter((_, item) => item !== index); }
  function resetForm() {
    if (driveCopying || busy) return;
    if (createSubmission && !persisted) {
      error = 'The previous create has not been acknowledged. Save again to check it before clearing this form.';
      return;
    }
    createSubmission = null;
    values = cloneJson(original); pending = {}; fileProgress = {}; uploadProgress = 0; uploadLabel = ''; conflicts = []; error = ''; liveNotice = ''; clearDraft();
  }
  async function copyRecordLink() {
    if (!current) return;
    const module = moduleUid || 'mx-default-records';
    const link = `${location.origin}${location.pathname}#module/${encodeURIComponent(module)}?record=${encodeURIComponent(current.uid)}`;
    try { await navigator.clipboard.writeText(link); liveNotice = 'Record link copied. Paste it into Collaboration to share this record securely.'; }
    catch { error = 'The browser could not copy the record link. Copy it from the address bar instead.'; }
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!canWrite || deletedRemotely || busy) return;
    if (driveCopying) { error = 'Wait for the Drive file to finish copying before saving.'; return; }
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
        for (const field of fields) {
          if (['attachments', 'auto_number', 'formula'].includes(field.field_type)) continue;
          if (JSON.stringify(values[field.key] ?? null) !== JSON.stringify(original[field.key] ?? null)) {
            changes[field.key] = values[field.key] ?? null;
          }
        }
        const hasPendingAttachments = fields.some((field) => field.field_type === 'attachments' && (pending[field.uid]?.length || 0) > 0);
        if (!Object.keys(changes).length && !hasPendingAttachments) { liveNotice = 'There are no changes to save.'; return; }
        saved = Object.keys(changes).length ? await patchRecord(current.uid, changes, workingRevision, moduleUid || undefined) : cloneJson(current);
      } else {
        const payload: Record<string, JsonValue> = {};
        for (const field of fields) if (!['attachments', 'auto_number', 'formula'].includes(field.field_type)) payload[field.key] = values[field.key] ?? null;
        if (!createSubmission) createSubmission = { uid: operationUid(), values: cloneJson(payload) };
        persistDraft();
        saved = await createRecord(createSubmission.values, moduleUid || undefined, createSubmission.uid);
        // A user may amend retained entries after an uncertain create. First
        // acknowledge that original operation, then merge the subsequent edit.
        persisted = saved;
        workingRevision = saved.revision;
        original = cloneJson(saved.values);
        const subsequent = Object.fromEntries(Object.entries(payload).filter(([key, value]) => JSON.stringify(value) !== JSON.stringify(createSubmission!.values[key])));
        if (Object.keys(subsequent).length) saved = await patchRecord(saved.uid, subsequent, saved.revision, moduleUid || undefined);
        createSubmission = null;
      }
      persisted = saved;
      workingRevision = saved.revision;
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
            const uploaded = await uploadAttachments(saved.uid, field.uid, [file], workingRevision, (loaded, total) => {
              const ratio = total > 0 ? Math.min(1, loaded / total) : 0;
              const percent = Math.round(ratio * 100);
              fileProgress[key] = { percent, label: percent >= 100 ? 'Processing…' : `Uploading · ${percent}%` };
              uploadProgress = uploadBytes ? Math.round((completedBytes + file.size * ratio) / uploadBytes * 100) : 100;
            });
            if (uploaded.record_revision > 0) workingRevision = uploaded.record_revision;
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
      clearDraft();
      await onSaved();
      onClose();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : 'The record could not be saved.';
      if (!persisted && reason instanceof ApiError && [400, 403, 404, 413, 422].includes(reason.status)) {
        // These responses definitively rejected creation. Permit corrected
        // entries to start a fresh operation; uncertain outcomes retain theirs.
        createSubmission = null;
        persistDraft();
      }
      if (reason instanceof ApiError && reason.status === 409 && reason.payload && typeof reason.payload === 'object') {
        const payload = reason.payload as { conflicts?: FieldConflict[]; current_record_revision?: number };
        if (Array.isArray(payload.conflicts)) conflicts = payload.conflicts;
        if (Number(payload.current_record_revision) > 0) workingRevision = Number(payload.current_record_revision);
      }
      if (persisted) {
        try { persisted = await getRecord(persisted.uid, moduleUid || undefined); } catch { /* The save error remains authoritative. */ }
      }
    } finally { busy = false; }
  }

  async function removeRecord() {
    if (!current || !await requestConfirmation({
      title: `Move this ${recordLabel.toLowerCase()} to trash?`,
      description: 'Its fields, attachments, and version history will be preserved so an administrator can inspect or restore it.',
      confirmLabel: 'Move to trash'
    })) return;
    busy = true;
    try { await deleteRecord(current.uid, moduleUid || undefined); await onSaved(); onClose(); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Delete failed.'; }
    finally { busy = false; }
  }

  async function openHistory() {
    if (!current) return;
    historyButtonElement?.blur();
    historyOpen = true; historyLoading = true; historyError = ''; selectedVersion = null; versions = [];
    await tick(); historyDialogElement?.focus();
    try {
      versions = (await listRecordVersions(current.uid, moduleUid || undefined)).versions;
      if (versions[0]) await selectVersion(versions[0]);
    }
    catch (reason) { historyError = reason instanceof Error ? reason.message : 'History could not be loaded.'; }
    finally { historyLoading = false; }
  }
  async function closeHistory() {
    historyOpen = false; historyError = ''; selectedVersion = null;
    await tick(); historyButtonElement?.focus();
  }
  async function selectVersion(item: RecordVersionSummary) {
    if (!current) return;
    historyLoading = true; historyError = '';
    try { selectedVersion = await getRecordVersion(current.uid, item.uid, moduleUid || undefined); }
    catch (reason) { historyError = reason instanceof Error ? reason.message : 'Version could not be loaded.'; }
    finally { historyLoading = false; }
  }
  async function restoreVersion(item: RecordVersionSummary) {
    if (!current) return;
    const hasUnsavedChanges = fields.some((field) => field.field_type !== 'attachments' && isDirty(field.key)) || Object.values(pending).some((files) => files.length);
    const warning = hasUnsavedChanges ? ' Your unsaved editor changes and queued files will be replaced.' : '';
    if (!await requestConfirmation({
      title: `Restore version ${item.version}?`,
      description: `The current saved state will remain in history.${warning}`,
      confirmLabel: 'Restore version',
      tone: 'primary'
    })) return;
    historyLoading = true; historyError = '';
    try {
      await restoreRecordVersion(current.uid, item.uid, moduleUid || undefined);
      persisted = await getRecord(current.uid, moduleUid || undefined); values = cloneJson(persisted.values); original = cloneJson(persisted.values); pending = {}; fileProgress = {};
      await onSaved(); liveNotice = `Version ${item.version} was restored. The previous state remains recoverable.`; await closeHistory();
    } catch (reason) { historyError = reason instanceof Error ? reason.message : 'Version could not be restored.'; }
    finally { historyLoading = false; }
  }

  function handleDialogKeydown(event: KeyboardEvent) {
    if (!historyOpen) return;
    if (event.key === 'Escape') { event.preventDefault(); void closeHistory(); return; }
    if (event.key !== 'Tab' || !historyDialogElement) return;
    const focusable = [...historyDialogElement.querySelectorAll<HTMLElement>('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')];
    if (!focusable.length) { event.preventDefault(); historyDialogElement.focus(); return; }
    const first = focusable[0]; const last = focusable[focusable.length - 1];
    if (document.activeElement === historyDialogElement || !historyDialogElement.contains(document.activeElement)) { event.preventDefault(); (event.shiftKey ? last : first).focus(); }
    else if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }

  async function removeFile(uid: string) {
    if (!current || !await requestConfirmation({
      title: 'Delete this attachment?',
      description: 'The file will be removed from this record. This action cannot be undone from the record editor.',
      confirmLabel: 'Delete attachment'
    })) return;
    busy = true;
    try { const result = await deleteAttachment(current.uid, uid, workingRevision); workingRevision = result.record_revision; persisted = await getRecord(current.uid, moduleUid || undefined); await onSaved(); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Delete failed.'; }
    finally { busy = false; }
  }

  function showPreview(uid: string) { previewKey = `stored:${uid}`; previewOpen = true; }
  function showPending(file: File) { previewKey = `pending:${fileKey(file)}`; previewOpen = true; }
  function closePreview() { previewOpen = false; previewKey = ''; }
  function navigatePreview(index: number) {
    const entry = previewEntries[index];
    if (entry) previewKey = entry.key;
  }
  async function loadPreview(entry: PreviewEntry) {
    if (entry.pendingFile) return { blob: entry.pendingFile, fileName: entry.name, mimeType: entry.pendingFile.type || 'application/octet-stream' };
    if (!current || !entry.storedUid) throw new Error('This attachment is no longer available.');
    const result = await previewAttachment(current.uid, entry.storedUid, entry.name, entry.mimeType);
    return { blob: result.blob, url: result.url, fileName: result.fileName || entry.name, mimeType: result.mimeType };
  }
  function jumpToAttachments() { dialogElement?.querySelector<HTMLElement>('.attachment-section')?.scrollIntoView({ behavior: 'smooth', block: 'start' }); }
  function attemptClose() {
    if (driveCopying) { error = 'Wait for the Drive file to finish copying before closing.'; return; }
    if (busy) { error = 'Wait for the record save and attachment upload to finish.'; return; }
    if (Object.values(pending).some((files) => files.length)) {
      error = 'Queued local files cannot be retained by the browser. Save the record or remove them before closing.';
      return;
    }
    persistDraft();
    onClose();
  }
</script>

<svelte:window onkeydown={handleDialogKeydown} />

<div class="overlay record-editor-overlay" class:subdialog-open={historyOpen || previewOpen} role="presentation" aria-hidden={historyOpen || previewOpen ? 'true' : undefined} inert={historyOpen || previewOpen} onclick={(event) => event.target === event.currentTarget && attemptClose()}>
  <div bind:this={dialogElement} class="dialog record-dialog" role="dialog" aria-modal="true" aria-labelledby="record-title">
    <header class="dialog-head">
      <div><p class="eyebrow">{current ? (canWrite ? 'Edit existing' : 'View existing') : 'Create new'}</p><h2 id="record-title">{current ? `${canWrite ? 'Edit' : 'View'} ${recordLabel}` : `New ${recordLabel}`}</h2>{#if current}<p class="muted record-dialog-context">{canWrite ? 'Editing' : 'Viewing'} {recordIdentity(current)}</p>{/if}</div>
      <div class="inline-actions">{#if current && allowEdit && !canWrite}<button class="button primary small icon-label" type="button" onclick={onEdit}><Pencil size={14} />Edit {recordLabel.toLowerCase()}</button>{/if}{#if current}<button class="button small" type="button" onclick={copyRecordLink}>Copy link</button><button bind:this={historyButtonElement} class="button small icon-label" type="button" onclick={openHistory}><HistoryIcon size={15} />History</button>{/if}{#if fields.some((field) => field.field_type === 'attachments') || unassignedFiles.length}<button class="button small attachment-shortcut" type="button" onclick={jumpToAttachments}>Attachments{current ? ` · ${attachmentCount}` : ''}</button>{/if}<button class="icon-button" type="button" onclick={attemptClose} aria-label="Close"><X size={18} /></button></div>
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
            {#if canWrite}<button class="button small icon-label" type="button" disabled={busy || driveCopying} onclick={() => driveAttachmentField = field}><HardDrive size={15} />{driveCopying ? 'Copying Drive file…' : 'Choose from MX Drive'}</button>{/if}
            {#if !filesFor(field).length && !(pending[field.uid]?.length)}<p class="muted attachment-empty">No stored files in this attachment field.</p>{/if}
            {#each filesFor(field) as file}
              <div class="file-row record-file-row"><RecordAttachmentThumbnail recordUid={current!.uid} {file} onOpen={() => showPreview(file.uid)} /><span class="record-file-copy"><strong>{file.file_name}</strong><small>{file.mime_type || 'application/octet-stream'} · {bytes(file.size)}</small></span><div class="inline-actions">
                <button class="file-action-icon" type="button" onclick={() => showPreview(file.uid)} aria-label={`Preview ${file.file_name}`} title="Preview"><Eye size={15} /></button>
                <button class="file-action-icon" type="button" onclick={() => download(`/mx/v1/records/${current?.uid}/attachments/${file.uid}/download`, file.file_name)} aria-label={`Download ${file.file_name}`} title="Download"><DownloadIcon size={15} /></button>
                {#if canDelete}<button class="file-action-icon danger-text" type="button" onclick={() => removeFile(file.uid)} aria-label={`Delete ${file.file_name}`} title="Delete"><Trash2 size={15} /></button>{/if}
              </div></div>
            {/each}
            {#each pending[field.uid] || [] as file, index}{@const progress = fileProgress[fileKey(file)] || { percent: 0, label: 'Ready — uploads when you save' }}<div class="file-row pending-file"><span>{file.name} <small>{file.type || 'application/octet-stream'} · {bytes(file.size)}</small><span class="file-progress"><i><b style={`width:${progress.percent}%`}></b></i><em>{progress.label}</em></span></span><div class="inline-actions"><button class="file-action-icon" type="button" onclick={() => showPending(file)} aria-label={`Preview ${file.name}`} title="Preview"><Eye size={15} /></button><button class="file-action-icon danger-text" type="button" disabled={busy} onclick={() => removePending(field.uid, index)} aria-label={`Remove ${file.name}`} title="Remove"><X size={15} /></button></div></div>{/each}
            {#if canWrite}<label class:drag-active={dragAttachmentField === field.uid} class="file-picker" ondragenter={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragover={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragleave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) dragAttachmentField = ''; }} ondrop={(event) => { event.preventDefault(); dragAttachmentField = ''; if (event.dataTransfer?.files) queueFiles(field, event.dataTransfer.files); }}><span class="file-picker-icon" aria-hidden="true"><Upload size={19} /></span><span class="file-picker-copy"><strong>Drop {field.config.multiple === false ? 'a file' : 'files'} here or browse</strong><small>Up to 50 MiB per file{attachmentLimit(field) > 0 ? ` · ${attachmentLimit(field)} maximum` : ''}</small></span><input type="file" multiple={field.config.multiple !== false} onchange={(event) => { if (event.currentTarget.files) queueFiles(field, event.currentTarget.files); event.currentTarget.value = ''; }} /></label>{/if}
          </fieldset>
        {:else if field.field_type === 'auto_number'}
          <label>{field.label}<input value={autoNumberValue(field)} disabled placeholder="Assigned automatically when saved" /></label>
        {:else if field.field_type === 'formula'}
          <label><span>{field.label}<small class="field-kind-hint">Calculated</small></span><input value={String(fieldValue(field))} disabled placeholder="Calculated when saved" /></label>
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
      {#if unassignedFiles.length}<fieldset class="field-block full attachment-section"><legend>Unassigned legacy attachments</legend><p class="muted small">These files predate File Attachment fields. They remain available for preview and download.</p>{#each unassignedFiles as file}<div class="file-row record-file-row"><RecordAttachmentThumbnail recordUid={current!.uid} {file} onOpen={() => showPreview(file.uid)} /><span class="record-file-copy"><strong>{file.file_name}</strong><small>{file.mime_type || 'application/octet-stream'} · {bytes(file.size)}</small></span><div class="inline-actions"><button class="file-action-icon" type="button" onclick={() => showPreview(file.uid)} aria-label={`Preview ${file.file_name}`} title="Preview"><Eye size={15} /></button><button class="file-action-icon" type="button" onclick={() => download(`/mx/v1/records/${current?.uid}/attachments/${file.uid}/download`, file.file_name)} aria-label={`Download ${file.file_name}`} title="Download"><DownloadIcon size={15} /></button>{#if canDelete}<button class="file-action-icon danger-text" type="button" onclick={() => removeFile(file.uid)} aria-label={`Delete ${file.file_name}`} title="Delete"><Trash2 size={15} /></button>{/if}</div></div>{/each}</fieldset>{/if}
      {#if uploadLabel}<section class="upload-progress-panel full" aria-live="polite"><div><strong>{uploadLabel}</strong><span>{uploadProgress}%</span></div><div class="upload-progress-track"><i style={`width:${uploadProgress}%`}></i></div><small>Keep this window open until every queued file finishes.</small></section>{/if}
      <footer class="dialog-actions">
        {#if current && canDelete}<button class="button danger" type="button" onclick={removeRecord} disabled={busy || driveCopying}>Delete {recordLabel.toLowerCase()}</button>{/if}
        <span class="spacer"></span><button class="button" type="button" onclick={resetForm} disabled={busy || driveCopying}>{current ? 'Reset changes' : 'Clear form'}</button><button class="button" type="button" onclick={attemptClose} disabled={busy || driveCopying}>Cancel</button>
        {#if canWrite}<button class="button primary" type="submit" disabled={busy || driveCopying || deletedRemotely}>{driveCopying ? 'Copying Drive file…' : busy ? 'Saving and uploading…' : current ? `Update ${recordLabel}` : `Save ${recordLabel}`}</button>{/if}
      </footer>
    </form>
  </div>
</div>

{#if driveAttachmentField}<DrivePicker onChoose={(item) => void attachDriveFile(item)} onClose={() => driveAttachmentField = null} />{/if}
{#if previewOpen && activePreview}
  {#key activePreview.key}<FilePreview fileName={activePreview.name} load={() => loadPreview(activePreview)} navigationItems={previewEntries} activeIndex={previewIndex} onNavigate={navigatePreview} onClose={closePreview} />{/key}
{/if}

{#if historyOpen}
  <div class="overlay history-overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && void closeHistory()}>
    <div bind:this={historyDialogElement} class="dialog history-dialog" role="dialog" aria-modal="true" aria-labelledby="history-title" aria-describedby="history-description" aria-busy={historyLoading} tabindex="-1">
      <header class="dialog-head">
        <div><p class="eyebrow">Record lifecycle</p><h2 id="history-title">{recordLabel} history</h2><p id="history-description" class="muted">Review every saved state without leaving this {recordLabel.toLowerCase()}. Unsaved editor changes remain in place unless you restore a version.</p></div>
        <div class="inline-actions"><button class="button small icon-label" type="button" onclick={() => void closeHistory()}><ArrowLeft size={15} />Back to {recordLabel.toLowerCase()}</button><button class="icon-button" type="button" aria-label="Close history" title="Close history" onclick={() => void closeHistory()}><X size={18} /></button></div>
      </header>
      {#if historyError}<div class="notice error history-error" role="alert">{historyError}</div>{/if}
      <div class="history-layout">
        <aside class="history-list" aria-label="Saved versions">
          <header><strong>Saved versions</strong><span>{versions.length}</span></header>
          {#if historyLoading && !versions.length}<div class="history-list-loading"><div class="loader"></div><span>Loading history…</span></div>
          {:else if !versions.length}<div class="empty-state compact"><strong>No saved versions yet</strong><p>A version is created whenever this {recordLabel.toLowerCase()} is saved.</p></div>{/if}
          {#each versions as item}<button class:active={selectedVersion?.uid === item.uid} aria-pressed={selectedVersion?.uid === item.uid} onclick={() => selectVersion(item)}><strong>Version {item.version}</strong><span>{item.event.replaceAll('_', ' ')} · {item.actor_name}</span><small>{new Date(item.created_at).toLocaleString()}</small></button>{/each}
        </aside>
        <section class="history-detail" aria-live="polite">
          {#if selectedVersion}
            <div class="panel-heading"><div><p class="eyebrow">Snapshot</p><h3>Version {selectedVersion.version}</h3><p class="muted">{selectedVersion.attachments.length} attachment{selectedVersion.attachments.length === 1 ? '' : 's'} in this saved state</p></div>{#if canWrite}<button class="button primary small" disabled={historyLoading} onclick={() => restoreVersion(selectedVersion!)}>Restore this version</button>{/if}</div>
            {#if historyLoading}<div class="history-detail-status"><div class="loader"></div><span>Loading saved state…</span></div>{/if}
            <dl>{#each fields.filter((field) => field.field_type !== 'attachments') as field}<div><dt>{field.label}</dt><dd>{String(selectedVersion.values[field.key] ?? '—')}</dd></div>{/each}</dl>
          {:else if historyLoading}<div class="history-detail-placeholder"><div class="loader"></div><strong>Loading the newest saved state…</strong></div>
          {:else}<div class="empty-state compact"><HistoryIcon size={24} /><strong>Select a version</strong><p>Review its saved field values before restoring it.</p></div>{/if}
        </section>
      </div>
    </div>
  </div>
{/if}
