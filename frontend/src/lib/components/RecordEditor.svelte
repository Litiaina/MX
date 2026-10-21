<script lang="ts">
  import { tick, onMount, onDestroy, untrack, getContext } from 'svelte';
  import { writable, type Writable } from 'svelte/store';
  import { fieldIsBold, moduleReadingContext } from '../util/readingPreferences';
  import type { ModuleReadingMap } from '../api/domain';
  import Send from '@lucide/svelte/icons/send';
  import SendRecordDialog from './SendRecordDialog.svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Eye from '@lucide/svelte/icons/eye';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Upload from '@lucide/svelte/icons/upload';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { ApiError } from '../api/client';
  import { loadUploadLimits } from '../api/workspace';
  import { operationUid } from '../api/operation';
  import type { FieldConflict, FieldDefinition, JsonValue, MxRecord, RecordVersionDetail, RecordVersionSummary, SchemaResponse } from '../api/domain';
  import type { LiveMessage } from '../live/client';
  import { requestConfirmation } from '../confirmation';
  import { cloneJson } from '../util/json';
  import { createRecord, deleteAttachment, deleteRecord, download, getRecord, getRecordVersion, listRecordVersions, patchRecord, previewAttachment, restoreRecordVersion, uploadAttachments } from '../api/workspace';
  import FilePreview from './FilePreview.svelte';
  import { officeExtension } from '../office/formats';
  import { openRecordOffice } from '../office/launch';
  import DrivePicker from './DrivePicker.svelte';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import { driveFile, type DriveItem } from '../api/drive';
  import RecordAttachmentThumbnail from './RecordAttachmentThumbnail.svelte';
  import RelationshipInput from './RelationshipInput.svelte';
  import RelatedRecords from './RelatedRecords.svelte';
  import {relationshipPreview} from '../api/relationships';
  import {applyRelationshipPreview} from '../util/relationshipAutofill';
  import SelectFieldInput from './SelectFieldInput.svelte';
  import {fieldState,fieldRules,optionDetails,prepareForm,writable as ruleWritable} from '../util/formRules';

  let { schema, moduleUid = '', record, canWrite, canDelete, canAttachments = true, allowEdit = false, liveMessage = null, focusAttachments = false, recordLabel = 'Record', initialChanges = null, submitCreate, draftScope = '', onEdit = () => undefined, onClose, onSaved }:
    { schema: SchemaResponse; moduleUid?: string; record: MxRecord | null; canWrite: boolean; canDelete: boolean; canAttachments?: boolean; allowEdit?: boolean; liveMessage?: LiveMessage | null; focusAttachments?: boolean; recordLabel?: string; initialChanges?:Record<string,JsonValue>|null; submitCreate?: (values: Record<string, JsonValue>) => Promise<MxRecord>; draftScope?: string; onEdit?: () => void; onClose: () => void; onSaved: () => Promise<void> } = $props();

  type PreviewEntry = { key: string; name: string; mimeType: string; detail: string; storedUid?: string; pendingFile?: File };

  let values = $state<Record<string, JsonValue>>({});
  const readingPreferences = getContext<Writable<ModuleReadingMap>>(moduleReadingContext) || writable<ModuleReadingMap>({});
  let sendOpen = $state(false);
  let destroyed=false;onDestroy(()=>destroyed=true);
  const relationshipRequests = new Map<string,AbortController>();
  let relationshipLoading = $state<Record<string,boolean>>({});
  let relationshipErrors = $state<Record<string,string>>({});
  let recoveredRelationshipPending = new Set<string>();
  const relationshipPending = $derived(Object.values(relationshipLoading).some(Boolean));
  const relationshipFailed = $derived(Object.values(relationshipErrors).some(Boolean));
  function cancelRelationshipRequests(){for(const controller of relationshipRequests.values())controller.abort();relationshipRequests.clear();relationshipLoading={};relationshipErrors={};}
  onDestroy(cancelRelationshipRequests);
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
  let officePreviewNotice = $state('');
  let attachmentMaxBytes = $state<number | null>(null);
  let attachmentProblem = $state('');
  let attachmentPolicyBusy = $state(false);
  let attachmentPolicyRequest: Promise<number> | null = null;
  const attachmentLimitLabel = $derived(attachmentMaxBytes === null ? 'Checking server upload limit…' : attachmentMaxBytes === 0 ? 'No file-size limit' : `Up to ${attachmentMaxBytes / 1024 ** 2} MiB per file`);
  async function refreshAttachmentLimit(): Promise<number> {
    if (attachmentPolicyRequest) return attachmentPolicyRequest;
    attachmentPolicyBusy = true;
    const request = loadUploadLimits().then(policy => {
      if (!Number.isSafeInteger(policy.record_max_size_bytes) || policy.record_max_size_bytes < 0) throw new Error('MX returned an invalid record attachment limit. Retry when connected.');
      if (!destroyed) attachmentMaxBytes = policy.record_max_size_bytes;
      return policy.record_max_size_bytes;
    });
    attachmentPolicyRequest = request;
    try { return await request; }
    finally { if (attachmentPolicyRequest === request) { attachmentPolicyRequest = null; attachmentPolicyBusy = false; } }
  }
  onMount(() => {
    if (!canAttachments || !schema.fields.some(field => field.active && field.field_type === 'attachments')) return;
    const refresh = () => { void refreshAttachmentLimit().catch(reason => { if (!destroyed) attachmentProblem = reason instanceof Error ? reason.message : 'The upload limit could not be checked. Retry before attaching files.'; }); };
    refresh(); window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  });

  $effect(() => {
    const source = record?.values || {};
    untrack(cancelRelationshipRequests);
    const base = cloneJson(source);
    if(!record)for(const field of schema.fields.filter(f=>f.active&&f.field_type==='boolean'))base[field.key]=false;
    if (!record && schema.fields.some((field) => field.active && field.key === 'date' && field.field_type === 'date')) {
      const now = new Date();
      base.date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    }
    const initial = cloneJson(base);
    let restoredDraft = false;
    let restoredRevision = record?.revision || 1;
    let restoredSubmission: typeof createSubmission = null;
    recoveredRelationshipPending = new Set();
    try {
      const saved = canWrite ? JSON.parse(sessionStorage.getItem(draftStorageKey()) || 'null') as { values?: Record<string, JsonValue>; original?: Record<string, JsonValue>; base_revision?: number; create_submission?: {uid:string;values:Record<string,JsonValue>}; relationship_pending?:string[] } | null : null;
      recoveredRelationshipPending = new Set(Array.isArray(saved?.relationship_pending)?saved.relationship_pending:[]);
      if (!record && saved?.create_submission?.uid) restoredSubmission = saved.create_submission;
      if (saved?.values && typeof saved.values === 'object') {
        const editable = new Set(schema.fields.filter((field) => field.active && !record?.restricted_fields?.includes(field.key) && !(schema.viewer_access_level !== 0 && field.config.access_scope === 'administrators') && !['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type)).map((field) => field.key));
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
    values = initialChanges?prepareForm(schema.fields.filter(f=>f.active),initial,initialChanges).values:initial;
    createSubmission = restoredSubmission;
    original = cloneJson(base);
    pending = {};
    persisted = null;
    workingRevision = restoredRevision;
    conflicts = [];
    liveNotice = initialChanges?'Complete the fields for this selection, then save the record.':restoredDraft ? 'Your unsaved entries from this tab were restored.' : '';
    deletedRemotely = false;
    uploadProgress = 0;
    uploadLabel = '';
    fileProgress = {};
    focusedAttachments = false;
    relationshipErrors = Object.fromEntries(schema.fields.filter(f=>f.active&&f.field_type==='relationship'&&recoveredRelationshipPending.has(f.key)).map(f=>[f.key,'Linked fields were not finished in this draft. Retry before saving.']));
    if(restoredDraft)untrack(()=>{for(const field of schema.fields.filter(f=>f.active&&f.field_type==='relationship'))if(isDirty(field.key)&&!recoveredRelationshipPending.has(field.key))void selectRelationship(field,values[field.key]??null,true);});
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

  const fields = $derived(schema.fields.filter((field) => field.active && !current?.restricted_fields?.includes(field.key) && (schema.viewer_access_level===0||field.config.access_scope!=='administrators') && (canAttachments || field.field_type !== 'attachments')).sort((a, b) => a.position - b.position));
  const states=$derived(Object.fromEntries(fields.map(field=>[field.key,fieldState(field,values)])));
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
    if (field.field_type === 'boolean'){const next=(target as HTMLInputElement).checked;(target as HTMLInputElement).checked=values[field.key]===true;void changeValues({[field.key]:next});return;}
    else if (field.field_type === 'integer') values[field.key] = target.value === '' ? null : Number.parseInt(target.value, 10);
    else if (field.field_type === 'decimal') values[field.key] = target.value === '' ? null : Number(target.value);
    else values[field.key] = target.value || null;
    persistDraft();
  }

  async function changeValues(input:Record<string,JsonValue>,knownResets:string[]=[]){
    const baseBeforeChoice=workingRevision;
    const result=prepareForm(fields,values,input);
    const cleared=[...new Set([...result.resets,...knownResets])].filter(key=>values[key]!=null&&values[key]!==''&&values[key]!==false&&(!!current||isDirty(key)));
    const hiddenUploads=fields.filter(f=>f.field_type==='attachments'&&!fieldState(f,result.values).visible&&pending[f.uid]?.length);
    if((cleared.length||hiddenUploads.length)&&!await requestConfirmation({title:'Clear details from the previous choice?',description:`This selection no longer uses ${[...cleared.map(key=>fields.find(f=>f.key===key)?.label||key),...hiddenUploads.map(f=>`${f.label} queued uploads`)].join(', ')}. Those entries will be cleared from this edit. Stored files are kept, and previously saved values remain in version history.`,confirmLabel:'Switch and clear',tone:'danger'}))return false;
    if(destroyed)return false;
    // Updates received while the user considered the confirmation must still
    // participate in normal stale-field checks, especially automatic clears.
    if(cleared.length||hiddenUploads.length)workingRevision=Math.min(workingRevision,baseBeforeChoice);
    const next=prepareForm(fields,values,input);values=next.values;
    for(const f of hiddenUploads){for(const file of pending[f.uid]||[])delete fileProgress[fileKey(file)];pending[f.uid]=[];}
    persistDraft();return true;
  }

  async function selectRelationship(field:FieldDefinition,value:JsonValue,preserveCopies=false) {
    relationshipRequests.get(field.key)?.abort();
    if(!preserveCopies&&!await changeValues({[field.key]:value}))return;
    const affected=fields.filter(f=>['lookup','rollup'].includes(f.field_type)&&f.config.relationship_field===field.key);
    const needsPreview=affected.length>0||(Array.isArray(field.config.auto_fill)&&field.config.auto_fill.length>0);
    if(!needsPreview){persistDraft();return;}
    const before=cloneJson(values);
    for(const item of affected)values[item.key]=null;
    const controller=new AbortController();relationshipRequests.set(field.key,controller);
    relationshipLoading[field.key]=true;relationshipErrors[field.key]='';
    persistDraft();
    try {
      const preview=await relationshipPreview(field.uid,value,controller.signal);
      if(controller.signal.aborted||relationshipRequests.get(field.key)!==controller)return;
      const next=applyRelationshipPreview(values,before,preserveCopies?{...preview,values:{}}:preview);
      if(!preserveCopies){const changed=Object.fromEntries(Object.entries(next).filter(([key,value])=>JSON.stringify(value)!==JSON.stringify(values[key])));if(!await changeValues(changed))throw new Error('Linked auto-fill was cancelled. Choose the previous name or retry the linked fields.');}
      else values=next;
    } catch(reason) {
      if(!controller.signal.aborted)relationshipErrors[field.key]=reason instanceof Error?reason.message:'Could not fill the linked fields. Retry before saving.';
    } finally {
      if(relationshipRequests.get(field.key)===controller){relationshipRequests.delete(field.key);relationshipLoading[field.key]=false;persistDraft();}
    }
  }

  function draftStorageKey() {
    return `mx-record-draft:${moduleUid || 'mx-default-records'}:${draftScope || record?.uid || 'new'}`;
  }

  function clearDraft() {
    try { sessionStorage.removeItem(draftStorageKey()); } catch { /* Ignore restricted storage. */ }
  }

  function persistDraft() {
    if (!canWrite) return;
    const changed = schema.fields.some((field) => !['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type) && JSON.stringify(values[field.key] ?? null) !== JSON.stringify(original[field.key] ?? null));
    try {
      if (changed || createSubmission) sessionStorage.setItem(draftStorageKey(), JSON.stringify({ values: cloneJson(values), original: cloneJson(original), base_revision: workingRevision, create_submission: createSubmission, relationship_pending:fields.filter(f=>relationshipLoading[f.key]||relationshipErrors[f.key]).map(f=>f.key), saved_at: Date.now() }));
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
    if ((message.type.startsWith('schema.')||message.type.startsWith('module.'))&&(!payload.module_uid||payload.module_uid===moduleUid)) {
      await refreshOpenRecord('Record access and field configuration were refreshed.');return;
    }
    if (payload.record_uid !== current.uid) return;
    if (message.type === 'record.deleted') { deletedRemotely = true; onClose(); return; }
    if (message.type === 'attachment.created' || message.type === 'attachment.deleted') {
      await refreshOpenRecord('The attachment list was refreshed after another user changed it.');
      return;
    }
    if (message.type !== 'record.fields.updated') { if (message.type === 'record.updated') await refreshOpenRecord('This record was refreshed after another user updated it.'); return; }
    if (payload.requires_refresh) { await refreshOpenRecord('This open record was updated with the latest saved values.'); return; }
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
      // Permission changes must discard newly restricted local values too.
      // Preserve drafts only for fields the current response still authorizes.
      for (const key of latest.restricted_fields || []) {
        delete values[key]; delete original[key];
        const field=schema.fields.find(f=>f.key===key);if(field)delete pending[field.uid];
      }
      if(JSON.stringify(latest.restricted_fields||[])!==JSON.stringify(opened.restricted_fields||[])) {previewOpen=false;historyOpen=false;selectedVersion=null;}
      const next = cloneJson(latest);
      let dirtyCount = 0;
      for (const field of fields.filter((item) => item.field_type !== 'attachments')) {
        if (latest.restricted_fields?.includes(field.key)) continue;
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
      persistDraft();
      if (!dirtyCount) workingRevision = latest.revision;
      liveNotice = dirtyCount ? `Saved data changed elsewhere. Your ${dirtyCount} unsaved field${dirtyCount === 1 ? '' : 's'} remain local and will be checked when you save.` : message;
    } catch (reason) {
      if(reason instanceof ApiError&&[403,404].includes(reason.status)&&current?.uid===opened.uid) {
        try{sessionStorage.removeItem(draftStorageKey());}catch{/* Restricted storage. */}
        previewOpen=false;historyOpen=false;selectedVersion=null;values={};original={};pending={};onClose();return;
      }
      liveNotice = 'Live data changed, but this open record could not be refreshed yet. Saving will still check for conflicts.';
    }
  }

  function useLatest(conflict: FieldConflict) { const restored={...conflict.related_values,[conflict.field_key]:conflict.current_value};for(const[key,value]of Object.entries(restored)){values[key]=cloneJson(value);original[key]=cloneJson(value);}conflicts = conflicts.filter((item) => !Object.hasOwn(restored,item.field_key));if(Object.keys(conflict.related_values||{}).length)liveNotice='The branch change was cancelled to keep the latest stored details. Other unsaved fields remain local.'; if (!conflicts.length) error = '';persistDraft(); }
  function keepMine(conflict: FieldConflict) { conflicts = conflicts.filter((item) => item.field_key !== conflict.field_key); if (!conflicts.length) error = ''; liveNotice = 'Your value is ready to save over the latest stored revision.'; }
  async function queueFiles(field: FieldDefinition, list: FileList | File[]) {
    const files = Array.from(list); // Snapshot before the file input is reset.
    let maximumBytes: number;
    try { maximumBytes = await refreshAttachmentLimit(); }
    catch (reason) { attachmentProblem = reason instanceof Error ? reason.message : 'Could not verify the upload limit. Retry when connected.'; return; }
    if (destroyed || busy || !canWrite || deletedRemotely) return;
    attachmentProblem = '';
    const queued = [...(pending[field.uid] || [])];
    const storedCount = filesFor(field).length;
    const maximum = attachmentLimit(field);
    for (const file of files) {
      if (maximumBytes > 0 && file.size > maximumBytes) { attachmentProblem = `${file.name} exceeds the current ${maximumBytes / 1024 ** 2} MiB record attachment limit. An administrator can change it in Administration → Deployment → Upload limits.`; continue; }
      if (queued.some((item) => item.name === file.name && item.size === file.size && item.lastModified === file.lastModified)) continue;
      if (maximum > 0 && storedCount + queued.length >= maximum) { attachmentProblem = `${field.label} allows at most ${maximum} file${maximum === 1 ? '' : 's'}.`; break; }
      queued.push(file);
      fileProgress[fileKey(file)] = { percent: 0, label: 'Ready — uploads when you save' };
    }
    pending[field.uid] = queued;
  }
  async function attachDriveFile(item: DriveItem | null) {
    const field = driveAttachmentField;
    if (!item || !field || !canWrite || busy) return;
    driveAttachmentField = null;
    driveCopying = true;
    try {
      const maximum = await refreshAttachmentLimit();
      if (maximum > 0 && item.size > maximum) { attachmentProblem = `This Drive file exceeds the current ${maximum / 1024 ** 2} MiB record attachment limit. An administrator can change it in Administration → Deployment → Upload limits.`; return; }
      const file = await driveFile(item); if (canWrite && !busy && !deletedRemotely) await queueFiles(field, [file]);
    }
    catch (reason) { attachmentProblem = reason instanceof Error ? reason.message : 'Could not copy this Drive file.'; }
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
    cancelRelationshipRequests();
    values = cloneJson(original); pending = {}; fileProgress = {}; uploadProgress = 0; uploadLabel = ''; conflicts = []; error = ''; liveNotice = ''; clearDraft();
  }
  async function copyRecordLink() {
    if (!current) return;
    const module = moduleUid || 'mx-default-records';
    const link = `${location.origin}${location.pathname}#module/${encodeURIComponent(module)}?record=${encodeURIComponent(current.uid)}`;
    try { await navigator.clipboard.writeText(link); liveNotice = 'Record link copied.'; }
    catch { error = 'The browser could not copy the record link. Copy it from the address bar instead.'; }
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!canWrite || deletedRemotely || busy || attachmentPolicyBusy) return;
    if(relationshipPending||relationshipFailed){error='Wait for linked fields to load, or retry the failed selection before saving.';return;}
    if (driveCopying) { error = 'Wait for the Drive file to finish copying before saving.'; return; }
    if (conflicts.length) { error = 'Resolve the live field conflicts before saving.'; return; }
    const inputs=Object.fromEntries(fields.filter(f=>ruleWritable(f)&&isDirty(f.key)).map(f=>[f.key,values[f.key]??null]));
    for(const f of fields.filter(f=>f.field_type==='select'&&isDirty(f.key)))for(const detail of optionDetails(f))for(const key of Object.keys(detail.fill||{}))inputs[key]=values[key]??null;
    const prepared=prepareForm(fields,original,inputs);
    if(!await changeValues(Object.fromEntries(Object.entries(prepared.values).filter(([key,value])=>JSON.stringify(value)!==JSON.stringify(values[key]))),prepared.resets))return;
    const problems=prepareForm(fields,values,{}).errors;if(problems.length){error=problems.join(' ');return;}
    for (const field of fields.filter((item) => item.field_type === 'attachments'&&states[item.key]?.visible)) {
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
          if (['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type)) continue;
          if (JSON.stringify(values[field.key] ?? null) !== JSON.stringify(original[field.key] ?? null)) {
            changes[field.key] = values[field.key] ?? null;
          }
        }
        // Keep intentional overrides, including ones equal to the loaded value.
        // Otherwise the API would correctly fill omitted destinations again.
        for(const relation of fields.filter(f=>f.field_type==='relationship'&&isDirty(f.key))) {
          for(const mapping of (Array.isArray(relation.config.auto_fill)?relation.config.auto_fill:[]) as {target_field:string}[]) changes[mapping.target_field]=values[mapping.target_field]??null;
        }
        for(const choice of fields.filter(f=>f.field_type==='select'&&isDirty(f.key)))for(const detail of optionDetails(choice))for(const key of Object.keys(detail.fill||{}))changes[key]=values[key]??null;
        const hasPendingAttachments = fields.some((field) => field.field_type === 'attachments' && (pending[field.uid]?.length || 0) > 0);
        if (!Object.keys(changes).length && !hasPendingAttachments) { liveNotice = 'There are no changes to save.'; return; }
        saved = Object.keys(changes).length ? await patchRecord(current.uid, changes, workingRevision, moduleUid || undefined) : cloneJson(current);
      } else {
        const payload: Record<string, JsonValue> = {};
        for (const field of fields) if (!['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type)) payload[field.key] = states[field.key]?.visible?values[field.key]??null:null;
        if (!createSubmission) createSubmission = { uid: operationUid(), values: cloneJson(payload) };
        persistDraft();
        saved = submitCreate ? await submitCreate(createSubmission.values) : await createRecord(createSubmission.values, moduleUid || undefined, createSubmission.uid);
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
            attachmentProblem = reason instanceof Error ? reason.message : 'This attachment could not be uploaded.';
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

  function showPreview(uid: string) {
    const file = current?.attached_files.find(file => file.uid === uid);
    officePreviewNotice = '';
    if (current && file && officeExtension(file.file_name)) {
      if (openRecordOffice(current.uid, uid, moduleUid)) return;
      officePreviewNotice = 'The browser blocked the viewer tab. Allow popups for MX, or download the original.';
    }
    previewKey = `stored:${uid}`; previewOpen = true;
  }
  function showPending(file: File) { previewKey = `pending:${fileKey(file)}`; previewOpen = true; }
  function closePreview() { previewOpen = false; previewKey = ''; officePreviewNotice = ''; }
  function navigatePreview(index: number) {
    officePreviewNotice = '';
    const entry = previewEntries[index];
    if (entry) previewKey = entry.key;
  }
  async function loadPreview(entry: PreviewEntry) {
    if (entry.pendingFile) return { blob: entry.pendingFile, fileName: entry.name, mimeType: entry.pendingFile.type || 'application/octet-stream' };
    if (!current || !entry.storedUid) throw new Error('This attachment is no longer available.');
    const result = await previewAttachment(current.uid, entry.storedUid, entry.name, entry.mimeType);
    return { blob: result.blob, url: result.url, fileName: result.fileName || entry.name, mimeType: result.mimeType };
  }
  function launchOfficePreview(entry: PreviewEntry) {
    if (!current || !entry.storedUid) return;
    if (openRecordOffice(current.uid, entry.storedUid, moduleUid)) closePreview();
    else officePreviewNotice = 'The browser blocked the viewer tab. Allow popups for MX, or download the original.';
  }
  async function downloadPreviewOriginal(entry: PreviewEntry) {
    if (entry.pendingFile) {
      const url = URL.createObjectURL(entry.pendingFile), link = document.createElement('a');
      link.href = url; link.download = entry.name; link.click(); setTimeout(() => URL.revokeObjectURL(url), 30000);
    } else if (current && entry.storedUid) await download(`/mx/v1/records/${encodeURIComponent(current.uid)}/attachments/${encodeURIComponent(entry.storedUid)}/download`, entry.name);
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

<div class="overlay record-editor-overlay" class:subdialog-open={historyOpen || previewOpen || sendOpen} role="presentation" aria-hidden={historyOpen || previewOpen || sendOpen ? 'true' : undefined} inert={historyOpen || previewOpen || sendOpen} onclick={(event) => event.target === event.currentTarget && attemptClose()}>
  <div bind:this={dialogElement} class="dialog record-dialog" role="dialog" aria-modal="true" aria-labelledby="record-title">
    <header class="dialog-head">
      <div><p class="eyebrow">{current ? (canWrite ? 'Edit existing' : 'View existing') : 'Create new'}</p><h2 id="record-title">{current ? `${canWrite ? 'Edit' : 'View'} ${recordLabel}` : `New ${recordLabel}`}</h2>{#if current}<p class="muted record-dialog-context">{canWrite ? 'Editing' : 'Viewing'} {recordIdentity(current)}</p>{/if}</div>
      <div class="inline-actions">{#if current && allowEdit && !canWrite}<button class="button primary small icon-label" type="button" onclick={onEdit}><Pencil size={14} />Edit {recordLabel.toLowerCase()}</button>{/if}{#if current}<button class="button small icon-label" type="button" disabled={busy || deletedRemotely} onclick={() => sendOpen = true}><Send size={15}/>Send record</button><button class="button small" type="button" onclick={copyRecordLink}>Copy link</button><button bind:this={historyButtonElement} class="button small icon-label" type="button" onclick={openHistory}><HistoryIcon size={15} />History</button>{/if}{#if fields.some((field) => field.field_type === 'attachments') || unassignedFiles.length}<button class="button small attachment-shortcut" type="button" onclick={jumpToAttachments}>Attachments{current ? ` · ${attachmentCount}` : ''}</button>{/if}<button class="icon-button" type="button" onclick={attemptClose} aria-label="Close"><X size={18} /></button></div>
    </header>
    {#if error}<div class="notice error">{error}</div>{/if}
    {#if liveNotice}<div class="notice live-notice">{liveNotice}</div>{/if}
    {#if conflicts.length}<section class="conflict-panel"><strong>Resolve concurrent changes</strong><p>Another user saved these fields after you opened the record.</p>{#each conflicts as conflict}<div class="conflict-row"><div><strong>{conflict.label}</strong><small>Latest: {String(conflict.current_value ?? 'Empty')} · Yours: {String(conflict.your_value ?? 'Empty')}</small></div><div class="inline-actions"><button class="button small" type="button" onclick={() => useLatest(conflict)}>Use latest</button><button class="button primary small" type="button" onclick={() => keepMine(conflict)}>Keep mine</button></div></div>{/each}</section>{/if}
    <form class="record-form" novalidate onsubmit={submit}>
      {#if !fields.length}<div class="empty-state compact full"><strong>This deployment has no record fields yet.</strong><p>Open Administration → Record structure and add the first field.</p></div>{/if}
      {#each fields as definition}
        {@const availability=states[definition.key]}
        {@const field={...definition,required:availability.required}}
        {#if availability.visible}
        <div class="personal-record-field" class:personal-field-bold={fieldIsBold($readingPreferences,moduleUid,field.uid)} data-field-uid={field.uid}>
        {#if field.field_type === 'attachments'}
          <fieldset class="field-block full attachment-drop attachment-section"><legend>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</legend>
            {#if canWrite && attachmentProblem}<div class="notice error" role="alert">{attachmentProblem}<button class="button small" type="button" onclick={() => { attachmentProblem = ''; void refreshAttachmentLimit().catch(reason => attachmentProblem = reason instanceof Error ? reason.message : 'Could not refresh the upload limit.'); }}>Refresh upload limit</button></div>{/if}
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
            {#if canWrite}<label class:drag-active={dragAttachmentField === field.uid} class="file-picker" ondragenter={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragover={(event) => { event.preventDefault(); dragAttachmentField = field.uid; }} ondragleave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) dragAttachmentField = ''; }} ondrop={(event) => { event.preventDefault(); dragAttachmentField = ''; if (event.dataTransfer?.files) queueFiles(field, event.dataTransfer.files); }}><span class="file-picker-icon" aria-hidden="true"><Upload size={19} /></span><span class="file-picker-copy"><strong>Drop {field.config.multiple === false ? 'a file' : 'files'} here or browse</strong><small>{attachmentLimitLabel}{attachmentLimit(field) > 0 ? ` · ${attachmentLimit(field)} maximum` : ''}</small></span><input type="file" disabled={busy || attachmentPolicyBusy} multiple={field.config.multiple !== false} onchange={(event) => { if (event.currentTarget.files) queueFiles(field, event.currentTarget.files); event.currentTarget.value = ''; }} /></label>{/if}
          </fieldset>
        {:else if field.field_type === 'auto_number'}
          <label>{field.label}<input value={autoNumberValue(field)} disabled placeholder="Assigned automatically when saved" /></label>
        {:else if field.field_type === 'formula'}
          <label><span>{field.label}<small class="field-kind-hint">Calculated</small></span><input value={String(fieldValue(field))} disabled placeholder="Calculated when saved" /></label>
        {:else if field.field_type === 'relationship'}
          <div class="record-relationship full"><RelationshipInput {field} value={values[field.key] ?? null} labels={current?.relationships?.[field.key]} disabled={!canWrite||busy||!availability.enabled} onChange={value=>void selectRelationship(field,value)} />
            {#if Array.isArray(field.config.auto_fill)&&field.config.auto_fill.length}<small class="muted">Automatically fills: {field.config.auto_fill.map(m=>fields.find(f=>f.key===(m as {target_field:string}).target_field)?.label).filter(Boolean).join(', ')}. Copied values remain editable.</small>{/if}
            {#if relationshipLoading[field.key]}<small role="status">Loading linked fields…</small>{/if}
            {#if relationshipErrors[field.key]}<div role="alert"><p>{relationshipErrors[field.key]}</p><button class="button small" type="button" onclick={()=>void selectRelationship(field,values[field.key]??null)}>Retry linked fields</button></div>{/if}
          </div>
        {:else if field.field_type === 'lookup' || field.field_type === 'rollup'}
          <label><span>{field.label}</span><input disabled value={Array.isArray(values[field.key]) ? (values[field.key] as JsonValue[]).join(', ') : String(values[field.key] ?? '—')} /><small class="muted">Calculated from linked records</small></label>
        {:else if field.field_type === 'long_text'}
          <label class="full"><span>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</span><textarea rows="4" required={field.required} disabled={!canWrite||!availability.enabled} value={String(fieldValue(field))} oninput={(event) => setValue(field, event.currentTarget)}></textarea></label>
        {:else if field.field_type === 'select'}
          <SelectFieldInput {field} value={values[field.key]??null} {values} disabled={!canWrite||!availability.enabled} required={field.required} onChange={value=>void changeValues({[field.key]:value})}/>
        {:else if field.field_type === 'boolean'}
          <label class="checkbox field-checkbox"><input type="checkbox" aria-label={field.label} checked={Boolean(fieldValue(field))} disabled={!canWrite||!availability.enabled} onchange={(event) => setValue(field, event.currentTarget)} /><span>{field.label}{#if fieldRules(field).exclusive_group}<small class="muted">Choose one in {fieldRules(field).exclusive_group}{fieldRules(field).exclusive_required?' (required)':''}</small>{/if}</span></label>
        {:else}
          <label><span>{field.label}{#if field.required}<b class="required-mark"> *</b>{/if}</span><input type={field.field_type === 'date' ? 'date' : field.field_type === 'integer' || field.field_type === 'decimal' ? 'number' : 'text'} step={field.field_type === 'decimal' ? 'any' : field.field_type === 'integer' ? 1 : undefined} required={field.required} disabled={!canWrite||!availability.enabled} value={String(fieldValue(field))} oninput={(event) => setValue(field, event.currentTarget)} /></label>
        {/if}
        </div>
        {/if}
      {/each}
      {#if current}<RelatedRecords module={moduleUid || 'mx-default-records'} record={current.uid} />{/if}
      {#if unassignedFiles.length}<fieldset class="field-block full attachment-section"><legend>Unassigned legacy attachments</legend><p class="muted small">These files predate File Attachment fields. They remain available for preview and download.</p>{#each unassignedFiles as file}<div class="file-row record-file-row"><RecordAttachmentThumbnail recordUid={current!.uid} {file} onOpen={() => showPreview(file.uid)} /><span class="record-file-copy"><strong>{file.file_name}</strong><small>{file.mime_type || 'application/octet-stream'} · {bytes(file.size)}</small></span><div class="inline-actions"><button class="file-action-icon" type="button" onclick={() => showPreview(file.uid)} aria-label={`Preview ${file.file_name}`} title="Preview"><Eye size={15} /></button><button class="file-action-icon" type="button" onclick={() => download(`/mx/v1/records/${current?.uid}/attachments/${file.uid}/download`, file.file_name)} aria-label={`Download ${file.file_name}`} title="Download"><DownloadIcon size={15} /></button>{#if canDelete}<button class="file-action-icon danger-text" type="button" onclick={() => removeFile(file.uid)} aria-label={`Delete ${file.file_name}`} title="Delete"><Trash2 size={15} /></button>{/if}</div></div>{/each}</fieldset>{/if}
      {#if uploadLabel}<section class="upload-progress-panel full" aria-live="polite"><div><strong>{uploadLabel}</strong><span>{uploadProgress}%</span></div><div class="upload-progress-track"><i style={`width:${uploadProgress}%`}></i></div><small>Keep this window open until every queued file finishes.</small></section>{/if}
      <footer class="dialog-actions">
        {#if current && canDelete}<button class="button danger" type="button" onclick={removeRecord} disabled={busy || driveCopying}>Delete {recordLabel.toLowerCase()}</button>{/if}
        <span class="spacer"></span><button class="button" type="button" onclick={resetForm} disabled={busy || driveCopying}>{current ? 'Reset changes' : 'Clear form'}</button><button class="button" type="button" onclick={attemptClose} disabled={busy || driveCopying}>Cancel</button>
        {#if canWrite}<button class="button primary" type="submit" disabled={busy || driveCopying || deletedRemotely || relationshipPending || relationshipFailed}>{relationshipPending ? 'Loading linked fields…' : driveCopying ? 'Copying Drive file…' : busy ? 'Saving and uploading…' : current ? `Update ${recordLabel}` : `Save ${recordLabel}`}</button>{/if}
      </footer>
    </form>
  </div>
</div>

{#if driveAttachmentField}<DrivePicker onChoose={(item) => void attachDriveFile(item)} onClose={() => driveAttachmentField = null} />{/if}
{#if sendOpen && current}<SendRecordDialog recordUid={current.uid} recordLabel={recordIdentity(current)} hasUnsavedChanges={fields.some(field=>isDirty(field.key)) || Object.values(pending).some(files=>files.length>0)} onClose={() => sendOpen = false}/>{/if}
{#if previewOpen && activePreview}
  {#key activePreview.key}<FilePreview fileName={activePreview.name} load={() => loadPreview(activePreview)} onDownloadOriginal={() => downloadPreviewOriginal(activePreview)} officeActions={/\.(docx?|docm|xlsx?|xlsm|pptx?|pptm|odt|ods|odp|rtf)$/i.test(activePreview.name) ? {open:activePreview.storedUid && officeExtension(activePreview.name)?()=>launchOfficePreview(activePreview):undefined,download:()=>void downloadPreviewOriginal(activePreview).catch(reason=>officePreviewNotice=reason instanceof Error?reason.message:String(reason)),description:officePreviewNotice || (activePreview.pendingFile?'This document is not uploaded yet. Save the record, then open its attachment in MX Office. You can download the queued file now.':officeExtension(activePreview.name)?'View the original attachment read-only in a new tab. No changes are made to the record.':'This Office format cannot be opened safely in the browser. Download the original to open locally.')} : undefined} navigationItems={previewEntries} activeIndex={previewIndex} onNavigate={navigatePreview} onClose={closePreview} />{/key}
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
            <dl>{#each fields.filter((field) => field.field_type !== 'attachments') as field}<div class:personal-field-bold={fieldIsBold($readingPreferences,moduleUid,field.uid)}><dt>{field.label}</dt><dd>{String(selectedVersion.values[field.key] ?? '—')}</dd></div>{/each}</dl>
          {:else if historyLoading}<div class="history-detail-placeholder"><div class="loader"></div><strong>Loading the newest saved state…</strong></div>
          {:else}<div class="empty-state compact"><HistoryIcon size={24} /><strong>Select a version</strong><p>Review its saved field values before restoring it.</p></div>{/if}
        </section>
      </div>
    </div>
  </div>
{/if}

<style>
  .personal-record-field{display:contents;}
  .personal-field-bold{font-weight:750;}
  .personal-field-bold :global(input),.personal-field-bold :global(textarea),.personal-field-bold :global(select),.personal-field-bold :global(label>span),.personal-field-bold :global(legend),.personal-field-bold :global(.relationship-input button){font-weight:750;}
</style>
