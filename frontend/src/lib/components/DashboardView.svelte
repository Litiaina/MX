<script lang="ts">
  import { onMount } from 'svelte';
  import type { ActionRateRow, DashboardWidget, ModuleDefinition, PresenceResponse, UserPerformanceRow } from '../api/domain';
  import { actionRate, exportReport, loadDashboardConfig, loadModules, loadPresence, userPerformance } from '../api/workspace';
  import ReportChart from './ReportChart.svelte';

  let { isAdmin, revision = 0, dashboardLabel = 'Dashboard', recordSingular = 'Record', recordPlural = 'Records' }: { isAdmin: boolean; revision?: number; dashboardLabel?: string; recordSingular?: string; recordPlural?: string } = $props();
  let widgets = $state<DashboardWidget[]>([]);
  let modules = $state<ModuleDefinition[]>([]); let exportModuleUid = $state('mx-default-records');
  let widgetRows = $state<Record<string, ActionRateRow[]>>({});
  let performance = $state<UserPerformanceRow[]>([]);
  let presence = $state<PresenceResponse | null>(null);
  let showPerformance = $state(false);
  let period = $state('30d'); let dateFrom = $state(''); let dateTo = $state('');
  let loading = $state(true); let error = $state(''); let lastRevision = $state(0);
  const allRows = $derived(widgets.flatMap((widget) => widgetRows[widget.uid] || []));
  const reportTotal = $derived(allRows.reduce((sum, row) => sum + row.total, 0));
  const reportAnswered = $derived(widgets.reduce((sum, widget) => sum + totalAnswer(widget, widgetRows[widget.uid] || []), 0));

  onMount(() => { setPeriodDates(); void refresh(); });
  $effect(() => { if (revision !== lastRevision && !loading) { lastRevision = revision; void refresh(); } });

  function localDate(date: Date) { const year = date.getFullYear(); const month = String(date.getMonth() + 1).padStart(2, '0'); const day = String(date.getDate()).padStart(2, '0'); return `${year}-${month}-${day}`; }
  function setPeriodDates() {
    const now = new Date(); const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()); const from = new Date(today);
    if (period === 'all') { dateFrom = ''; dateTo = ''; return; }
    if (period === 'today') from.setDate(today.getDate());
    else if (period === '7d') from.setDate(today.getDate() - 6);
    else if (period === '30d') from.setDate(today.getDate() - 29);
    else if (period === '90d') from.setDate(today.getDate() - 89);
    else if (period === 'month') from.setDate(1);
    else if (period === 'year') { from.setMonth(0); from.setDate(1); }
    else return;
    dateFrom = localDate(from); dateTo = localDate(today);
  }
  async function changePeriod() { setPeriodDates(); if (period !== 'custom') await refresh(); }
  function periodLabel() { return ({ all: 'All records', today: 'Today', '7d': 'Last 7 days', '30d': 'Last 30 days', '90d': 'Last 90 days', month: 'This month', year: 'This year', custom: 'Custom range' } as Record<string, string>)[period] || 'Selected period'; }

  async function refresh() {
    loading = true; error = '';
    try {
      const [config, moduleData] = await Promise.all([loadDashboardConfig(), loadModules()]);
      modules = moduleData.modules.filter((module) => module.active);
      if (!modules.some((module) => module.uid === exportModuleUid)) exportModuleUid = modules[0]?.uid || 'mx-default-records';
      const readableModules = new Set(modules.map((module) => module.uid));
      widgets = (Array.isArray(config.config.widgets) ? config.config.widgets : []).filter((widget) => readableModules.has(widget.module_uid || 'mx-default-records'));
      showPerformance = config.config.show_user_performance === true && isAdmin;
      const reports = await Promise.all(widgets.map(async (widget) => [widget.uid, (await actionRate(widget, dateFrom, dateTo)).rows] as const));
      widgetRows = Object.fromEntries(reports);
      presence = await loadPresence();
      performance = showPerformance ? (await userPerformance(dateFrom, dateTo)).rows : [];
    } catch (reason) { error = reason instanceof Error ? reason.message : 'Dashboard data could not be loaded.'; }
    finally { loading = false; }
  }

  function total(rows: ActionRateRow[], key: 'total' | 'actioned' | 'pending') { return rows.reduce((sum, row) => sum + row[key], 0); }
  function focusesPending(widget: DashboardWidget) { return widget.result_focus === 'pending' || widget.chart_value === 'pending'; }
  function answerValue(widget: DashboardWidget, row: ActionRateRow) { return focusesPending(widget) ? row.pending : row.actioned; }
  function otherValue(widget: DashboardWidget, row: ActionRateRow) { return focusesPending(widget) ? row.actioned : row.pending; }
  function answerRate(widget: DashboardWidget, row: ActionRateRow) { return row.total ? answerValue(widget, row) * 100 / row.total : 0; }
  function totalAnswer(widget: DashboardWidget, rows: ActionRateRow[]) { return rows.reduce((sum, row) => sum + answerValue(widget, row), 0); }
  function metricValue(widget: DashboardWidget, rows: ActionRateRow[]) { return totalAnswer(widget, rows); }
  function resultLabel(widget: DashboardWidget) {
    if (widget.kind === 'attachment_presence') return focusesPending(widget) ? 'without the configured attachment' : 'with the configured attachment';
    return focusesPending(widget) ? 'not meeting the configured rule' : 'meeting the configured rule';
  }
  function time(value: number | null) { return value ? new Date(value).toLocaleString() : 'No audited activity in this period'; }
</script>

<section class="workspace-page dashboard-page">
  <section class="dashboard-hero">
    <div>
      <p class="hero-kicker">Live reporting</p>
      <h1>{dashboardLabel}</h1>
      <p>Track the measures configured for this workspace across the selected reporting period.</p>
    </div>
  </section>

  <div class="filter-strip"><label>Reporting period<select bind:value={period} onchange={changePeriod}><option value="all">All records</option><option value="today">Today</option><option value="7d">Last 7 days</option><option value="30d">Last 30 days</option><option value="90d">Last 90 days</option><option value="month">This month</option><option value="year">This year</option><option value="custom">Custom range</option></select></label>{#if period === 'custom'}<label>From<input type="date" bind:value={dateFrom} /></label><label>To<input type="date" bind:value={dateTo} /></label><button class="button primary" onclick={refresh}>Apply range</button>{/if}<button class="button" onclick={() => { period = '30d'; void changePeriod(); }}>Reset</button><button class="button" onclick={refresh} disabled={loading}>Refresh data</button></div>
  {#if error}<div class="notice error">{error}</div>{/if}

  <section class="panel online-presence-card"><div class="panel-heading"><div><h2>Online now</h2><p class="muted">Only people with an active MX session are shown.</p></div><span class="online-presence-count">{presence?.online || 0} online</span></div>{#if presence?.accounts.length}<div class="online-presence-list">{#each presence.accounts as account}<article class="online-person"><i aria-hidden="true"></i><div><strong>{account.name}</strong><small>{account.access_name}</small></div></article>{/each}</div>{:else}<p class="online-presence-empty">Waiting for an active session…</p>{/if}</section>

  {#if !loading && widgets.length}<div class="report-summary"><div><span>Measures</span><strong>{widgets.length}</strong></div><div><span>Groups</span><strong>{allRows.length}</strong></div><div><span>{recordPlural} measured</span><strong>{reportTotal.toLocaleString()}</strong></div><div><span>Matching {recordPlural.toLowerCase()}</span><strong>{reportAnswered.toLocaleString()}</strong></div></div>{/if}

  {#if loading}<div class="panel loading-card"><div class="loader"></div><span>Refreshing dashboard reports…</span></div>{:else if !widgets.length}<div class="empty-state"><h2>Your dashboard is ready to configure</h2><p>Administrators can create summary numbers, charts, progress views, and tables from Administration → Dashboard.</p></div>{:else}
    <div class="dashboard-grid">{#each widgets as widget}
      {@const rows = widgetRows[widget.uid] || []}
      {@const measured = total(rows, 'total')}
      {@const answered = totalAnswer(widget, rows)}
      <article class:metric-card={widget.display_mode === 'metric'} class:wide-report={widget.display_mode !== 'metric'} class="panel report-card">
        <header><div><p class="eyebrow">{widget.display_mode.replace('_', ' ')}</p><h2>{widget.title}</h2><p class="muted report-subtitle">{answered.toLocaleString()} {resultLabel(widget)} out of {measured.toLocaleString()} measured · {measured ? (answered / measured * 100).toFixed(1) : '0.0'}%</p></div><button class="button small" onclick={() => exportReport('action_rate', widget, dateFrom, dateTo)}>Export CSV</button></header>
        {#if widget.definition}<p class="definition top-definition">{widget.definition}</p>{/if}
        {#if widget.display_mode === 'metric'}
          <div class="metric-layout"><div><strong class="metric-value">{metricValue(widget, rows).toLocaleString()}</strong><p class="muted">{recordPlural.toLowerCase()} {resultLabel(widget)}</p></div><div class="metric-rate"><strong>{measured ? (answered / measured * 100).toFixed(1) : '0.0'}%</strong><small>{Math.max(0, measured - answered).toLocaleString()} other {recordPlural.toLowerCase()}</small></div></div>
        {:else if ['bar', 'line', 'pie', 'donut'].includes(widget.display_mode)}
          <ReportChart {rows} {widget} {recordPlural} />
        {:else if widget.display_mode === 'progress'}
          <div class="progress-list">{#each rows as row}<div><div class="progress-meta"><strong>{row.group || 'Unspecified'}</strong><span>{answerValue(widget, row).toLocaleString()} / {row.total.toLocaleString()} {resultLabel(widget)} · {answerRate(widget, row).toFixed(1)}%</span></div><div class="progress-track"><i style={`width:${Math.max(0, Math.min(100, answerRate(widget, row)))}%`}></i></div></div>{/each}</div>
        {:else}
          <div class="table-wrap compact-table"><table><thead><tr><th>Group</th><th>Total</th><th>Selected result</th><th>Other records</th><th>Share</th></tr></thead><tbody>{#if !rows.length}<tr><td colspan="5">No records are available in this period.</td></tr>{/if}{#each rows as row}<tr><td><strong>{row.group || 'Unspecified'}</strong></td><td>{row.total.toLocaleString()}</td><td>{answerValue(widget, row).toLocaleString()}</td><td>{otherValue(widget, row).toLocaleString()}</td><td>{answerRate(widget, row).toFixed(1)}%</td></tr>{/each}</tbody></table></div>
        {/if}
      </article>
    {/each}</div>
  {/if}

  <section class="panel section-panel"><div class="panel-heading"><div><h2>Data exports</h2><p class="muted">Portable CSV reports from the authoritative database, scoped to the selected module.</p></div><label>Module<select bind:value={exportModuleUid}>{#each modules as module}<option value={module.uid}>{module.name}</option>{/each}</select></label></div><div class="button-row"><button class="button" onclick={() => exportReport('records', undefined, '', '', exportModuleUid)}>Detailed records</button><button class="button" onclick={() => exportReport('attachments', undefined, '', '', exportModuleUid)}>Attachments</button>{#if isAdmin}<button class="button" onclick={() => exportReport('user_performance', undefined, dateFrom, dateTo)}>User performance</button>{/if}</div></section>

  {#if showPerformance}<section class="panel section-panel"><div class="panel-heading"><div><h2>User performance</h2><p class="muted">Audited activity during {periodLabel().toLowerCase()}.</p></div></div><div class="table-wrap"><table><thead><tr><th>User</th><th>Created</th><th>Updated</th><th>Uploads</th><th>Deleted</th><th>Downloads</th><th>Successful</th><th>Failed</th><th>Records touched</th><th>Last activity</th></tr></thead><tbody>{#each performance as row}<tr><td><strong>{row.actor_name}</strong><small class="block">{row.actor_email}</small></td><td>{row.records_created}</td><td>{row.records_updated}</td><td>{row.attachments_uploaded}</td><td>{row.records_deleted}</td><td>{row.attachment_downloads}</td><td>{row.successful_actions}</td><td>{row.failed_actions}</td><td>{row.unique_records_touched}</td><td>{time(row.last_activity)}</td></tr>{/each}</tbody></table></div></section>{/if}
</section>
