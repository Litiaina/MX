<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import FileSpreadsheet from '@lucide/svelte/icons/file-spreadsheet';
  import { ApiError } from '../api/client';
  import { operationUid } from '../api/operation';
  import type { SchemaResponse, JsonValue } from '../api/domain';
  import { getRecord } from '../api/workspace';
  import { requestConfirmation } from '../confirmation';
  import { configureImport, correctImport, importHistory, importJob, importRows, processImport, removeImportLog, uploadImport, type ImportJob, type ImportMapping, type ImportRow, type ImportRows } from '../api/imports';
  import { guessMapping, restoreImportMapping, importWritable, importAttachmentFields, columnName, csvCell } from '../util/importMapping';
  import RecordEditor from './RecordEditor.svelte';
  let { moduleUid, schema, history = false, onClose, onChanged }: { moduleUid: string; schema: SchemaResponse; history?: boolean; onClose: () => void; onChanged: () => Promise<void> } = $props();
  let stage = $state<'file' | 'mapping' | 'review' | 'results' | 'history'>(untrack(() => history) ? 'history' : 'file');
  let job = $state<ImportJob | null>(null), jobs = $state<ImportJob[]>([]), historyTotal = $state(0), historyPage = $state(1);
  let mappings = $state<ImportMapping[]>([]), selected = $state<number[]>([]), sheetIndex = $state(0);
  let file = $state<File | null>(null), delimiter = $state('comma'), uploadUid = $state(operationUid());
  let busy = $state(false), running = $state(false), keepRunning = $state(false), error = $state(''), status = $state(''), transferred = $state(0);
  let result = $state<ImportRows>({ rows: [], total: 0, page: 1, limit: 20 }), rowState = $state('pending'), rowPage = $state(1), correction = $state<ImportRow | null>(null);
  let gone = false, retryTimer: ReturnType<typeof setTimeout> | undefined, dialog: HTMLDivElement;
  const fields = $derived(schema.fields.filter(importWritable));
  const attachmentFields = $derived(importAttachmentFields(schema.fields));
  const noValidRows = $derived(!!job?.pending && job.validation?.ready === 0);
  const sheet = $derived(job?.sheets[sheetIndex]);
  const mapping = $derived(mappings.find(m => m.index === sheetIndex));
  const header = $derived(sheet?.sample.find(row => row.row === mapping?.header_row));
  const columns = $derived([...new Set([...(mapping?.columns.map(c => c.column) || []), ...Object.keys(header?.cells || {}).map(Number)])].sort((a, b) => a - b));
  const percent = $derived(job?.total ? Math.floor(job.processed / job.total * 100) : 0);
  const canClose = $derived(!busy && !correction);
  const message = (reason: unknown) => reason instanceof Error ? reason.message : 'Import could not continue.';
  onMount(() => { dialog.focus(); if (history) void loadHistory(); });
  onDestroy(() => { gone = true; keepRunning = false; clearTimeout(retryTimer); });
  function close() { if (!canClose) return; keepRunning = false; clearTimeout(retryTimer); onClose(); }
  async function loadHistory() { if (running) return; keepRunning = false; clearTimeout(retryTimer); busy = true; error = ''; try { const data = await importHistory(moduleUid, historyPage); if (!gone) { jobs = data.jobs; historyTotal = data.total; stage = 'history'; } } catch (e) { error = message(e); } finally { busy = false; } }
  async function readFile() {
    if (!file) return; if (file.size > 64 * 1024 * 1024) { error = 'Spreadsheet imports accept up to 64 MiB compressed. Split this workbook into smaller imports; Drive uploads are not affected.'; return; } busy = true; error = ''; transferred = 0; status = 'Transferring spreadsheet to MX…';
    try {
      const data = await uploadImport(moduleUid, file, uploadUid, delimiter, (loaded, total) => { transferred = total ? Math.floor(loaded / total * 100) : 0; if (transferred >= 100) status = 'Reading populated cells and preparing sheets. No records have been created yet…'; });
      if (gone) return; job = data; mappings = data.sheets.map(s => guessMapping(s, fields)); const first = data.sheets.find(s => !s.error && s.row_count > 0); selected = first ? [first.index] : []; sheetIndex = first?.index || 0; stage = 'mapping';
    } catch (e) { error = message(e); } finally { busy = false; status = ''; }
  }
  function chooseFile(event: Event) { file = (event.currentTarget as HTMLInputElement).files?.[0] || null; delimiter = file?.name.toLowerCase().endsWith('.tsv') ? 'tab' : 'comma'; uploadUid = operationUid(); }
  function headerChanged() { if (!sheet || !mapping) return; const old = mapping; const guessed = guessMapping({ ...sheet, sample: sheet.sample.filter(r => r.row === old.header_row) }, fields); mappings = mappings.map(m => m.index === sheetIndex ? { ...guessed, header_row: old.header_row, first_data_row: Math.max(old.header_row + 1, old.first_data_row) } : m); }
  function columnConfig(column: number) { return mapping!.columns.find(c => c.column === column)!; }
  async function preview() {
    if (!job) return; busy = true; error = ''; status = 'Validating mapped rows against module fields and form rules…';
    try { job = await configureImport(moduleUid, job.uid, mappings.filter(m => selected.includes(m.index)).map(m => ({ ...m, columns: m.columns.filter(c => c.key) }))); stage = 'review'; rowState = job.validation?.invalid ? 'issues' : 'pending'; rowPage = 1; await loadRows(); } catch (e) { error = message(e); } finally { busy = false; status = ''; }
  }
  let loadSequence = 0;
  async function loadRows() { if (!job) return; const sequence = ++loadSequence; const data = await importRows(moduleUid, job.uid, rowState, rowPage); if (!gone && sequence === loadSequence) result = data; }
  async function changeRows(state: string) { rowState = state; rowPage = 1; error = ''; try { await loadRows(); } catch (e) { error = message(e); } }
  async function openJob(data: ImportJob) {
    busy = true; error = '';
    try {
      data = await importJob(moduleUid, data.uid); if (gone) return;
      job = data; const saved = data.config.sheets; mappings = data.sheets.map(s => { const existing = saved?.find(m => m.index === s.index); return existing ? restoreImportMapping(s, existing) : guessMapping(s, fields); }); const first = data.sheets.find(s => !s.error && s.row_count > 0); selected = saved ? saved.map(m => m.index) : first ? [first.index] : []; sheetIndex = selected[0] || 0;
      stage = data.state === 'draft' ? 'mapping' : data.processed ? 'results' : 'review'; rowState = data.failed ? 'failed' : data.validation?.invalid ? 'issues' : data.processed ? 'imported' : 'pending'; rowPage = 1;
      if (stage !== 'mapping') await loadRows();
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  function reviewMapping() { if (busy || running || !job || job.imported) return; keepRunning = false; clearTimeout(retryTimer); error = ''; status = ''; stage = 'mapping'; }
  async function start() {
    if (!job || running || busy || noValidRows) return; keepRunning = true; clearTimeout(retryTimer); running = true; stage = 'results'; error = ''; status = 'Saving validated rows…';
    try {
      while (keepRunning && !gone && job.pending) { const updated = await processImport(moduleUid, job.uid); if (gone) return; job = updated; }
      if (gone) return;
      keepRunning = false; job = await importJob(moduleUid, job.uid);
      status = job.pending ? 'Paused. Saved rows are safe; resume here or from Import history.' : job.failed ? 'Completed with issues. Inspect and correct the skipped rows below.' : 'Import complete. Every selected data row was saved.';
      rowState = job.failed ? 'failed' : 'imported'; rowPage = 1; await loadRows(); await onChanged();
    } catch (e) {
      if (gone) return; error = message(e); status = 'Connection or service interrupted. Confirming committed progress before retrying…';
      try { job = await importJob(moduleUid, job.uid); } catch { /* persisted progress is reconciled on reconnect */ }
      const transient = !(e instanceof ApiError) || e.status === 0 || e.status === 408 || e.status === 429 || e.status >= 500;
      if (keepRunning && transient) retryTimer = setTimeout(() => { if (!gone && keepRunning) void start(); }, 5000);
      else { keepRunning = false; status = 'Paused. Resolve the error, then resume; successful rows will not be duplicated.'; }
    } finally { running = false; }
  }
  function pause() { keepRunning = false; clearTimeout(retryTimer); status = 'Pausing after the current batch. Committed rows remain saved.'; }
  async function saveCorrection(values: Record<string, JsonValue>) { if (!job || !correction) throw Error('Select a failed row first.'); const saved = await correctImport(moduleUid, job.uid, correction, values); return getRecord(saved.record_uid, moduleUid); }
  async function corrected() { correction = null; if (!job) return; job = await importJob(moduleUid, job.uid); status = job.pending ? 'Paused. Resume to process the remaining rows.' : job.failed ? 'Completed with issues. Inspect and correct the remaining skipped rows.' : 'Import complete. Every selected data row was saved.'; await loadRows(); await onChanged(); }
  function downloadText(name: string, text: string) { const url = URL.createObjectURL(new Blob(['\uFEFF', text], { type: 'text/csv;charset=utf-8' })); const a = document.createElement('a'); a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); }
  function template() { downloadText('mx-import-template.csv', fields.map(f => csvCell(`mx:${f.key}`)).join(',') + '\r\n' + fields.map(f => csvCell(f.label)).join(',') + '\r\n'); }
  async function exportFailed() {
    if (!job) return; busy = true; error = ''; const lines = ['Sheet,Source row,Field,Column,Problem,Original mapped values'];
    try { for (let page = 1; ; page++) { const data = await importRows(moduleUid, job.uid, 'failed', page); for (const row of data.rows) for (const issue of row.errors) lines.push([job.sheets[row.sheet].name, String(row.row), issue.field, issue.column ? columnName(issue.column) : '', issue.message, JSON.stringify(row.cells)].map(csvCell).join(',')); if (page * data.limit >= data.total) break; } downloadText('mx-import-failed-rows.csv', lines.join('\r\n')); } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function forget() { if (!job || !await requestConfirmation({ title: 'Remove this import log?', description: 'This removes its source rows and results log. Records already imported, their attachments and version history remain unchanged.', confirmLabel: 'Remove log' })) return; busy = true; try { await removeImportLog(moduleUid, job.uid); job = null; await loadHistory(); } catch (e) { error = message(e); } finally { busy = false; } }
</script>

<div class="import-backdrop" inert={!!correction} aria-hidden={correction ? 'true' : undefined}>
  <div class="import-dialog" bind:this={dialog} role="dialog" aria-modal="true" aria-label="Import records" tabindex="-1" onkeydown={event => { if (event.key === 'Escape' && !correction) { event.stopPropagation(); close(); } if (event.key === 'Tab' && !correction) { const controls = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),a[href],summary')].filter(e => e.getClientRects().length); const first = controls[0], last = controls.at(-1); if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { event.preventDefault(); last?.focus(); } else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); } } }}>
    <header><div><h2>{stage === 'history' ? 'Import history' : 'Import records'}</h2><p>{job?.filename || 'Bring spreadsheet rows into this module. Existing records are never overwritten.'}</p></div><button class="button icon" aria-label="Close import" disabled={!canClose} onclick={close}><X size={18} /></button></header>
    <div class="import-body">
      {#if error}<div class="notice error" role="alert">{error}</div>{/if}
      {#if busy}<p role="status">{status || 'Loading…'}</p>{/if}
      {#if stage === 'file'}
        <div class="import-intro"><FileSpreadsheet size={32} /><h3>Choose your spreadsheet</h3><p>XLSX, ODS, UTF-8 CSV or TSV. Select the sheets, match columns to your module fields, then preview before importing.</p></div>
        <label>Spreadsheet<input type="file" accept=".xlsx,.ods,.csv,.tsv" disabled={busy} onchange={chooseFile} /></label>
        {#if file && /\.(csv|tsv)$/i.test(file.name)}<label>Column separator<select bind:value={delimiter} disabled={busy}><option value="comma">Comma (CSV)</option><option value="semicolon">Semicolon</option><option value="tab">Tab (TSV)</option></select></label>{/if}
        {#if busy}<progress max="100" value={transferred} aria-label="Spreadsheet transfer"></progress>{/if}
        <p class="help">Starting from scratch? Download a tagged template. Keep row 1 field tags, row 2 labels, and enter data from row 3. Attachments are added after import; relationship columns use MX record IDs, not names.</p>
        <div class="actions"><button class="button" disabled={busy} onclick={template}>Download template</button><button class="button" disabled={busy} onclick={loadHistory}>Import history</button><button class="button primary" disabled={busy || !file} onclick={readFile}>Read spreadsheet</button></div>
      {:else if stage === 'mapping' && job && sheet && mapping}
        <h3>1. Select sheets · 2. Match columns</h3><p class="help">All selected sheets go into this module. Each sheet has its own mapping. Nothing is saved as a record until you start the import.</p>
        {#if attachmentFields.length}
          <section class="import-info" role="note" aria-label="Attachments are not imported">
            <strong>Import record values now; add files later</strong>
            <p>Attachment fields ({attachmentFields.map(field => field.label).join(', ')}) are not part of spreadsheet import and will not block it, even if required in the normal entry form. Imported records start without attachments; no module settings are changed.</p>
          </section>
        {/if}
        <div class="sheets">{#each job.sheets as item}<label class="sheet-choice"><input type="checkbox" disabled={!!item.error || !item.row_count} checked={selected.includes(item.index)} onchange={event => selected = event.currentTarget.checked ? [...selected, item.index] : selected.filter(i => i !== item.index)} /><button class:active={sheetIndex === item.index} onclick={() => sheetIndex = item.index}>{item.name}<small>{item.error ? 'Sheet needs attention' : `${item.row_count.toLocaleString()} populated rows`}</small></button></label>{/each}</div>
        {#if sheet.error}<div class="notice error" role="alert"><strong>{sheet.name}</strong>: {sheet.error} This sheet is excluded. Other sheets can still be imported.</div>{/if}
        <div class="row-options"><label>Header row<input type="number" min="1" bind:value={mapping.header_row} onchange={headerChanged} /></label><label>First data row<input type="number" min={mapping.header_row + 1} bind:value={mapping.first_data_row} /></label></div>
        <p class="help">A column marked “Do not import” is ignored. Date fill-down copies the last nonblank date in this sheet only. Review every suggested match.</p>
        <div class="import-table"><table><thead><tr><th>Spreadsheet column</th><th>MX field</th><th>Date interpretation</th></tr></thead><tbody>{#each columns as col}{@const config = columnConfig(col)}{@const target = fields.find(f => f.key === config.key)}<tr><td><strong>{columnName(col)} · {header?.cells[String(col)]?.text || 'No header'}</strong><small>{sheet.sample.find(r => r.row >= mapping.first_data_row)?.cells[String(col)]?.text || 'No sample value'}</small></td><td><select aria-label={`Map column ${columnName(col)}`} bind:value={config.key}><option value="">Do not import</option>{#each fields as field}<option value={field.key}>{field.label} · {field.field_type}{field.required ? ' · required' : ''}</option>{/each}</select>{#if target?.field_type === 'relationship'}<small>Use the linked MX record’s ID. Duplicate names are never guessed.</small>{/if}</td><td>{#if target?.field_type === 'date'}<select aria-label={`Date format for ${target.label}`} bind:value={config.date_format}><option value="iso">YYYY-MM-DD (Excel dates detected automatically)</option><option value="dmy">DD/MM/YYYY</option><option value="mdy">MM/DD/YYYY</option><option value="mdy-short-dot">M.D.YY (1.5.26)</option><option value="month-short">MON DD,YYYY (FEB 02,2026)</option><option value="month-long">MONTH DD YYYY (MARCH 02 2026)</option></select><label class="check"><input type="checkbox" bind:checked={config.fill_down} />Fill down blank dates</label>{:else}<small>Preserve source text; validate against the configured field type.</small>{/if}</td></tr>{/each}</tbody></table></div>
        {#if !columns.length}<p role="alert">No header was found in the sample. Choose a populated header row shown in the source preview below.</p>{/if}
        <details><summary>Source preview (first populated rows)</summary><div class="source-preview">{#each sheet.sample as row}<p><strong>Row {row.row}</strong> {Object.entries(row.cells).map(([col,cell]) => `${columnName(Number(col))}: ${cell.text}`).join(' · ')}</p>{/each}</div></details>
        <div class="actions"><button class="button" disabled={busy} onclick={() => { stage = 'file'; job = null; uploadUid = operationUid(); }}>Choose another file</button><button class="button primary" disabled={busy || !selected.length} onclick={preview}>Validate and preview</button></div>
      {:else if (stage === 'review' || stage === 'results') && job}
        <h3>{stage === 'review' ? '3. Review before importing' : running || keepRunning || job.pending ? 'Import progress' : job.failed ? 'Completed with issues' : 'Import complete'}</h3>
        {#if !running && job.validation?.invalid}
          <section class="import-warning" role="alert" aria-label="Import validation problems">
            <strong>{stage === 'review' && noValidRows ? 'No rows can be imported yet' : !job.imported && !job.pending ? 'No records were saved' : 'Some rows need correction'}</strong>
            <p>{job.validation.ready.toLocaleString()} ready to import · {job.validation.invalid.toLocaleString()} need attention. {stage === 'review' ? 'These are validation problems, not a storage or connection failure.' : 'These are the saved validation results from this import attempt.'}</p>
            <ul>{#each job.validation.issues as issue}<li><strong>{issue.rows.toLocaleString()} {issue.rows === 1 ? 'row' : 'rows'}:</strong> {issue.message}</li>{/each}</ul>
            <p>{job.imported ? 'Saved records are unchanged; correct the skipped rows individually below.' : 'No need to upload again: review the mapping and validate this same job using the current import rules. Revalidation replaces old errors without creating records.'}</p>
          </section>
        {/if}
        <p class="help">{stage === 'review' ? `${job.total.toLocaleString()} mapped data rows. Rows with invalid values will be skipped and kept in the results log. Unique values and current permissions are checked again when saving.` : status || 'Saved progress is kept by MX. Resume pending rows or inspect skipped rows below.'}</p>
        {#if stage !== 'review'}<progress max={job.total || 1} value={job.processed} aria-label="Import progress"></progress>{/if}
        <div class="counts" aria-live="polite">{#if stage === 'review' && job.validation}<strong>{job.total.toLocaleString()} rows checked · no records saved yet</strong><span>{job.validation.ready.toLocaleString()} ready to import</span><span>{job.validation.invalid.toLocaleString()} need attention</span>{:else}<strong>{percent}% · {job.processed.toLocaleString()} / {job.total.toLocaleString()} processed</strong><span>{job.imported.toLocaleString()} saved</span><span>{job.failed.toLocaleString()} need attention</span><span>{job.pending.toLocaleString()} pending</span>{/if}</div>
        <div class="actions">{#if running || keepRunning}<button class="button" onclick={pause}>Pause import</button>{:else if job.pending}<button class="button primary" disabled={busy || noValidRows} onclick={start}>{job.processed ? 'Resume import' : 'Start import'}</button>{/if}{#if !job.imported}<button class="button" disabled={busy || running} onclick={reviewMapping}>{stage === 'review' ? 'Back to mapping' : 'Review mapping and revalidate'}</button>{/if}<button class="button" disabled={busy || running} onclick={loadHistory}>Import history</button>{#if job.failed}<button class="button" disabled={busy || running} onclick={exportFailed}>Download failed-row log</button>{/if}</div>
        <div class="result-tabs" aria-label="Import result filters">{#each stage === 'review' ? ['issues','pending'] : ['failed','pending','imported'] as state}<button class="button" class:primary={rowState === state} disabled={busy || running} onclick={() => changeRows(state)}>{state === 'failed' || state === 'issues' ? 'Needs attention' : state === 'pending' ? 'Pending / preview' : 'Saved records'}</button>{/each}</div>
        <section class="import-results" aria-label="Import row results">
          <div class="import-result-grid import-results-head" aria-hidden="true"><span>Source</span><span>Mapped values / issue</span><span>Action</span></div>
          <ul class="import-result-list">
            {#each result.rows as row (`${row.sheet}:${row.row}`)}
              <li class="import-result-grid import-result-row" data-import-row={row.row}>
                <div class="import-row-source"><strong>{job.sheets[row.sheet].name}</strong><small>Row {row.row}</small></div>
                <div class="import-row-issues">
                  {#if row.errors.length}
                    <ul class="import-row-errors">{#each row.errors as issue}<li><strong>{issue.field}{issue.column ? ` · ${columnName(issue.column)}` : ''}:</strong> {issue.message}{issue.value ? ` (source: ${issue.value})` : ''}</li>{/each}</ul>
                  {/if}
                  <details>
                    <summary>{Object.entries(row.values).slice(0,2).map(([key,value]) => `${fields.find(f => f.key === key)?.label || key}: ${String(value ?? 'Empty')}`).join(' · ') || 'Inspect source values'}</summary>
                    <strong>Mapped values (including available auto-fill)</strong>
                    <dl class="mapped-values">{#each Object.entries(row.preview_values || row.values) as [key,value]}<dt>{fields.find(f => f.key === key)?.label || key}</dt><dd>{typeof value === "object" && value !== null ? JSON.stringify(value) : String(value ?? "Empty")}</dd>{/each}</dl>
                    <strong>Original source cells</strong>
                    <dl>{#each Object.entries(row.cells) as [col,cell]}<dt>{columnName(Number(col))}</dt><dd>{cell.text}</dd>{/each}</dl>
                  </details>
                </div>
                <div class="import-row-action">
                  {#if rowState === 'failed'}<button class="button" disabled={busy || running} onclick={() => correction = row}>Correct and retry</button>
                  {:else if row.record_uid}<a class="button" href={`#module/${encodeURIComponent(moduleUid || 'mx-default-records')}?record=${encodeURIComponent(row.record_uid)}`} onclick={close}>Open record</a>
                  {:else}<small>{rowState === 'issues' ? 'Fix the mapping or field configuration, then validate again.' : 'Not saved yet'}</small>{/if}
                </div>
              </li>
            {:else}<li class="import-results-empty">No rows in this view.</li>{/each}
          </ul>
        </section>
        <div class="pagination"><span>{result.total.toLocaleString()} rows · Page {rowPage} of {Math.max(1,Math.ceil(result.total / result.limit))}</span><button class="button" disabled={rowPage <= 1 || running} onclick={async () => { rowPage--; try { await loadRows(); } catch (e) { error = message(e); } }}>Previous</button><button class="button" disabled={rowPage * result.limit >= result.total || running} onclick={async () => { rowPage++; try { await loadRows(); } catch (e) { error = message(e); } }}>Next</button></div>
        <details><summary>Import log storage</summary><p class="help">MX keeps source values so skipped rows can be inspected later. Remove this log when it is no longer needed; imported records remain.</p><button class="button danger" disabled={busy || running} onclick={forget}>Remove import log</button></details>
      {:else if stage === 'history'}
        <p class="help">Your imports for this module. Reopen a job to resume pending rows or correct skipped rows, including after an MX restart.</p>
        <div class="history-list">{#each jobs as item}<button onclick={() => openJob(item)}><strong>{item.filename}</strong><small>{new Date(item.created_at * 1000).toLocaleString()} · {item.state === 'draft' ? 'Awaiting mapping' : `${item.imported} saved · ${item.failed} need attention · ${item.pending} pending`}</small></button>{:else}<p>No imports yet.</p>{/each}</div>
        <div class="pagination"><button class="button" disabled={busy || historyPage <= 1} onclick={() => { historyPage--; void loadHistory(); }}>Previous</button><span>Page {historyPage} of {Math.max(1,Math.ceil(historyTotal / 20))}</span><button class="button" disabled={busy || historyPage * 20 >= historyTotal} onclick={() => { historyPage++; void loadHistory(); }}>Next</button><button class="button primary" disabled={busy} onclick={() => { stage = 'file'; job = null; file = null; uploadUid = operationUid(); }}>New import</button></div>
      {/if}
    </div>
  </div>
</div>
{#if correction && job}<RecordEditor {schema} {moduleUid} record={null} initialChanges={correction.values} draftScope={`import:${job.uid}:${correction.sheet}:${correction.row}`} canWrite={true} canDelete={false} canAttachments={false} recordLabel={`import row ${correction.row}`} submitCreate={saveCorrection} onClose={() => correction = null} onSaved={corrected} />{/if}

<style>
  .import-backdrop { position: fixed; inset: 0; z-index: 70; display: grid; place-items: center; grid-template-columns: minmax(0,1fr); padding: 24px; background: #0008; }
  .import-dialog { width: min(1100px,100%); min-width: 0; max-height: calc(100dvh - 48px); display: flex; flex-direction: column; background: var(--surface); color: var(--text); border: 1px solid var(--line); border-radius: 12px; box-shadow: 0 20px 60px #0005; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 20px 24px; border-bottom: 1px solid var(--line); } h2 { font-size: 20px; margin: 0 0 4px; } h3 { font-size: 16px; margin: 0 0 12px; } header p { margin: 0; color: var(--muted); overflow-wrap: anywhere; font-size: 13px; }
  .import-body { padding: 24px; overflow: auto; min-height: 0; min-width: 0; } .import-body > * + * { margin-top: 16px; }
  label:not(.sheet-choice):not(.check) { display: grid; gap: 6px; font-size: 13px; } input:not([type=checkbox]),select { min-height: 38px; width: 100%; min-width: 0; } .import-intro { text-align: center; max-width: 650px; margin: 0 auto 24px; } .import-intro :global(svg) { margin-bottom: 12px; } .help,small { color: var(--muted); font-size: 12px; line-height: 1.6; } small { display: block; margin-top: 4px; overflow-wrap: anywhere; } .help { margin-bottom: 0; }
  .actions,.counts,.result-tabs,.pagination { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; } .actions { justify-content: flex-end; } .counts { font-size: 13px; } .counts strong { margin-right: auto; } .pagination span { margin-right: auto; font-size: 12px; color: var(--muted); }
  .sheets { display: flex; flex-wrap: wrap; gap: 8px; } .sheet-choice { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border: 1px solid var(--line); border-radius: 8px; } .sheet-choice button { text-align: left; padding: 6px; border: 0; background: transparent; color: var(--text); cursor: pointer; font-size: 12px; } .sheet-choice button.active { color: var(--primary); text-decoration: underline; text-underline-offset: 4px; } .row-options { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); max-width: 376px; gap: 16px; }
  .import-table { overflow-x: auto; border: 1px solid var(--line); border-radius: 8px; } table { width: 100%; border-collapse: collapse; table-layout: fixed; font-size: 13px; } th,td { padding: 12px; text-align: left; vertical-align: top; border-bottom: 1px solid var(--line); overflow-wrap: anywhere; } th { color: var(--muted); font-size: 12px; background: var(--surface-2); } td:first-child { width: 28%; } .check { display: flex; align-items: center; gap: 8px; margin-top: 8px; font-size: 12px; } progress { display: block; width: 100%; height: 12px; accent-color: var(--primary); } ul { margin: 0 0 8px; padding-left: 16px; } li + li { margin-top: 8px; } dl { display: grid; grid-template-columns: 35px 1fr; gap: 6px; margin: 12px 0; } .mapped-values { grid-template-columns: minmax(90px,1fr) minmax(0,2fr); } dt { color: var(--muted); } dd { margin: 0; white-space: pre-wrap; } .source-preview { max-height: 220px; overflow: auto; font-size: 12px; overflow-wrap: anywhere; } details summary { cursor: pointer; font-size: 12px; }
  .history-list { display: grid; gap: 8px; } .history-list > button { display: block; width: 100%; padding: 14px; text-align: left; background: var(--surface-2); color: var(--text); border: 1px solid var(--line); border-radius: 8px; cursor: pointer; overflow-wrap: anywhere; } .notice { white-space: pre-wrap; }
  .import-warning,.import-info{padding:14px 16px;border:1px solid var(--line);border-radius:8px;background:var(--surface-2);font-size:13px;line-height:1.6;overflow-wrap:anywhere}.import-warning{border-left:3px solid #c58a24}.import-warning p,.import-info p{margin:8px 0 0}.import-warning ul{margin:8px 0 0}
  .import-results{border:1px solid var(--line);border-radius:8px;overflow:hidden;font-size:13px;line-height:1.6;min-width:0}
  .import-result-grid{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,2.2fr) 9rem;gap:16px;align-items:start;padding:12px}
  .import-result-grid>div{min-width:0;white-space:normal;overflow-wrap:anywhere}
  .import-results-head{background:var(--surface-2);color:var(--muted);font-size:12px;font-weight:600}
  .import-result-list{margin:0;padding:0;list-style:none}.import-result-list>.import-result-row{margin:0;border-top:1px solid var(--line)}
  .import-row-errors{margin:0 0 8px;padding-left:16px;white-space:normal;overflow-wrap:anywhere}
  .import-row-action{display:flex;justify-content:flex-end}.import-row-action .button{max-width:100%;white-space:normal;overflow-wrap:anywhere;text-align:center}
  .import-results-empty{padding:16px}.import-row-issues details,.import-row-issues summary,.import-row-issues dt,.import-row-issues dd{min-width:0;overflow-wrap:anywhere;white-space:normal}
  @media(max-width:700px){.import-results-head{display:none}.import-result-row{grid-template-columns:minmax(0,1fr);gap:10px;padding:14px}.import-row-source{display:flex;gap:12px;flex-wrap:wrap;align-items:baseline;justify-content:space-between}.import-row-source small{margin:0}.import-row-action{justify-content:flex-start}.import-row-action .button{min-height:38px}.mapped-values{grid-template-columns:minmax(0,1fr) minmax(0,2fr)}}
  @media(max-width:700px) { .import-backdrop { padding: 8px; } .import-dialog { max-height: calc(100dvh - 16px); } header,.import-body { padding: 16px; } .import-table table { min-width: 680px; } .actions .button { flex: 1; } }
</style>
