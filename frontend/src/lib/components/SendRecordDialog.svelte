<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import Send from '@lucide/svelte/icons/send';
  import FileText from '@lucide/svelte/icons/file-text';
  import type { CollaborationPerson } from '../api/domain';
  import { createDirectChannel, listCollaborationPeople, sendMessage } from '../api/workspace';
  import { operationUid } from '../api/operation';
  import { ApiError } from '../api/client';
  import { focusDialog } from '../util/focusDialog';

  let { recordUid, recordLabel, hasUnsavedChanges = false, onClose }: {
    recordUid: string; recordLabel: string; hasUnsavedChanges?: boolean; onClose: () => void;
  } = $props();
  let search = $state('');
  let people = $state<CollaborationPerson[]>([]);
  let recipient = $state<CollaborationPerson | null>(null);
  let note = $state('');
  let loading = $state(true);
  let more = $state(false);
  let busy = $state(false);
  let error = $state('');
  let peopleError = $state('');
  let sent = $state(false);
  let doneButton = $state<HTMLButtonElement>();
  // Keep the exact operation and payload after an uncertain response. Retry
  // checks the original send rather than creating another message/notification.
  let submission = $state<{ uid: string; person: CollaborationPerson; body: string; channel?: string } | null>(null);
  let generation = 0;
  let destroyed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onMount(() => void findPeople());
  onDestroy(() => { destroyed = true; generation++; clearTimeout(timer); });
  async function findPeople() {
    const request = ++generation;
    loading = true; peopleError = '';
    try {
      const response = await listCollaborationPeople(search.trim(), 50);
      if (destroyed || request !== generation) return;
      people = response.people; more = response.has_more === true;
    } catch (reason) {
      if (!destroyed && request === generation) peopleError = reason instanceof Error ? reason.message : 'Could not find people.';
    } finally { if (!destroyed && request === generation) loading = false; }
  }
  function scheduleSearch() { generation++; loading = true; clearTimeout(timer); timer = setTimeout(() => void findPeople(), 250); }
  async function send(event: SubmitEvent) {
    event.preventDefault();
    if (busy || sent || (!submission && !recipient)) return;
    if (!submission && recipient) submission = { uid: operationUid(), person: recipient, body: note.trim() || 'Please view this record.' };
    const attempt = submission!;
    busy = true; error = '';
    try {
      if (!attempt.channel) attempt.channel = (await createDirectChannel(attempt.person.uid)).uid;
      await sendMessage(attempt.channel, attempt.body, [], [recordUid], null, attempt.uid);
      if (!destroyed) { sent = true; await tick(); if(!destroyed)doneButton?.focus(); }
    } catch (reason) {
      if (!destroyed) {
        const rejected = reason instanceof ApiError && [400,403,404,413,422].includes(reason.status);
        if (rejected) submission = null;
        error = `${reason instanceof Error ? reason.message : 'The send could not be confirmed.'}${rejected ? ' Nothing was sent. You can adjust your message or recipient.' : ' Retry to confirm the same message without sending a duplicate.'}`;
      }
    } finally { if (!destroyed) busy = false; }
  }
</script>

<svelte:window onkeydown={event => { if (event.key === 'Escape' && !busy) { event.preventDefault(); event.stopImmediatePropagation(); onClose(); } }}/>
<div class="overlay send-record-overlay" role="presentation" onclick={event => { if (event.target === event.currentTarget && !busy) onClose(); }}>
  <div class="dialog send-record-dialog" role="dialog" aria-modal="true" aria-labelledby="send-record-title" tabindex="-1" use:focusDialog>
    <header class="dialog-head"><div><p class="eyebrow">Collaboration</p><h2 id="send-record-title">Send record</h2></div><button class="icon-button" type="button" aria-label="Close send record" disabled={busy} onclick={onClose}><X size={19}/></button></header>
    {#if sent}
      <div class="send-body" role="status"><h3>Sent to {submission?.person.name}</h3><p>Your message and a record card are now in your direct conversation.</p><p class="muted">Their existing access still applies. This did not give them any new permissions.</p></div>
      <footer class="dialog-actions"><button bind:this={doneButton} class="button primary" type="button" onclick={onClose}>Done</button></footer>
    {:else}
    <form onsubmit={send}>
      <div class="send-body">
        <div class="record-card"><FileText size={22}/><div><strong>{recordLabel}</strong><small>Live link to the saved record</small></div></div>
        {#if hasUnsavedChanges}<p class="notice">You have unsaved changes. This sends the saved record, not your unfinished edits.</p>{/if}
        {#if error}<div class="notice error" role="alert">{error}</div>{/if}
        {#if recipient}<div class="selected-person"><div><small>To</small><strong>{recipient.name}</strong></div>{#if !submission}<button class="button small" type="button" onclick={() => recipient = null}>Change person</button>{/if}</div>
        {:else}
          <label>Send to<input type="search" bind:value={search} oninput={scheduleSearch} placeholder="Find a person by name…" autocomplete="off"/></label>
          {#if peopleError}<div class="notice error" role="alert">{peopleError}<button class="button small" type="button" onclick={()=>void findPeople()}>Retry search</button></div>
          {:else if loading}<p role="status">Finding people…</p>
          {:else}<div class="people-list" aria-label="People">{#each people as person (person.uid)}<button type="button" onclick={() => recipient = person}><span class="person-initial" aria-hidden="true">{person.name.slice(0,1).toLocaleUpperCase()}</span><span>{person.name}</span></button>{:else}<p>No people found. Try another name.</p>{/each}</div>{#if more}<small class="muted">Showing the first 50 matches. Type more of their name to narrow the search.</small>{/if}{/if}
        {/if}
        <label><span>Message <span class="optional">(optional)</span></span><textarea bind:value={note} disabled={!!submission} rows="3" maxlength="20000" placeholder="What should they know about this record?"></textarea></label>
        <p class="access-note">People need existing module access to view the record. Sending does not grant access.</p>
      </div>
      <footer class="dialog-actions"><button class="button" type="button" disabled={busy} onclick={onClose}>Cancel</button><span class="spacer"></span><button class="button primary" type="submit" disabled={busy||!recipient}><Send size={16}/>{busy?'Sending…':submission?'Retry same send':'Send record'}</button></footer>
    </form>
    {/if}
  </div>
</div>

<style>
  .send-record-overlay{z-index:180;}.send-record-dialog{padding:0;width:min(34rem,100%);max-height:calc(100dvh - 2rem);overflow:auto;}
  .dialog-head,.dialog-actions{padding:1rem 1.2rem;margin:0;}.dialog-head{border-bottom:1px solid var(--line);}.dialog-head h2{font-size:1.2rem;}.dialog-actions{border-top:1px solid var(--line);display:flex;flex-wrap:wrap;gap:.6rem;}
  .send-body{padding:1.2rem;display:grid;gap:1rem;min-width:0;}.send-body p{margin:0;font-size:.875rem;line-height:1.6;}.send-body label{display:grid;gap:.4rem;min-width:0;}.send-body input,.send-body textarea{width:100%;}.optional{font-weight:400;color:var(--muted);}
  .record-card,.selected-person{display:flex;gap:.8rem;align-items:center;padding:.8rem;background:var(--surface-2);border:1px solid var(--line);border-radius:.55rem;}.record-card :global(svg){flex-shrink:0;}.record-card div,.selected-person div{min-width:0;flex:1;}.record-card strong,.selected-person strong{display:block;overflow-wrap:anywhere;}.record-card small,.selected-person small{display:block;color:var(--muted);font-size:.75rem;}.selected-person{flex-wrap:wrap;}
  .people-list{display:grid;max-height:12rem;overflow:auto;border:1px solid var(--line);border-radius:.5rem;}.people-list button{display:flex;align-items:center;gap:.7rem;border:0;border-bottom:1px solid var(--line);padding:.65rem .8rem;min-height:2.8rem;background:transparent;color:var(--text);font:inherit;text-align:left;cursor:pointer;}.people-list button:last-child{border-bottom:0;}.people-list button:hover,.people-list button:focus-visible{background:var(--surface-2);}.people-list button>span:last-child{min-width:0;overflow-wrap:anywhere;}.people-list p{padding:.8rem;}.person-initial{display:grid;place-items:center;flex-shrink:0;width:2rem;height:2rem;border-radius:50%;background:var(--surface-2);font-size:.8rem;}.access-note{color:var(--muted);}.notice{overflow-wrap:anywhere;}
</style>
