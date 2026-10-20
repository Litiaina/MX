<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import Bold from '@lucide/svelte/icons/bold';
  import type { FieldDefinition, ModuleReadingMap } from '../api/domain';
  import { loadPreferences, savePreferences } from '../api/workspace';
  import { withBoldFields } from '../util/readingPreferences';
  import { focusDialog } from '../util/focusDialog';

  let { moduleUid, moduleName, fields, onSaved, onClose }: {
    moduleUid: string; moduleName: string; fields: FieldDefinition[];
    onSaved: (preferences: ModuleReadingMap) => void; onClose: () => void;
  } = $props();
  let selected = $state<string[]>([]);
  let search = $state('');
  let loading = $state(true);
  let busy = $state(false);
  let error = $state('');
  let original = '';
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
    } catch (reason) { if (!destroyed) error = reason instanceof Error ? reason.message : 'Could not load your reading settings.'; }
    finally { if (!destroyed) loading = false; }
  }
  function toggle(uid: string, checked: boolean) { selected = checked ? [...new Set([...selected, uid])] : selected.filter(id => id !== uid); }
  async function save() {
    busy = true; error = '';
    try {
      const latest = await loadPreferences();
      if (JSON.stringify(latest.preferences.module_reading?.[moduleKey] || null) !== original) {
        throw new Error('Reading settings for this module changed in another window. Reload settings to review them before saving.');
      }
      const next = withBoldFields(latest.preferences.module_reading || {}, moduleKey, selected);
      const result = await savePreferences({ ...latest.preferences, module_reading: next }, latest.revision);
      if (destroyed) return;
      onSaved(result.preferences.module_reading || {});
      onClose();
    } catch (reason) { if (!destroyed) error = reason instanceof Error ? reason.message : 'Could not save your reading settings.'; }
    finally { if (!destroyed) busy = false; }
  }
</script>

<svelte:window onkeydown={event => { if (event.key === 'Escape' && !busy) { event.preventDefault(); onClose(); } }} />
<div class="overlay" role="presentation" onclick={event => { if (event.currentTarget === event.target && !busy) onClose(); }}>
  <div class="dialog reading-dialog" role="dialog" aria-modal="true" aria-labelledby="module-reading-title" tabindex="-1" use:focusDialog>
    <header class="dialog-head"><div><p class="eyebrow">Personal reading settings</p><h2 id="module-reading-title">Make {moduleName} easier to read</h2><p>Only your view changes.</p></div><button class="icon-button" type="button" aria-label="Close reading settings" disabled={busy} onclick={onClose}><X size={20}/></button></header>
    <div class="reading-body">
      {#if error}<div class="notice error" role="alert">{error}<button class="button small" type="button" disabled={busy||loading} onclick={()=>void load()}>Reload settings</button></div>{/if}
      {#if loading}<p role="status">Loading your preferences…</p>{:else}
        <div class="reading-intro"><Bold size={20}/><div><h3>Choose fields to show in bold</h3><p>Emphasize labels and values in the table, record form and history.</p></div></div>
        <label>Find a field<input type="search" bind:value={search} placeholder="Search field names…" disabled={busy}/></label>
        <div class="reading-actions"><button class="button small" type="button" disabled={busy} onclick={()=>selected=[...new Set([...selected,...visibleFields.map(field=>field.uid)])]}>Bold shown fields</button><button class="button small" type="button" disabled={busy} onclick={()=>selected=selected.filter(uid=>!visibleFields.some(field=>field.uid===uid))}>Normal shown fields</button><span>{fields.filter(field=>selected.includes(field.uid)).length} bold</span></div>
        <div class="reading-fields">{#each visibleFields as field (field.uid)}<label class="reading-field"><input type="checkbox" checked={selected.includes(field.uid)} disabled={busy} aria-label={`Bold ${field.label}`} onchange={event=>toggle(field.uid,event.currentTarget.checked)}/><span class:bold={selected.includes(field.uid)}>{field.label}<small>{field.field_type.replaceAll('_',' ')}</small></span><span class="weight-preview" class:bold={selected.includes(field.uid)}>{selected.includes(field.uid)?'Bold':'Normal'}</span></label>{:else}<p class="muted">No fields match your search.</p>{/each}</div>
      {/if}
    </div>
    <footer class="dialog-actions"><button class="button" type="button" disabled={busy} onclick={onClose}>Cancel</button><span class="spacer"></span><button class="button primary" type="button" disabled={busy||loading} onclick={()=>void save()}>{busy?'Saving…':'Save my reading settings'}</button></footer>
  </div>
</div>

<style>
  .reading-dialog{width:min(40rem,100%);max-height:calc(100dvh - 2rem);display:block;overflow:auto;padding:0;}
  .dialog-head,.dialog-actions{margin:0;padding:1rem 1.2rem;flex-shrink:0;}.dialog-head{align-items:flex-start;border-bottom:1px solid var(--line);}.dialog-head>div{min-width:0;}.dialog-head h2{font-size:1.1rem;overflow-wrap:anywhere;}.dialog-head p:last-child{font-size:.85rem;line-height:1.55;color:var(--muted);margin:.4rem 0 0;}
  .reading-body{padding:1.2rem;display:grid;gap:1rem;min-height:0;}.reading-body>label{display:grid;gap:.4rem;}.reading-body input[type=search]{width:100%;}
  .reading-intro{display:flex;gap:.75rem;align-items:flex-start;}.reading-intro :global(svg){flex-shrink:0;color:var(--primary-text);}.reading-intro h3{margin:0 0 .3rem;font-size:1rem;}.reading-intro p{margin:0;color:var(--muted);font-size:.85rem;line-height:1.55;}
  .reading-actions{display:flex;gap:.5rem;align-items:center;flex-wrap:wrap;}.reading-actions>span{margin-left:auto;color:var(--muted);font-size:.8rem;}
  .reading-fields{display:grid;border:1px solid var(--line);border-radius:.6rem;overflow:hidden;}.reading-field{display:flex;gap:.7rem;align-items:center;min-height:3.6rem;padding:.7rem .8rem;border-bottom:1px solid var(--line);cursor:pointer;font-size:.9rem;font-weight:400;}.reading-field:last-child{border-bottom:0;}.reading-field>span{min-width:0;overflow-wrap:anywhere;}.reading-field>span:first-of-type{flex:1;}.reading-field small{display:block;font-size:.75rem;color:var(--muted);font-weight:400;margin-top:.2rem;}.weight-preview{font-size:.8rem;}.bold{font-weight:750;}
  .notice.error{display:grid;gap:.6rem;}.notice.error button{justify-self:start;}.dialog-actions{border-top:1px solid var(--line);flex-wrap:wrap;}
  @media(max-width:480px){.dialog-head,.dialog-actions,.reading-body{padding:1rem;}.dialog-actions .spacer{display:none;}.dialog-actions button{flex:1;}.weight-preview{display:none;}.reading-field{align-items:flex-start;}.reading-field input{margin-top:.2rem;}}
</style>
