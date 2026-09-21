<script lang="ts">
  import type { ActionRateRow, DashboardWidget } from '../api/domain';

  let { rows, widget, recordPlural = 'Records' }: { rows: ActionRateRow[]; widget: DashboardWidget; recordPlural?: string } = $props();
  const palette = ['var(--primary)', '#7c3aed', '#0891b2', '#059669', '#d97706', '#dc2626', '#4f46e5', '#0f766e', '#9333ea', '#ea580c', '#0284c7', '#65a30d', '#be123c', '#475569', '#a16207'];
  const mode = $derived(widget.chart_value || 'matched');
  const prepared = $derived.by(() => {
    const source = rows.map((row) => ({ ...row }));
    if (widget.display_mode === 'line') return source;
    source.sort((a, b) => sortableValue(b) - sortableValue(a) || a.group.localeCompare(b.group));
    const limit = Math.max(0, Number(widget.category_limit ?? 8));
    if (!limit || source.length <= limit) return source;
    const kept = source.slice(0, limit);
    if (mode !== 'rate') {
      const other = source.slice(limit).reduce((sum, row) => ({ group: 'Other', total: sum.total + row.total, actioned: sum.actioned + row.actioned, pending: sum.pending + row.pending, rate: 0 }), { group: 'Other', total: 0, actioned: 0, pending: 0, rate: 0 });
      other.rate = other.total ? other.actioned * 100 / other.total : 0;
      kept.push(other);
    }
    return kept;
  });
  const maximum = $derived(mode === 'rate' ? 100 : Math.max(1, ...prepared.map((row) => sortableValue(row))));
  const lineSeries = $derived(mode === 'breakdown'
    ? [{ key: 'actioned' as const, label: 'Matched', color: palette[0] }, { key: 'pending' as const, label: 'Not matched', color: palette[5] }]
    : [{ key: (mode === 'total' ? 'total' : mode === 'rate' ? 'rate' : 'actioned') as keyof ActionRateRow, label: valueLabel(), color: palette[0] }]);
  const pieRows = $derived(prepared.map((row, index) => ({ row, index, value: mode === 'total' ? row.total : row.actioned })).filter((item) => item.value > 0));
  const pieTotal = $derived(pieRows.reduce((sum, item) => sum + item.value, 0));
  const pieGradient = $derived.by(() => {
    let position = 0;
    return `conic-gradient(${pieRows.map((item) => { const start = position; position += pieTotal ? item.value / pieTotal * 100 : 0; return `${palette[item.index % palette.length]} ${start}% ${position}%`; }).join(', ')})`;
  });

  function sortableValue(row: ActionRateRow) { if (mode === 'total') return row.total; if (mode === 'rate') return row.rate; if (mode === 'breakdown') return row.total; return row.actioned; }
  function valueLabel() { if (mode === 'total') return 'Total records'; if (mode === 'rate') return 'Match rate'; if (mode === 'breakdown') return 'Matched vs not matched'; return 'Matched records'; }
  function displayValue(value: number) { return mode === 'rate' ? `${value.toFixed(1)}%` : value.toLocaleString(); }
  function xAt(index: number) { return prepared.length === 1 ? 460 : 58 + 834 * index / Math.max(1, prepared.length - 1); }
  function yAt(value: number) { const max = mode === 'rate' ? 100 : Math.max(1, ...lineSeries.flatMap((series) => prepared.map((row) => Number(row[series.key] || 0)))); return 282 - Math.max(0, value) / max * 250; }
  function points(key: keyof ActionRateRow) { return prepared.map((row, index) => `${xAt(index)},${yAt(Number(row[key] || 0))}`).join(' '); }
</script>

{#if !prepared.length}
  <div class="chart-empty">No data is available for this chart in the selected period.</div>
{:else if widget.display_mode === 'bar'}
  <div class="horizontal-chart" role="img" aria-label={`${widget.title} bar chart`}>
    {#each prepared as row, index}
      <div class="horizontal-row"><span title={row.group}>{row.group || 'Unspecified'}</span><div class="horizontal-track">
        {#if mode === 'breakdown'}
          <i style={`width:${row.actioned / maximum * 100}%;background:${palette[0]}`} title={`${row.actioned} matched`}></i><i style={`width:${row.pending / maximum * 100}%;background:${palette[5]};opacity:.72`} title={`${row.pending} not matched`}></i>
        {:else}<i style={`width:${sortableValue(row) / maximum * 100}%;background:${palette[index % palette.length]}`}></i>{/if}
      </div><strong>{mode === 'breakdown' ? row.total.toLocaleString() : displayValue(sortableValue(row))}</strong></div>
    {/each}
  </div>
  {#if mode === 'breakdown' && widget.show_legend !== false}<div class="chart-legend"><span><i style={`background:${palette[0]}`}></i>Matched</span><span><i style={`background:${palette[5]}`}></i>Not matched</span></div>{/if}
{:else if widget.display_mode === 'line'}
  <div class="line-chart"><svg viewBox="0 0 920 340" role="img" aria-label={`${widget.title} line chart`}>
    {#each [0, .25, .5, .75, 1] as part}<line x1="58" y1={282 - 250 * part} x2="892" y2={282 - 250 * part}></line><text x="48" y={286 - 250 * part} text-anchor="end">{mode === 'rate' ? `${Math.round(part * 100)}%` : Math.round(maximum * part)}</text>{/each}
    {#each lineSeries as series}<polyline points={points(series.key)} style={`stroke:${series.color}`}></polyline>{#each prepared as row, index}<circle cx={xAt(index)} cy={yAt(Number(row[series.key] || 0))} r="4" style={`fill:${series.color}`}><title>{row.group}: {series.label} {displayValue(Number(row[series.key] || 0))}</title></circle>{/each}{/each}
    {#each prepared as row, index}{#if index % Math.max(1, Math.ceil(prepared.length / 8)) === 0 || index === prepared.length - 1}<text x={xAt(index)} y="318" text-anchor="middle">{row.group.length > 12 ? `${row.group.slice(0, 11)}…` : row.group}</text>{/if}{/each}
  </svg></div>
  {#if widget.show_legend !== false}<div class="chart-legend">{#each lineSeries as series}<span><i style={`background:${series.color}`}></i>{series.label}</span>{/each}</div>{/if}
{:else if widget.display_mode === 'pie' || widget.display_mode === 'donut'}
  <div class="pie-layout"><div class:donut={widget.display_mode === 'donut'} class="pie" style={`background:${pieGradient}`} role="img" aria-label={`${widget.title} ${widget.display_mode} chart`}>{#if widget.display_mode === 'donut'}<div><strong>{pieTotal.toLocaleString()}</strong><small>{mode === 'total' ? `total ${recordPlural.toLowerCase()}` : `matched ${recordPlural.toLowerCase()}`}</small></div>{/if}</div>
    {#if widget.show_legend !== false}<div class="pie-legend">{#each pieRows as item}<div><i style={`background:${palette[item.index % palette.length]}`}></i><span>{item.row.group || 'Unspecified'}</span><strong>{item.value.toLocaleString()} · {pieTotal ? (item.value / pieTotal * 100).toFixed(1) : '0.0'}%</strong></div>{/each}</div>{/if}
  </div>
{/if}

<style>
  .chart-empty { min-height: 15rem; display: grid; place-items: center; color: var(--muted); font-size: .75rem; }
  .horizontal-chart { display: grid; gap: .8rem; padding: 1rem 0 .25rem; }
  .horizontal-row { display: grid; grid-template-columns: minmax(7rem, 1fr) minmax(10rem, 2.5fr) 4.5rem; align-items: center; gap: .75rem; font-size: .72rem; }
  .horizontal-row > span { overflow: hidden; color: var(--text-2); font-weight: 750; text-overflow: ellipsis; white-space: nowrap; }
  .horizontal-row > strong { color: var(--text-2); }
  .horizontal-track { height: 1rem; display: flex; overflow: hidden; border-radius: .3rem; background: var(--surface-3); }
  .horizontal-track i { height: 100%; min-width: 1px; }
  .line-chart { overflow-x: auto; }
  svg { width: 100%; min-width: 36rem; display: block; }
  svg line { stroke: var(--line); stroke-width: 1; }
  svg text { fill: var(--muted); font-size: 10px; font-weight: 650; }
  svg polyline { fill: none; stroke-width: 3; stroke-linecap: round; stroke-linejoin: round; }
  svg circle { stroke: var(--surface); stroke-width: 2; }
  .chart-legend { display: flex; flex-wrap: wrap; gap: .6rem 1rem; margin-top: .7rem; color: var(--text-2); font-size: .68rem; font-weight: 700; }
  .chart-legend span { display: inline-flex; align-items: center; gap: .4rem; }
  .chart-legend i, .pie-legend i { width: .65rem; height: .65rem; flex: none; border-radius: .2rem; }
  .pie-layout { display: grid; grid-template-columns: minmax(14rem, .8fr) minmax(15rem, 1.2fr); align-items: center; gap: 2rem; padding: .8rem; }
  .pie { width: min(100%, 16rem); aspect-ratio: 1; justify-self: center; display: grid; place-items: center; border-radius: 50%; box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text) 6%, transparent); }
  .pie.donut::after { content: ''; width: 53%; aspect-ratio: 1; grid-area: 1 / 1; border-radius: 50%; background: var(--surface); }
  .pie > div { z-index: 1; grid-area: 1 / 1; display: grid; text-align: center; }
  .pie > div strong { font-size: 1.5rem; }
  .pie > div small { color: var(--muted); }
  .pie-legend { display: grid; gap: .55rem; }
  .pie-legend div { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: .5rem; font-size: .7rem; }
  .pie-legend span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (max-width: 700px) { .pie-layout { grid-template-columns: 1fr; } .horizontal-row { grid-template-columns: minmax(5rem, .8fr) minmax(7rem, 1.5fr) 3.5rem; } }
</style>
