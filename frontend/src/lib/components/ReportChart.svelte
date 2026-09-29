<script lang="ts">
  import type { ActionRateRow, DashboardWidget } from '../api/domain';

  let { rows, widget, recordPlural = 'Records' }: { rows: ActionRateRow[]; widget: DashboardWidget; recordPlural?: string } = $props();
  type SeriesKey = 'focused' | 'other' | 'total' | 'rate';
  // Reporting deliberately uses the MX light/dark visualization palette. It
  // must remain independent from administrator and personal accent colors.
  const palette = Array.from({ length: 10 }, (_, index) => `var(--chart-${index + 1})`);
  const mode = $derived(widget.chart_value || 'matched');
  const focus = $derived(widget.result_focus || (mode === 'pending' ? 'pending' : 'matched'));
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
    ? [{ key: 'focused' as SeriesKey, label: 'Answers question', color: 'var(--chart-1)' }, { key: 'other' as SeriesKey, label: 'Other records', color: 'var(--chart-neutral)' }]
    : [{ key: (mode === 'total' ? 'total' : mode === 'rate' ? 'rate' : 'focused') as SeriesKey, label: valueLabel(), color: palette[0] }]);
  const pieRows = $derived(prepared.map((row, index) => ({ row, index, value: mode === 'total' ? row.total : focusedValue(row) })).filter((item) => item.value > 0));
  const pieTotal = $derived(pieRows.reduce((sum, item) => sum + item.value, 0));
  const pieGradient = $derived.by(() => {
    let position = 0;
    return `conic-gradient(${pieRows.map((item) => { const start = position; position += pieTotal ? item.value / pieTotal * 100 : 0; return `${palette[item.index % palette.length]} ${start}% ${position}%`; }).join(', ')})`;
  });

  function focusedValue(row: ActionRateRow) { return focus === 'pending' ? row.pending : row.actioned; }
  function otherValue(row: ActionRateRow) { return focus === 'pending' ? row.actioned : row.pending; }
  function focusedRate(row: ActionRateRow) { return row.total ? focusedValue(row) * 100 / row.total : 0; }
  function seriesValue(row: ActionRateRow, key: SeriesKey) { if (key === 'focused') return focusedValue(row); if (key === 'other') return otherValue(row); if (key === 'total') return row.total; return focusedRate(row); }
  function sortableValue(row: ActionRateRow) { if (mode === 'total') return row.total; if (mode === 'rate') return focusedRate(row); if (mode === 'breakdown') return row.total; return focusedValue(row); }
  function valueLabel() { if (mode === 'total') return 'Total records'; if (mode === 'rate') return 'Answer rate'; if (mode === 'breakdown') return 'Answer and remainder'; return 'Records answering the question'; }
  function displayValue(value: number) { return mode === 'rate' ? `${value.toFixed(1)}%` : value.toLocaleString(); }
  function xAt(index: number) { return prepared.length === 1 ? 460 : 58 + 834 * index / Math.max(1, prepared.length - 1); }
  function yAt(value: number) { const max = mode === 'rate' ? 100 : Math.max(1, ...lineSeries.flatMap((series) => prepared.map((row) => seriesValue(row, series.key)))); return 282 - Math.max(0, value) / max * 250; }
  function points(key: SeriesKey) { return prepared.map((row, index) => `${xAt(index)},${yAt(seriesValue(row, key))}`).join(' '); }
</script>

{#if !prepared.length}
  <div class="chart-empty">No data is available for this chart in the selected period.</div>
{:else if widget.display_mode === 'bar'}
  <div class="horizontal-chart chart-surface" role="img" aria-label={`${widget.title} bar chart`}>
    {#each prepared as row}
      <div class="horizontal-row"><span title={row.group}>{row.group || 'Unspecified'}</span><div class="horizontal-track">
        {#if mode === 'breakdown'}
          <i class="focused-bar" style={`width:${focusedValue(row) / maximum * 100}%`} title={`${focusedValue(row)} answer the question`}></i><i class="other-bar" style={`width:${otherValue(row) / maximum * 100}%`} title={`${otherValue(row)} other records`}></i>
        {:else}<i class="focused-bar" style={`width:${sortableValue(row) / maximum * 100}%`}></i>{/if}
      </div><strong>{mode === 'breakdown' ? row.total.toLocaleString() : displayValue(sortableValue(row))}</strong></div>
    {/each}
  </div>
  {#if mode === 'breakdown' && widget.show_legend !== false}<div class="chart-legend"><span><i style="background:var(--chart-1)"></i>Answers question</span><span><i style="background:var(--chart-neutral)"></i>Other records</span></div>{/if}
{:else if widget.display_mode === 'line'}
  <div class="line-chart chart-surface"><svg viewBox="0 0 920 340" role="img" aria-label={`${widget.title} line chart`}>
    {#each [0, .25, .5, .75, 1] as part}<line x1="58" y1={282 - 250 * part} x2="892" y2={282 - 250 * part}></line><text x="48" y={286 - 250 * part} text-anchor="end">{mode === 'rate' ? `${Math.round(part * 100)}%` : Math.round(maximum * part)}</text>{/each}
    {#each lineSeries as series}<polyline points={points(series.key)} style={`stroke:${series.color}`}></polyline>{#each prepared as row, index}<circle cx={xAt(index)} cy={yAt(seriesValue(row, series.key))} r="4" style={`fill:${series.color}`}><title>{row.group}: {series.label} {displayValue(seriesValue(row, series.key))}</title></circle>{/each}{/each}
    {#each prepared as row, index}{#if index % Math.max(1, Math.ceil(prepared.length / 8)) === 0 || index === prepared.length - 1}<text x={xAt(index)} y="318" text-anchor="middle">{row.group.length > 12 ? `${row.group.slice(0, 11)}…` : row.group}</text>{/if}{/each}
  </svg></div>
  {#if widget.show_legend !== false}<div class="chart-legend">{#each lineSeries as series}<span><i style={`background:${series.color}`}></i>{series.label}</span>{/each}</div>{/if}
{:else if widget.display_mode === 'pie' || widget.display_mode === 'donut'}
  <div class:without-legend={widget.show_legend === false} class="pie-layout">
    <figure class="pie-figure"><div class:donut={widget.display_mode === 'donut'} class="pie" style={`background:${pieGradient}`} role="img" aria-label={`${widget.title} ${widget.display_mode} chart`}>{#if widget.display_mode === 'donut'}<div><strong>{pieTotal.toLocaleString()}</strong><small>{mode === 'total' ? `total ${recordPlural.toLowerCase()}` : 'selected results'}</small></div>{/if}</div>{#if widget.display_mode === 'pie'}<figcaption><strong>{pieTotal.toLocaleString()}</strong><span>{recordPlural.toLowerCase()} across {pieRows.length} categor{pieRows.length === 1 ? 'y' : 'ies'}</span></figcaption>{/if}</figure>
    {#if widget.show_legend !== false}<div class="pie-legend">{#each pieRows as item}<div style={`--legend-color:${palette[item.index % palette.length]}`}><i></i><span><strong title={item.row.group || 'Unspecified'}>{item.row.group || 'Unspecified'}</strong><small>{item.value.toLocaleString()} total</small></span><b>{pieTotal ? (item.value / pieTotal * 100).toFixed(1) : '0.0'}%</b></div>{/each}</div>{/if}
  </div>
{/if}

<style>
  .chart-empty { min-height: 11rem; display: grid; place-items: center; margin-top: .75rem; border: 1px dashed var(--line); border-radius: .55rem; background: var(--chart-surface); color: var(--muted); font-size: calc(.72rem * var(--font-scale)); }
  .chart-surface { border: 1px solid var(--line); border-radius: .55rem; background: var(--chart-surface); }
  .horizontal-chart { width: min(100%, 68rem); display: grid; gap: .7rem; margin: .85rem auto 0; padding: .9rem 1rem; }
  .horizontal-row { display: grid; grid-template-columns: minmax(7rem, 1fr) minmax(10rem, 2.7fr) 4.5rem; align-items: center; gap: .75rem; font-size: calc(.7rem * var(--font-scale)); }
  .horizontal-row > span { overflow: hidden; color: var(--text-2); font-weight: 750; text-overflow: ellipsis; white-space: nowrap; }
  .horizontal-row > strong { color: var(--text); font-variant-numeric: tabular-nums; text-align: right; }
  .horizontal-track { height: .62rem; display: flex; overflow: hidden; border-radius: 999px; background: var(--chart-track); }
  .horizontal-track i { height: 100%; min-width: 1px; }
  .horizontal-track .focused-bar { background: var(--chart-1); }
  .horizontal-track .other-bar { background: var(--chart-neutral); }
  .line-chart { width: min(100%, 72rem); margin: .85rem auto 0; padding: .6rem .75rem .25rem; overflow-x: auto; }
  svg { width: 100%; min-width: 36rem; display: block; }
  svg line { stroke: var(--chart-grid); stroke-width: 1; stroke-dasharray: 3 5; }
  svg text { fill: var(--muted); font-size: calc(10px * var(--font-scale)); font-weight: 650; }
  svg polyline { fill: none; stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; }
  svg circle { stroke: var(--chart-surface); stroke-width: 3; }
  .chart-legend { display: flex; flex-wrap: wrap; justify-content: center; gap: .55rem 1rem; margin-top: .7rem; color: var(--text-2); font-size: calc(.66rem * var(--font-scale)); font-weight: 700; }
  .chart-legend span { display: inline-flex; align-items: center; gap: .4rem; }
  .chart-legend i, .pie-legend i { width: .62rem; height: .62rem; flex: none; border-radius: .18rem; }
  .pie-layout { width: min(100%, 40rem); display: grid; grid-template-columns: 12.5rem minmax(14rem, 1fr); align-items: center; justify-content: center; gap: 1.5rem; margin: .85rem auto 0; padding: 1rem; border: 1px solid var(--line); border-radius: .55rem; background: var(--chart-surface); }
  .pie-layout.without-legend { grid-template-columns: minmax(12rem, 18rem); }
  .pie-figure { display: grid; justify-items: center; gap: .65rem; margin: 0; }
  .pie { width: min(100%, 11.5rem); aspect-ratio: 1; display: grid; place-items: center; border: .2rem solid var(--chart-surface); border-radius: 50%; box-shadow: 0 0 0 1px var(--line), 0 .45rem 1.1rem color-mix(in srgb, var(--text) 8%, transparent); }
  .pie.donut::after { content: ''; width: 57%; aspect-ratio: 1; grid-area: 1 / 1; border: 1px solid var(--line); border-radius: 50%; background: var(--chart-surface); }
  .pie > div { z-index: 1; grid-area: 1 / 1; display: grid; text-align: center; }
  .pie > div strong { color: var(--text); font-size: calc(1.35rem * var(--font-scale)); font-variant-numeric: tabular-nums; letter-spacing: -.035em; }
  .pie > div small { max-width: 6rem; color: var(--muted); font-size: calc(.57rem * var(--font-scale)); line-height: 1.25; }
  .pie-figure figcaption { display: grid; justify-items: center; gap: .08rem; color: var(--muted); font-size: calc(.6rem * var(--font-scale)); }
  .pie-figure figcaption strong { color: var(--text); font-size: calc(.85rem * var(--font-scale)); font-variant-numeric: tabular-nums; }
  .pie-legend { min-width: 0; display: grid; gap: .35rem; }
  .pie-legend > div { min-width: 0; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: .55rem; padding: .48rem .55rem; border: 1px solid transparent; border-radius: .42rem; font-size: calc(.68rem * var(--font-scale)); }
  .pie-legend > div:hover { border-color: var(--line); background: var(--surface); }
  .pie-legend i { background: var(--legend-color); }
  .pie-legend span { min-width: 0; display: grid; gap: .04rem; }
  .pie-legend span strong { overflow: hidden; color: var(--text-2); text-overflow: ellipsis; white-space: nowrap; }
  .pie-legend span small { color: var(--muted); font-size: calc(.56rem * var(--font-scale)); }
  .pie-legend b { color: var(--text); font-size: calc(.67rem * var(--font-scale)); font-variant-numeric: tabular-nums; }
  @media (max-width: 700px) {
    .pie-layout { grid-template-columns: 1fr; gap: 1rem; padding: .85rem; }
    .horizontal-chart { padding: .8rem; }
    .horizontal-row { grid-template-columns: minmax(0, 1fr) auto; gap: .28rem .65rem; }
    .horizontal-row > span { grid-column: 1; }
    .horizontal-row > strong { grid-column: 2; }
    .horizontal-track { grid-column: 1 / -1; grid-row: 2; }
  }
</style>
