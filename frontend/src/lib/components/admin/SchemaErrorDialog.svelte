<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import type { FieldKeyConflict } from '../../api/fieldKeys';
  import { focusDialog } from '../../util/focusDialog';

  let { title, message, conflict = null, onClose, onUseKey, onOpenField }: {
    title: string; message: string; conflict?: FieldKeyConflict | null;
    onClose: () => void; onUseKey: (key: string) => void; onOpenField: (conflict: FieldKeyConflict) => void;
  } = $props();
</script>

<svelte:window onkeydown={event => { if (event.key === 'Escape') { event.preventDefault(); onClose(); } }} />
<div class="overlay" role="presentation" onclick={event => { if (event.target === event.currentTarget) onClose(); }}>
  <div class="dialog schema-error-dialog" role="alertdialog" aria-modal="true" aria-labelledby="schema-error-title" aria-describedby="schema-error-message" tabindex="-1" use:focusDialog>
    <header class="dialog-head">
      <div class="error-heading"><ShieldAlert size={22} /><h2 id="schema-error-title">{title}</h2></div>
      <button class="icon-button" type="button" aria-label="Close error" onclick={onClose}><X size={18} /></button>
    </header>
    <div class="error-body">
      <p id="schema-error-message">{message}</p>
      {#if conflict}
        <dl>
          <div><dt>Module</dt><dd>{conflict.field.module_name}</dd></div>
          <div><dt>Existing field</dt><dd>{conflict.field.label}</dd></div>
          <div><dt>Datatype</dt><dd>{conflict.field.field_type}</dd></div>
          <div><dt>Status</dt><dd>{conflict.field.active ? 'Active' : 'Archived · history preserved'}</dd></div>
          <div><dt>Reserved key</dt><dd><code>{conflict.field.key}</code></dd></div>
        </dl>
        <p>Your new field’s settings are retained. Use <code>{conflict.suggested_key}</code> for a separate field, or review the existing field. No field or record has been changed.</p>
      {/if}
    </div>
    <footer class="dialog-actions">
      {#if conflict}
        <button class="button" type="button" onclick={() => onOpenField(conflict)}>View existing field</button>
        <button class="button primary" type="button" onclick={() => onUseKey(conflict.suggested_key)}>Use key {conflict.suggested_key}</button>
      {:else}
        <button class="button primary" type="button" onclick={onClose}>Back to settings</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .schema-error-dialog{width:min(38rem,100%);max-height:calc(100dvh - 2rem);padding:0;display:flex;flex-direction:column;overflow:hidden}
  .dialog-head,.dialog-actions{padding:1rem;margin:0;flex-shrink:0;gap:.65rem}
  .dialog-head{border-bottom:1px solid var(--line)}.dialog-actions{border-top:1px solid var(--line);flex-wrap:wrap}
  .error-heading{display:flex;gap:.65rem;align-items:center;min-width:0}.error-heading :global(svg){color:var(--danger);flex-shrink:0}
  h2{font-size:1.1rem;margin:0;overflow-wrap:anywhere}.error-body{padding:1rem;overflow:auto;min-height:0}
  p{margin:0 0 1rem;overflow-wrap:anywhere}p:last-child{margin-bottom:0}
  dl{margin:0 0 1rem;padding:.75rem;background:var(--surface-2);border:1px solid var(--line);border-radius:.5rem;display:grid;gap:.6rem}
  dl>div{display:grid;grid-template-columns:8rem minmax(0,1fr);gap:.5rem}dt{color:var(--muted)}dd{margin:0;overflow-wrap:anywhere}
  .dialog-actions>button{white-space:normal;overflow-wrap:anywhere;min-width:0}
  @media(max-width:540px){.overlay{padding:.5rem}.schema-error-dialog{max-height:calc(100dvh - 1rem)}.dialog-actions>button{flex:1}dl>div{grid-template-columns:6rem minmax(0,1fr)}}
</style>
