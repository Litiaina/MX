<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import AtSign from '@lucide/svelte/icons/at-sign';
  import Check from '@lucide/svelte/icons/check';
  import CheckCheck from '@lucide/svelte/icons/check-check';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FileText from '@lucide/svelte/icons/file-text';
  import Hash from '@lucide/svelte/icons/hash';
  import Info from '@lucide/svelte/icons/info';
  import Link2 from '@lucide/svelte/icons/link-2';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import Send from '@lucide/svelte/icons/send';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UserRound from '@lucide/svelte/icons/user-round';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import X from '@lucide/svelte/icons/x';
  import type { ChannelReadState, ChatMessage, CollaborationChannel, CollaborationPerson, MessageFile, MessageRecordLink } from '../api/domain';
  import type { Session } from '../api/types';
  import type { LiveMessage } from '../live/client';
  import { createChannel, createDirectChannel, deleteMessage, download, editMessage, listChannels, listCollaborationPeople, listMessages, loadPresence, markChannelRead, previewMessageFile, sendMessage, uploadMessageFile } from '../api/workspace';
  import FilePreview from './FilePreview.svelte';
  import SharedImageThumbnail from './SharedImageThumbnail.svelte';

  let { session, openChannelUid = '', revision = 0, connected = false, liveEvent = null }: { session: Session; openChannelUid?: string; revision?: number; connected?: boolean; liveEvent?: LiveMessage | null } = $props();
  type ConversationFilter = 'all' | 'direct' | 'spaces';
  type DeliveryReceipt = { seen: boolean; label: string; detail: string };

  let channels = $state<CollaborationChannel[]>([]); let selected = $state<CollaborationChannel | null>(null);
  let messages = $state<ChatMessage[]>([]); let readStates = $state<ChannelReadState[]>([]); let users = $state<CollaborationPerson[]>([]); let onlineUserUids = $state<string[]>([]); let body = $state('');
  let pendingFiles = $state<File[]>([]); let loading = $state(true); let conversationLoading = $state(false); let sending = $state(false); let error = $state('');
  let hasMoreMessages = $state(false); let reachedBeginning = $state(false); let loadingOlder = $state(false);
  let section = $state<ConversationFilter>('all'); let conversationSearch = $state(''); let detailsOpen = $state(false); let toolsOpen = $state(false);
  let recordLinkInput = $state(''); let pendingRecordUids = $state<string[]>([]); let mentionUser = $state(''); let pendingMentionUids = $state<string[]>([]);
  let createOpen = $state(false); let channelName = $state(''); let channelDescription = $state(''); let selectedMembers = $state<string[]>([]); let memberSearch = $state('');
  let directUser = $state(''); let messageList = $state<HTMLDivElement>(); let composerForm = $state<HTMLFormElement>();
  let previewFile = $state<MessageFile | null>(null);
  let messageMenuUid = $state(''); let editingMessageUid = $state(''); let editingBody = $state(''); let messageActionBusy = $state(''); let deleteCandidate = $state<ChatMessage | null>(null);
  let liveRefreshPromise: Promise<void> | null = null; let liveRefreshQueued = false;
  const pendingPreviewUrls = new Map<File, string>();

  const visibleChannels = $derived(channels.filter((channel) => {
    const correctSection = section === 'all' || (section === 'direct' ? channel.kind === 'direct' : channel.kind !== 'direct');
    const needle = conversationSearch.trim().toLocaleLowerCase();
    return correctSection && (!needle || channel.name.toLocaleLowerCase().includes(needle) || channel.description.toLocaleLowerCase().includes(needle));
  }));
  const visiblePeople = $derived(users.filter((user) => !memberSearch.trim() || user.name.toLocaleLowerCase().includes(memberSearch.trim().toLocaleLowerCase())));
  const sharedFiles = $derived(messages.filter((message) => !message.deleted_at).flatMap((message) => message.files).filter((file, index, items) => items.findIndex((item) => item.uid === file.uid) === index));
  const sharedRecords = $derived(messages.filter((message) => !message.deleted_at).flatMap((message) => message.record_links).filter((record, index, items) => items.findIndex((item) => item.uid === record.uid) === index));

  onMount(() => {
    void initialize();
    const handleVisibility = () => { if (document.visibilityState === 'visible' && selected) void refreshLive(); };
    const handleOutsideMessageMenu = (event: PointerEvent) => {
      if (!messageMenuUid) return;
      const owner = event.target instanceof Element ? event.target.closest('[data-message-actions]')?.getAttribute('data-message-actions') : null;
      if (owner !== messageMenuUid) messageMenuUid = '';
    };
    const handleMessageMenuKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && messageMenuUid) { event.preventDefault(); messageMenuUid = ''; }
    };
    document.addEventListener('visibilitychange', handleVisibility);
    document.addEventListener('pointerdown', handleOutsideMessageMenu);
    document.addEventListener('keydown', handleMessageMenuKey);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibility);
      document.removeEventListener('pointerdown', handleOutsideMessageMenu);
      document.removeEventListener('keydown', handleMessageMenuKey);
      releasePendingPreviews();
    };
  });
  $effect(() => {
    revision;
    const event = liveEvent;
    if (!loading) untrack(() => { applyLiveEvent(event); void refreshLive(); });
  });
  $effect(() => { if (!loading && openChannelUid && selected?.uid !== openChannelUid) { const channel = channels.find((item) => item.uid === openChannelUid); if (channel) void openConversation(channel); } });

  async function initialize() {
    try {
      const [channelData, peopleData, presenceData] = await Promise.all([
        listChannels(),
        listCollaborationPeople(),
        loadPresence().catch(() => null)
      ]);
      channels = channelData.channels;
      users = peopleData.people;
      if (presenceData) onlineUserUids = presenceData.accounts.map((account) => account.uid);
      const initial = channels.find((item) => item.uid === openChannelUid) || channels[0];
      if (initial) await openConversation(initial);
    }
    catch (reason) { fail(reason); } finally { loading = false; }
  }
  function refreshLive(): Promise<void> {
    if (liveRefreshPromise) { liveRefreshQueued = true; return liveRefreshPromise; }
    liveRefreshPromise = (async () => {
      do {
        liveRefreshQueued = false;
        await refreshLiveOnce();
      } while (liveRefreshQueued);
    })().finally(() => { liveRefreshPromise = null; });
    return liveRefreshPromise;
  }
  async function refreshLiveOnce() {
    try {
      const stayAtEnd = isNearEnd();
      const [channelData, presenceData] = await Promise.all([listChannels(), loadPresence().catch(() => null)]);
      channels = channelData.channels;
      if (presenceData) onlineUserUids = presenceData.accounts.map((account) => account.uid);
      if (selected) {
        selected = channels.find((item) => item.uid === selected?.uid) || selected;
        const data = await listMessages(selected.uid);
        messages = mergeMessages(messages, data.messages);
        readStates = data.read_states || [];
        hasMoreMessages = !reachedBeginning && (hasMoreMessages || data.has_more);
        await markVisibleConversationRead(selected.uid, stayAtEnd);
        if (stayAtEnd) await scrollEnd();
      }
    }
    catch { /* The live client will retry after reconnecting. */ }
  }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The collaboration operation failed.'; }
  function applyLiveEvent(event: LiveMessage | null) {
    if (!event || !selected || !event.payload || typeof event.payload !== 'object') return;
    const payload = event.payload as Partial<ChatMessage> & { channel_uid?: string; uid?: string };
    if (payload.channel_uid !== selected.uid || !payload.uid) return;
    if (event.type === 'message.deleted') {
      messages = messages.map((message) => message.uid === payload.uid ? { ...message, body: '', files: [], record_links: [], deleted_at: event.at || Date.now() } : message);
    } else if ((event.type === 'message.updated' || event.type === 'message.created') && typeof payload.sender_uid === 'string' && typeof payload.sequence === 'number') {
      messages = mergeMessages(messages, [payload as ChatMessage]);
    }
  }
  async function openConversation(channel: CollaborationChannel) {
    selected = channel; messages = []; readStates = []; hasMoreMessages = false; conversationLoading = true; error = ''; toolsOpen = false; messageMenuUid = ''; editingMessageUid = ''; deleteCandidate = null; reachedBeginning = false;
    try {
      const data = await listMessages(channel.uid);
      messages = data.messages;
      readStates = data.read_states || [];
      hasMoreMessages = data.has_more ?? data.messages.length >= 256;
      reachedBeginning = !hasMoreMessages;
      await markVisibleConversationRead(channel.uid);
      channels = channels.map((item) => item.uid === channel.uid ? { ...item, unread_count: document.visibilityState === 'visible' ? 0 : item.unread_count } : item);
      selected = channels.find((item) => item.uid === channel.uid) || channel;
      conversationLoading = false;
      await scrollEnd();
    }
    catch (reason) { fail(reason); } finally { conversationLoading = false; }
  }
  async function markVisibleConversationRead(channelUid: string, newestMessagesAreVisible = true) {
    if (document.visibilityState !== 'visible' || !newestMessagesAreVisible) return;
    const result = await markChannelRead(channelUid);
    readStates = [...readStates.filter((state) => state.user_uid !== result.read_state.user_uid), result.read_state];
    channels = channels.map((item) => item.uid === channelUid ? { ...item, unread_count: 0 } : item);
  }
  function isNearEnd() { return !messageList || messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight < 120; }
  async function scrollEnd() { await tick(); if (messageList) messageList.scrollTop = messageList.scrollHeight; }
  function mergeMessages(current: ChatMessage[], incoming: ChatMessage[]) {
    const byUid = new Map(current.map((message) => [message.uid, message]));
    for (const message of incoming) byUid.set(message.uid, message);
    return [...byUid.values()].sort((left, right) => left.sequence - right.sequence);
  }
  async function loadOlderMessages() {
    if (!selected || loadingOlder || !hasMoreMessages || !messages.length) return;
    const channelUid = selected.uid;
    const before = messages[0].sequence;
    const previousHeight = messageList?.scrollHeight || 0;
    const previousTop = messageList?.scrollTop || 0;
    loadingOlder = true;
    try {
      const data = await listMessages(channelUid, before);
      if (selected?.uid !== channelUid) return;
      messages = mergeMessages(data.messages, messages);
      readStates = data.read_states || readStates;
      hasMoreMessages = data.has_more ?? data.messages.length >= 256;
      reachedBeginning = !hasMoreMessages;
      await tick();
      if (messageList) messageList.scrollTop = previousTop + messageList.scrollHeight - previousHeight;
    } catch (reason) { fail(reason); } finally { loadingOlder = false; }
  }
  function messageListScrolled() { if (messageList && messageList.scrollTop < 180) void loadOlderMessages(); }
  function fileSize(value: number) { if (value < 1024) return `${value} B`; if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`; return `${(value / 1024 ** 2).toFixed(1)} MB`; }
  function isImageFile(file: MessageFile) { return file.mime_type.toLocaleLowerCase().startsWith('image/') || /\.(?:avif|bmp|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/i.test(file.file_name); }
  function isPendingImage(file: File) { return file.type.toLocaleLowerCase().startsWith('image/') || /\.(?:avif|bmp|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/i.test(file.name); }
  function pendingImageUrl(file: File) { let url = pendingPreviewUrls.get(file); if (!url) { url = URL.createObjectURL(file); pendingPreviewUrls.set(file, url); } return url; }
  function releasePendingPreview(file: File) { const url = pendingPreviewUrls.get(file); if (url) URL.revokeObjectURL(url); pendingPreviewUrls.delete(file); }
  function releasePendingPreviews() { for (const url of pendingPreviewUrls.values()) URL.revokeObjectURL(url); pendingPreviewUrls.clear(); }
  function removePendingFile(index: number) { const file = pendingFiles[index]; if (file) releasePendingPreview(file); pendingFiles = pendingFiles.filter((_, item) => item !== index); }
  function clearPendingFiles() { releasePendingPreviews(); pendingFiles = []; }
  function clipboardImageExtension(mimeType: string) { return ({ 'image/jpeg': 'jpg', 'image/svg+xml': 'svg', 'image/tiff': 'tiff' } as Record<string, string>)[mimeType.toLocaleLowerCase()] || mimeType.split('/')[1]?.replace(/[^a-z0-9]/gi, '') || 'png'; }
  function namedClipboardImage(file: File, index: number, timestamp: string) {
    if (file.name.trim() && !/^(?:image|clipboard)(?:\.[a-z0-9]+)?$/i.test(file.name.trim())) return file;
    const suffix = index ? `-${index + 1}` : '';
    return new File([file], `Screenshot-${timestamp}${suffix}.${clipboardImageExtension(file.type)}`, { type: file.type || 'image/png', lastModified: file.lastModified || Date.now() });
  }
  function composerPaste(event: ClipboardEvent) {
    const clipboard = event.clipboardData;
    if (!clipboard) return;
    let images = Array.from(clipboard.items).filter((item) => item.kind === 'file' && item.type.toLocaleLowerCase().startsWith('image/')).map((item) => item.getAsFile()).filter((file): file is File => !!file);
    if (!images.length) images = Array.from(clipboard.files).filter((file) => file.type.toLocaleLowerCase().startsWith('image/'));
    if (!images.length) return;
    event.preventDefault();
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    pendingFiles = [...pendingFiles, ...images.map((file, index) => namedClipboardImage(file, index, timestamp))];
    error = '';
  }
  function initials(value: string) { return value.split(/\s+/).filter(Boolean).map((part) => part[0]).join('').slice(0, 2).toUpperCase() || 'MX'; }
  function dayKey(value: number) { const date = new Date(value); return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`; }
  function dayLabel(value: number) { const date = new Date(value); const today = new Date(); if (dayKey(value) === dayKey(today.getTime())) return 'Today'; const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1); if (dayKey(value) === dayKey(yesterday.getTime())) return 'Yesterday'; return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(date); }
  function clock(value: number) { return new Intl.DateTimeFormat(undefined, { timeStyle: 'short' }).format(value); }
  function isUserOnline(uid: string | null | undefined) { return !!uid && onlineUserUids.includes(uid); }
  function isDirectOnline(channel: CollaborationChannel) { return channel.kind === 'direct' && isUserOnline(channel.direct_user_uid); }
  function onlineMemberCount() { return readStates.filter((state) => isUserOnline(state.user_uid)).length; }
  function conversationPresence(channel: CollaborationChannel) {
    if (channel.kind === 'direct') return isDirectOnline(channel) ? 'Online' : 'Offline';
    if (!connected) return 'Reconnecting';
    const count = onlineMemberCount();
    return `${count} online`;
  }
  function conversationPresenceIsOnline(channel: CollaborationChannel) {
    return channel.kind === 'direct' ? isDirectOnline(channel) : connected && onlineMemberCount() > 0;
  }
  function channelKind(channel: CollaborationChannel) { if (channel.kind === 'direct') return isDirectOnline(channel) ? 'Online' : 'Offline'; return `${channel.member_count} member${channel.member_count === 1 ? '' : 's'}`; }
  function deliveryReceipt(message: ChatMessage, index: number): DeliveryReceipt | null {
    if (message.sender_uid !== session.uid || message.deleted_at || messages.slice(index + 1).some((item) => item.sender_uid === session.uid)) return null;
    const readers = readStates.filter((state) => state.user_uid !== session.uid && state.last_read_message_id >= message.sequence);
    if (!readers.length) return { seen: false, label: 'Sent to MX', detail: 'The message is stored securely. A seen receipt appears when another member opens the newest messages.' };
    const readAt = Math.max(...readers.map((state) => state.last_read_at));
    if (selected?.kind === 'direct') return { seen: true, label: `Seen by ${selected.name} · ${clock(readAt)}`, detail: `${selected.name} opened this conversation after the message was sent.` };
    const names = readers.slice(0, 2).map((state) => state.user_name).join(', ');
    const label = `Seen by ${names}${readers.length > 2 ? ` +${readers.length - 2}` : ''} · ${clock(readAt)}`;
    return { seen: true, label, detail: `${readers.length} member${readers.length === 1 ? '' : 's'} read this message.` };
  }
  function toggleMember(uid: string) { selectedMembers = selectedMembers.includes(uid) ? selectedMembers.filter((item) => item !== uid) : [...selectedMembers, uid]; }
  function addRecordLink() {
    const raw = recordLinkInput.trim(); if (!raw) return; let uid = raw;
    try { const decoded = decodeURIComponent(raw); uid = /[?&]record=([^&#]+)/.exec(decoded)?.[1] || decoded; } catch { /* Keep the supplied UID. */ }
    uid = uid.trim(); if (!/^[a-zA-Z0-9-]{8,128}$/.test(uid)) { error = 'Paste a record UID or an MX record link.'; return; }
    if (!pendingRecordUids.includes(uid)) pendingRecordUids = [...pendingRecordUids, uid]; recordLinkInput = ''; error = '';
  }
  function addMention() { if (!mentionUser || pendingMentionUids.includes(mentionUser)) return; pendingMentionUids = [...pendingMentionUids, mentionUser]; mentionUser = ''; }
  function personName(uid: string) { return users.find((user) => user.uid === uid)?.name || uid; }
  function canModerateMessages() { return selected?.role === 'owner' || selected?.role === 'admin'; }
  function canDeleteMessage(message: ChatMessage) { return !message.deleted_at && (message.sender_uid === session.uid || canModerateMessages()); }
  function beginMessageEdit(message: ChatMessage) { messageMenuUid = ''; editingMessageUid = message.uid; editingBody = message.body; void tick().then(() => document.querySelector<HTMLTextAreaElement>(`[data-edit-message="${message.uid}"]`)?.focus()); }
  function cancelMessageEdit() { editingMessageUid = ''; editingBody = ''; }
  async function saveMessageEdit(event: SubmitEvent, message: ChatMessage) {
    event.preventDefault(); const nextBody = editingBody.trim(); if (!nextBody || messageActionBusy) return;
    messageActionBusy = message.uid; error = '';
    try {
      const result = await editMessage(message.uid, nextBody);
      messages = messages.map((item) => item.uid === result.message.uid ? result.message : item);
      cancelMessageEdit();
    } catch (reason) { fail(reason); } finally { messageActionBusy = ''; }
  }
  async function confirmMessageDeletion() {
    const message = deleteCandidate; if (!message || messageActionBusy) return;
    messageActionBusy = message.uid; error = '';
    try {
      await deleteMessage(message.uid);
      messages = messages.map((item) => item.uid === message.uid ? { ...item, body: '', deleted_at: Date.now() } : item);
      if (editingMessageUid === message.uid) cancelMessageEdit();
      deleteCandidate = null; messageMenuUid = '';
    } catch (reason) { fail(reason); } finally { messageActionBusy = ''; }
  }
  function closeCreate() { createOpen = false; channelName = ''; channelDescription = ''; selectedMembers = []; memberSearch = ''; }
  function closeCreateFromBackdrop(event: MouseEvent) { if (event.target === event.currentTarget) closeCreate(); }
  async function addChannel(event: SubmitEvent) {
    event.preventDefault();
    try { const result = await createChannel({ name: channelName.trim(), description: channelDescription.trim(), member_uids: selectedMembers, kind: 'group' }); closeCreate(); channels = (await listChannels()).channels; const channel = channels.find((item) => item.uid === result.uid); if (channel) await openConversation(channel); }
    catch (reason) { fail(reason); }
  }
  async function startDirect() {
    if (!directUser) return;
    try { const result = await createDirectChannel(directUser); directUser = ''; channels = (await listChannels()).channels; const channel = channels.find((item) => item.uid === result.uid); if (channel) await openConversation(channel); }
    catch (reason) { fail(reason); }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault(); if (!selected || (!body.trim() && !pendingFiles.length && !pendingRecordUids.length)) return; sending = true; error = '';
    try {
      const channelUid = selected.uid;
      const messageText = body.trim() || (pendingFiles.length ? `Shared ${pendingFiles.length} file${pendingFiles.length === 1 ? '' : 's'}` : `Shared ${pendingRecordUids.length} record${pendingRecordUids.length === 1 ? '' : 's'}`);
      const result = await sendMessage(channelUid, messageText, pendingMentionUids, pendingRecordUids); for (const file of pendingFiles) await uploadMessageFile(result.message.uid, file);
      body = ''; clearPendingFiles(); pendingRecordUids = []; pendingMentionUids = []; toolsOpen = false;
      const data = await listMessages(channelUid); messages = mergeMessages(messages, data.messages); readStates = data.read_states || [];
      channels = (await listChannels()).channels; selected = channels.find((item) => item.uid === channelUid) || selected;
      await markVisibleConversationRead(channelUid); await scrollEnd();
    } catch (reason) { fail(reason); } finally { sending = false; }
  }
  function composerKeydown(event: KeyboardEvent) { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); composerForm?.requestSubmit(); } }
  function downloadSharedFile(file: MessageFile) { void download(`/mx/v1/collaboration/files/${file.uid}/download`, file.file_name); }
  function showSharedFile(file: MessageFile) { previewFile = file; }
  function recordHref(record: MessageRecordLink) { return `#module/${encodeURIComponent(record.module_uid)}?record=${encodeURIComponent(record.uid)}`; }
</script>

<section class="workspace-page collaboration-page">
  <div class="collaboration-shell">
    <nav class="collaboration-rail" aria-label="Collaboration filters">
      <div class="collaboration-mark" title="MX Connect">MX</div>
      <button class:active={section === 'all'} onclick={() => section = 'all'} aria-label="All conversations"><span><MessagesSquare size={16} /></span><small>All</small></button>
      <button class:active={section === 'direct'} onclick={() => section = 'direct'} aria-label="Direct messages"><span><UserRound size={16} /></span><small>People</small></button>
      <button class:active={section === 'spaces'} onclick={() => section = 'spaces'} aria-label="Groups and channels"><span><UsersRound size={16} /></span><small>Spaces</small></button>
      <button class="rail-new" onclick={() => createOpen = true} aria-label="Create a group"><span><Plus size={17} /></span><small>New</small></button>
    </nav>

    <aside class="conversation-sidebar">
      <header><div><p class="eyebrow">MX Connect</p><h1>{section === 'direct' ? 'People' : section === 'spaces' ? 'Spaces' : 'Conversations'}</h1></div><button class="collaboration-icon-button" onclick={() => createOpen = true} title="Create group" aria-label="Create group"><Plus size={17} /></button></header>
      <div class="conversation-controls">
        <label class="conversation-search"><span><Search size={14} /></span><input bind:value={conversationSearch} placeholder="Find a conversation" aria-label="Find a conversation" /></label>
        <div class="direct-start"><select bind:value={directUser} aria-label="Start a direct message"><option value="">Message a person…</option>{#each users as user}<option value={user.uid}>{user.name}</option>{/each}</select><button class="button small" onclick={startDirect} disabled={!directUser}>Start</button></div>
      </div>
      <div class="conversation-list-heading"><span>{visibleChannels.length} conversation{visibleChannels.length === 1 ? '' : 's'}</span><button onclick={() => void refreshLive()} aria-label="Refresh conversations" title="Refresh"><RefreshCw size={13} /></button></div>
      <nav class="conversation-list" aria-label="Conversations">
        {#if loading}<div class="conversation-skeleton" aria-label="Loading conversations"><i></i><i></i><i></i></div>
        {:else if !visibleChannels.length}<div class="empty-state compact"><strong>Nothing here yet</strong><p>{conversationSearch ? 'Try a different search.' : 'Start a direct message or create a space.'}</p></div>
        {:else}{#each visibleChannels as channel}<button class:active={selected?.uid === channel.uid} onclick={() => openConversation(channel)}><span class:direct={channel.kind === 'direct'} class="channel-glyph">{#if channel.kind === 'direct'}{initials(channel.name)}{#if isDirectOnline(channel)}<i class="avatar-presence" aria-label="Online"></i>{/if}{:else if channel.kind === 'channel'}<Hash size={15} />{:else}<UsersRound size={15} />{/if}</span><span class="channel-summary"><strong>{channel.name}</strong><small class:online={isDirectOnline(channel)}>{channelKind(channel)}</small></span><span class="channel-state">{#if channel.unread_count}<b>{channel.unread_count > 99 ? '99+' : channel.unread_count}</b>{:else if channel.last_message_at}<time>{clock(channel.last_message_at)}</time>{/if}</span></button>{/each}{/if}
      </nav>
    </aside>

    <div class:show-details={detailsOpen && !!selected} class="collaboration-content">
      <main class="conversation-main">
        {#if error}<div class="collaboration-error notice error"><span>{error}</span><button onclick={() => error = ''} aria-label="Dismiss error"><X size={15} /></button></div>{/if}
        {#if selected}
          <header><div class="conversation-title"><span class:direct={selected.kind === 'direct'} class="channel-glyph large">{#if selected.kind === 'direct'}{initials(selected.name)}{#if isDirectOnline(selected)}<i class="avatar-presence" aria-label="Online"></i>{/if}{:else if selected.kind === 'channel'}<Hash size={20} />{:else}<UsersRound size={20} />{/if}</span><div><h2>{selected.name}</h2><p class:online={isDirectOnline(selected)}>{selected.description || channelKind(selected)}</p></div></div><div class="conversation-actions"><span class:online={conversationPresenceIsOnline(selected)} class="live-label"><i></i>{conversationPresence(selected)}</span><button class="collaboration-icon-button" onclick={() => void refreshLive()} title="Refresh conversation" aria-label="Refresh conversation"><RefreshCw size={15} /></button><button class:active={detailsOpen} class="collaboration-icon-button details-toggle" onclick={() => detailsOpen = !detailsOpen} title="Conversation details" aria-label="Toggle conversation details"><Info size={16} /></button></div></header>
          <div class="message-list" bind:this={messageList} onscroll={messageListScrolled}>
            {#if conversationLoading}
              <div class="message-ghost" role="status" aria-label={`Loading conversation with ${selected.name}`}>
                <div class="message-ghost-day"><span></span><i></i><span></span></div>
                <article class="message-ghost-row">
                  <i class="message-ghost-avatar"></i>
                  <div class="message-ghost-bubble short"><header><i></i><i></i></header><span></span><span></span></div>
                </article>
                <article class="message-ghost-row mine">
                  <i class="message-ghost-avatar"></i>
                  <div class="message-ghost-bubble"><header><i></i><i></i></header><span></span><span></span><div class="message-ghost-file"><i></i><span></span></div></div>
                </article>
                <article class="message-ghost-row">
                  <i class="message-ghost-avatar"></i>
                  <div class="message-ghost-bubble medium"><header><i></i><i></i></header><span></span><span></span></div>
                </article>
                <p>Loading messages securely…</p>
              </div>
            {:else if !messages.length}<div class="conversation-welcome"><span class:direct={selected.kind === 'direct'} class="channel-glyph large">{#if selected.kind === 'direct'}{initials(selected.name)}{:else if selected.kind === 'channel'}<Hash size={20} />{:else}<UsersRound size={20} />{/if}</span><h3>{selected.name}</h3><p>This is the beginning of the conversation. Share an update, a document, or an MX record.</p></div>
            {:else}
            {#if loadingOlder}<div class="message-history-ghost" role="status" aria-label="Loading earlier messages"><i></i><span><b></b><b></b></span><small>Loading earlier messages…</small></div>{:else if hasMoreMessages}<div class="message-history-hint">Scroll upward for earlier messages</div>{:else}<div class="message-history-hint beginning">Beginning of conversation</div>{/if}
            {#each messages as message, index}
              {#if index === 0 || dayKey(messages[index - 1].created_at) !== dayKey(message.created_at)}<div class="message-day"><span>{dayLabel(message.created_at)}</span></div>{/if}
              {@const receipt = deliveryReceipt(message, index)}
              <article data-message-uid={message.uid} class:mine={message.sender_uid === session.uid} class:deleted={!!message.deleted_at} class="chat-message"><div class="message-avatar">{initials(message.sender_name)}</div><div class="message-content"><header><span class="message-author"><strong>{message.sender_name}</strong><time>{clock(message.created_at)}{message.edited_at && !message.deleted_at ? ' · edited' : ''}</time></span>{#if canDeleteMessage(message)}<span class="message-actions" data-message-actions={message.uid}><button class="message-action-trigger" type="button" onclick={() => messageMenuUid = messageMenuUid === message.uid ? '' : message.uid} aria-label={`Message actions for ${message.sender_name}`} aria-expanded={messageMenuUid === message.uid}><Ellipsis size={15} /></button>{#if messageMenuUid === message.uid}<span class="message-action-menu">{#if message.sender_uid === session.uid}<button type="button" onclick={() => beginMessageEdit(message)}><Pencil size={13} />Edit message</button>{/if}<button class="danger-text" type="button" onclick={() => { deleteCandidate = message; messageMenuUid = ''; }}><Trash2 size={13} />Delete message</button></span>{/if}</span>{/if}</header>{#if message.deleted_at}<p class="deleted-message"><Trash2 size={13} /><em>Message deleted</em></p>{:else if editingMessageUid === message.uid}<form class="message-edit-form" onsubmit={(event) => saveMessageEdit(event, message)}><textarea data-edit-message={message.uid} bind:value={editingBody} rows="3" maxlength="20000" aria-label="Edit message"></textarea><div><small>Edited messages remain marked as edited.</small><button class="button small" type="button" onclick={cancelMessageEdit}>Cancel</button><button class="button primary small" disabled={!editingBody.trim() || messageActionBusy === message.uid}>{messageActionBusy === message.uid ? 'Saving…' : 'Save'}</button></div></form>{:else}<p>{message.body}</p>{#if message.record_links.length}<div class="shared-records">{#each message.record_links as record}<a href={recordHref(record)}><span><FileText size={15} /></span><div><strong>{record.label}</strong><small>{record.module_name} · Open {record.singular_name.toLowerCase()}</small></div><i><ChevronRight size={15} /></i></a>{/each}</div>{/if}{#if message.files.length}<div class="shared-files">{#each message.files as file}{#if isImageFile(file)}<SharedImageThumbnail {file} onOpen={() => showSharedFile(file)} />{:else}<div class="shared-file-item"><button class="shared-file-preview" type="button" onclick={() => showSharedFile(file)} title={`Preview ${file.file_name}`}><span><FileText size={15} /></span><div><strong>{file.file_name}</strong><small>{file.mime_type} · {fileSize(file.size)} · Preview</small></div></button><button class="shared-file-download" type="button" onclick={() => downloadSharedFile(file)} aria-label={`Download ${file.file_name}`} title={`Download ${file.file_name}`}><DownloadIcon size={15} /></button></div>{/if}{/each}</div>{/if}{/if}{#if receipt}<div class:seen={receipt.seen} class="message-receipt" title={receipt.detail}>{#if receipt.seen}<CheckCheck size={13} strokeWidth={2.4} />{:else}<Check size={13} strokeWidth={2.4} />{/if}<span>{receipt.label}</span></div>{/if}</div></article>
            {/each}{/if}
          </div>
          <form class="message-composer" bind:this={composerForm} onsubmit={submit}>
            {#if pendingFiles.length || pendingRecordUids.length || pendingMentionUids.length}<div class="pending-shares">{#each pendingFiles as file, index}{#if isPendingImage(file)}<div class="pending-image-share"><img src={pendingImageUrl(file)} alt="" /><span><strong>{file.name}</strong><small>Image ready to send</small></span><button type="button" aria-label={`Remove ${file.name}`} onclick={() => removePendingFile(index)}><X size={13} /></button></div>{:else}<span><i><Paperclip size={12} /></i>{file.name}<button type="button" aria-label={`Remove ${file.name}`} onclick={() => removePendingFile(index)}><X size={12} /></button></span>{/if}{/each}{#each pendingRecordUids as uid}<span><i><FileText size={12} /></i>Record · {uid.slice(0, 12)}<button type="button" aria-label={`Remove record ${uid}`} onclick={() => pendingRecordUids = pendingRecordUids.filter((item) => item !== uid)}><X size={12} /></button></span>{/each}{#each pendingMentionUids as uid}<span><i><AtSign size={12} /></i>{personName(uid)}<button type="button" aria-label={`Remove mention ${personName(uid)}`} onclick={() => pendingMentionUids = pendingMentionUids.filter((item) => item !== uid)}><X size={12} /></button></span>{/each}</div>{/if}
            {#if toolsOpen}<div class="composer-context-tools"><label><span>Link an MX record</span><div><input bind:value={recordLinkInput} placeholder="Record UID or copied MX link" aria-label="Record to link" /><button class="button small" type="button" onclick={addRecordLink} disabled={!recordLinkInput.trim()}>Add</button></div></label><label><span>Mention someone</span><div><select bind:value={mentionUser} aria-label="Person to mention"><option value="">Choose a person…</option>{#each users.filter((user) => !pendingMentionUids.includes(user.uid)) as user}<option value={user.uid}>{user.name}</option>{/each}</select><button class="button small" type="button" onclick={addMention} disabled={!mentionUser}>Add</button></div></label></div>{/if}
            <div class="composer-entry"><textarea bind:value={body} onkeydown={composerKeydown} onpaste={composerPaste} rows="2" placeholder={`Message ${selected.name}`} aria-label="Message"></textarea><div class="composer-commandbar"><div><label class="composer-action" title="Attach files"><span><Paperclip size={13} /></span> Attach<input type="file" multiple onchange={(event) => { if (event.currentTarget.files) pendingFiles = [...pendingFiles, ...event.currentTarget.files]; event.currentTarget.value = ''; }} /></label><button class:active={toolsOpen} class="composer-action" type="button" onclick={() => toolsOpen = !toolsOpen}><span><Link2 size={13} /></span> Add context</button></div><span class="composer-hint">Paste screenshots with Ctrl + V · Ctrl + Enter to send</span><button class="button primary send-message icon-label" disabled={sending || (!body.trim() && !pendingFiles.length && !pendingRecordUids.length)}>{#if !sending}<Send size={13} />{/if}{sending ? 'Sending…' : 'Send'}</button></div></div>
          </form>
        {:else}<div class="collaboration-empty"><div class="collaboration-empty-mark">MX</div><h2>Choose a conversation</h2><p>Keep work, documents, and record context together without leaving MX.</p><button class="button primary" onclick={() => createOpen = true}>Create a space</button></div>{/if}
      </main>

      {#if detailsOpen && selected}<aside class="conversation-details"><header><strong>Conversation details</strong><button onclick={() => detailsOpen = false} aria-label="Close details"><X size={15} /></button></header><section class="details-identity"><span class:direct={selected.kind === 'direct'} class="channel-glyph large">{#if selected.kind === 'direct'}{initials(selected.name)}{#if isDirectOnline(selected)}<i class="avatar-presence" aria-label="Online"></i>{/if}{:else if selected.kind === 'channel'}<Hash size={20} />{:else}<UsersRound size={20} />{/if}</span><h3>{selected.name}</h3><p>{selected.description || (selected.kind === 'direct' ? 'A private MX conversation.' : 'A shared MX workspace.')}</p></section><dl>{#if selected.kind === 'direct'}<div><dt>Status</dt><dd class:online={isDirectOnline(selected)}>{isDirectOnline(selected) ? 'Online' : 'Offline'}</dd></div>{/if}<div><dt>Type</dt><dd>{selected.kind === 'direct' ? 'Direct message' : selected.kind === 'channel' ? 'Channel' : 'Group space'}</dd></div><div><dt>Members</dt><dd>{selected.member_count}</dd></div><div><dt>Your access</dt><dd>{selected.role}</dd></div></dl><section class="details-shared"><header><strong>Shared records</strong><span>{sharedRecords.length}</span></header>{#if sharedRecords.length}<div>{#each sharedRecords.slice(-6).reverse() as record}<a href={recordHref(record)}><i><FileText size={14} /></i><span><strong>{record.label}</strong><small>{record.module_name}</small></span></a>{/each}</div>{:else}<p>No records shared yet.</p>{/if}</section><section class="details-shared"><header><strong>Shared files</strong><span>{sharedFiles.length}</span></header>{#if sharedFiles.length}<div>{#each sharedFiles.slice(-6).reverse() as file}<div class="details-file-row"><button type="button" onclick={() => showSharedFile(file)} title={`Preview ${file.file_name}`}><i><FileText size={14} /></i><span><strong>{file.file_name}</strong><small>{fileSize(file.size)} · Preview</small></span></button><button class="details-file-download" type="button" onclick={() => downloadSharedFile(file)} aria-label={`Download ${file.file_name}`} title={`Download ${file.file_name}`}><DownloadIcon size={14} /></button></div>{/each}</div>{:else}<p>No files shared yet.</p>{/if}</section></aside>{/if}
    </div>
  </div>
</section>

{#if previewFile}
  <FilePreview fileName={previewFile.file_name} load={() => previewMessageFile(previewFile!.uid)} onClose={() => previewFile = null} />
{/if}

{#if deleteCandidate}
  <div class="overlay message-delete-overlay" role="presentation" onclick={(event) => { if (event.target === event.currentTarget && !messageActionBusy) deleteCandidate = null; }}>
    <div class="dialog message-delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-message-title" aria-describedby="delete-message-description">
      <div class="delete-message-icon"><Trash2 size={20} /></div>
      <h2 id="delete-message-title">Delete this message?</h2>
      <p id="delete-message-description">The message will become a permanent “Message deleted” entry. It cannot be edited or restored from the conversation.</p>
      <div class="message-delete-preview">{deleteCandidate.body}</div>
      <div class="dialog-actions"><button class="button" type="button" onclick={() => deleteCandidate = null} disabled={!!messageActionBusy}>Cancel</button><button class="button danger" type="button" onclick={confirmMessageDeletion} disabled={!!messageActionBusy}>{messageActionBusy ? 'Deleting…' : 'Delete message'}</button></div>
    </div>
  </div>
{/if}

{#if createOpen}
  <div class="collaboration-modal-backdrop" role="presentation" onclick={closeCreateFromBackdrop}>
    <div class="dialog collaboration-create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-space-title">
      <form onsubmit={addChannel}>
        <header><div><p class="eyebrow">MX Connect</p><h2 id="create-space-title">Create a space</h2><p>Bring the right people, messages, files, and records together.</p></div><button class="icon-button" type="button" onclick={closeCreate} aria-label="Close"><X size={18} /></button></header>
        <label>Space name<input bind:value={channelName} required maxlength="80" placeholder="For example: Operations planning" /></label>
        <label>Description <span class="optional">Optional</span><textarea bind:value={channelDescription} rows="3" maxlength="500" placeholder="What will this space be used for?"></textarea></label>
        <fieldset class="space-member-picker"><legend>Add people <span>{selectedMembers.length} selected</span></legend><label class="conversation-search"><span><Search size={14} /></span><input bind:value={memberSearch} placeholder="Find a person" aria-label="Find a person" /></label><div>{#each visiblePeople as user}<label class="space-member"><input type="checkbox" checked={selectedMembers.includes(user.uid)} onchange={() => toggleMember(user.uid)} /><span class="message-avatar">{initials(user.name)}{#if isUserOnline(user.uid)}<i class="avatar-presence" aria-label="Online"></i>{/if}</span><strong>{user.name}</strong><small class:online={isUserOnline(user.uid)}>{isUserOnline(user.uid) ? 'Online' : 'Offline'}</small></label>{/each}{#if !visiblePeople.length}<p class="muted">No matching people.</p>{/if}</div></fieldset>
        <div class="dialog-actions"><button class="button" type="button" onclick={closeCreate}>Cancel</button><button class="button primary">Create space</button></div>
      </form>
    </div>
  </div>
{/if}
