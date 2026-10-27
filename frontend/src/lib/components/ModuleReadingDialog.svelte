<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import type { ColumnPresentation, ColumnTone, FieldDefinition, ModuleReadingMap } from '../api/domain';
  import { loadPreferences, savePreferences } from '../api/workspace';
  import { columnPresentation, columnTone, columnTones, withBoldFields, withColumnPresentation } from '../util/readingPreferences';
  import { focusDialog } from '../util/focusDialog';
  import { dismissBackdrop } from '../util/dismissBackdrop';
  import { requestConfirmation } from '../confirmation';

  let { moduleUid, moduleName, fields, onSaved, onClose }: {
    moduleUid: string; moduleName: string; fields: FieldDefinition[];
    onSaved: (preferences: ModuleReadingMap) => void; onClose: () => void;
  } = $props();
  let selected = $state<string[]>([]);
  let columns = $state<Record<string, ColumnPresentation>>({});
  let search = $state('');
  let loading = $state(true);
  let busy = $state(false);
  let error = $state('');
  let errorPanel = $state<HTMLDivElement>();
  let original = '';
  let originalDraft = '';
  let confirmingClose = false;
  let destroyed = false;
  const visibleFields = $derived(fields.filter(field => field.active && field.label.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())));
  const moduleKey = $derived(moduleUid || 'mx-default-records');
  onDestroy(() => destroyed = true);
  onMount(() => void load());

  async function load() {
    loading = true; error = '';
    try {
      const response = await loadPreferences();
      if (destroyed) return;
      const current = response.preferences.module_reading?.[moduleKey];
      original = JSON.stringify(current || null);
      selected = [...(current?.bold_fields || [])];
      columns = structuredClone(current?.columns || {});
      originalDraft = JSON.stringify({selected,columns});
    } catch (reason) { if (!destroyed) error = reason instanceof Error ? reason.message : 'Could not load your reading settings.'; }
    finally { if (!destroyed) loading = false; }
  }
  function toggle(uid: string, checked: boolean) { selected = checked ? [...new Set([...selected, uid])] : selected.filter(id => id !== uid); }
  function presentation(field: FieldDefinition) { return columnPresentation({ [moduleKey]: { bold_fields: selected, columns } }, moduleKey, field); }
  function updateColumn(field: FieldDefinition, change: Partial<ColumnPresentation>) {
    columns = { ...columns, [field.uid]: { ...presentation(field), ...change } };
  }
  function resetColumn(uid: string) { const next = { ...columns }; delete next[uid]; columns = next; }
  async function close() {
    if (busy || confirmingClose) return;
    if (!loading && originalDraft && originalDraft !== JSON.stringify({selected,columns})) {
      confirmingClose = true;
      const discard = await requestConfirmation({ title: 'Discard appearance changes?', description: 'Your saved reading settings will stay unchanged.', confirmLabel: 'Discard changes', tone: 'danger' });
      confirmingClose = false;
      if (!discard) return;
    }
    onClose();
  }
  async function save() {
    busy = true; error = '';
    try {
      const latest = await loadPreferences();
      if (JSON.stringify(latest.preferences.module_reading?.[moduleKey] || null) !== original) {
        throw new Error('Reading settings for this module changed in another window. Reload settings to review them before saving.');
      }
      const next = withColumnPresentation(withBoldFields(latest.preferences.module_reading || {}, moduleKey, selected), moduleKey, columns);
      const result = await savePreferences({ ...latest.preferences, module_reading: next }, latest.revision);
      if (destroyed) return;
      onSaved(result.preferences.module_reading || {});
      onClose();
    } catch (reason) { if (!destroyed) { error = reason instanceof Error ? reason.message : 'Could not save your reading settings.'; await tick(); if (!destroyed) errorPanel?.focus(); } }
    finally { if (!destroyed) busy = false; }
  }
</script>

<div class="overlay reading-overlay" use:dismissBackdrop={()=>void close()}>
  <div class="dialog reading-dialog" role="dialog" aria-modal="true" aria-labelledby="module-reading-title" tabindex="-1" use:focusDialog onkeydown={event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();void close();}}}>
    <header class="dialog-head"><div><h2 id="module-reading-title">Reading settings</h2><p>{moduleName} · Only your view changes.</p></div><button class="icon-button" type="button" aria-label="Close reading settings" disabled={busy} onclick={()=>void close()}><X size={20}/></button></header>
    <div class="reading-body">
      {#if error}<div class="notice error" role="alert" tabindex="-1" bind:this={errorPanel}>{error}<button class="button small" type="button" disabled={busy||loading} onclick={()=>void load()}>Reload settings</button></div>{/if}
      {#if loading}<p role="status">Loading your preferences…</p>{:else}
        <details class="reading-guide"><summary>How column appearance works</summary><p>Choose bold text, plain values or colored badges. Bold also applies to record forms and history. Automatic badge colors distinguish categories; they do not indicate approval or priority. Your changes do not alter records or other people’s settings.</p></details>
        <label>Find a field<input type="search" bind:value={search} placeholder="Search field names…" disabled={busy}/></label>
        <div class="reading-actions"><button class="button small" type="button" disabled={busy} onclick={()=>selected=[...new Set([...selected,...visibleFields.map(field=>field.uid)])]}>Bold shown fields</button><button class="button small" type="button" disabled={busy} onclick={()=>selected=selected.filter(uid=>!visibleFields.some(field=>field.uid===uid))}>Normal shown fields</button><span>{fields.filter(field=>selected.includes(field.uid)).length} bold</span></div>
        <div class="reading-fields">{#each visibleFields as field (field.uid)}
          {@const appearance = presentation(field)}
          <div class="reading-field">
            <label class="reading-field-name"><input type="checkbox" checked={selected.includes(field.uid)} disabled={busy} aria-label={`Bold ${field.label}`} onchange={event=>toggle(field.uid,event.currentTarget.checked)}/><span class:bold={selected.includes(field.uid)}>{field.label}<small>{field.field_type.replaceAll('_',' ')}</small></span></label>
            {#if field.field_type !== 'attachments'}
              <div class="column-controls">
                <label>Display<select aria-label={`Display ${field.label}`} value={appearance.style} disabled={busy} onchange={event=>updateColumn(field,{style:event.currentTarget.value as ColumnPresentation['style']})}><option value="text">Plain text</option><option value="badge">Badge</option></select></label>
                <label>Color<select aria-label={`Color ${field.label}`} value={appearance.tone} disabled={busy} onchange={event=>updateColumn(field,{tone:event.currentTarget.value as ColumnTone})}>{#each columnTones as tone}<option value={tone}>{tone==='auto'?'Automatic':tone.charAt(0).toUpperCase()+tone.slice(1)}</option>{/each}</select></label>
                <span class="column-sample" aria-label={`Preview ${field.label}`}><span class="column-value" class:column-badge={appearance.style==='badge'} class:bold={selected.includes(field.uid)} data-tone={columnTone(appearance,'Sample',field.field_type)}>Sample</span></span>
              </div>
              {#if columns[field.uid]}<button class="column-reset" type="button" disabled={busy} aria-label={`Reset appearance ${field.label}`} onclick={()=>resetColumn(field.uid)}>Use default</button>{/if}
            {/if}
          </div>
        {:else}<p class="muted">No fields match your search.</p>{/each}</div>
      {/if}
    </div>
    <footer class="dialog-actions"><button class="button" type="button" disabled={busy} onclick={()=>void close()}>Cancel</button><span class="spacer"></span><button class="button primary" type="button" disabled={busy||loading} onclick={()=>void save()}>{busy?'Saving…':'Save my reading settings'}</button></footer>
  </div>
</div>

<style>
  .reading-overlay{padding:clamp(.5rem,2vw,1rem);}
  .reading-dialog{width:min(48rem,100%);max-height:calc(100dvh - 2rem);display:block;overflow:auto;overscroll-behavior:contain;padding:0;}
  .dialog-head,.dialog-actions{margin:0;padding:1rem 1.2rem;flex-shrink:0;}.dialog-head{align-items:flex-start;border-bottom:1px solid var(--line);}.dialog-head>div{min-width:0;}.dialog-head h2{font-size:1.1rem;overflow-wrap:anywhere;}.dialog-head p:last-child{font-size:.85rem;line-height:1.55;color:var(--muted);margin:.4rem 0 0;}
  .reading-body{padding:1.2rem;display:grid;gap:1rem;min-height:0;}.reading-body>label{display:grid;gap:.4rem;}.reading-body input[type=search]{width:100%;}
  .reading-guide{font-size:.8125rem;color:var(--text-2);}.reading-guide summary{cursor:pointer;font-weight:600;}.reading-guide p{margin:.6rem 0 0;line-height:1.6;color:var(--muted);}
  .reading-actions{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap;}.reading-actions>span{margin-left:auto;color:var(--muted);font-size:.8rem;}
  .reading-fields{display:grid;border:1px solid var(--line);border-radius:.45rem;overflow:hidden;}.reading-field{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.5fr);gap:.7rem;align-items:center;padding:.85rem;border-bottom:1px solid var(--line);font-size:.9rem;font-weight:400;}.reading-field:last-child{border-bottom:0;}.reading-field-name{display:flex;align-items:center;gap:.7rem;min-width:0;cursor:pointer;font-weight:400;}.reading-field-name>span{flex:1;min-width:0;overflow-wrap:anywhere;}.reading-field small{display:block;font-size:.75rem;color:var(--muted);font-weight:400;margin-top:.2rem;}.bold{font-weight:750;}
  .column-controls{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr) minmax(0,.85fr);gap:.5rem;align-items:center;}.column-controls label{display:grid;font-size:.75rem;min-width:0;}.column-controls select{width:100%;min-width:0;font-size:.8rem;}.column-sample{min-width:0;overflow-wrap:anywhere;}.column-reset{grid-column:2;justify-self:start;border:0;background:none;padding:0;color:var(--primary-text);font:inherit;font-size:.75rem;text-decoration:underline;text-underline-offset:3px;}
  .notice.error{display:grid;gap:.6rem;}.notice.error button{justify-self:start;}.dialog-actions{border-top:1px solid var(--line);flex-wrap:wrap;}
  @media(max-width:600px){.dialog-head,.dialog-actions,.reading-body{padding:clamp(12px,1rem,20px);}.dialog-actions .spacer{display:none;}.dialog-actions button{flex:1;white-space:normal;}.reading-field{grid-template-columns:minmax(0,1fr);padding:clamp(10px,.85rem,16px);}.column-reset{grid-column:1;}.column-controls{grid-template-columns:repeat(auto-fit,minmax(min(100%,9rem),1fr));}.column-sample{grid-column:1/-1;}}
</style>
