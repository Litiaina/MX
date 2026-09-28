<script lang="ts">
  import { tick } from 'svelte';
  import AlertTriangle from '@lucide/svelte/icons/triangle-alert';
  import CheckCircle2 from '@lucide/svelte/icons/circle-check-big';
  import X from '@lucide/svelte/icons/x';
  import { activeConfirmation, answerConfirmation } from '../confirmation';

  let dialog = $state<HTMLDivElement>();
  let initialFocus = $state<HTMLButtonElement>();
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    if (!$activeConfirmation) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => initialFocus?.focus());
  });

  async function answer(confirmed: boolean) {
    answerConfirmation(confirmed);
    await tick();
    returnFocus?.focus();
    returnFocus = null;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!$activeConfirmation) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      void answer(false);
      return;
    }
    if (event.key !== 'Tab' || !dialog) return;
    const controls = [...dialog.querySelectorAll<HTMLElement>('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')];
    if (!controls.length) return;
    const first = controls[0];
    const last = controls[controls.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $activeConfirmation}
  {@const request = $activeConfirmation}
  {@const dangerous = (request.tone || 'danger') === 'danger'}
  <div class="overlay confirmation-overlay" role="presentation" onclick={(event) => event.currentTarget === event.target && void answer(false)}>
    <div bind:this={dialog} class:dangerous class="dialog confirmation-dialog" role="alertdialog" aria-modal="true" aria-labelledby="mx-confirmation-title" aria-describedby="mx-confirmation-description" tabindex="-1">
      <header>
        <span class="confirmation-icon">{#if dangerous}<AlertTriangle size={21} />{:else}<CheckCircle2 size={21} />{/if}</span>
        <div><p class="eyebrow">Please confirm</p><h2 id="mx-confirmation-title">{request.title}</h2></div>
        <button class="icon-button" type="button" aria-label="Cancel and close" onclick={() => void answer(false)}><X size={18} /></button>
      </header>
      <p id="mx-confirmation-description">{request.description}</p>
      <footer class="dialog-actions">
        <button bind:this={initialFocus} class="button" type="button" onclick={() => void answer(false)}>{request.cancelLabel || 'Cancel'}</button>
        <button class:danger={dangerous} class:primary={!dangerous} class="button" type="button" onclick={() => void answer(true)}>{request.confirmLabel || 'Confirm'}</button>
      </footer>
    </div>
  </div>
{/if}
