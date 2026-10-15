<script lang="ts">
  import { onMount, onDestroy, tick, untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Eye from '@lucide/svelte/icons/eye';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Search from '@lucide/svelte/icons/search';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';
  import History from '@lucide/svelte/icons/history';
  import { ApiError } from '../api/client';
  import type { LiveMessage } from '../live/client';
  import { requestConfirmation } from '../confirmation';
  import type { ColumnSummary, FieldConflict, JsonValue, ModulePermission, MxRecord, SchemaResponse } from '../api/domain';
  import {relationshipPreview} from '../api/relationships';
  import { summaryNames, summaryValue } from '../util/columnSummaries';
  import type { Session } from '../api/types';
  import { deleteRecord, getRecord, listRecords, loadModuleSchema, loadSchema, patchRecord } from '../api/workspace';
  import RecordEditor from './RecordEditor.svelte';
  import ImportRecords from './ImportRecords.svelte';
  import RelationshipInput from './RelationshipInput.svelte';
  import {fieldState,optionLabel,availableOptions,prepareForm} from '../util/formRules';
  import TableScrollbar from './TableScrollbar.svelte';
  import { floatingMenu } from '../util/floating';

  let { session, accessLevel, moduleUid = '', modulePermission = null, openRecordUid = '', openRequestRevision = 0, focusLinkedAttachments = false, revision = 0, liveMessage = null, recordSingular = 'Record', recordPlural = 'Records' }: { session: Session; accessLevel: number; moduleUid?: string; modulePermission?: ModulePermission | null; openRecordUid?: string; openRequestRevision?: number; focusLinkedAttachments?: boolean; revision?: number; liveMessage?: LiveMessage | null; recordSingular?: string; recordPlural?: string } = $props();
  let schema = $state<SchemaResponse | null>(null);
  let importer = $state<'new' | 'history' | null>(null);
  let importMenuOpen = $state(false);
  let importMenuAnchor = $state<HTMLButtonElement | null>(null);
  let importFocusFrame = 0;
  let rows = $state<MxRecord[]>([]);
  let summaries = $state<Record<string, ColumnSummary>>({});
  let page = $state(1); let pageInput = $state(1); let pages = $state(0); let total = $state(0); let pageSize = $state(50);
  let search = $state(''); let sortBy = $state(''); let sortDir = $state('asc');
  let matchMode = $state<'contains' | 'prefix' | 'exact'>('contains');
  let attachmentMode = $state<'with' | 'without' | ''>('');
  let filters = $state<Record<string, string>>({});
  let selectedKeys = $state<string[]>([]);
  let loading = $state(true); let error = $state('');
  let deletingUid = $state('');
  let rowMenuUid = $state('');
  let rowMenuAnchor = $state<HTMLButtonElement | null>(null);
  let tableViewport = $state<HTMLDivElement>();
  const menuRow = $derived(rows.find((row) => row.uid === rowMenuUid));
  let editor = $state<MxRecord | null | undefined>(undefined);
  let editorChanges=$state<Record<string,JsonValue>|null>(null);
  let editorWritable = $state(false);
  let cellEdit = $state<{ rowUid: string; fieldKey: string; value: string; baseRevision: number; fills:Record<string,JsonValue>; computed:Record<string,JsonValue>; previewLoading:boolean; previewError:string } | null>(null);
  let cellRequest:AbortController|undefined;
  onDestroy(()=>{ cellRequest?.abort(); cancelAnimationFrame(importFocusFrame); });
  let cellSaving = $state(false);
  let cellConflict = $state<{ conflicts: FieldConflict[]; recordRevision: number } | null>(null);
  let focusAttachments = $state(false);
  let lastRevision = $state(0);
  let columnOverrides = $state<{ shown: string[]; hidden: string[] }>({ shown: [], hidden: [] });
  let legacyColumns: string[] | null = null;
  let searchTimer: number | undefined;
  let refreshSequence = 0;
  // A response can have been read before a concurrent deletion committed.
  // Keep deletion barriers until an explicit restore, not just another fetch.
  const deletedRecordUids = new Set<string>();
  const lifecycleSequences = new Map<string, number>();
  const deletionGenerations = new Map<string, number>();
  let deletionGeneration = 0;
  let openedDeepLink = '';
  let openedRequestRevision = -1;
  const canCreate = $derived(modulePermission ? modulePermission.can_create : accessLevel <= 2);
  const canUpdate = $derived(modulePermission ? modulePermission.can_update : accessLevel <= 2);
  const canDelete = $derived(modulePermission ? modulePermission.can_delete : accessLevel <= 1);
  const canAttachments = $derived(modulePermission ? modulePermission.can_attachments : true);
  const eligibleFields = $derived(schema?.fields.filter((field) => field.active && (canAttachments || field.field_type !== 'attachments')).sort((a, b) => a.position - b.position) || []);
  const visibleFields = $derived(eligibleFields.filter((field) => selectedKeys.includes(field.key)));
  const summaryFields = $derived(visibleFields.filter(field => !!field.config.table_summary && field.config.table_summary !== 'none'));
  const searchableFields = $derived(schema?.fields.filter((field) => field.active && field.searchable && field.field_type !== 'attachments').sort((a, b) => a.position - b.position) || []);

  const columnStorageKey = $derived(`mx_record_columns_v3_${moduleUid || 'default'}`);
  onMount(() => {
    try { const saved = JSON.parse(localStorage.getItem(columnStorageKey) || localStorage.getItem('mx_record_columns_v3') || 'null'); if (saved && Array.isArray(saved.shown) && Array.isArray(saved.hidden)) columnOverrides = { shown: saved.shown.filter((key: unknown): key is string => typeof key === 'string'), hidden: saved.hidden.filter((key: unknown): key is string => typeof key === 'string') }; else { const old = JSON.parse(localStorage.getItem('mx_record_columns_v2') || 'null'); if (Array.isArray(old)) legacyColumns = old.filter((key): key is string => typeof key === 'string'); } } catch { /* Use schema defaults. */ }
    const closeRowMenu = (event: PointerEvent) => { const target = event.target instanceof Element ? event.target : null; if (!target?.closest('[data-record-actions]')) rowMenuUid = ''; if (!target?.closest('[data-module-import-actions]')) importMenuOpen = false; };
    const closeImportMenuOnFocus = (event: FocusEvent) => { if (importMenuOpen && !(event.target instanceof Element && event.target.closest('[data-module-import-actions]'))) importMenuOpen = false; };
    const closeRowMenuWithKeyboard = (event: KeyboardEvent) => { if (event.key === 'Escape' && rowMenuUid) { event.preventDefault(); closeRowMenuAndFocus(); } if (event.key === 'Escape' && importMenuOpen) { event.preventDefault(); closeImportMenuAndFocus(); } };
    document.addEventListener('pointerdown', closeRowMenu);
    document.addEventListener('keydown', closeRowMenuWithKeyboard);
    document.addEventListener('focusin', closeImportMenuOnFocus);
    void refresh(true);
    return () => { window.clearTimeout(searchTimer); document.removeEventListener('pointerdown', closeRowMenu); document.removeEventListener('keydown', closeRowMenuWithKeyboard); document.removeEventListener('focusin', closeImportMenuOnFocus); };
  });
  $effect(() => {
    const message = liveMessage; const selected = moduleUid || 'mx-default-records';
    untrack(() => {
      if (message?.type === 'sync.required') {
        // Live sequence numbers restart with the server process. A new
        // connection must not reject its lifecycle events using the old epoch.
        lifecycleSequences.clear();
        void reconcileDeletedRecords(); return;
      }
      if (!message?.payload || typeof message.payload !== 'object') return;
      if (!['record.deleted', 'record.restored'].includes(message.type)) return;
      const payload = message.payload as Record<string, unknown>;
      if (payload.module_uid && payload.module_uid !== selected) return;
      const uid = String(payload.record_uid || ''); if (!uid) return;
      const sequence = Number(message.sequence || 0);
      if (sequence && sequence <= (lifecycleSequences.get(uid) || 0)) return;
      if (sequence) lifecycleSequences.set(uid, sequence);
      if (message.type === 'record.deleted') forgetDeletedRecord(uid);
      else { deletedRecordUids.delete(uid); deletionGenerations.delete(uid); }
    });
  });
  $effect(() => { if (revision !== lastRevision && schema) { lastRevision = revision; void refresh(liveMessage?.type.startsWith('schema.') === true); } });
  $effect(() => { if (schema && openRecordUid && (openRecordUid !== openedDeepLink || openRequestRevision !== openedRequestRevision)) void openLinkedRecord(openRecordUid); });

  function closeRowMenuAndFocus() { rowMenuUid = ''; rowMenuAnchor?.focus({ preventScroll: true }); }
  function toggleRowMenu(event: MouseEvent, uid: string) {
    event.stopPropagation();
    importMenuOpen = false;
    rowMenuAnchor = event.currentTarget as HTMLButtonElement;
    rowMenuUid = rowMenuUid === uid ? '' : uid;
  }
  function menuKeydown(event: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault(); (event.currentTarget as HTMLElement).querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    } else if (event.key === 'Tab') closeRowMenuAndFocus();
  }

  function closeImportMenuAndFocus() {
    cancelAnimationFrame(importFocusFrame);
    importMenuOpen = false;
    importMenuAnchor?.focus({ preventScroll: true });
  }
  async function openImportMenu(focusLast = false) {
    rowMenuUid = '';
    importMenuOpen = true;
    await tick();
    cancelAnimationFrame(importFocusFrame);
    // Reopened portals may not admit focus until layout/paint finishes (Firefox/WebKit).
    // Retry only a bounded number of frames, and stop on close or user navigation.
    const focusItem = (remaining: number) => {
      importFocusFrame = 0;
      if (!importMenuOpen) return;
      const items = document.getElementById('module-import-menu')?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]');
      const target = items?.[focusLast ? items.length - 1 : 0];
      target?.focus();
      if (target !== document.activeElement && remaining > 0) importFocusFrame = requestAnimationFrame(() => focusItem(remaining - 1));
    };
    importFocusFrame = requestAnimationFrame(() => focusItem(8));
  }
  function toggleImportMenu(event: MouseEvent) {
    importMenuAnchor = event.currentTarget as HTMLButtonElement;
    if (importMenuOpen) closeImportMenuAndFocus();
    else void openImportMenu();
  }
  function importTriggerKeydown(event: KeyboardEvent) {
    if (event.key === 'Tab') { importMenuOpen = false; cancelAnimationFrame(importFocusFrame); return; }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    importMenuAnchor = event.currentTarget as HTMLButtonElement;
    void openImportMenu(event.key === 'ArrowUp');
  }
  function importMenuKeydown(event: KeyboardEvent) {
    // Do not let deferred opening focus overwrite the user's next navigation key.
    cancelAnimationFrame(importFocusFrame);
    if (event.key === 'Tab') { closeImportMenuAndFocus(); return; }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from((event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
    const active = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : active < 0 ? (event.key === 'ArrowUp' ? items.length - 1 : 0) : (active + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
    items[next]?.focus();
  }
  function importMenuKeyboard(node: HTMLElement) {
    // Native listener keeps keyboard ownership local when this menu is portalled.
    node.addEventListener('keydown', importMenuKeydown);
    return { destroy() { node.removeEventListener('keydown', importMenuKeydown); } };
  }
  function openImporter(mode: 'new' | 'history') { importMenuOpen = false; importer = mode; }

  async function openLinkedRecord(uid: string) {
    rowMenuUid = ''; openedDeepLink = uid; openedRequestRevision = openRequestRevision; error = '';
    try { const record = await getRecord(uid, moduleUid || undefined); if (deletedRecordUids.has(uid) || openedDeepLink !== uid) return; editor = record; editorWritable = false; focusAttachments = focusLinkedAttachments; }
    catch (reason) { error = reason instanceof Error ? reason.message : `The linked ${recordSingular.toLowerCase()} could not be opened.`; }
  }

  async function refresh(withSchema = false) {
    const request = ++refreshSequence;
    loading = true; error = ''; summaries = {};
    try {
      if (withSchema || !schema) {
        const nextSchema = moduleUid ? await loadModuleSchema(moduleUid) : await loadSchema();
        if (request !== refreshSequence) return;
        schema = nextSchema;
        if (sortBy && !nextSchema.fields.some(field => field.key === sortBy && field.active && field.sortable)
          && !nextSchema.system_fields.some(field => field.key === sortBy)) sortBy = '';
        const allowed = new Set(schema.fields.filter((field) => field.active).map((field) => field.key));
        const defaults = schema.fields.filter((field) => field.active && field.table_visible).sort((a, b) => a.position - b.position).map((field) => field.key);
        columnOverrides = { shown: columnOverrides.shown.filter((key) => allowed.has(key)), hidden: columnOverrides.hidden.filter((key) => allowed.has(key)) };
        if (legacyColumns) {
          const selected = legacyColumns.filter((key) => allowed.has(key));
          columnOverrides = { shown: selected.filter((key) => !defaults.includes(key)), hidden: defaults.filter((key) => !selected.includes(key)) };
          legacyColumns = null; localStorage.removeItem('mx_record_columns_v2'); saveColumnOverrides();
        }
        selectedKeys = [...defaults.filter((key) => !columnOverrides.hidden.includes(key)), ...columnOverrides.shown.filter((key) => !defaults.includes(key))];
      }
      const activeFilters = Object.fromEntries(Object.entries(filters).filter(([, value]) => value.trim()));
      const result = await listRecords({ page, limit: pageSize, q: search.trim(), match: matchMode, filters: activeFilters, attachments: attachmentMode, sort_by: sortBy || undefined, sort_dir: sortDir }, moduleUid || undefined);
      if (request !== refreshSequence) return;
      rows = result.data.filter((row) => !deletedRecordUids.has(row.uid)); page = result.page; pageInput = result.page; pages = result.total_pages; total = Math.max(0, result.total - (result.data.length - rows.length));
      summaries = result.summaries || {};
    } catch (reason) { if (request === refreshSequence) error = reason instanceof Error ? reason.message : 'Records could not be loaded.'; }
    finally { if (request === refreshSequence) loading = false; }
  }

  function forgetDeletedRecord(uid: string) {
    ++refreshSequence; loading=false; summaries={};
    deletedRecordUids.add(uid);
    deletionGenerations.set(uid, ++deletionGeneration);
    if (rows.some((row) => row.uid === uid)) total = Math.max(0, total - 1);
    rows = rows.filter((row) => row.uid !== uid);
    if (cellEdit?.rowUid === uid) { cellEdit = null; cellConflict = null; }
    if (rowMenuUid === uid) rowMenuUid = '';
    if (editor?.uid === uid) closeEditor();
  }

  async function reconcileDeletedRecords() {
    // A restore event may have been missed during an outage. Only an
    // authoritative GET can lift a deletion barrier during resynchronization.
    const pending = [...deletionGenerations]; let restored = false;
    for (let offset = 0; offset < pending.length; offset += 8) {
      await Promise.all(pending.slice(offset, offset + 8).map(async ([uid, generation]) => {
        try {
          await getRecord(uid, moduleUid || undefined);
          if (deletionGenerations.get(uid) !== generation) return;
          deletedRecordUids.delete(uid); deletionGenerations.delete(uid); restored = true;
        } catch { /* Still deleted, forbidden, or offline: keep the barrier. */ }
      }));
    }
    if (restored) await refresh();
  }

  function display(row: MxRecord, key: string) {
    const field = schema?.fields.find((item) => item.key === key);
    if(field&&!fieldState(field,row.values).visible)return 'Not applicable';
    if (field?.field_type === 'attachments') {
      const files = row.attached_files.filter((item) => item.attachment_field_uid === field.uid);
      if (!files.length) return '—';
      return files.length === 1 ? files[0].file_name : `${files.length} files`;
    }
    const preview=cellEdit?.rowUid===row.uid?{...cellEdit.fills,...cellEdit.computed}:{};
    const value = Object.hasOwn(preview,key)?preview[key]:row.values[key];
    if (field?.field_type === 'relationship' && row.relationships?.[key]?.restricted) return 'Access restricted';
    if (value == null || value === '') return '—';
    if (typeof value === 'boolean') return value ? 'Yes' : 'No';
    if (field?.field_type === 'relationship') return row.relationships?.[key]?.restricted ? 'Access restricted' : row.relationships?.[key]?.items.map(item=>item.label).join(', ') || '—';
    if (Array.isArray(value)) return value.map(item=>String(item ?? '—')).join(', ');
    if(field?.field_type==='select')return optionLabel(field,value);
    if (field?.field_type === 'auto_number') {
      const prefix = String(field.config.prefix || ''); const padding = Number(field.config.padding || 0);
      return `${prefix}${String(value).padStart(padding, '0')}`;
    }
    return String(value);
  }

  function canEditCell(field: SchemaResponse['fields'][number],row?:MxRecord) {
    return canUpdate && !['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type)&&(!row||fieldState(field,row.values).enabled);
  }
  function beginCellEdit(event: MouseEvent, row: MxRecord, field: SchemaResponse['fields'][number]) {
    event.preventDefault(); event.stopPropagation();
    if (!canEditCell(field,row) || cellSaving) return;
    error = '';
    const raw = row.values[field.key];
    cellConflict = null;
    cellRequest?.abort();
    cellEdit = { rowUid: row.uid, fieldKey: field.key, value: field.field_type === 'boolean' ? String(raw === true) : Array.isArray(raw) ? JSON.stringify(raw) : raw == null ? '' : String(raw), baseRevision: row.revision, fills:{},computed:{},previewLoading:false,previewError:'' };
    void tick().then(() => {
      const control = document.querySelector<HTMLInputElement | HTMLSelectElement>('[data-active-cell-editor]');
      control?.focus();
      if (control instanceof HTMLInputElement && control.type === 'text') control.select();
    });
  }
  function cancelCellEdit(event?: Event) { event?.stopPropagation(); cellRequest?.abort();cellEdit = null;cellConflict=null; }
  async function resolveCellConflict(conflict:FieldConflict,keepMine:boolean) {
    if (!cellConflict || !cellEdit || !schema) return;
    const { recordRevision } = cellConflict;
    if(!keepMine&&Object.keys(conflict.related_values||{}).length){cancelCellEdit();error='The latest values were kept; the conflicting choice change was cancelled.';await refresh();return;}
    if(!keepMine){if(conflict.field_key===cellEdit.fieldKey)cellEdit.value=Array.isArray(conflict.current_value)?JSON.stringify(conflict.current_value):String(conflict.current_value??'');else cellEdit.fills[conflict.field_key]=conflict.current_value;}
    cellConflict.conflicts=cellConflict.conflicts.filter(item=>item.field_key!==conflict.field_key);
    if(cellConflict.conflicts.length)return;
    const row = rows.find((item) => item.uid === cellEdit!.rowUid);
    const field = schema.fields.find((item) => item.key === cellEdit!.fieldKey);
    if (!row || !field) return;
    cellEdit.baseRevision = recordRevision;
    cellConflict = null; error = '';
    await commitCellEdit(row, field);
  }
  async function selectCellRelationship(field:SchemaResponse['fields'][number],value:JsonValue){
    if(!cellEdit)return;
    cellRequest?.abort();const edit=cellEdit;
    edit.value=Array.isArray(value)?JSON.stringify(value):String(value??'');edit.fills={};edit.computed={};edit.previewError='';
    if(!schema?.fields.some(f=>['lookup','rollup'].includes(f.field_type)&&f.config.relationship_field===field.key)&&!(Array.isArray(field.config.auto_fill)&&field.config.auto_fill.length))return;
    const controller=new AbortController();cellRequest=controller;edit.previewLoading=true;
    try{const preview=await relationshipPreview(field.uid,value,controller.signal);if(controller.signal.aborted||cellEdit!==edit)return;edit.fills=preview.values;edit.computed=preview.computed;}
    catch(reason){if(!controller.signal.aborted&&cellEdit===edit)edit.previewError=reason instanceof Error?reason.message:'Could not load linked fields.';}
    finally{if(cellRequest===controller&&cellEdit===edit)edit.previewLoading=false;}
  }
  function cellEditValue(field: SchemaResponse['fields'][number], value: string) {
    if (value === '') return null;
    if (field.field_type === 'relationship' && field.config.multiple === true) { try { return JSON.parse(value); } catch { return []; } }
    if (field.field_type === 'boolean') return value === 'true';
    if (field.field_type === 'integer') return Number.parseInt(value, 10);
    if (field.field_type === 'decimal') return Number(value);
    return value;
  }
  async function commitCellEdit(row: MxRecord, field: SchemaResponse['fields'][number]) {
    if (!cellEdit || cellSaving || cellEdit.rowUid !== row.uid || cellEdit.fieldKey !== field.key) return;
    const edit = cellEdit;
    if(edit.previewLoading||edit.previewError||cellConflict)return;
    const nextValue = cellEditValue(field, edit.value);
    if (!Object.keys(edit.fills).length&&JSON.stringify(nextValue) === JSON.stringify(row.values[field.key] ?? null)) { cellEdit = null; return; }
    cellSaving = true; error = '';
    try {
      const input={...edit.fills,[field.key]:nextValue};
      const prepared=prepareForm(eligibleFields,row.values,input);
      if(prepared.resets.length&&!await requestConfirmation({title:'Clear details from the previous choice?',description:`The following fields no longer apply: ${prepared.resets.map(key=>eligibleFields.find(f=>f.key===key)?.label||key).join(', ')}. Their values will be cleared in the next record version. Stored files are retained.`,confirmLabel:'Change choice',tone:'danger'}))return;
      if(cellEdit!==edit)return;
      if(prepared.errors.length){openRecord(row,false,true);editorChanges=input;return;}
      const saved = await patchRecord(row.uid, { ...edit.fills,[field.key]: nextValue }, edit.baseRevision, moduleUid || undefined);
      if (!deletedRecordUids.has(saved.uid)) rows = rows.map((item) => item.uid === saved.uid ? saved : item);
      cellEdit = null;
      await refresh();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : `The ${field.label.toLowerCase()} cell could not be saved.`;
      if (reason instanceof ApiError && reason.status === 409 && reason.payload && typeof reason.payload === 'object') {
        const payload = reason.payload as { conflicts?: FieldConflict[]; current_record_revision?: number };
        if (payload.conflicts?.length) cellConflict = { conflicts: payload.conflicts, recordRevision: Number(payload.current_record_revision || row.revision) };
      }
    } finally { cellSaving = false; }
  }
  function cellEditorKeydown(event: KeyboardEvent, row: MxRecord, field: SchemaResponse['fields'][number]) {
    event.stopPropagation();
    if (event.key === 'Escape') { event.preventDefault(); cancelCellEdit(); }
    else if (event.key === 'Enter' && (field.field_type !== 'long_text' || !event.shiftKey)) { event.preventDefault(); void commitCellEdit(row, field); }
  }

  async function searchSubmit(event: SubmitEvent) { event.preventDefault(); page = 1; await refresh(); }
  function scheduleSearch() { window.clearTimeout(searchTimer); searchTimer = window.setTimeout(() => { page = 1; void refresh(); }, 350); }
  async function sort(key: string) { if (sortBy === key) sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else { sortBy = key; sortDir = 'asc'; } page = 1; await refresh(); }
  async function movePage(next: number) { page = next; await refresh(); }
  async function goToPage(event: SubmitEvent) { event.preventDefault(); page = Math.min(Math.max(1, Number(pageInput) || 1), Math.max(1, pages)); await refresh(); }
  function saveColumnOverrides() { localStorage.setItem(columnStorageKey, JSON.stringify(columnOverrides)); }
  function toggleColumn(key: string, checked: boolean) {
    const defaultVisible = schema?.fields.find((field) => field.key === key)?.table_visible === true;
    selectedKeys = checked ? [...selectedKeys.filter((item) => item !== key), key] : selectedKeys.filter((item) => item !== key);
    if (defaultVisible) columnOverrides = { shown: columnOverrides.shown.filter((item) => item !== key), hidden: checked ? columnOverrides.hidden.filter((item) => item !== key) : [...columnOverrides.hidden.filter((item) => item !== key), key] };
    else columnOverrides = { hidden: columnOverrides.hidden.filter((item) => item !== key), shown: checked ? [...columnOverrides.shown.filter((item) => item !== key), key] : columnOverrides.shown.filter((item) => item !== key) };
    saveColumnOverrides();
  }
  function resetColumns() { columnOverrides = { shown: [], hidden: [] }; selectedKeys = eligibleFields.filter((field) => field.table_visible).map((field) => field.key); saveColumnOverrides(); }
  async function changePageSize() { page = 1; await refresh(); }
  async function clearFilters() { filters = {}; search = ''; attachmentMode = ''; sortBy = ''; sortDir = 'asc'; page = 1; await refresh(); }
  function openRecord(row: MxRecord, attachments = false, edit = false) { if (deletedRecordUids.has(row.uid)) return; editorChanges=null;rowMenuUid = ''; if (cellEdit) cancelCellEdit(); focusAttachments = attachments; editorWritable = edit && canUpdate; editor = row; }
  function closeEditor() { editorChanges=null;focusAttachments = false; editorWritable = false; editor = undefined; if (openRecordUid) location.hash = `module/${encodeURIComponent(moduleUid || 'mx-default-records')}`; }
  async function removeRecord(row: MxRecord, event: MouseEvent) {
    event.stopPropagation();
    closeRowMenuAndFocus();
    if (!await requestConfirmation({
      title: `Move this ${recordSingular.toLowerCase()} to trash?`,
      description: 'Its fields, attachments, and version history will be preserved so an administrator can inspect or restore it.',
      confirmLabel: 'Move to trash'
    })) return;
    deletingUid = row.uid; error = '';
    try { await deleteRecord(row.uid, moduleUid || undefined); if (rows.length === 1 && page > 1) page -= 1; forgetDeletedRecord(row.uid); await refresh(); }
    catch (reason) { error = reason instanceof Error ? reason.message : `The ${recordSingular.toLowerCase()} could not be deleted.`; }
    finally { deletingUid = ''; }
  }
</script>

<section class="workspace-page records-page">
  <div class="toolbar records-toolbar">
    {#if canCreate}<button class="button primary records-new-button" onclick={() => { editorChanges=null;focusAttachments = false; editorWritable = true; editor = null; }}>+ New {recordSingular.toLowerCase()}</button>{/if}
    <form class="search-form" onsubmit={searchSubmit}><label class="records-search-input"><Search size={16} aria-hidden="true" /><input bind:value={search} oninput={scheduleSearch} type="search" placeholder={canAttachments ? 'Search configured fields and attachment filenames…' : 'Search configured fields…'} aria-label={`Search ${recordPlural.toLowerCase()}`} /></label><select bind:value={matchMode} onchange={() => { page = 1; void refresh(); }} aria-label="Search matching"><option value="contains">Contains</option><option value="prefix">Starts with</option><option value="exact">Exact</option></select><button class="button" type="submit">Search</button></form>
    <details class="toolbar-menu"><summary class="button">Columns</summary><div class="toolbar-popover">{#each eligibleFields as field}<label class="checkbox"><input type="checkbox" checked={selectedKeys.includes(field.key)} onchange={(event) => toggleColumn(field.key, event.currentTarget.checked)} /> {field.label}</label>{/each}<button class="button small" type="button" onclick={resetColumns}>Use administrator defaults</button><small>Only your overrides are saved, so new fields can follow the deployment defaults.</small></div></details>
    <button class="button records-refresh-button" onclick={() => refresh(true)}>Refresh</button>
    {#if canCreate && schema}<button class="button module-import-trigger" type="button" data-module-import-actions aria-label="Module actions" title="Module actions" aria-haspopup="menu" aria-expanded={importMenuOpen} aria-controls={importMenuOpen ? 'module-import-menu' : undefined} onclick={toggleImportMenu} onkeydown={importTriggerKeydown}><Ellipsis size={18} aria-hidden="true" /></button>{/if}
  </div>
    <details class="advanced-search"><summary>Advanced filters and sorting</summary><form class="filter-grid" onsubmit={searchSubmit}>{#each searchableFields as field}<label>{field.label}{#if field.field_type === 'select'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Any value</option>{#each (field.config.options as (string | number | boolean)[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>{:else if field.field_type === 'boolean'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Either</option><option value="true">Yes</option><option value="false">No</option></select>{:else}<input type={field.field_type === 'date' ? 'date' : ['integer', 'decimal', 'formula'].includes(field.field_type) ? 'number' : 'text'} value={filters[field.key] || ''} oninput={(event) => filters[field.key] = event.currentTarget.value} placeholder={field.field_type === 'date' ? undefined : 'Field contains…'} />{/if}</label>{/each}{#if canAttachments}<label>Attachments<select bind:value={attachmentMode}><option value="">With or without files</option><option value="with">With attachments</option><option value="without">Without attachments</option></select></label>{/if}<label>Sort field<select bind:value={sortBy}><option value="">Default order</option>{#each eligibleFields.filter((field) => field.sortable) as field}<option value={field.key}>{field.label}</option>{/each}</select></label><label>Sort direction<select bind:value={sortDir}><option value="asc">Ascending</option><option value="desc">Descending</option></select></label><div class="button-row"><button class="button primary">Apply filters</button><button class="button" type="button" onclick={clearFilters}>Clear all</button></div></form></details>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if cellConflict}{#each cellConflict.conflicts as conflict}<div class="notice record-cell-conflict"><div><strong>{conflict.label} changed elsewhere</strong><small>Stored: {String(conflict.current_value ?? 'Empty')} · Yours: {String(conflict.your_value ?? 'Empty')}</small></div><div class="inline-actions"><button class="button small" type="button" onclick={()=>void resolveCellConflict(conflict,false)}>Use stored</button><button class="button primary small" type="button" onclick={() => void resolveCellConflict(conflict,true)}>Keep mine</button></div></div>{/each}{/if}
  <div class="records-result-summary" aria-live="polite" aria-label={`${total.toLocaleString()} ${total === 1 ? recordSingular.toLowerCase() : recordPlural.toLowerCase()}`}><span><strong>{total.toLocaleString()}</strong> {total === 1 ? 'result' : 'results'}</span><small>{rows.length ? `Showing ${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()}` : 'No rows in this view'}</small></div>
  <div class="table-wrap records-table" id={`records-table-${moduleUid || 'default'}`} bind:this={tableViewport}>
    <table>
      <thead><tr><th class="row-index-column">#</th>{#each visibleFields as field}<th><button class="table-sort" class:sortable={field.sortable} onclick={() => field.sortable && sort(field.key)}>{field.label}{#if sortBy === field.key}{#if sortDir === 'asc'}<ChevronUp size={14} />{:else}<ChevronDown size={14} />{/if}{/if}</button></th>{/each}<th class="record-actions-column">Actions</th></tr></thead>
      <tbody>
        {#if loading && !rows.length}
          <tr class="records-state-row"><td class="table-message" colspan={visibleFields.length + 2}>Loading {recordPlural.toLowerCase()}…</td></tr>
        {:else if !rows.length}
          <tr class="records-state-row"><td class="table-message" colspan={visibleFields.length + 2}>No {recordPlural.toLowerCase()} match this view.</td></tr>
        {:else}
          {#each rows as row, rowIndex (row.uid)}
            <tr onclick={(event) => { if (event.target instanceof Element && event.target.closest('.relationship-input')) return; openRecord(row); }} class="clickable-row">
              <td class="row-index-column" data-label="Row"><span title={`${recordSingular} ${row.uid}`}>{(page - 1) * pageSize + rowIndex + 1}</span></td>
              {#each visibleFields as field}
                {@const editingCell = cellEdit?.rowUid === row.uid && cellEdit.fieldKey === field.key}
                <td class:inline-editable-cell={canEditCell(field,row)} class:editing-cell={editingCell} class={field.field_type === 'long_text' ? 'long-text-cell' : ''} data-label={field.label} data-field-type={field.field_type} title={editingCell || canEditCell(field,row) || field.field_type === 'long_text' ? undefined : display(row, field.key)}>
                  {#if !fieldState(field,row.values).visible}<span class="muted">Not applicable</span>
                  {:else if field.field_type === 'attachments'}
                    {@const fieldFiles = row.attached_files.filter((file) => file.attachment_field_uid === field.uid)}
                    <button class="attachment-cell-button" class:empty={!fieldFiles.length} onclick={(event) => { event.stopPropagation(); openRecord(row, true); }}>{fieldFiles.length ? `${fieldFiles.length} file${fieldFiles.length === 1 ? '' : 's'}` : 'No files'}</button>
                  {:else if editingCell && cellEdit}
                    {#if field.field_type === 'relationship'}
                      <RelationshipInput {field} value={field.config.multiple === true ? JSON.parse(cellEdit.value || '[]') : cellEdit.value || null} labels={row.relationships?.[field.key]} disabled={cellSaving||!!cellConflict} onChange={value=>void selectCellRelationship(field,value)} />
                      {#if cellEdit.previewLoading}<small role="status">Loading linked fields…</small>{/if}
                      {#if cellEdit.previewError}<p role="alert">{cellEdit.previewError}</p><button class="button small" type="button" onclick={()=>void selectCellRelationship(field,cellEditValue(field,cellEdit!.value))}>Retry linked fields</button>{/if}
                      <div class="inline-actions"><button type="button" class="button small primary" disabled={cellSaving||cellEdit.previewLoading||!!cellEdit.previewError||!!cellConflict} onclick={event=>{event.stopPropagation();void commitCellEdit(row,field);}}>Save link</button><button type="button" class="button small" disabled={cellSaving} onclick={event=>{event.stopPropagation();cancelCellEdit();}}>Cancel</button></div>
                    {:else if field.field_type === 'long_text'}
                      <textarea class="cell-editor long-text-cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} rows="4" onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)} aria-label={`Edit ${field.label}`} placeholder="Enter text · Shift + Enter for a new line"></textarea>
                    {:else if field.field_type === 'select'}
                      <select class="cell-editor" aria-label={`Edit ${field.label}`} data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)}><option value="">Select…</option>{#each availableOptions(field,row.values) as option}<option value={option}>{optionLabel(field,option)}</option>{/each}</select>
                    {:else if field.field_type === 'boolean'}
                      <select class="cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)}><option value="true">Yes</option><option value="false">No</option></select>
                    {:else}
                      <input class="cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} type={field.field_type === 'date' ? 'date' : ['integer', 'decimal'].includes(field.field_type) ? 'number' : 'text'} step={field.field_type === 'decimal' ? 'any' : field.field_type === 'integer' ? 1 : undefined} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)} />
                    {/if}
                  {:else if canEditCell(field,row)}
                    <button class="cell-edit-button" type="button" disabled={cellSaving} title={`Edit ${field.label}`} aria-label={`Edit ${field.label} for row ${(page - 1) * pageSize + rowIndex + 1}`} onclick={(event) => beginCellEdit(event, row, field)}><span>{display(row, field.key)}</span><Pencil size={12} /></button>
                  {:else}
                    {display(row, field.key)}
                  {/if}
                </td>
              {/each}
              <td class="record-actions-column">
                <div class="record-row-actions" data-record-actions={row.uid}>
                  <button class="button record-primary-action" type="button" title={canUpdate ? `Edit ${recordSingular.toLowerCase()}` : `View ${recordSingular.toLowerCase()}`} aria-label={`${canUpdate ? 'Edit' : 'View'} ${recordSingular} ${(page - 1) * pageSize + rowIndex + 1}`} onclick={(event) => { event.stopPropagation(); rowMenuUid = ''; openRecord(row, false, canUpdate); }}>{#if canUpdate}<Pencil size={14} /><span>Edit</span>{:else}<Eye size={14} /><span>View</span>{/if}</button>
                  {#if canDelete}<button class="record-actions-trigger" type="button" aria-label={`More actions for ${recordSingular} ${(page - 1) * pageSize + rowIndex + 1}`} aria-haspopup="menu" aria-expanded={rowMenuUid === row.uid} onclick={(event) => toggleRowMenu(event, row.uid)}><Ellipsis size={17} /></button>{/if}
                </div>
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
      {#if summaryFields.length}<tfoot aria-label="Filtered column summaries"><tr><th scope="row" class="summary-caption">Totals</th>{#each visibleFields as field}<td data-label={field.label}>{#if field.config.table_summary && field.config.table_summary !== 'none'}<span class="column-summary"><small>{summaryNames[field.config.table_summary as keyof typeof summaryNames]}</small><strong aria-label={`${field.label} summary`}>{loading ? 'Calculating…' : summaryValue(summaries[field.key])}</strong></span>{/if}</td>{/each}<td></td></tr></tfoot>{/if}
    </table>
  </div>
  {#if summaryFields.length}<p class="summary-scope" role="status">Summaries cover all {total.toLocaleString()} matching records across every page, not only the rows shown. Search and filters apply.</p>{/if}
  <div class="pagination records-pagination"><span>{rows.length ? `${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()} of ${total.toLocaleString()}` : 'No records'}</span><label>Rows<select bind:value={pageSize} onchange={changePageSize}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option><option value={256}>256</option></select></label><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(1)}>First</button><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(page - 1)}>Previous</button><form onsubmit={goToPage}><label>Page<input type="number" min="1" max={Math.max(1, pages)} bind:value={pageInput} /></label><button class="button" aria-label="Go to page">Go</button></form><span>of {pages}</span><button class="button" disabled={page >= pages || loading} onclick={() => movePage(page + 1)}>Next</button><button class="button" disabled={page >= pages || loading} onclick={() => movePage(pages)}>Last</button></div>
</section>

<TableScrollbar target={tableViewport} enabled={editor === undefined} />

{#if canDelete && menuRow && rowMenuAnchor}
  <div class="record-row-menu" data-record-actions={menuRow.uid} role="menu" aria-label={`${recordSingular} actions`} tabindex="-1" onkeydown={menuKeydown} use:floatingMenu={{ anchor: rowMenuAnchor, close: () => rowMenuUid = '' }}>
    <button class="danger-text" role="menuitem" type="button" disabled={deletingUid === menuRow.uid} onclick={(event) => removeRecord(menuRow, event)}><Trash2 size={14} />{deletingUid === menuRow.uid ? 'Moving…' : 'Move to trash'}</button>
  </div>
{/if}

{#if canCreate && schema && importMenuOpen && importMenuAnchor}
  <div class="module-import-menu" id="module-import-menu" data-module-import-actions role="menu" aria-label="Module actions" tabindex="-1" use:importMenuKeyboard use:floatingMenu={{ anchor: importMenuAnchor, close: () => importMenuOpen = false }}>
    <button role="menuitem" tabindex="-1" type="button" onclick={() => openImporter('new')}><Upload size={16} aria-hidden="true" /><span>Import records</span></button>
    <button role="menuitem" tabindex="-1" type="button" onclick={() => openImporter('history')}><History size={16} aria-hidden="true" /><span>Import history</span></button>
  </div>
{/if}

{#if editor !== undefined && schema}<RecordEditor {schema} {moduleUid} record={editor} initialChanges={editorChanges} canWrite={editor === null ? canCreate : editorWritable && canUpdate} canDelete={editorWritable && canDelete} {canAttachments} allowEdit={editor !== null && canUpdate} {liveMessage} {focusAttachments} recordLabel={recordSingular} onEdit={() => editorWritable = true} onClose={closeEditor} onSaved={() => refresh(true)} />{/if}

{#if importer && schema}{#key moduleUid}<ImportRecords {schema} {moduleUid} history={importer === 'history'} onClose={() => { importer = null; importMenuAnchor?.focus({ preventScroll: true }); }} onChanged={() => refresh(true)} />{/key}{/if}

<style>
  .records-refresh-button { order: 4; }
  .module-import-trigger { order: 5; flex: 0 0 auto; width: 2.2rem; padding-inline: 0; justify-content: center; }
  .module-import-menu { position: fixed; visibility: hidden; min-width: 13rem; z-index: 120; overflow: auto; padding: .35rem; background: var(--surface); border: 1px solid var(--line); border-radius: .55rem; box-shadow: var(--shadow-popover); color: var(--text); font-size: calc(.8rem * var(--font-scale)); }
  .module-import-menu button { display: flex; align-items: center; gap: .65rem; width: 100%; min-height: 2.5rem; padding: .65rem .75rem; border: 0; border-radius: .35rem; text-align: left; font: inherit; line-height: 1.35; color: inherit; background: transparent; cursor: pointer; }
  .module-import-menu button:hover, .module-import-menu button:focus-visible { background: var(--surface-2); }
  .module-import-menu button :global(svg) { flex-shrink: 0; }
  @media (max-width: 760px) {
    .records-toolbar { grid-template-columns: minmax(0, 1fr) auto auto; }
  }
</style>
