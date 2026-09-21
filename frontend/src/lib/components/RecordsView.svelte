<script lang="ts">
  import { onMount } from 'svelte';
  import type { LiveMessage } from '../live/client';
  import type { MxRecord, SchemaResponse } from '../api/domain';
  import { deleteRecord, listRecords, loadSchema } from '../api/workspace';
  import RecordEditor from './RecordEditor.svelte';

  let { accessLevel, revision = 0, liveMessage = null, recordSingular = 'Record', recordPlural = 'Records' }: { accessLevel: number; revision?: number; liveMessage?: LiveMessage | null; recordSingular?: string; recordPlural?: string } = $props();
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
  let editor = $state<MxRecord | null | undefined>(undefined);
  let focusAttachments = $state(false);
  let lastRevision = $state(0);
  let columnOverrides = $state<{ shown: string[]; hidden: string[] }>({ shown: [], hidden: [] });
  let legacyColumns: string[] | null = null;
  let searchTimer: number | undefined;
  let refreshSequence = 0;
  const canWrite = $derived(accessLevel <= 2); const canDelete = $derived(accessLevel <= 1);
  const eligibleFields = $derived(schema?.fields.filter((field) => field.active).sort((a, b) => a.position - b.position) || []);
  const visibleFields = $derived(eligibleFields.filter((field) => selectedKeys.includes(field.key)));
  const searchableFields = $derived(schema?.fields.filter((field) => field.active && field.searchable && field.field_type !== 'attachments').sort((a, b) => a.position - b.position) || []);

  onMount(() => { try { const saved = JSON.parse(localStorage.getItem('mx_record_columns_v3') || 'null'); if (saved && Array.isArray(saved.shown) && Array.isArray(saved.hidden)) columnOverrides = { shown: saved.shown.filter((key: unknown): key is string => typeof key === 'string'), hidden: saved.hidden.filter((key: unknown): key is string => typeof key === 'string') }; else { const old = JSON.parse(localStorage.getItem('mx_record_columns_v2') || 'null'); if (Array.isArray(old)) legacyColumns = old.filter((key): key is string => typeof key === 'string'); } } catch { /* Use schema defaults. */ } void refresh(true); return () => window.clearTimeout(searchTimer); });
  $effect(() => { if (revision !== lastRevision && schema) { lastRevision = revision; void refresh(liveMessage?.type.startsWith('schema.') === true); } });

  async function refresh(withSchema = false) {
    const request = ++refreshSequence;
    loading = true; error = '';
    try {
      if (withSchema || !schema) {
        schema = await loadSchema();
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
      const result = await listRecords({ page, limit: pageSize, q: search.trim(), match: matchMode, filters: activeFilters, attachments: attachmentMode, sort_by: sortBy || undefined, sort_dir: sortDir });
      if (request !== refreshSequence) return;
      rows = result.data; page = result.page; pageInput = result.page; pages = result.total_pages; total = result.total;
    } catch (reason) { error = reason instanceof Error ? reason.message : 'Records could not be loaded.'; }
    finally { if (request === refreshSequence) loading = false; }
  }

  function display(row: MxRecord, key: string) {
    const field = schema?.fields.find((item) => item.key === key);
    if (field?.field_type === 'attachments') {
      const files = row.attached_files.filter((item) => item.attachment_field_uid === field.uid);
      if (!files.length) return '—';
      return files.length === 1 ? files[0].file_name : `${files.length} files`;
    }
    const value = row.values[key];
    if (value == null || value === '') return '—';
    if (typeof value === 'boolean') return value ? 'Yes' : 'No';
    if (field?.field_type === 'auto_number') {
      const prefix = String(field.config.prefix || ''); const padding = Number(field.config.padding || 0);
      return `${prefix}${String(value).padStart(padding, '0')}`;
    }
    return String(value);
  }

  async function searchSubmit(event: SubmitEvent) { event.preventDefault(); page = 1; await refresh(); }
  function scheduleSearch() { window.clearTimeout(searchTimer); searchTimer = window.setTimeout(() => { page = 1; void refresh(); }, 350); }
  async function sort(key: string) { if (sortBy === key) sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else { sortBy = key; sortDir = 'asc'; } page = 1; await refresh(); }
  async function movePage(next: number) { page = next; await refresh(); }
  async function goToPage(event: SubmitEvent) { event.preventDefault(); page = Math.min(Math.max(1, Number(pageInput) || 1), Math.max(1, pages)); await refresh(); }
  function saveColumnOverrides() { localStorage.setItem('mx_record_columns_v3', JSON.stringify(columnOverrides)); }
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
  function openRecord(row: MxRecord, attachments = false) { focusAttachments = attachments; editor = row; }
  function closeEditor() { focusAttachments = false; editor = undefined; }
  async function removeRecord(row: MxRecord, event: MouseEvent) {
    event.stopPropagation();
    if (!confirm(`Delete this ${recordSingular.toLowerCase()} and all of its attachments?`)) return;
    deletingUid = row.uid; error = '';
    try { await deleteRecord(row.uid); if (rows.length === 1 && page > 1) page -= 1; await refresh(); }
    catch (reason) { error = reason instanceof Error ? reason.message : `The ${recordSingular.toLowerCase()} could not be deleted.`; }
    finally { deletingUid = ''; }
  }
</script>

<section class="workspace-page records-page">
  <div class="toolbar">
    {#if canWrite}<button class="button primary records-new-button" onclick={() => { focusAttachments = false; editor = null; }}>+ New {recordSingular.toLowerCase()}</button>{/if}
    <form class="search-form" onsubmit={searchSubmit}><input bind:value={search} oninput={scheduleSearch} type="search" placeholder="Search configured fields and attachment filenames…" aria-label={`Search ${recordPlural.toLowerCase()}`} /><select bind:value={matchMode} onchange={() => { page = 1; void refresh(); }} aria-label="Search matching"><option value="contains">Contains</option><option value="prefix">Starts with</option><option value="exact">Exact</option></select><button class="button" type="submit">Search</button></form>
    <details class="toolbar-menu"><summary class="button">Columns</summary><div class="toolbar-popover">{#each eligibleFields as field}<label class="checkbox"><input type="checkbox" checked={selectedKeys.includes(field.key)} onchange={(event) => toggleColumn(field.key, event.currentTarget.checked)} /> {field.label}</label>{/each}<button class="button small" type="button" onclick={resetColumns}>Use administrator defaults</button><small>Only your overrides are saved, so new fields can follow the deployment defaults.</small></div></details>
    <button class="button" onclick={() => refresh(true)}>Refresh</button><span class="record-count">{total.toLocaleString()} {total === 1 ? recordSingular.toLowerCase() : recordPlural.toLowerCase()}</span>
  </div>
  <details class="advanced-search"><summary>Advanced filters and sorting</summary><form class="filter-grid" onsubmit={searchSubmit}>{#each searchableFields as field}<label>{field.label}{#if field.field_type === 'select'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Any value</option>{#each (field.config.options as (string | number | boolean)[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>{:else if field.field_type === 'boolean'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Either</option><option value="true">Yes</option><option value="false">No</option></select>{:else}<input type={field.field_type === 'date' ? 'date' : ['integer', 'decimal'].includes(field.field_type) ? 'number' : 'text'} value={filters[field.key] || ''} oninput={(event) => filters[field.key] = event.currentTarget.value} placeholder={field.field_type === 'date' ? undefined : 'Field contains…'} />{/if}</label>{/each}<label>Attachments<select bind:value={attachmentMode}><option value="">With or without files</option><option value="with">With attachments</option><option value="without">Without attachments</option></select></label><label>Sort field<select bind:value={sortBy}><option value="">Default order</option>{#each eligibleFields.filter((field) => field.sortable) as field}<option value={field.key}>{field.label}</option>{/each}</select></label><label>Sort direction<select bind:value={sortDir}><option value="asc">Ascending</option><option value="desc">Descending</option></select></label><div class="button-row"><button class="button primary">Apply filters</button><button class="button" type="button" onclick={clearFilters}>Clear all</button></div></form></details>
  {#if error}<div class="notice error">{error}</div>{/if}
  <div class="table-wrap records-table"><table><thead><tr><th class="row-index-column">#</th>{#each visibleFields as field}<th><button class="table-sort" class:sortable={field.sortable} onclick={() => field.sortable && sort(field.key)}>{field.label}{sortBy === field.key ? (sortDir === 'asc' ? ' ↑' : ' ↓') : ''}</button></th>{/each}<th class="record-actions-column">Action</th></tr></thead>
    <tbody>{#if loading}<tr><td class="table-message" colspan={visibleFields.length + 2}>Loading {recordPlural.toLowerCase()}…</td></tr>{:else if !rows.length}<tr><td class="table-message" colspan={visibleFields.length + 2}>No {recordPlural.toLowerCase()} match this view.</td></tr>{:else}{#each rows as row, rowIndex}<tr onclick={() => openRecord(row)} class="clickable-row"><td class="row-index-column"><span title={`${recordSingular} ${row.uid}`}>{(page - 1) * pageSize + rowIndex + 1}</span></td>{#each visibleFields as field}<td class={field.field_type === 'long_text' ? 'long-text-cell' : ''} data-field-type={field.field_type} title={field.field_type === 'long_text' ? undefined : display(row, field.key)}>{#if field.field_type === 'attachments'}{@const fieldFiles = row.attached_files.filter((file) => file.attachment_field_uid === field.uid)}<button class="attachment-cell-button" class:empty={!fieldFiles.length} onclick={(event) => { event.stopPropagation(); openRecord(row, true); }}>{fieldFiles.length ? `${fieldFiles.length} file${fieldFiles.length === 1 ? '' : 's'}` : 'No files'}</button>{:else}{display(row, field.key)}{/if}</td>{/each}<td class="record-actions-column"><div class="record-row-actions"><button class="button small" onclick={(event) => { event.stopPropagation(); openRecord(row); }}>{canWrite ? 'Edit' : 'View'}</button>{#if canDelete}<button class="button danger small" disabled={deletingUid === row.uid} onclick={(event) => removeRecord(row, event)}>{deletingUid === row.uid ? 'Deleting…' : 'Delete'}</button>{/if}</div></td></tr>{/each}{/if}</tbody></table></div>
  <div class="pagination records-pagination"><span>{rows.length ? `${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()} of ${total.toLocaleString()}` : 'No records'}</span><label>Rows<select bind:value={pageSize} onchange={changePageSize}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option><option value={256}>256</option></select></label><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(1)}>First</button><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(page - 1)}>Previous</button><form onsubmit={goToPage}><label>Page<input type="number" min="1" max={Math.max(1, pages)} bind:value={pageInput} /></label><button class="button" aria-label="Go to page">Go</button></form><span>of {pages}</span><button class="button" disabled={page >= pages || loading} onclick={() => movePage(page + 1)}>Next</button><button class="button" disabled={page >= pages || loading} onclick={() => movePage(pages)}>Last</button></div>
</section>

{#if editor !== undefined && schema}<RecordEditor {schema} record={editor} {canWrite} {canDelete} {liveMessage} {focusAttachments} recordLabel={recordSingular} onClose={closeEditor} onSaved={() => refresh(true)} />{/if}
