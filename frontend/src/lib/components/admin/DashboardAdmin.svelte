<script lang="ts">
  import { onMount } from 'svelte';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import ChartNoAxesCombined from '@lucide/svelte/icons/chart-no-axes-combined';
  import Database from '@lucide/svelte/icons/database';
  import Eye from '@lucide/svelte/icons/eye';
  import Filter from '@lucide/svelte/icons/filter';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Target from '@lucide/svelte/icons/target';
  import type { DashboardFilter, DashboardWidget, FieldDefinition, ModuleDefinition } from '../../api/domain';
  import { loadDashboardConfig, loadModuleSchema, loadModules, saveDashboardConfig } from '../../api/workspace';
  import { requestConfirmation } from '../../confirmation';
  import { cloneJson } from '../../util/json';

  type Measurement = 'all' | 'attachment_present' | 'attachment_missing' | 'nonempty' | 'empty' | 'equals' | 'not_equals';
  type ValueMode = 'answer' | 'total' | 'rate' | 'breakdown';

  let fields = $state<FieldDefinition[]>([]);
  let widgets = $state<DashboardWidget[]>([]);
  let showPerformance = $state(false);
  let revision = $state(0);
  let loading = $state(true);
  let error = $state('');
  let notice = $state('');
  let modules = $state<ModuleDefinition[]>([]);
  let moduleUid = $state('mx-default-records');

  let title = $state('');
  let display = $state<DashboardWidget['display_mode']>('metric');
  let measure = $state<Measurement>('all');
  let group = $state('');
  let secondaryGroup = $state('');
  let actionField = $state('');
  let actionValue = $state('');
  let attachmentField = $state('');
  let dateField = $state('');
  let bucket = $state<NonNullable<DashboardWidget['time_bucket']>>('month');
  let valueMode = $state<ValueMode>('answer');
  let limit = $state(8);
  let legend = $state(true);
  let editing = $state<number | null>(null);
  let advancedOpen = $state(false);
  let filtersOpen = $state(false);

  let filters = $state<DashboardFilter[]>([]);
  let filterField = $state('');
  let filterOperator = $state<DashboardFilter['operator']>('equals');
  let filterValue = $state('');

  const normalFields = $derived(fields.filter((field) => field.active && !['attachments', 'relationship', 'lookup', 'rollup'].includes(field.field_type)));
  const attachmentFields = $derived(fields.filter((field) => field.active && field.field_type === 'attachments'));

  onMount(() => void refresh());

  async function refresh() {
    loading = true;
    try {
      const [moduleData, config] = await Promise.all([loadModules(), loadDashboardConfig()]);
      modules = moduleData.modules;
      if (!modules.some((module) => module.uid === moduleUid)) moduleUid = modules[0]?.uid || 'mx-default-records';
      fields = (await loadModuleSchema(moduleUid)).fields;
      widgets = cloneJson(config.config.widgets || []);
      showPerformance = config.config.show_user_performance === true;
      revision = config.revision;
    } catch (reason) {
      fail(reason);
    } finally {
      loading = false;
    }
  }

  async function changeModule() {
    try {
      fields = (await loadModuleSchema(moduleUid)).fields;
      group = '';
      secondaryGroup = '';
      actionField = '';
      attachmentField = '';
      dateField = '';
      filters = [];
      filterField = '';
    } catch (reason) {
      fail(reason);
    }
  }

  function fail(reason: unknown) {
    error = reason instanceof Error ? reason.message : 'The operation failed.';
    notice = '';
  }

  function fieldName(uid: string | null | undefined) {
    return fields.find((field) => field.uid === uid)?.label || 'the selected field';
  }

  function moduleName(uid: string | null | undefined) {
    return modules.find((module) => module.uid === (uid || 'mx-default-records'))?.name || 'Module';
  }

  function displayName(mode: DashboardWidget['display_mode']) {
    return ({ metric: 'Summary number', bar: 'Bar chart', line: 'Timeline', pie: 'Pie chart', donut: 'Donut chart', progress: 'Progress by group', table: 'Detailed table' } as Record<string, string>)[mode] || mode;
  }

  function isAttachmentMeasurement(value: Measurement = measure) {
    return value === 'attachment_present' || value === 'attachment_missing';
  }

  function isGroupedDisplay(mode: DashboardWidget['display_mode'] = display) {
    return ['bar', 'pie', 'donut', 'progress', 'table'].includes(mode);
  }

  function measurementChanged() {
    if (isAttachmentMeasurement()) {
      actionField = '';
      actionValue = '';
    } else {
      attachmentField = '';
      if (measure === 'all') {
        actionField = '';
        actionValue = '';
      } else if (!['equals', 'not_equals'].includes(measure)) actionValue = '';
    }
    valueMode = 'answer';
  }

  function displayChanged() {
    if (['pie', 'donut'].includes(display) && ['rate', 'breakdown'].includes(valueMode)) valueMode = 'answer';
    if (display === 'metric' || display === 'line') {
      group = '';
      secondaryGroup = '';
    }
  }

  function draftQuestion() {
    if (measure === 'all') return 'all records';
    if (measure === 'attachment_present') return `records with a file in ${fieldName(attachmentField)}`;
    if (measure === 'attachment_missing') return `records missing a file in ${fieldName(attachmentField)}`;
    const name = fieldName(actionField);
    if (measure === 'nonempty') return `records where ${name} is filled in`;
    if (measure === 'empty') return `records where ${name} is empty`;
    if (measure === 'not_equals') return `records where ${name} does not equal “${actionValue || '…'}”`;
    return `records where ${name} equals “${actionValue || '…'}”`;
  }

  function widgetQuestion(item: DashboardWidget) {
    if (item.kind === 'attachment_presence') {
      return item.result_focus === 'pending' || item.chart_value === 'pending'
        ? `${fieldName(item.attachment_field_uid)} is missing a file`
        : `${fieldName(item.attachment_field_uid)} has a file`;
    }
    if (item.action_mode === 'all') return 'All records';
    const name = fieldName(item.action_field_uid);
    if (item.action_mode === 'nonempty') return `${name} is filled in`;
    if (item.action_mode === 'empty') return `${name} is empty`;
    if (item.action_mode === 'not_equals') return `${name} does not equal “${item.action_value || ''}”`;
    return `${name} equals “${item.action_value || ''}”`;
  }

  function operatorLabel(operator: DashboardFilter['operator']) {
    return ({ equals: 'equals', not_equals: 'does not equal', contains: 'contains', empty: 'is empty', nonempty: 'is filled in', has: 'has a file', missing: 'has no file' } as Record<string, string>)[operator] || operator;
  }

  function filterText(filter: DashboardFilter) {
    const name = fieldName(filter.field_uid || filter.attachment_field_uid);
    return `${name} ${operatorLabel(filter.operator)}${filter.value ? ` “${filter.value}”` : ''}`;
  }

  function draftDescription() {
    let statement = `Count ${draftQuestion()}`;
    if (display === 'line') statement += ` over time by ${bucket}, using ${fieldName(dateField)}`;
    else if (isGroupedDisplay()) {
      statement += `, broken down by ${fieldName(group)}`;
      if (secondaryGroup) statement += ` and then ${fieldName(secondaryGroup)}`;
    } else statement += ' as one overall number';
    if (display !== 'line') statement += dateField ? `, using ${fieldName(dateField)} for the dashboard period` : ', without applying the dashboard period';
    if (filters.length) statement += `. Only include records where ${filters.map(filterText).join(' and ')}`;
    return `${statement}.`;
  }

  function storedChartValue(): NonNullable<DashboardWidget['chart_value']> {
    if (valueMode === 'answer') return measure === 'attachment_missing' ? 'pending' : 'matched';
    return valueMode;
  }

  function reset() {
    title = '';
    display = 'metric';
    measure = 'all';
    group = '';
    secondaryGroup = '';
    actionField = '';
    actionValue = '';
    attachmentField = '';
    dateField = '';
    filters = [];
    filterField = '';
    filterValue = '';
    filterOperator = 'equals';
    bucket = 'month';
    valueMode = 'answer';
    limit = 8;
    legend = true;
    editing = null;
    advancedOpen = false;
    filtersOpen = false;
  }

  function syncFilterField() {
    const field = fields.find((item) => item.uid === filterField);
    filterOperator = field?.field_type === 'attachments' ? 'has' : 'equals';
    filterValue = '';
  }

  function addContextFilter() {
    const field = fields.find((item) => item.uid === filterField);
    if (!field) return;
    const attachment = field.field_type === 'attachments';
    const operator = attachment ? (filterOperator === 'missing' ? 'missing' : 'has') : filterOperator;
    if (!attachment && !['empty', 'nonempty'].includes(operator) && !filterValue.trim()) {
      error = 'Enter the value for this filter.';
      return;
    }
    filters = [...filters, attachment
      ? { attachment_field_uid: field.uid, operator }
      : { field_uid: field.uid, operator, value: ['empty', 'nonempty'].includes(operator) ? null : filterValue.trim() }];
    filterField = '';
    filterValue = '';
    filterOperator = 'equals';
    error = '';
  }

  function add(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    const attachmentMode = isAttachmentMeasurement();
    const fieldMode = !attachmentMode && measure !== 'all';
    const line = display === 'line';
    const grouped = isGroupedDisplay();
    if (!title.trim()) return fail(new Error('Give this widget a clear title.'));
    if (attachmentMode && !attachmentField) return fail(new Error('Choose the file attachment field to inspect.'));
    if (fieldMode && !actionField) return fail(new Error('Choose the field used by this question.'));
    if (['equals', 'not_equals'].includes(measure) && !actionValue.trim()) return fail(new Error('Enter the value to compare.'));
    if (grouped && !group) return fail(new Error('Choose how the answer should be broken down.'));
    if (line && !dateField) return fail(new Error('Choose the date field used by the timeline.'));

    const prior = editing == null ? null : widgets[editing];
    const widget: DashboardWidget = {
      uid: prior?.uid || crypto.randomUUID(),
      kind: attachmentMode ? 'attachment_presence' : 'action_rate',
      display_mode: display,
      title: title.trim(),
      module_uid: moduleUid,
      group_field_uid: grouped ? group || null : null,
      secondary_group_field_uid: grouped ? secondaryGroup || null : null,
      action_field_uid: fieldMode ? actionField : null,
      action_mode: attachmentMode ? null : measure as DashboardWidget['action_mode'],
      action_value: ['equals', 'not_equals'].includes(measure) ? actionValue.trim() : null,
      attachment_field_uid: attachmentMode ? attachmentField : null,
      date_field_uid: dateField || null,
      time_bucket: line ? bucket : null,
      chart_value: storedChartValue(),
      result_focus: measure === 'attachment_missing' ? 'pending' : 'matched',
      category_limit: ['bar', 'pie', 'donut'].includes(display) ? limit : null,
      show_legend: ['bar', 'line', 'pie', 'donut'].includes(display) ? legend : null,
      definition: draftDescription(),
      filters
    };
    if (editing == null) widgets = [...widgets, widget];
    else widgets[editing] = widget;
    reset();
    notice = 'Widget staged. Publish the dashboard when the list is ready.';
  }

  async function edit(index: number) {
    const item = widgets[index];
    editing = index;
    moduleUid = item.module_uid || 'mx-default-records';
    fields = (await loadModuleSchema(moduleUid)).fields;
    title = item.title;
    display = item.display_mode;
    group = item.group_field_uid || '';
    secondaryGroup = item.secondary_group_field_uid || '';
    actionField = item.action_field_uid || '';
    actionValue = item.action_value || '';
    attachmentField = item.attachment_field_uid || '';
    dateField = item.date_field_uid || '';
    filters = cloneJson(item.filters || []);
    bucket = item.time_bucket || 'month';
    const savedValue = item.chart_value || 'matched';
    valueMode = savedValue === 'matched' || savedValue === 'pending' ? 'answer' : savedValue;
    limit = item.category_limit ?? 8;
    legend = item.show_legend !== false;
    measure = item.kind === 'attachment_presence'
      ? (item.result_focus === 'pending' || savedValue === 'pending' ? 'attachment_missing' : 'attachment_present')
      : (item.action_mode || 'all') as Measurement;
    advancedOpen = Boolean(item.secondary_group_field_uid || valueMode !== 'answer' || item.category_limit != null || item.show_legend === false);
    filtersOpen = filters.length > 0;
    notice = '';
    error = '';
  }

  function move(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= widgets.length) return;
    const copy = [...widgets];
    [copy[index], copy[target]] = [copy[target], copy[index]];
    widgets = copy;
  }

  async function removeWidget(index: number) {
    const widget = widgets[index];
    if (!await requestConfirmation({
      title: `Remove “${widget.title}”?`,
      description: 'The widget will be removed from the staged dashboard. The published dashboard changes only after you publish.',
      confirmLabel: 'Remove widget'
    })) return;
    widgets = widgets.filter((_, item) => item !== index);
    if (editing === index) reset();
    else if (editing != null && editing > index) editing -= 1;
    notice = 'Widget removed from the staged dashboard.';
  }

  async function save() {
    loading = true;
    try {
      const result = await saveDashboardConfig(widgets, showPerformance);
      revision = result.revision;
      notice = 'Dashboard published.';
      error = '';
    } catch (reason) {
      fail(reason);
    } finally {
      loading = false;
    }
  }
</script>

{#if notice}<div class="notice success dashboard-builder-notice">{notice}</div>{/if}
{#if error}<div class="notice error dashboard-builder-notice">{error}</div>{/if}

<div class="dashboard-designer">
  <section class="panel report-builder-panel">
    <header class="report-builder-heading">
      <div><p class="eyebrow">Report builder</p><h2>{editing == null ? 'Create a dashboard question' : 'Edit dashboard question'}</h2><p>Describe the answer you need. MX will build the underlying report.</p></div>
      {#if editing != null}<span>Editing widget {editing + 1}</span>{/if}
    </header>

    <form class="report-builder-form" onsubmit={add}>
      <section class="builder-step">
        <header><span>1</span><div><strong>Choose the data</strong><small>Select the module and give the result a recognizable title.</small></div><Database size={18} /></header>
        <div class="builder-field-grid">
          <label>Module<select bind:value={moduleUid} onchange={changeModule}>{#each modules.filter((module) => module.active) as module}<option value={module.uid}>{module.name}</option>{/each}</select><small>Only fields from this module can be used.</small></label>
          <label>Widget title<input bind:value={title} required placeholder="e.g. Missing action files by office" /><small>This appears as the heading on the dashboard.</small></label>
        </div>
      </section>

      <section class="builder-step">
        <header><span>2</span><div><strong>Ask the question</strong><small>Choose exactly which records should count as the answer.</small></div><Target size={18} /></header>
        <label class="builder-primary-field">What do you want to count?<select bind:value={measure} onchange={measurementChanged}><option value="all">All records</option><option value="attachment_present">Records with a file attachment</option><option value="attachment_missing">Records missing a file attachment</option><option value="nonempty">Records where a field is filled in</option><option value="empty">Records where a field is empty</option><option value="equals">Records where a field equals a value</option><option value="not_equals">Records where a field does not equal a value</option></select></label>
        {#if isAttachmentMeasurement()}
          <label class="builder-primary-field">Which file attachment field?<select bind:value={attachmentField} required><option value="">Choose an attachment field…</option>{#each attachmentFields as field}<option value={field.uid}>{field.label}</option>{/each}</select><small>MX checks this configured attachment field—not every file on the record.</small></label>
        {:else if measure !== 'all'}
          <div class="builder-field-grid">
            <label>Which field?<select bind:value={actionField} required><option value="">Choose a field…</option>{#each normalFields as field}<option value={field.uid}>{field.label}</option>{/each}</select></label>
            {#if ['equals', 'not_equals'].includes(measure)}<label>Compared with<input bind:value={actionValue} required placeholder="Value to match" /></label>{/if}
          </div>
        {/if}
      </section>

      <section class="builder-step">
        <header><span>3</span><div><strong>Organize the answer</strong><small>Choose the presentation and, when needed, how results are divided.</small></div><ChartNoAxesCombined size={18} /></header>
        <div class="builder-field-grid">
          <label>Show the answer as<select bind:value={display} onchange={displayChanged}><option value="metric">One summary number</option><option value="bar">Comparison bars</option><option value="progress">Progress by group</option><option value="table">Detailed result table</option><option value="line">Timeline</option><option value="pie">Pie comparison</option><option value="donut">Donut comparison</option></select><small>{displayName(display)}</small></label>
          {#if isGroupedDisplay()}<label>Break the results down by<select bind:value={group} required><option value="">Choose a field…</option>{#each normalFields as field}<option value={field.uid}>{field.label}</option>{/each}</select><small>Example: choose Office to get one result for each office.</small></label>{/if}
          {#if display === 'line'}<label>Group dates by<select bind:value={bucket}><option value="day">Day</option><option value="week">Week</option><option value="month">Month</option><option value="quarter">Quarter</option><option value="year">Year</option></select></label>{/if}
        </div>
      </section>

      <section class="builder-step">
        <header><span>4</span><div><strong>Connect the reporting period</strong><small>Tell MX which date the dashboard’s period selector should use.</small></div><CalendarDays size={18} /></header>
        <label class="builder-primary-field">Date used for reporting<select bind:value={dateField} required={display === 'line'}><option value="">Do not apply the dashboard period</option>{#each normalFields.filter((field) => field.field_type === 'date') as field}<option value={field.uid}>{field.label}</option>{/each}</select><small class:warning-text={!dateField}>{dateField ? `The dashboard period will filter this widget using ${fieldName(dateField)}.` : display === 'line' ? 'A timeline requires a date field.' : 'This widget will always use all dates, even when viewers select a dashboard period.'}</small></label>
      </section>

      <details class="builder-disclosure" bind:open={filtersOpen}>
        <summary><span><Filter size={17} /><strong>Narrow the records</strong><small>Optional filters applied before MX counts the answer.</small></span><b>{filters.length || 'Optional'}</b></summary>
        <div class="builder-disclosure-body">
          <div class="builder-filter-grid">
            <label>Field<select bind:value={filterField} onchange={syncFilterField}><option value="">Choose a field…</option>{#each fields.filter((field) => field.active && !['relationship', 'lookup', 'rollup'].includes(field.field_type)) as field}<option value={field.uid}>{field.label}{field.field_type === 'attachments' ? ' (files)' : ''}</option>{/each}</select></label>
            <label>Condition<select bind:value={filterOperator}>{#if fields.find((field) => field.uid === filterField)?.field_type === 'attachments'}<option value="has">Has a file</option><option value="missing">Has no file</option>{:else}<option value="equals">Equals</option><option value="not_equals">Does not equal</option><option value="contains">Contains</option><option value="nonempty">Is filled in</option><option value="empty">Is empty</option>{/if}</select></label>
            {#if !['empty', 'nonempty', 'has', 'missing'].includes(filterOperator)}<label>Value<input bind:value={filterValue} placeholder="Required value" /></label>{/if}
            <button class="button" type="button" onclick={addContextFilter} disabled={!filterField}>Add filter</button>
          </div>
          {#if filters.length}<ul class="builder-filter-list">{#each filters as filter, index}<li><span>{filterText(filter)}</span><button class="link-button danger-text" type="button" onclick={() => filters = filters.filter((_, item) => item !== index)}>Remove</button></li>{/each}</ul>{:else}<p class="builder-empty-note">No extra filters. Every record in the selected module can enter the calculation.</p>{/if}
        </div>
      </details>

      <details class="builder-disclosure" bind:open={advancedOpen}>
        <summary><span><Settings2 size={17} /><strong>Presentation options</strong><small>Optional controls for second-level breakdowns and chart display.</small></span><b>Advanced</b></summary>
        <div class="builder-disclosure-body builder-field-grid">
          {#if isGroupedDisplay()}<label>Optional second breakdown<select bind:value={secondaryGroup}><option value="">No second breakdown</option>{#each normalFields.filter((field) => field.uid !== group) as field}<option value={field.uid}>{field.label}</option>{/each}</select><small>Creates combined categories such as Office / Status.</small></label>{/if}
          {#if ['bar', 'line', 'pie', 'donut'].includes(display)}<label>Values shown<select bind:value={valueMode}><option value="answer">Records answering the question</option><option value="total">All records in scope</option>{#if !['pie', 'donut'].includes(display)}<option value="rate">Percentage answering the question</option><option value="breakdown">Answer and remainder together</option>{/if}</select></label><label class="checkbox builder-checkbox"><input type="checkbox" bind:checked={legend} /> Show chart legend</label>{/if}
          {#if ['bar', 'pie', 'donut'].includes(display)}<label>Maximum categories<input type="number" min="0" bind:value={limit} /><small>Use 0 to show every category.</small></label>{/if}
        </div>
      </details>

      <aside class="report-question-preview">
        <span><Eye size={18} /></span><div><strong>MX understands this question as</strong><p>{draftDescription()}</p></div>
      </aside>

      <div class="report-builder-actions"><button class="button" type="button" onclick={reset}>{editing == null ? 'Clear builder' : 'Cancel editing'}</button><button class="button primary">{editing == null ? 'Stage widget' : 'Update widget'}</button></div>
    </form>
  </section>

  <section class="panel dashboard-published-panel">
    <header class="dashboard-published-heading"><div><p class="eyebrow">Dashboard layout</p><h2>Staged widgets</h2><p>Arrange the list, then publish once. Viewers continue using revision {revision} until then.</p></div><button class="button primary" onclick={save} disabled={loading}>{loading ? 'Publishing…' : 'Publish dashboard'}</button></header>

    <label class="dashboard-performance-option"><input type="checkbox" bind:checked={showPerformance} /><span><strong>Administrator performance report</strong><small>Show audited user activity beneath the configured widgets.</small></span></label>

    <div class="dashboard-widget-list">
      {#if !widgets.length}<div class="empty-state compact"><strong>No dashboard questions yet</strong><p>Use the builder to stage the first widget.</p></div>{/if}
      {#each widgets as widget, index}
        <article class:editing-widget={editing === index} class="dashboard-widget-card">
          <span class="dashboard-widget-position">{index + 1}</span>
          <div class="dashboard-widget-copy"><div><strong>{widget.title}</strong><span>{displayName(widget.display_mode)}</span></div><p>{widget.definition || `Count records where ${widgetQuestion(widget)}.`}</p><small>{moduleName(widget.module_uid)}{widget.filters?.length ? ` · ${widget.filters.length} filter${widget.filters.length === 1 ? '' : 's'}` : ' · All records in scope'}{widget.date_field_uid ? ' · Uses dashboard period' : ' · All dates'}</small></div>
          <div class="dashboard-widget-actions"><button class="button small" aria-label={`Move ${widget.title} up`} title="Move up" onclick={() => move(index, -1)} disabled={index === 0}><ArrowUp size={15} /></button><button class="button small" aria-label={`Move ${widget.title} down`} title="Move down" onclick={() => move(index, 1)} disabled={index === widgets.length - 1}><ArrowDown size={15} /></button><button class="button small" onclick={() => edit(index)}>Edit</button><button class="button small danger" onclick={() => removeWidget(index)}>Remove</button></div>
        </article>
      {/each}
    </div>
  </section>
</div>
