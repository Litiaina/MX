<script lang="ts">
  import { onMount } from 'svelte';
  import type { DashboardWidget, FieldDefinition } from '../../api/domain';
  import { loadDashboardConfig, loadSchema, saveDashboardConfig } from '../../api/workspace';

  let fields = $state<FieldDefinition[]>([]); let widgets = $state<DashboardWidget[]>([]); let showPerformance = $state(false); let revision = $state(0);
  let loading = $state(true); let error = $state(''); let notice = $state('');
  let title = $state(''); let display = $state<DashboardWidget['display_mode']>('metric'); let measure = $state('all');
  let group = $state(''); let actionField = $state(''); let actionValue = $state(''); let attachmentField = $state(''); let dateField = $state(''); let bucket = $state<NonNullable<DashboardWidget['time_bucket']>>('month'); let chartValue = $state<NonNullable<DashboardWidget['chart_value']>>('matched'); let limit = $state(8); let legend = $state(true); let editing = $state<number | null>(null);
  const normalFields = $derived(fields.filter((field) => field.active && field.field_type !== 'attachments'));
  const attachmentFields = $derived(fields.filter((field) => field.active && field.field_type === 'attachments'));

  onMount(() => void refresh());
  async function refresh() { loading = true; try { const [schema, config] = await Promise.all([loadSchema(true), loadDashboardConfig()]); fields = schema.fields; widgets = structuredClone(config.config.widgets || []); showPerformance = config.config.show_user_performance === true; revision = config.revision; } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function fieldName(uid: string | null) { return fields.find((field) => field.uid === uid)?.label || 'the selected field'; }
  function displayName(mode: DashboardWidget['display_mode']) { return ({ metric: 'Summary number', bar: 'Bar chart', line: 'Timeline', pie: 'Pie chart', donut: 'Donut chart', progress: 'Grouped progress', table: 'Detailed table' } as Record<string, string>)[mode] || mode; }
  function ruleText(item: Pick<DashboardWidget, 'kind' | 'action_mode' | 'action_field_uid' | 'action_value' | 'attachment_field_uid'>) {
    if (item.kind === 'attachment_presence') return `${fieldName(item.attachment_field_uid)} has one or more files`;
    if (item.action_mode === 'all') return 'Every record';
    const name = fieldName(item.action_field_uid);
    if (item.action_mode === 'nonempty') return `${name} is not empty`;
    if (item.action_mode === 'empty') return `${name} is empty`;
    if (item.action_mode === 'not_equals') return `${name} does not equal “${item.action_value || ''}”`;
    return `${name} equals “${item.action_value || ''}”`;
  }
  function draftRule() { return ruleText({ kind: measure === 'attachment' ? 'attachment_presence' : 'action_rate', action_mode: measure === 'attachment' ? null : measure as DashboardWidget['action_mode'], action_field_uid: actionField || null, action_value: actionValue || null, attachment_field_uid: attachmentField || null }); }
  function draftDescription() {
    let organization = 'as one overall number';
    if (display === 'line') organization = `over time by ${bucket} using ${fieldName(dateField)}`;
    else if (display !== 'metric') organization = `grouped by ${fieldName(group)}`;
    const dateUse = display === 'line' ? '' : dateField ? ` Reporting periods use ${fieldName(dateField)}.` : ' Reporting periods will not limit this widget.';
    return `Count when ${draftRule()}, ${organization}.${dateUse}`;
  }
  function reset() { title = ''; display = 'metric'; measure = 'all'; group = ''; actionField = ''; actionValue = ''; attachmentField = ''; dateField = ''; bucket = 'month'; chartValue = 'matched'; limit = 8; legend = true; editing = null; }
  function add(event: SubmitEvent) {
    event.preventDefault(); error = '';
    const attachmentMode = measure === 'attachment'; const fieldMode = !attachmentMode && measure !== 'all'; const line = display === 'line'; const grouped = ['bar', 'pie', 'donut', 'progress', 'table'].includes(display);
    if (!title.trim()) return fail(new Error('Give the widget a title.'));
    if (attachmentMode && !attachmentField) return fail(new Error('Choose an attachment field.'));
    if (fieldMode && !actionField) return fail(new Error('Choose the field to test.'));
    if (['equals', 'not_equals'].includes(measure) && !actionValue.trim()) return fail(new Error('Enter the comparison value.'));
    if (grouped && !group) return fail(new Error('Choose a grouping field.'));
    if (line && !dateField) return fail(new Error('Choose a date field for the timeline.'));
    const prior = editing == null ? null : widgets[editing];
    const widget: DashboardWidget = {
      uid: prior?.uid || crypto.randomUUID(), kind: attachmentMode ? 'attachment_presence' : 'action_rate', display_mode: display, title: title.trim(),
      group_field_uid: display === 'metric' || line ? null : group || null, action_field_uid: fieldMode ? actionField : null,
      action_mode: attachmentMode ? null : measure as DashboardWidget['action_mode'], action_value: ['equals', 'not_equals'].includes(measure) ? actionValue.trim() : null,
      attachment_field_uid: attachmentMode ? attachmentField : null, date_field_uid: dateField || null, time_bucket: line ? bucket : null,
      chart_value: ['bar', 'line', 'pie', 'donut'].includes(display) ? chartValue : null, category_limit: ['bar', 'pie', 'donut'].includes(display) ? limit : null,
      show_legend: ['bar', 'line', 'pie', 'donut'].includes(display) ? legend : null, definition: draftDescription()
    };
    if (editing == null) widgets = [...widgets, widget]; else widgets[editing] = widget;
    reset(); notice = 'Widget staged. Save dashboard to publish it.';
  }
  function edit(index: number) { const item = widgets[index]; editing = index; title = item.title; display = item.display_mode; group = item.group_field_uid || ''; actionField = item.action_field_uid || ''; actionValue = item.action_value || ''; attachmentField = item.attachment_field_uid || ''; dateField = item.date_field_uid || ''; bucket = item.time_bucket || 'month'; chartValue = item.chart_value || 'matched'; limit = item.category_limit ?? 8; legend = item.show_legend !== false; measure = item.kind === 'attachment_presence' ? 'attachment' : item.action_mode || 'all'; }
  function move(index: number, delta: number) { const target = index + delta; if (target < 0 || target >= widgets.length) return; const copy = [...widgets]; [copy[index], copy[target]] = [copy[target], copy[index]]; widgets = copy; }
  async function save() { loading = true; try { const result = await saveDashboardConfig(widgets, showPerformance); revision = result.revision; notice = 'Dashboard published.'; error = ''; } catch (reason) { fail(reason); } finally { loading = false; } }
</script>

{#if notice}<div class="notice success">{notice}</div>{/if}{#if error}<div class="notice error">{error}</div>{/if}
<div class="admin-grid dashboard-admin">
  <section class="panel"><h2>{editing == null ? 'Add widget' : 'Edit widget'}</h2><form class="form-stack" onsubmit={add}><label>Title<input bind:value={title} required /></label><label>Display<select bind:value={display}><option value="metric">Summary number</option><option value="bar">Bar chart</option><option value="line">Timeline</option><option value="pie">Pie-style comparison</option><option value="donut">Donut-style comparison</option><option value="progress">Grouped progress</option><option value="table">Detailed table</option></select></label><label>Measure<select bind:value={measure}><option value="all">All records</option><option value="nonempty">Field is not empty</option><option value="empty">Field is empty</option><option value="equals">Field equals value</option><option value="not_equals">Field does not equal value</option><option value="attachment">Has attachment</option></select></label>
    {#if measure === 'attachment'}<label>Attachment field<select bind:value={attachmentField} required><option value="">Select…</option>{#each attachmentFields as field}<option value={field.uid}>{field.label}</option>{/each}</select></label>{:else if measure !== 'all'}<label>Field<select bind:value={actionField} required><option value="">Select…</option>{#each normalFields as field}<option value={field.uid}>{field.label}</option>{/each}</select></label>{#if ['equals', 'not_equals'].includes(measure)}<label>Value<input bind:value={actionValue} required /></label>{/if}{/if}
    {#if !['metric', 'line'].includes(display)}<label>Group by<select bind:value={group} required><option value="">Select…</option>{#each normalFields as field}<option value={field.uid}>{field.label}</option>{/each}</select></label>{/if}
    <label>Date filter field<select bind:value={dateField} required={display === 'line'}><option value="">None</option>{#each normalFields.filter((field) => field.field_type === 'date') as field}<option value={field.uid}>{field.label}</option>{/each}</select></label>
    {#if display === 'line'}<label>Time bucket<select bind:value={bucket}><option value="day">Day</option><option value="week">Week</option><option value="month">Month</option><option value="quarter">Quarter</option><option value="year">Year</option></select></label>{/if}
    {#if ['bar', 'line', 'pie', 'donut'].includes(display)}<label>Chart value<select bind:value={chartValue}><option value="matched">Matched records</option><option value="total">Total records</option>{#if !['pie', 'donut'].includes(display)}<option value="rate">Match rate</option><option value="breakdown">Matched / pending</option>{/if}</select></label><label class="checkbox"><input type="checkbox" bind:checked={legend} /> Show legend</label>{/if}
    {#if ['bar', 'pie', 'donut'].includes(display)}<label>Category limit<input type="number" min="0" bind:value={limit} /></label>{/if}
    <aside class="dashboard-rule-preview"><span>What this widget will do</span><strong>{displayName(display)}</strong><p>{draftDescription()}</p></aside><div class="button-row"><button class="button primary">{editing == null ? 'Add widget' : 'Update widget'}</button>{#if editing != null}<button class="button" type="button" onclick={reset}>Cancel</button>{/if}</div></form></section>
  <section class="panel admin-span"><div class="panel-heading"><div><h2>Published dashboard</h2><p class="muted">Revision {revision}. Changes remain staged until saved.</p></div><button class="button primary" onclick={save} disabled={loading}>Save dashboard</button></div><label class="checkbox performance-toggle"><input type="checkbox" bind:checked={showPerformance} /> Show administrator user-performance report</label>
    <div class="widget-list">{#if !widgets.length}<div class="empty-state compact"><p>No widgets configured.</p></div>{/if}{#each widgets as widget, index}<article class="widget-row detailed"><div><div class="widget-title-line"><strong>{widget.title}</strong><span>{displayName(widget.display_mode)}</span></div><small>{widget.display_mode === 'line' ? `Timeline by ${widget.time_bucket || 'month'}` : widget.group_field_uid ? `Grouped by ${fieldName(widget.group_field_uid)}` : 'Overall result'} · {widget.date_field_uid ? `Period uses ${fieldName(widget.date_field_uid)}` : 'No period filter'}</small><p><b>Matched when:</b> {ruleText(widget)}</p></div><div class="inline-actions"><button class="button small" aria-label="Move up" onclick={() => move(index, -1)} disabled={index === 0}>↑</button><button class="button small" aria-label="Move down" onclick={() => move(index, 1)} disabled={index === widgets.length - 1}>↓</button><button class="button small" onclick={() => edit(index)}>Edit</button><button class="button small danger" onclick={() => widgets = widgets.filter((_, item) => item !== index)}>Remove</button></div></article>{/each}</div>
  </section>
</div>
