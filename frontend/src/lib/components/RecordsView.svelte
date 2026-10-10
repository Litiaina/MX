<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Eye from '@lucide/svelte/icons/eye';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Search from '@lucide/svelte/icons/search';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { ApiError } from '../api/client';
  import type { LiveMessage } from '../live/client';
  import { requestConfirmation } from '../confirmation';
  import type { FieldConflict, ModulePermission, MxRecord, SchemaResponse } from '../api/domain';
  import type { Session } from '../api/types';
  import { deleteRecord, getRecord, listRecords, loadModuleSchema, loadSchema, patchRecord } from '../api/workspace';
  import RecordEditor from './RecordEditor.svelte';
  import RelationshipInput from './RelationshipInput.svelte';
  import TableScrollbar from './TableScrollbar.svelte';
  import { floatingMenu } from '../util/floating';

  let { session, accessLevel, moduleUid = '', modulePermission = null, openRecordUid = '', openRequestRevision = 0, focusLinkedAttachments = false, revision = 0, liveMessage = null, recordSingular = 'Record', recordPlural = 'Records' }: { session: Session; accessLevel: number; moduleUid?: string; modulePermission?: ModulePermission | null; openRecordUid?: string; openRequestRevision?: number; focusLinkedAttachments?: boolean; revision?: number; liveMessage?: LiveMessage | null; recordSingular?: string; recordPlural?: string } = $props();
  let schema = $state<SchemaResponse | null>(null);
  let rows = $state<MxRecord[]>([]);
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
  let editorWritable = $state(false);
  let cellEdit = $state<{ rowUid: string; fieldKey: string; value: string; baseRevision: number } | null>(null);
  let cellSaving = $state(false);
  let cellConflict = $state<{ conflict: FieldConflict; recordRevision: number } | null>(null);
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
  const searchableFields = $derived(schema?.fields.filter((field) => field.active && field.searchable && field.field_type !== 'attachments').sort((a, b) => a.position - b.position) || []);

  const columnStorageKey = $derived(`mx_record_columns_v3_${moduleUid || 'default'}`);
  onMount(() => {
    try { const saved = JSON.parse(localStorage.getItem(columnStorageKey) || localStorage.getItem('mx_record_columns_v3') || 'null'); if (saved && Array.isArray(saved.shown) && Array.isArray(saved.hidden)) columnOverrides = { shown: saved.shown.filter((key: unknown): key is string => typeof key === 'string'), hidden: saved.hidden.filter((key: unknown): key is string => typeof key === 'string') }; else { const old = JSON.parse(localStorage.getItem('mx_record_columns_v2') || 'null'); if (Array.isArray(old)) legacyColumns = old.filter((key): key is string => typeof key === 'string'); } } catch { /* Use schema defaults. */ }
    const closeRowMenu = (event: PointerEvent) => { const target = event.target instanceof Element ? event.target : null; if (!target?.closest('[data-record-actions]')) rowMenuUid = ''; };
    const closeRowMenuWithKeyboard = (event: KeyboardEvent) => { if (event.key === 'Escape' && rowMenuUid) { event.preventDefault(); closeRowMenuAndFocus(); } };
    document.addEventListener('pointerdown', closeRowMenu);
    document.addEventListener('keydown', closeRowMenuWithKeyboard);
    void refresh(true);
    return () => { window.clearTimeout(searchTimer); document.removeEventListener('pointerdown', closeRowMenu); document.removeEventListener('keydown', closeRowMenuWithKeyboard); };
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
    rowMenuAnchor = event.currentTarget as HTMLButtonElement;
    rowMenuUid = rowMenuUid === uid ? '' : uid;
  }
  function menuKeydown(event: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault(); (event.currentTarget as HTMLElement).querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    } else if (event.key === 'Tab') closeRowMenuAndFocus();
  }

  async function openLinkedRecord(uid: string) {
    rowMenuUid = ''; openedDeepLink = uid; openedRequestRevision = openRequestRevision; error = '';
    try { const record = await getRecord(uid, moduleUid || undefined); if (deletedRecordUids.has(uid) || openedDeepLink !== uid) return; editor = record; editorWritable = false; focusAttachments = focusLinkedAttachments; }
    catch (reason) { error = reason instanceof Error ? reason.message : `The linked ${recordSingular.toLowerCase()} could not be opened.`; }
  }

  async function refresh(withSchema = false) {
    const request = ++refreshSequence;
    loading = true; error = '';
    try {
      if (withSchema || !schema) {
        const nextSchema = moduleUid ? await loadModuleSchema(moduleUid) : await loadSchema();
        if (request !== refreshSequence) return;
        schema = nextSchema;
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
    } catch (reason) { if (request === refreshSequence) error = reason instanceof Error ? reason.message : 'Records could not be loaded.'; }
    finally { if (request === refreshSequence) loading = false; }
  }

  function forgetDeletedRecord(uid: string) {
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
    if (field?.field_type === 'attachments') {
      const files = row.attached_files.filter((item) => item.attachment_field_uid === field.uid);
      if (!files.length) return '—';
      return files.length === 1 ? files[0].file_name : `${files.length} files`;
    }
    const value = row.values[key];
    if (field?.field_type === 'relationship' && row.relationships?.[key]?.restricted) return 'Access restricted';
    if (value == null || value === '') return '—';
    if (typeof value === 'boolean') return value ? 'Yes' : 'No';
    if (field?.field_type === 'relationship') return row.relationships?.[key]?.restricted ? 'Access restricted' : row.relationships?.[key]?.items.map(item=>item.label).join(', ') || '—';
    if (Array.isArray(value)) return value.map(item=>String(item ?? '—')).join(', ');
    if (field?.field_type === 'auto_number') {
      const prefix = String(field.config.prefix || ''); const padding = Number(field.config.padding || 0);
      return `${prefix}${String(value).padStart(padding, '0')}`;
    }
    return String(value);
  }

  function canEditCell(field: SchemaResponse['fields'][number]) {
    return canUpdate && !['attachments', 'auto_number', 'formula', 'lookup', 'rollup'].includes(field.field_type);
  }
  function beginCellEdit(event: MouseEvent, row: MxRecord, field: SchemaResponse['fields'][number]) {
    if (!canEditCell(field) || cellSaving) return;
    event.preventDefault(); event.stopPropagation();
    error = '';
    const raw = row.values[field.key];
    cellConflict = null;
    cellEdit = { rowUid: row.uid, fieldKey: field.key, value: field.field_type === 'boolean' ? String(raw === true) : Array.isArray(raw) ? JSON.stringify(raw) : raw == null ? '' : String(raw), baseRevision: row.revision };
    void tick().then(() => {
      const control = document.querySelector<HTMLInputElement | HTMLSelectElement>('[data-active-cell-editor]');
      control?.focus();
      if (control instanceof HTMLInputElement && control.type === 'text') control.select();
    });
  }
  function cancelCellEdit(event?: Event) { event?.stopPropagation(); cellEdit = null; }
  function useStoredCellValue() {
    if (!cellConflict || !cellEdit) return;
    const { conflict, recordRevision } = cellConflict;
    rows = rows.map((row) => row.uid === cellEdit!.rowUid ? { ...row, revision: recordRevision, values: { ...row.values, [conflict.field_key]: conflict.current_value } } : row);
    cellConflict = null; cellEdit = null; error = '';
  }
  async function keepProposedCellValue() {
    if (!cellConflict || !cellEdit || !schema) return;
    const { recordRevision } = cellConflict;
    const row = rows.find((item) => item.uid === cellEdit!.rowUid);
    const field = schema.fields.find((item) => item.key === cellEdit!.fieldKey);
    if (!row || !field) return;
    const latest = { ...row, revision: recordRevision };
    cellEdit.baseRevision = recordRevision;
    rows = rows.map((item) => item.uid === latest.uid ? latest : item);
    cellConflict = null; error = '';
    await commitCellEdit(latest, field);
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
    const nextValue = cellEditValue(field, edit.value);
    if (field.required && (nextValue === null || nextValue === '')) { error = `${field.label} is required.`; return; }
    if (JSON.stringify(nextValue) === JSON.stringify(row.values[field.key] ?? null)) { cellEdit = null; return; }
    cellSaving = true; error = '';
    try {
      const saved = await patchRecord(row.uid, { [field.key]: nextValue }, edit.baseRevision, moduleUid || undefined);
      if (!deletedRecordUids.has(saved.uid)) rows = rows.map((item) => item.uid === saved.uid ? saved : item);
      cellEdit = null;
    } catch (reason) {
      error = reason instanceof Error ? reason.message : `The ${field.label.toLowerCase()} cell could not be saved.`;
      if (reason instanceof ApiError && reason.status === 409 && reason.payload && typeof reason.payload === 'object') {
        const payload = reason.payload as { conflicts?: FieldConflict[]; current_record_revision?: number };
        if (payload.conflicts?.[0]) cellConflict = { conflict: payload.conflicts[0], recordRevision: Number(payload.current_record_revision || row.revision) };
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
  function openRecord(row: MxRecord, attachments = false, edit = false) { if (deletedRecordUids.has(row.uid)) return; rowMenuUid = ''; if (cellEdit) cancelCellEdit(); focusAttachments = attachments; editorWritable = edit && canUpdate; editor = row; }
  function closeEditor() { focusAttachments = false; editorWritable = false; editor = undefined; if (openRecordUid) location.hash = `module/${encodeURIComponent(moduleUid || 'mx-default-records')}`; }
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
  <div class="toolbar">
    {#if canCreate}<button class="button primary records-new-button" onclick={() => { focusAttachments = false; editorWritable = true; editor = null; }}>+ New {recordSingular.toLowerCase()}</button>{/if}
    <form class="search-form" onsubmit={searchSubmit}><label class="records-search-input"><Search size={16} aria-hidden="true" /><input bind:value={search} oninput={scheduleSearch} type="search" placeholder={canAttachments ? 'Search configured fields and attachment filenames…' : 'Search configured fields…'} aria-label={`Search ${recordPlural.toLowerCase()}`} /></label><select bind:value={matchMode} onchange={() => { page = 1; void refresh(); }} aria-label="Search matching"><option value="contains">Contains</option><option value="prefix">Starts with</option><option value="exact">Exact</option></select><button class="button" type="submit">Search</button></form>
    <details class="toolbar-menu"><summary class="button">Columns</summary><div class="toolbar-popover">{#each eligibleFields as field}<label class="checkbox"><input type="checkbox" checked={selectedKeys.includes(field.key)} onchange={(event) => toggleColumn(field.key, event.currentTarget.checked)} /> {field.label}</label>{/each}<button class="button small" type="button" onclick={resetColumns}>Use administrator defaults</button><small>Only your overrides are saved, so new fields can follow the deployment defaults.</small></div></details>
    <button class="button" onclick={() => refresh(true)}>Refresh</button>
  </div>
    <details class="advanced-search"><summary>Advanced filters and sorting</summary><form class="filter-grid" onsubmit={searchSubmit}>{#each searchableFields as field}<label>{field.label}{#if field.field_type === 'select'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Any value</option>{#each (field.config.options as (string | number | boolean)[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>{:else if field.field_type === 'boolean'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Either</option><option value="true">Yes</option><option value="false">No</option></select>{:else}<input type={field.field_type === 'date' ? 'date' : ['integer', 'decimal', 'formula'].includes(field.field_type) ? 'number' : 'text'} value={filters[field.key] || ''} oninput={(event) => filters[field.key] = event.currentTarget.value} placeholder={field.field_type === 'date' ? undefined : 'Field contains…'} />{/if}</label>{/each}{#if canAttachments}<label>Attachments<select bind:value={attachmentMode}><option value="">With or without files</option><option value="with">With attachments</option><option value="without">Without attachments</option></select></label>{/if}<label>Sort field<select bind:value={sortBy}><option value="">Default order</option>{#each eligibleFields.filter((field) => field.sortable) as field}<option value={field.key}>{field.label}</option>{/each}</select></label><label>Sort direction<select bind:value={sortDir}><option value="asc">Ascending</option><option value="desc">Descending</option></select></label><div class="button-row"><button class="button primary">Apply filters</button><button class="button" type="button" onclick={clearFilters}>Clear all</button></div></form></details>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if cellConflict}<div class="notice record-cell-conflict"><div><strong>{cellConflict.conflict.label} changed elsewhere</strong><small>Stored: {String(cellConflict.conflict.current_value ?? 'Empty')} · Yours: {String(cellConflict.conflict.your_value ?? 'Empty')}</small></div><div class="inline-actions"><button class="button small" type="button" onclick={useStoredCellValue}>Use stored</button><button class="button primary small" type="button" onclick={() => void keepProposedCellValue()}>Keep mine</button></div></div>{/if}
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
                <td class:inline-editable-cell={canEditCell(field)} class:editing-cell={editingCell} class={field.field_type === 'long_text' ? 'long-text-cell' : ''} data-label={field.label} data-field-type={field.field_type} title={editingCell || canEditCell(field) || field.field_type === 'long_text' ? undefined : display(row, field.key)}>
                  {#if field.field_type === 'attachments'}
                    {@const fieldFiles = row.attached_files.filter((file) => file.attachment_field_uid === field.uid)}
                    <button class="attachment-cell-button" class:empty={!fieldFiles.length} onclick={(event) => { event.stopPropagation(); openRecord(row, true); }}>{fieldFiles.length ? `${fieldFiles.length} file${fieldFiles.length === 1 ? '' : 's'}` : 'No files'}</button>
                  {:else if editingCell && cellEdit}
                    {#if field.field_type === 'relationship'}
                      <RelationshipInput {field} value={field.config.multiple === true ? JSON.parse(cellEdit.value || '[]') : cellEdit.value || null} labels={row.relationships?.[field.key]} disabled={cellSaving} onChange={value=>{if(cellEdit)cellEdit.value=Array.isArray(value)?JSON.stringify(value):String(value ?? '');}} />
                      <div class="inline-actions"><button type="button" class="button small primary" disabled={cellSaving} onclick={event=>{event.stopPropagation();void commitCellEdit(row,field);}}>Save link</button><button type="button" class="button small" onclick={event=>{event.stopPropagation();cancelCellEdit();}}>Cancel</button></div>
                    {:else if field.field_type === 'long_text'}
                      <textarea class="cell-editor long-text-cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} rows="4" onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)} aria-label={`Edit ${field.label}`} placeholder="Enter text · Shift + Enter for a new line"></textarea>
                    {:else if field.field_type === 'select'}
                      <select class="cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)}><option value="">Select…</option>{#each (field.config.options as (string | number | boolean)[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>
                    {:else if field.field_type === 'boolean'}
                      <select class="cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)}><option value="true">Yes</option><option value="false">No</option></select>
                    {:else}
                      <input class="cell-editor" data-active-cell-editor bind:value={cellEdit.value} disabled={cellSaving} type={field.field_type === 'date' ? 'date' : ['integer', 'decimal'].includes(field.field_type) ? 'number' : 'text'} step={field.field_type === 'decimal' ? 'any' : field.field_type === 'integer' ? 1 : undefined} onclick={(event) => event.stopPropagation()} onkeydown={(event) => cellEditorKeydown(event, row, field)} onblur={() => void commitCellEdit(row, field)} />
                    {/if}
                  {:else if canEditCell(field)}
                    <button class="cell-edit-button" type="button" title={`Edit ${field.label}`} aria-label={`Edit ${field.label} for row ${(page - 1) * pageSize + rowIndex + 1}`} onclick={(event) => beginCellEdit(event, row, field)}><span>{display(row, field.key)}</span><Pencil size={12} /></button>
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
    </table>
  </div>
  <div class="pagination records-pagination"><span>{rows.length ? `${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()} of ${total.toLocaleString()}` : 'No records'}</span><label>Rows<select bind:value={pageSize} onchange={changePageSize}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option><option value={256}>256</option></select></label><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(1)}>First</button><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(page - 1)}>Previous</button><form onsubmit={goToPage}><label>Page<input type="number" min="1" max={Math.max(1, pages)} bind:value={pageInput} /></label><button class="button" aria-label="Go to page">Go</button></form><span>of {pages}</span><button class="button" disabled={page >= pages || loading} onclick={() => movePage(page + 1)}>Next</button><button class="button" disabled={page >= pages || loading} onclick={() => movePage(pages)}>Last</button></div>
</section>

<TableScrollbar target={tableViewport} enabled={editor === undefined} />

{#if canDelete && menuRow && rowMenuAnchor}
  <div class="record-row-menu" data-record-actions={menuRow.uid} role="menu" aria-label={`${recordSingular} actions`} tabindex="-1" onkeydown={menuKeydown} use:floatingMenu={{ anchor: rowMenuAnchor, close: () => rowMenuUid = '' }}>
    <button class="danger-text" role="menuitem" type="button" disabled={deletingUid === menuRow.uid} onclick={(event) => removeRecord(menuRow, event)}><Trash2 size={14} />{deletingUid === menuRow.uid ? 'Moving…' : 'Move to trash'}</button>
  </div>
{/if}

{#if editor !== undefined && schema}<RecordEditor {schema} {moduleUid} record={editor} canWrite={editor === null ? canCreate : editorWritable && canUpdate} canDelete={editorWritable && canDelete} {canAttachments} allowEdit={editor !== null && canUpdate} {liveMessage} {focusAttachments} recordLabel={recordSingular} onEdit={() => editorWritable = true} onClose={closeEditor} onSaved={() => refresh(true)} />{/if}
