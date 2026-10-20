<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import { focusDialog } from '../util/focusDialog';
  import type { DeploymentConfig } from '../api/domain';

  let {
    deployment,
    error,
    busy,
    accentPresets,
    theme = $bindable(),
    accentColor = $bindable(),
    density = $bindable(),
    contentWidth = $bindable(),
    autoScale = $bindable(),
    scale = $bindable(),
    fontScale = $bindable(),
    reducedMotion = $bindable(),
    autoRefresh = $bindable(),
    recommendedScale,
    onClose,
    onThemeChange,
    onAppearanceChange,
    onScaleChange,
    onSave
  }: {
    deployment: DeploymentConfig;
    error: string;
    busy: boolean;
    accentPresets: string[];
    theme: string;
    accentColor: string;
    density: string;
    contentWidth: string;
    autoScale: boolean;
    scale: number;
    fontScale: number;
    reducedMotion: boolean;
    autoRefresh: number;
    recommendedScale: () => number;
    onClose: () => void;
    onThemeChange: (theme: string) => void;
    onAppearanceChange: () => void;
    onScaleChange: () => void;
    onSave: () => Promise<void>;
  } = $props();

  function chooseAccent(color: string) {
    accentColor = color;
    onAppearanceChange();
  }
</script>

<svelte:window onkeydown={event=>{if(event.key==='Escape'&&!busy){event.preventDefault();onClose();}}}/>

<div class="overlay" role="presentation" onclick={(event) => { if (event.currentTarget === event.target) onClose(); }}>
  <div class="dialog settings-dialog" role="dialog" aria-modal="true" aria-labelledby="display-settings-title" tabindex="-1" use:focusDialog>
    <div class="dialog-head">
      <div>
        <p class="eyebrow">Personal preferences</p>
        <h2 id="display-settings-title">Display and refresh</h2>
        <p class="muted">Saved to your MX account and applied over administrator defaults.</p>
      </div>
      <button class="icon-button" aria-label="Close" disabled={busy} onclick={onClose}><X size={19} /></button>
    </div>

    {#if error}<div class="notice error">{error}</div>{/if}

    <form class="form-stack" onsubmit={(event) => { event.preventDefault(); void onSave(); }}>
      <label>
        Theme
        <select bind:value={theme} onchange={() => onThemeChange(theme)}>
          <option value="">Use administrator default</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
          <option value="system">Use system setting</option>
        </select>
      </label>

      <fieldset class="personal-accent">
        <legend>Accent color</legend>
        <div class="accent-swatches">
          <button class:active={!accentColor} type="button" style={`--swatch:${deployment.appearance.primary_color}`} onclick={() => chooseAccent('')} title="Use administrator default" aria-label="Use administrator default accent"><i></i></button>
          {#each accentPresets as color}
            <button class:active={accentColor === color} type="button" style={`--swatch:${color}`} onclick={() => chooseAccent(color)} title={color} aria-label={`Use accent ${color}`}><i></i></button>
          {/each}
          <label class="accent-custom" title="Choose a custom accent">
            <input type="color" value={accentColor || deployment.appearance.primary_color} style={`background:${accentColor || deployment.appearance.primary_color}`} oninput={(event) => chooseAccent(event.currentTarget.value)} />
            <span>Custom</span>
          </label>
        </div>
        <small>{accentColor ? `Personal override · ${accentColor}` : 'Using the administrator default'}</small>
      </fieldset>

      <label>Density<select bind:value={density} onchange={onAppearanceChange}><option value="">Use administrator default</option><option value="compact">Compact</option><option value="normal">Normal</option><option value="comfortable">Comfortable</option></select></label>
      <label>Content width<select bind:value={contentWidth} onchange={onAppearanceChange}><option value="">Use administrator default</option><option value="standard">Standard</option><option value="wide">Wide</option><option value="full">Full width</option></select></label>
      <label class="checkbox setting-checkbox"><input type="checkbox" bind:checked={autoScale} onchange={onScaleChange} /> Automatically fit the interface to this screen</label>
      <label>Interface size: {autoScale ? `${recommendedScale()}% recommended` : `${scale}%`}<input type="range" min="85" max="160" step="5" bind:value={scale} disabled={autoScale} oninput={onScaleChange} /><small>Changes control height, spacing, and navigation geometry without shrinking the workspace.</small></label>
      <fieldset class="text-size-settings"><legend>Text size</legend>
        <p>Enlarges navigation, tables, forms and administration together. Controls grow with the text; document contents keep their own formatting.</p>
        <div class="text-size-presets" role="group" aria-label="Text size presets">{#each [[16,'Standard'],[20,'Large'],[24,'Extra large'],[32,'200%']] as [size,name]}<button class="button" class:active={fontScale===size} type="button" aria-pressed={fontScale===size} onclick={()=>{fontScale=Number(size);onScaleChange();}}>{name}</button>{/each}</div>
        <label>Base text size<input class="font-size-input" type="number" min="10" max="32" step="1" bind:value={fontScale} oninput={onScaleChange} /><small>10–32, relative to a standard base of 16. Your browser's text-size preference is also respected.</small></label>
        <div class="text-size-preview"><strong>Readable at a glance</strong><span>Employee · Request · Status</span><small>This is a live preview of your interface text.</small></div>
        <p>To emphasize particular record fields, open a module and choose <strong>Reading settings</strong>.</p>
      </fieldset>
      <label class="checkbox setting-checkbox"><input type="checkbox" bind:checked={reducedMotion} onchange={onAppearanceChange} /> Reduce animation and motion</label>
      <label>Automatic data refresh<select bind:value={autoRefresh}><option value={0}>Off — live updates only</option><option value={30}>Every 30 seconds</option><option value={60}>Every minute</option><option value={300}>Every 5 minutes</option><option value={900}>Every 15 minutes</option></select></label>

      <div class="dialog-actions">
        <button class="button" type="button" disabled={busy} onclick={onClose}>Cancel</button>
        <span class="spacer"></span>
        <button class="button primary" disabled={busy}>{busy ? 'Saving…' : 'Save preferences'}</button>
      </div>
    </form>
  </div>
</div>

<style>
  .settings-dialog{max-height:calc(100dvh - 2rem);overflow:auto;min-width:0;}
  .text-size-settings{display:grid;gap:.8rem;padding:1rem;border:1px solid var(--line);border-radius:.65rem;min-width:0;}
  .text-size-settings legend{font-weight:700;}.text-size-settings p{margin:0;color:var(--muted);font-size:.85rem;line-height:1.55;}
  .text-size-presets{display:flex;gap:.45rem;flex-wrap:wrap;}.text-size-presets .active{border-color:var(--primary);color:var(--primary-text);background:color-mix(in srgb,var(--primary) 9%,var(--surface));}
  .text-size-preview{display:grid;gap:.35rem;padding:1rem;background:var(--surface-2);border-radius:.45rem;}.text-size-preview strong{font-size:1rem;}.text-size-preview span{font-size:.9rem;}.text-size-preview small{font-size:.8rem;color:var(--muted);}
</style>
