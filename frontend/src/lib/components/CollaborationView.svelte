<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import AtSign from '@lucide/svelte/icons/at-sign';
  import Check from '@lucide/svelte/icons/check';
  import CheckCheck from '@lucide/svelte/icons/check-check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FileText from '@lucide/svelte/icons/file-text';
  import Hash from '@lucide/svelte/icons/hash';
  import Info from '@lucide/svelte/icons/info';
  import Link2 from '@lucide/svelte/icons/link-2';
  import LockKeyhole from '@lucide/svelte/icons/lock-keyhole';
  import LogOut from '@lucide/svelte/icons/log-out';
  import MessageCircle from '@lucide/svelte/icons/message-circle';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Pin from '@lucide/svelte/icons/pin';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Reply from '@lucide/svelte/icons/reply';
  import Save from '@lucide/svelte/icons/save';
  import Search from '@lucide/svelte/icons/search';
  import Send from '@lucide/svelte/icons/send';
  import Shield from '@lucide/svelte/icons/shield';
  import SmilePlus from '@lucide/svelte/icons/smile-plus';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import UserMinus from '@lucide/svelte/icons/user-minus';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import UserRound from '@lucide/svelte/icons/user-round';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import X from '@lucide/svelte/icons/x';
  import type { ChannelFile, ChannelMember, ChannelReadState, ChatMessage, CollaborationChannel, CollaborationPerson, MessageFile, MessageRecordLink, MessageReplyPreview } from '../api/domain';
  import type { Session } from '../api/types';
  import type { LiveMessage } from '../live/client';
  import { createChannel, createDirectChannel, deleteMessage, download, editMessage, listChannelFiles, listChannelMembers, listChannels, listCollaborationPeople, listMessages, loadPresence, markChannelRead, prepareMessageFilePreview, removeChannelMember, saveChannelMember, searchChannel, sendMessage, setMessagePinned, toggleMessageReaction, updateChannel, uploadMessageFile } from '../api/workspace';
  import { requestConfirmation } from '../confirmation';
  import { shouldSendChatMessage } from '../util/interactions';
  import FilePreview from './FilePreview.svelte';
  import ProfileAvatar from './ProfileAvatar.svelte';
  import SharedImageThumbnail from './SharedImageThumbnail.svelte';

  let { session, openChannelUid = '', revision = 0, connected = false, liveEvent = null, onChannelChanged = (_uid: string, _newestVisible: boolean) => undefined }: { session: Session; openChannelUid?: string; revision?: number; connected?: boolean; liveEvent?: LiveMessage | null; onChannelChanged?: (uid: string, newestVisible: boolean) => void } = $props();
  type ConversationFilter = 'all' | 'direct' | 'spaces';
  type DeliveryReceipt = { seen: boolean; label: string; detail: string };
  type MessageTextSegment = { text: string; mention?: { uid: string; name: string } };
  const FALLBACK_SHARED_FILE_MAX_BYTES = 100 * 1024 * 1024;

  let channels = $state<CollaborationChannel[]>([]); let selected = $state<CollaborationChannel | null>(null);
  let messages = $state<ChatMessage[]>([]); let pinnedMessages = $state<ChatMessage[]>([]); let pinsOpen = $state(false); let readStates = $state<ChannelReadState[]>([]); let users = $state<CollaborationPerson[]>([]); let onlineUserUids = $state<string[]>([]); let peopleLoading = $state(false); let peopleError = $state(''); let body = $state('');
  let pendingFiles = $state<File[]>([]); let loading = $state(true); let conversationLoading = $state(false); let sending = $state(false); let error = $state('');
  let hasMoreMessages = $state(false); let reachedBeginning = $state(false); let loadingOlder = $state(false);
  let section = $state<ConversationFilter>('all'); let conversationSearch = $state(''); let detailsOpen = $state(false); let toolsOpen = $state(false);
  let recordLinkInput = $state(''); let pendingRecordUids = $state<string[]>([]); let mentionUser = $state(''); let pendingMentionUids = $state<string[]>([]);
  let createOpen = $state(false); let channelName = $state(''); let channelDescription = $state(''); let selectedMembers = $state<string[]>([]); let memberSearch = $state('');
  let messageList = $state<HTMLDivElement>(); let composerForm = $state<HTMLFormElement>(); let composerTextarea = $state<HTMLTextAreaElement>(); let sidebarSearch = $state<HTMLInputElement>();
  let previewFile = $state<MessageFile | null>(null);
  let profilePerson = $state<CollaborationPerson | null>(null); let directBusyUid = $state('');
  let replyingTo = $state<ChatMessage | null>(null);
  let newMenuOpen = $state(false);
  let mobileThreadOpen = $state(false);
  let messageMenuUid = $state(''); let reactionMenuUid = $state(''); let editingMessageUid = $state(''); let editingBody = $state(''); let messageActionBusy = $state(''); let deleteCandidate = $state<ChatMessage | null>(null);
  let conversationSearchOpen = $state(false); let messageSearch = $state(''); let messageSearchResults = $state<ChatMessage[]>([]); let messageSearching = $state(false); let focusedMessageUid = $state('');
  let channelFiles = $state<ChannelFile[]>([]); let channelFileTotal = $state(0); let fileSearch = $state(''); let filesLoading = $state(false); let filesLoaded = $state(false);
  let manageOpen = $state(false); let manageLoading = $state(false); let manageBusy = $state(''); let manageError = $state(''); let manageNotice = $state(''); let channelMembers = $state<ChannelMember[]>([]); let membersLoaded = $state(false); let manageName = $state(''); let manageDescription = $state(''); let manageInvitePolicy = $state<CollaborationChannel['invite_policy']>('admins'); let addMemberUid = $state(''); let addMemberRole = $state<'member' | 'admin'>('member'); let manageMemberSearch = $state(''); let addMemberPickerOpen = $state(false);
  let manageMemberSearchInput = $state<HTMLInputElement>();
  let mentionOpen = $state(false); let mentionQuery = $state(''); let mentionStart = $state(-1); let mentionActiveIndex = $state(0);
  let liveRefreshPromise: Promise<void> | null = null; let liveRefreshQueued = false;
  let conversationRequestSequence = 0;
  let sharedFileMaxBytes = $state(FALLBACK_SHARED_FILE_MAX_BYTES);
  const pendingPreviewUrls = new Map<File, string>();
  const reactionChoices = ['👍', '❤️', '😂', '🎉', '😮', '😢'];

  const visibleChannels = $derived(channels.filter((channel) => {
    const correctSection = section === 'all' || (section === 'direct' ? channel.kind === 'direct' : channel.kind !== 'direct');
    const needle = conversationSearch.trim().toLocaleLowerCase();
    return correctSection && (!needle || channel.name.toLocaleLowerCase().includes(needle) || channel.description.toLocaleLowerCase().includes(needle));
  }));
  const visiblePeople = $derived(users.filter((user) => !memberSearch.trim() || user.name.toLocaleLowerCase().includes(memberSearch.trim().toLocaleLowerCase())));
  const directoryPeople = $derived(users.filter((user) => !conversationSearch.trim() || user.name.toLocaleLowerCase().includes(conversationSearch.trim().toLocaleLowerCase())).sort((left, right) => Number(onlineUserUids.includes(right.uid)) - Number(onlineUserUids.includes(left.uid)) || left.name.localeCompare(right.name)));
  const sharedRecords = $derived(messages.filter((message) => !message.deleted_at).flatMap((message) => message.record_links).filter((record, index, items) => items.findIndex((item) => item.uid === record.uid) === index));
  const canManageSpace = $derived(selected?.kind !== 'direct' && (selected?.role === 'owner' || selected?.role === 'admin'));
  const canAddPeople = $derived(!!selected && selected.kind !== 'direct' && (
    selected.role === 'owner'
    || (selected.invite_policy === 'admins' && selected.role === 'admin')
    || selected.invite_policy === 'members'
  ));
  const canPinMessages = $derived(!!selected && (selected.kind === 'direct' || selected.role === 'owner' || selected.role === 'admin'));
  const availableMembers = $derived(users.filter((user) => !channelMembers.some((member) => member.user_uid === user.uid) && (!manageMemberSearch.trim() || user.name.toLocaleLowerCase().includes(manageMemberSearch.trim().toLocaleLowerCase()))));
  const selectedAddMember = $derived(users.find((user) => user.uid === addMemberUid) || null);
  const mentionablePeople = $derived(users.filter((user) => {
    if (!selected || user.uid === session.uid) return false;
    if (selected.kind === 'direct') return user.uid === selected.direct_user_uid;
    return channelMembers.some((member) => member.user_uid === user.uid);
  }).sort((left, right) => Number(isUserOnline(right.uid)) - Number(isUserOnline(left.uid)) || left.name.localeCompare(right.name)));
  const mentionSuggestions = $derived(mentionOpen ? mentionablePeople.filter((user) => user.name.toLocaleLowerCase().includes(mentionQuery.trim().toLocaleLowerCase())).slice(0, 8) : []);

  onMount(() => {
    void initialize();
    const handleVisibility = () => { if (document.visibilityState === 'visible' && selected) void refreshLive(); };
    const handleOutsideMessageMenu = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (messageMenuUid) {
        const owner = target?.closest('[data-message-actions]')?.getAttribute('data-message-actions');
        if (owner !== messageMenuUid) messageMenuUid = '';
      }
      if (reactionMenuUid) {
        const owner = target?.closest('[data-reaction-picker]')?.getAttribute('data-reaction-picker');
        if (owner !== reactionMenuUid) reactionMenuUid = '';
      }
      if (newMenuOpen && !target?.closest('[data-new-collaboration]')) newMenuOpen = false;
      if (addMemberPickerOpen && !target?.closest('[data-manage-person-picker]')) addMemberPickerOpen = false;
      if (mentionOpen && !target?.closest('[data-mention-composer]')) closeMentionPicker();
      if (pinsOpen && !target?.closest('[data-conversation-pins]')) pinsOpen = false;
    };
    const handleMessageMenuKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      if (messageMenuUid) { event.preventDefault(); messageMenuUid = ''; }
      else if (reactionMenuUid) { event.preventDefault(); reactionMenuUid = ''; }
      else if (newMenuOpen) { event.preventDefault(); newMenuOpen = false; }
      else if (mentionOpen) { event.preventDefault(); closeMentionPicker(); }
      else if (pinsOpen) { event.preventDefault(); pinsOpen = false; }
      else if (conversationSearchOpen) { event.preventDefault(); closeConversationSearch(); }
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
      const [channelResult, peopleResult, presenceResult] = await Promise.allSettled([
        listChannels(),
        listCollaborationPeople(),
        loadPresence()
      ]);
      if (channelResult.status === 'rejected') throw channelResult.reason;
      channels = channelResult.value.channels;
      sharedFileMaxBytes = channelResult.value.file_max_size_bytes || FALLBACK_SHARED_FILE_MAX_BYTES;
      if (peopleResult.status === 'fulfilled') users = peopleResult.value.people;
      else peopleError = peopleResult.reason instanceof Error ? peopleResult.reason.message : 'The coworker directory is temporarily unavailable.';
      if (presenceResult.status === 'fulfilled') onlineUserUids = presenceResult.value.accounts.map((account) => account.uid);
      const initial = channels.find((item) => item.uid === openChannelUid) || channels[0];
      if (initial) await openConversation(initial, window.innerWidth > 820 || Boolean(openChannelUid));
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
      sharedFileMaxBytes = channelData.file_max_size_bytes || FALLBACK_SHARED_FILE_MAX_BYTES;
      if (presenceData) onlineUserUids = presenceData.accounts.map((account) => account.uid);
      if (selected) {
        const channelUid = selected.uid;
        const requestSequence = conversationRequestSequence;
        const refreshedChannel = channels.find((item) => item.uid === channelUid);
        if (!refreshedChannel) {
          selected = null; messages = []; pinnedMessages = []; readStates = []; detailsOpen = false; pinsOpen = false; mobileThreadOpen = false;
          return;
        }
        selected = refreshedChannel;
        const data = await listMessages(channelUid);
        if (selected?.uid !== channelUid || requestSequence !== conversationRequestSequence) return;
        messages = mergeMessages(messages, data.messages);
        pinnedMessages = data.pinned_messages || [];
        readStates = data.read_states || [];
        hasMoreMessages = !reachedBeginning && (hasMoreMessages || data.has_more);
        await markVisibleConversationRead(channelUid, stayAtEnd);
        if (selected?.uid !== channelUid || requestSequence !== conversationRequestSequence) return;
        if (stayAtEnd) await scrollEnd();
      }
    }
    catch { /* The live client will retry after reconnecting. */ }
  }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The collaboration operation failed.'; }
  function applyLiveEvent(event: LiveMessage | null) {
    if (!event || !event.payload || typeof event.payload !== 'object') return;
    if (event.type === 'profile.updated') {
      const profile = event.payload as { uid?: string; profile_photo_updated_at?: number | null };
      if (!profile.uid) return;
      users = users.map((user) => user.uid === profile.uid ? { ...user, profile_photo_updated_at: profile.profile_photo_updated_at ?? null } : user);
      channelMembers = channelMembers.map((member) => member.user_uid === profile.uid ? { ...member, profile_photo_updated_at: profile.profile_photo_updated_at ?? null } : member);
      messages = messages.map((message) => message.sender_uid === profile.uid ? { ...message, sender_profile_photo_updated_at: profile.profile_photo_updated_at ?? null } : message);
      return;
    }
    if (!selected) return;
    const payload = event.payload as Partial<ChatMessage> & { channel_uid?: string; uid?: string };
    if (payload.channel_uid !== selected.uid || !payload.uid) return;
    if (event.type === 'message.deleted') {
      messages = messages.map((message) => message.uid === payload.uid ? { ...message, body: '', files: [], record_links: [], mentions: [], deleted_at: event.at || Date.now() } : message);
      pinnedMessages = pinnedMessages.filter((message) => message.uid !== payload.uid);
      if (replyingTo?.uid === payload.uid) replyingTo = null;
    } else if ((event.type === 'message.updated' || event.type === 'message.created') && typeof payload.sender_uid === 'string' && typeof payload.sequence === 'number') {
      const incoming = payload as ChatMessage;
      messages = mergeMessages(messages, [incoming]);
      syncPinnedMessage(incoming);
    }
  }
  async function openConversation(channel: CollaborationChannel, revealThread = true) {
    const requestSequence = ++conversationRequestSequence;
    const channelUid = channel.uid;
    if (selected && selected.uid !== channel.uid) detailsOpen = false;
    if (revealThread) mobileThreadOpen = true;
    selected = channel; messages = []; pinnedMessages = []; pinsOpen = false; readStates = []; hasMoreMessages = false; conversationLoading = true; error = ''; toolsOpen = false; mentionOpen = false; messageMenuUid = ''; reactionMenuUid = ''; editingMessageUid = ''; deleteCandidate = null; replyingTo = null; reachedBeginning = false;
    onChannelChanged(channel.uid, false);
    closeConversationSearch(); channelFiles = []; channelFileTotal = 0; fileSearch = ''; filesLoaded = false; manageOpen = false; channelMembers = []; membersLoaded = false;
    if (channel.kind !== 'direct') void loadConversationMembers();
    try {
      const data = await listMessages(channelUid);
      if (requestSequence !== conversationRequestSequence || selected?.uid !== channelUid) return;
      messages = data.messages;
      pinnedMessages = data.pinned_messages || [];
      readStates = data.read_states || [];
      hasMoreMessages = data.has_more ?? data.messages.length >= 256;
      reachedBeginning = !hasMoreMessages;
      await markVisibleConversationRead(channelUid);
      if (requestSequence !== conversationRequestSequence || selected?.uid !== channelUid) return;
      channels = channels.map((item) => item.uid === channelUid ? { ...item, unread_count: document.visibilityState === 'visible' ? 0 : item.unread_count } : item);
      selected = channels.find((item) => item.uid === channelUid) || channel;
      conversationLoading = false;
      await scrollEnd();
      if (requestSequence !== conversationRequestSequence || selected?.uid !== channelUid) return;
      onChannelChanged(channelUid, true);
      if (detailsOpen) {
        void loadSharedFiles(true);
        if (channel.kind !== 'direct') void loadConversationMembers();
      }
    }
    catch (reason) { if (requestSequence === conversationRequestSequence) fail(reason); }
    finally { if (requestSequence === conversationRequestSequence) conversationLoading = false; }
  }
  async function markVisibleConversationRead(channelUid: string, newestMessagesAreVisible = true) {
    if (document.visibilityState !== 'visible' || !newestMessagesAreVisible) return;
    const result = await markChannelRead(channelUid);
    if (selected?.uid !== channelUid) return;
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
  function syncPinnedMessage(message: ChatMessage) {
    const withoutMessage = pinnedMessages.filter((item) => item.uid !== message.uid);
    pinnedMessages = message.pinned_at
      ? [...withoutMessage, message].sort((left, right) => (right.pinned_at || 0) - (left.pinned_at || 0))
      : withoutMessage;
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
  function messageListScrolled() {
    if (selected) onChannelChanged(selected.uid, isNearEnd());
    if (messageList && messageList.scrollTop < 180) void loadOlderMessages();
  }
  function fileSize(value: number) { if (value < 1024) return `${value} B`; if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`; return `${(value / 1024 ** 2).toFixed(1)} MB`; }
  function isImageFile(file: MessageFile) { return file.mime_type.toLocaleLowerCase().startsWith('image/') || /\.(?:avif|bmp|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/i.test(file.file_name); }
  function isPendingImage(file: File) { return file.type.toLocaleLowerCase().startsWith('image/') || /\.(?:avif|bmp|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/i.test(file.name); }
  function isLongMessage(message: ChatMessage) { return message.body.length > 420 || message.body.split('\n').length > 8; }
  function pendingImageUrl(file: File) { let url = pendingPreviewUrls.get(file); if (!url) { url = URL.createObjectURL(file); pendingPreviewUrls.set(file, url); } return url; }
  function releasePendingPreview(file: File) { const url = pendingPreviewUrls.get(file); if (url) URL.revokeObjectURL(url); pendingPreviewUrls.delete(file); }
  function releasePendingPreviews() { for (const url of pendingPreviewUrls.values()) URL.revokeObjectURL(url); pendingPreviewUrls.clear(); }
  function removePendingFile(index: number) { const file = pendingFiles[index]; if (file) releasePendingPreview(file); pendingFiles = pendingFiles.filter((_, item) => item !== index); }
  function clearPendingFiles() { releasePendingPreviews(); pendingFiles = []; }
  function sharedFileLimitLabel() { const mib = sharedFileMaxBytes / 1024 ** 2; return `${Number.isInteger(mib) ? mib : mib.toFixed(1)} MiB`; }
  function queueSharedFiles(files: FileList | File[]) {
    const incoming = Array.from(files);
    const oversized = incoming.filter((file) => file.size > sharedFileMaxBytes);
    const accepted = incoming.filter((file) => file.size <= sharedFileMaxBytes && !pendingFiles.some((pending) => pending.name === file.name && pending.size === file.size && pending.lastModified === file.lastModified));
    if (accepted.length) pendingFiles = [...pendingFiles, ...accepted];
    error = oversized.length
      ? `${oversized.map((file) => file.name).join(', ')} ${oversized.length === 1 ? 'is' : 'are'} larger than the ${sharedFileLimitLabel()} chat file limit.`
      : '';
  }
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
    queueSharedFiles(images.map((file, index) => namedClipboardImage(file, index, timestamp)));
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
  function channelSubtitle(channel: CollaborationChannel) { return channel.kind === 'direct' ? `Private chat · ${channelKind(channel)}` : channel.description || channelKind(channel); }
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
  function escapeRegularExpression(value: string) { return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }
  function messageTextSegments(message: ChatMessage): MessageTextSegment[] {
    const mentions = message.mentions || [];
    if (!mentions.length || !message.body) return [{ text: message.body }];
    const byName = new Map(mentions.map((mention) => [mention.name.toLocaleLowerCase(), mention]));
    const names = [...byName.keys()].sort((left, right) => right.length - left.length).map(escapeRegularExpression);
    const pattern = new RegExp(`(@(?:${names.join('|')}))(?=$|[\\s.,!?;:()[\\]{}])`, 'giu');
    return message.body.split(pattern).filter(Boolean).map((text) => {
      const mention = text.startsWith('@') ? byName.get(text.slice(1).toLocaleLowerCase()) : undefined;
      return mention ? { text, mention } : { text };
    });
  }
  function closeMentionPicker() { mentionOpen = false; mentionQuery = ''; mentionStart = -1; mentionActiveIndex = 0; }
  function updateMentionPicker(textarea: HTMLTextAreaElement) {
    const caret = textarea.selectionStart ?? textarea.value.length;
    const prefix = textarea.value.slice(0, caret);
    const start = prefix.lastIndexOf('@');
    const previous = start > 0 ? prefix[start - 1] : '';
    const query = start >= 0 ? prefix.slice(start + 1) : '';
    if (start < 0 || (previous && !/[\s([{]/.test(previous)) || query.length > 80 || /[\n\r.,!?;:()[\]{}]/.test(query) || /\s{2}/.test(query)) {
      closeMentionPicker();
      return;
    }
    mentionStart = start;
    mentionQuery = query;
    mentionActiveIndex = 0;
    mentionOpen = true;
    if (selected?.kind !== 'direct' && !membersLoaded) void loadConversationMembers();
  }
  function composerInput(event: Event) {
    const textarea = event.currentTarget as HTMLTextAreaElement;
    pendingMentionUids = pendingMentionUids.filter((uid) => textarea.value.includes(`@${personName(uid)}`));
    updateMentionPicker(textarea);
  }
  async function insertMention(user: CollaborationPerson, replaceTypedMention = false) {
    const textarea = composerTextarea;
    const caret = textarea?.selectionStart ?? body.length;
    const start = replaceTypedMention && mentionStart >= 0 ? mentionStart : caret;
    const before = body.slice(0, start);
    const after = body.slice(caret);
    const separator = !replaceTypedMention && before && !/\s$/.test(before) ? ' ' : '';
    const inserted = `${separator}@${user.name} `;
    body = `${before}${inserted}${after}`;
    if (!pendingMentionUids.includes(user.uid)) pendingMentionUids = [...pendingMentionUids, user.uid];
    mentionUser = '';
    closeMentionPicker();
    await tick();
    const nextCaret = before.length + inserted.length;
    composerTextarea?.focus();
    composerTextarea?.setSelectionRange(nextCaret, nextCaret);
  }
  function addMention() {
    const user = users.find((person) => person.uid === mentionUser);
    if (user) void insertMention(user);
  }
  function chooseMention(user: CollaborationPerson) { void insertMention(user, true); }
  function personName(uid: string) { return users.find((user) => user.uid === uid)?.name || uid; }
  function personByUid(uid: string | null | undefined) { return users.find((user) => user.uid === uid) || null; }
  function replyPreviewFor(message: ChatMessage): MessageReplyPreview | null {
    if (message.reply_preview) return message.reply_preview;
    const parent = message.reply_to_uid ? messages.find((item) => item.uid === message.reply_to_uid) : null;
    return parent ? { uid: parent.uid, sender_uid: parent.sender_uid, sender_name: parent.sender_name, body: parent.deleted_at ? '' : parent.body, deleted_at: parent.deleted_at } : null;
  }
  function replySnippet(message: Pick<ChatMessage, 'body' | 'deleted_at'>) { return message.deleted_at ? 'Original message was deleted' : message.body.trim() || 'Shared content'; }
  function beginReply(message: ChatMessage) {
    if (message.deleted_at) return;
    messageMenuUid = ''; cancelMessageEdit(); replyingTo = message; error = '';
    void tick().then(() => document.querySelector<HTMLTextAreaElement>('.message-composer textarea[aria-label="Message"]')?.focus());
  }
  async function reactToMessage(message: ChatMessage, emoji: string) {
    if (message.deleted_at || messageActionBusy) return;
    messageActionBusy = message.uid; error = '';
    try {
      const result = await toggleMessageReaction(message.uid, emoji);
      messages = messages.map((item) => item.uid === result.message.uid ? result.message : item);
      reactionMenuUid = '';
    } catch (reason) { fail(reason); } finally { messageActionBusy = ''; }
  }
  async function toggleMessagePin(message: ChatMessage) {
    if (message.deleted_at || !canPinMessages || messageActionBusy) return;
    messageActionBusy = message.uid; error = '';
    try {
      const result = await setMessagePinned(message.uid, !message.pinned_at);
      messages = messages.map((item) => item.uid === result.message.uid ? result.message : item);
      syncPinnedMessage(result.message);
      messageMenuUid = '';
    } catch (reason) { fail(reason); } finally { messageActionBusy = ''; }
  }
  async function openPinnedMessage(message: ChatMessage) {
    pinsOpen = false;
    await openSearchResult(message);
  }
  function reactionLabel(message: ChatMessage, emoji: string) {
    const reaction = (message.reactions || []).find((item) => item.emoji === emoji);
    if (!reaction) return `React with ${emoji}`;
    const names = reaction.users.map((user) => user.uid === session.uid ? 'You' : user.name);
    return `${names.join(', ')} reacted ${emoji}`;
  }
  async function focusOriginalMessage(uid: string) {
    const target = document.querySelector<HTMLElement>(`[data-message-uid="${uid}"]`);
    if (!target) { error = 'The original message is outside the messages currently loaded. Its quoted text remains available here.'; return; }
    focusedMessageUid = uid; target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    window.setTimeout(() => { if (focusedMessageUid === uid) focusedMessageUid = ''; }, 2600);
  }
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
      pinnedMessages = pinnedMessages.filter((item) => item.uid !== message.uid);
      if (replyingTo?.uid === message.uid) replyingTo = null;
      if (editingMessageUid === message.uid) cancelMessageEdit();
      deleteCandidate = null; messageMenuUid = '';
    } catch (reason) { fail(reason); } finally { messageActionBusy = ''; }
  }
  function closeCreate() { createOpen = false; channelName = ''; channelDescription = ''; selectedMembers = []; memberSearch = ''; }
  function closeCreateFromBackdrop(event: MouseEvent) { if (event.target === event.currentTarget) closeCreate(); }
  function directChannelFor(userUid: string) { return channels.find((channel) => channel.kind === 'direct' && channel.direct_user_uid === userUid) || null; }
  function openPersonCard(user: CollaborationPerson) { if (user.uid !== session.uid) profilePerson = user; }
  async function showPeople() { section = 'direct'; conversationSearch = ''; await tick(); sidebarSearch?.focus(); }
  function beginSidebarAction() { if (section === 'spaces') createOpen = true; else void showPeople(); }
  function chooseNewPrivateMessage() { newMenuOpen = false; void showPeople(); }
  function chooseNewSpace() { newMenuOpen = false; createOpen = true; }
  async function refreshPeople() {
    peopleLoading = true; peopleError = '';
    try { users = (await listCollaborationPeople()).people; }
    catch (reason) { peopleError = reason instanceof Error ? reason.message : 'The coworker directory is temporarily unavailable.'; }
    finally { peopleLoading = false; }
  }
  async function addChannel(event: SubmitEvent) {
    event.preventDefault();
    try { const result = await createChannel({ name: channelName.trim(), description: channelDescription.trim(), member_uids: selectedMembers, kind: 'group' }); closeCreate(); channels = (await listChannels()).channels; const channel = channels.find((item) => item.uid === result.uid); if (channel) await openConversation(channel); }
    catch (reason) { fail(reason); }
  }
  async function startDirectWith(user: CollaborationPerson) {
    if (user.uid === session.uid || directBusyUid) return;
    directBusyUid = user.uid; error = '';
    try {
      let channel = directChannelFor(user.uid);
      if (!channel) {
        const result = await createDirectChannel(user.uid);
        channels = (await listChannels()).channels;
        channel = channels.find((item) => item.uid === result.uid) || directChannelFor(user.uid);
      }
      if (!channel) throw new Error('MX opened the private chat but could not load it. Refresh conversations and try again.');
      profilePerson = null; section = 'direct'; conversationSearch = '';
      await openConversation(channel);
    }
    catch (reason) { fail(reason); }
    finally { directBusyUid = ''; }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault(); if (!selected || (!body.trim() && !pendingFiles.length && !pendingRecordUids.length)) return;
    const oversized = pendingFiles.find((file) => file.size > sharedFileMaxBytes);
    if (oversized) { error = `${oversized.name} is larger than the ${sharedFileLimitLabel()} chat file limit.`; return; }
    sending = true; error = '';
    try {
      const channelUid = selected.uid;
      const messageText = body.trim() || (pendingFiles.length ? `Shared ${pendingFiles.length} file${pendingFiles.length === 1 ? '' : 's'}` : `Shared ${pendingRecordUids.length} record${pendingRecordUids.length === 1 ? '' : 's'}`);
      const result = await sendMessage(channelUid, messageText, pendingMentionUids, pendingRecordUids, replyingTo?.uid || null); for (const file of pendingFiles) await uploadMessageFile(result.message.uid, file);
      body = ''; clearPendingFiles(); pendingRecordUids = []; pendingMentionUids = []; replyingTo = null; toolsOpen = false;
      const data = await listMessages(channelUid); messages = mergeMessages(messages, data.messages); readStates = data.read_states || [];
      channels = (await listChannels()).channels; selected = channels.find((item) => item.uid === channelUid) || selected;
      await markVisibleConversationRead(channelUid); await scrollEnd();
    } catch (reason) { fail(reason); } finally { sending = false; }
  }
  function composerKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (mentionOpen && mentionSuggestions.length && !event.ctrlKey && !event.metaKey) {
      if (event.key === 'ArrowDown') { event.preventDefault(); mentionActiveIndex = (mentionActiveIndex + 1) % mentionSuggestions.length; return; }
      if (event.key === 'ArrowUp') { event.preventDefault(); mentionActiveIndex = (mentionActiveIndex - 1 + mentionSuggestions.length) % mentionSuggestions.length; return; }
      if (event.key === 'Enter' || event.key === 'Tab') { event.preventDefault(); chooseMention(mentionSuggestions[Math.min(mentionActiveIndex, mentionSuggestions.length - 1)]); return; }
      if (event.key === 'Escape') { event.preventDefault(); closeMentionPicker(); return; }
    }
    if (shouldSendChatMessage(event)) {
      event.preventDefault();
      composerForm?.requestSubmit();
    }
  }
  function downloadSharedFile(file: MessageFile) { void download(`/mx/v1/collaboration/files/${file.uid}/download`, file.file_name); }
  function showSharedFile(file: MessageFile) { previewFile = file; }
  function recordHref(record: MessageRecordLink) { return `#module/${encodeURIComponent(record.module_uid)}?record=${encodeURIComponent(record.uid)}`; }

  function closeConversationSearch() { conversationSearchOpen = false; messageSearch = ''; messageSearchResults = []; messageSearching = false; }
  async function runConversationSearch(event?: SubmitEvent) {
    event?.preventDefault();
    if (!selected || !messageSearch.trim()) { messageSearchResults = []; return; }
    messageSearching = true; error = '';
    try { messageSearchResults = (await searchChannel(selected.uid, messageSearch.trim())).messages; }
    catch (reason) { fail(reason); }
    finally { messageSearching = false; }
  }
  async function openSearchResult(message: ChatMessage) {
    messages = mergeMessages(messages, [message]);
    focusedMessageUid = message.uid;
    conversationSearchOpen = false;
    await tick();
    document.querySelector<HTMLElement>(`[data-message-uid="${message.uid}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    window.setTimeout(() => { if (focusedMessageUid === message.uid) focusedMessageUid = ''; }, 2600);
  }
  function toggleDetails() {
    detailsOpen = !detailsOpen;
    if (detailsOpen && selected) {
      if (!filesLoaded) void loadSharedFiles(true);
      if (selected.kind !== 'direct' && !membersLoaded) void loadConversationMembers();
    }
  }
  async function loadConversationMembers() {
    if (!selected || selected.kind === 'direct' || membersLoaded) return;
    const channelUid = selected.uid;
    try {
      const result = await listChannelMembers(channelUid);
      if (selected?.uid !== channelUid) return;
      channelMembers = result.members; membersLoaded = true;
    }
    catch (reason) { fail(reason); }
  }
  async function loadSharedFiles(reset = false) {
    if (!selected || filesLoading) return;
    filesLoading = true; error = '';
    try {
      const offset = reset ? 0 : channelFiles.length;
      const result = await listChannelFiles(selected.uid, fileSearch.trim(), 50, offset);
      channelFiles = reset ? result.files : [...channelFiles, ...result.files];
      channelFileTotal = result.total;
      filesLoaded = true;
    } catch (reason) { fail(reason); }
    finally { filesLoading = false; }
  }
  function searchSharedFiles(event: SubmitEvent) { event.preventDefault(); void loadSharedFiles(true); }
  async function openManageSpace(focusAddPerson = false) {
    if (!selected || (!canManageSpace && !(focusAddPerson && canAddPeople))) return;
    manageOpen = true; manageLoading = true; manageError = ''; manageNotice = ''; manageName = selected.name; manageDescription = selected.description || ''; manageInvitePolicy = selected.invite_policy || 'admins'; manageMemberSearch = ''; addMemberUid = ''; addMemberRole = 'member';
    try { channelMembers = (await listChannelMembers(selected.uid)).members; membersLoaded = true; }
    catch (reason) { manageOpen = false; fail(reason); }
    finally { manageLoading = false; }
    if (focusAddPerson && manageOpen) {
      await tick();
      manageMemberSearchInput?.focus();
    }
  }
  function closeManageSpace() { if (manageBusy) return; manageOpen = false; manageMemberSearch = ''; addMemberUid = ''; addMemberPickerOpen = false; }
  async function saveSpaceDetails(event: SubmitEvent) {
    event.preventDefault(); if (!selected || !manageName.trim() || manageBusy) return;
    const channelUid = selected.uid; manageBusy = 'details'; manageError = ''; manageNotice = '';
    try {
      await updateChannel(channelUid, manageName.trim(), manageDescription.trim(), manageInvitePolicy);
      channels = (await listChannels()).channels;
      selected = channels.find((channel) => channel.uid === channelUid) || selected;
      manageNotice = 'Space details saved.';
    } catch (reason) { manageError = reason instanceof Error ? reason.message : 'The space could not be updated.'; }
    finally { manageBusy = ''; }
  }
  async function addManagedMember() {
    if (!selected || !addMemberUid || manageBusy) return;
    manageBusy = `add:${addMemberUid}`; manageError = ''; manageNotice = '';
    try {
      await saveChannelMember(selected.uid, addMemberUid, addMemberRole);
      channelMembers = (await listChannelMembers(selected.uid)).members;
      channels = (await listChannels()).channels;
      selected = channels.find((channel) => channel.uid === selected?.uid) || selected;
      addMemberUid = ''; addMemberRole = 'member'; manageMemberSearch = ''; addMemberPickerOpen = false;
      manageNotice = 'Member added to the space.';
    } catch (reason) { manageError = reason instanceof Error ? reason.message : 'The member could not be added.'; }
    finally { manageBusy = ''; }
  }
  async function changeManagedRole(member: ChannelMember, role: 'owner' | 'admin' | 'member') {
    if (!selected || member.role === role || manageBusy) return;
    if (role === 'owner' && !await requestConfirmation({ title: `Transfer ownership to ${member.user_name}?`, description: 'They will become the space owner. You will remain an administrator and may then leave the space if needed.', confirmLabel: 'Transfer ownership' })) return;
    manageBusy = `role:${member.user_uid}`; manageError = ''; manageNotice = '';
    try {
      await saveChannelMember(selected.uid, member.user_uid, role);
      channelMembers = (await listChannelMembers(selected.uid)).members;
      channels = (await listChannels()).channels;
      selected = channels.find((channel) => channel.uid === selected?.uid) || selected;
      manageNotice = role === 'owner' ? `${member.user_name} is now the space owner. You remain an administrator.` : `${member.user_name} is now a space ${role === 'admin' ? 'administrator' : 'member'}.`;
    } catch (reason) { manageError = reason instanceof Error ? reason.message : 'The member role could not be changed.'; }
    finally { manageBusy = ''; }
  }
  async function removeManagedMember(member: ChannelMember) {
    if (!selected || manageBusy) return;
    if (!await requestConfirmation({ title: `Remove ${member.user_name} from this space?`, description: 'They will lose access to its messages, files, and shared records.', confirmLabel: 'Remove member' })) return;
    manageBusy = `remove:${member.user_uid}`; manageError = ''; manageNotice = '';
    try {
      await removeChannelMember(selected.uid, member.user_uid);
      channelMembers = (await listChannelMembers(selected.uid)).members;
      channels = (await listChannels()).channels;
      selected = channels.find((channel) => channel.uid === selected?.uid) || selected;
      manageNotice = `${member.user_name} was removed from the space.`;
    } catch (reason) { manageError = reason instanceof Error ? reason.message : 'The member could not be removed.'; }
    finally { manageBusy = ''; }
  }
  async function leaveCurrentSpace() {
    if (!selected || selected.kind === 'direct' || selected.role === 'owner' || manageBusy) return;
    const leavingChannel = selected;
    if (!await requestConfirmation({ title: `Leave ${leavingChannel.name}?`, description: 'You will lose access to this space, its messages, files, and shared records. An authorized member must add you again to restore access.', confirmLabel: 'Leave space' })) return;
    manageBusy = 'leave'; error = '';
    try {
      await removeChannelMember(leavingChannel.uid, session.uid);
      const remaining = (await listChannels()).channels;
      channels = remaining;
      selected = null; messages = []; pinnedMessages = []; readStates = []; channelMembers = []; detailsOpen = false; pinsOpen = false; mobileThreadOpen = false;
      const next = remaining[0];
      if (next && window.innerWidth > 820) await openConversation(next, false);
    } catch (reason) { fail(reason); }
    finally { manageBusy = ''; }
  }
</script>

<section class="workspace-page collaboration-page">
  <div class="collaboration-shell">
    <aside class:thread-open={mobileThreadOpen} class="conversation-sidebar">
      <header class="collaboration-sidebar-head">
        <div><p class="eyebrow">Collaboration</p><h1>Messages</h1></div>
        <div class="collaboration-sidebar-actions">
          {#if section === 'direct'}<button class="collaboration-icon-button" onclick={refreshPeople} title="Refresh coworker directory" aria-label="Refresh coworker directory"><RefreshCw size={16} /></button>{/if}
          <div class="collaboration-new" data-new-collaboration>
            <button class:active={newMenuOpen} class="collaboration-icon-button new-conversation-button" type="button" onclick={() => newMenuOpen = !newMenuOpen} aria-label="Create new" aria-expanded={newMenuOpen}><Plus size={18} /></button>
            {#if newMenuOpen}
              <div class="collaboration-new-menu" role="menu" aria-label="Create new">
                <header><strong>Start something new</strong><small>Choose a private conversation or a shared space.</small></header>
                <button role="menuitem" type="button" onclick={chooseNewPrivateMessage}><MessageCircle size={17} /><span><strong>Private message</strong><small>Chat with one coworker</small></span></button>
                <button role="menuitem" type="button" onclick={chooseNewSpace}><UsersRound size={17} /><span><strong>Space</strong><small>Bring a team or project together</small></span></button>
              </div>
            {/if}
          </div>
        </div>
      </header>
      <nav class="conversation-sections" aria-label="Collaboration views">
        <button class:active={section === 'all'} type="button" onclick={() => section = 'all'}><MessagesSquare size={15} /><span>Chats</span></button>
        <button class:active={section === 'direct'} type="button" onclick={() => section = 'direct'}><UserRound size={15} /><span>People</span></button>
        <button class:active={section === 'spaces'} type="button" onclick={() => section = 'spaces'}><UsersRound size={15} /><span>Spaces</span></button>
      </nav>
      <div class="conversation-controls">
        <label class="conversation-search"><span><Search size={14} /></span><input bind:this={sidebarSearch} bind:value={conversationSearch} placeholder={section === 'direct' ? 'Search people…' : section === 'spaces' ? 'Search spaces…' : 'Search conversations…'} aria-label={section === 'direct' ? 'Search people' : section === 'spaces' ? 'Search spaces' : 'Search conversations'} /></label>
        {#if section === 'direct'}<p class="conversation-privacy-note"><LockKeyhole size={12} /><span>Coworkers are discoverable; private chats are participant-only.</span></p>{/if}
      </div>
      <div class="conversation-list-heading"><span>{section === 'direct' ? `People · ${directoryPeople.length}` : `Recent · ${visibleChannels.length}`}</span>{#if section !== 'direct'}<button onclick={() => void refreshLive()} aria-label="Refresh conversations" title="Refresh"><RefreshCw size={13} /></button>{/if}</div>
      {#if section === 'direct'}
        <div class="conversation-list people-directory" aria-label="Coworker directory">
          {#if peopleLoading}<div class="conversation-skeleton" aria-label="Loading coworkers"><i></i><i></i><i></i></div>
          {:else if peopleError}<div class="empty-state compact"><strong>Directory unavailable</strong><p>{peopleError}</p><button class="button small" onclick={refreshPeople}>Try again</button></div>
          {:else if !directoryPeople.length}<div class="empty-state compact"><strong>No coworkers found</strong><p>{conversationSearch ? 'Try a different name.' : 'No other MX accounts are available.'}</p></div>
          {:else}{#each directoryPeople as user}{@const direct = directChannelFor(user.uid)}<article class:active={selected?.uid === direct?.uid} class="person-directory-row"><button class="person-directory-profile" type="button" onclick={() => openPersonCard(user)} aria-label={`View ${user.name}`}><span class="channel-glyph direct"><ProfileAvatar userUid={user.uid} name={user.name} updatedAt={user.profile_photo_updated_at} online={isUserOnline(user.uid)} /></span><span class="channel-summary"><strong>{user.name}</strong><small class:online={isUserOnline(user.uid)}>{isUserOnline(user.uid) ? 'Online' : 'Offline'}{direct ? ' · Private chat exists' : ''}</small></span></button><button class="person-directory-message" type="button" onclick={() => startDirectWith(user)} disabled={!!directBusyUid} aria-label={`${direct ? 'Open' : 'Start'} private chat with ${user.name}`} title={`${direct ? 'Open' : 'Start'} private chat`}><MessageCircle size={15} />{#if direct?.unread_count}<b>{direct.unread_count > 99 ? '99+' : direct.unread_count}</b>{/if}</button></article>{/each}{/if}
        </div>
      {:else}
        <nav class="conversation-list" aria-label="Conversations">
          {#if loading}<div class="conversation-skeleton" aria-label="Loading conversations"><i></i><i></i><i></i></div>
          {:else if !visibleChannels.length}<div class="empty-state compact"><strong>Nothing here yet</strong><p>{conversationSearch ? 'Try a different search.' : section === 'spaces' ? 'Create a space for a team or project.' : 'Start a private message or create a space.'}</p></div>
          {:else}{#each visibleChannels as channel}{@const directUser = channel.kind === 'direct' ? personByUid(channel.direct_user_uid) : null}<button class:active={selected?.uid === channel.uid} onclick={() => openConversation(channel)}><span class:direct={channel.kind === 'direct'} class="channel-glyph">{#if directUser}<ProfileAvatar userUid={directUser.uid} name={directUser.name} updatedAt={directUser.profile_photo_updated_at} online={isDirectOnline(channel)} />{:else if channel.kind === 'direct'}{initials(channel.name)}{:else if channel.kind === 'channel'}<Hash size={15} />{:else}<UsersRound size={15} />{/if}</span><span class="channel-summary"><strong>{channel.name}</strong><small class:online={isDirectOnline(channel)}>{channelKind(channel)}</small></span><span class="channel-state">{#if channel.unread_count}<b>{channel.unread_count > 99 ? '99+' : channel.unread_count}</b>{:else if channel.last_message_at}<time>{clock(channel.last_message_at)}</time>{/if}</span></button>{/each}{/if}
        </nav>
      {/if}
    </aside>

    <div class:show-details={detailsOpen && !!selected} class:thread-open={mobileThreadOpen} class="collaboration-content">
      <main class="conversation-main">
        {#if error}<div class="collaboration-error notice error"><span>{error}</span><button onclick={() => error = ''} aria-label="Dismiss error"><X size={15} /></button></div>{/if}
        {#if selected}
          {@const directPerson = selected.kind === 'direct' ? personByUid(selected.direct_user_uid) : null}
          <header><div class="conversation-title"><button class="collaboration-icon-button conversation-back" type="button" onclick={() => { mobileThreadOpen = false; detailsOpen = false; }} aria-label="Back to conversations"><ArrowLeft size={18} /></button>{#if directPerson}<button class="channel-glyph large direct person-trigger" type="button" onclick={() => openPersonCard(directPerson)} aria-label={`View ${directPerson.name}`}><ProfileAvatar userUid={directPerson.uid} name={directPerson.name} updatedAt={directPerson.profile_photo_updated_at} online={isDirectOnline(selected)} /></button>{:else}<span class:direct={selected.kind === 'direct'} class="channel-glyph large">{#if selected.kind === 'direct'}{initials(selected.name)}{:else if selected.kind === 'channel'}<Hash size={20} />{:else}<UsersRound size={20} />{/if}</span>{/if}<div><h2>{#if directPerson}<button class="conversation-person-name" type="button" onclick={() => openPersonCard(directPerson)}>{selected.name}</button>{:else}{selected.name}{/if}</h2><p class:online={isDirectOnline(selected)}>{channelSubtitle(selected)}</p></div></div><div class="conversation-actions"><span class:online={conversationPresenceIsOnline(selected)} class="live-label"><i></i>{conversationPresence(selected)}</span><div class="conversation-pins" data-conversation-pins><button class:active={pinsOpen} class="collaboration-icon-button" type="button" onclick={() => pinsOpen = !pinsOpen} title="Pinned messages" aria-label={`Pinned messages, ${pinnedMessages.length}`} aria-expanded={pinsOpen}><Pin size={15} />{#if pinnedMessages.length}<b>{pinnedMessages.length > 99 ? '99+' : pinnedMessages.length}</b>{/if}</button>{#if pinsOpen}<div class="pinned-message-panel"><header><span><Pin size={14} /><strong>Pinned messages</strong></span><small>{pinnedMessages.length} saved</small></header>{#if pinnedMessages.length}<div>{#each pinnedMessages as pinned}<button type="button" onclick={() => openPinnedMessage(pinned)}><span><strong>{pinned.sender_name}</strong><time>{clock(pinned.created_at)}</time></span><p>{pinned.body.trim() || 'Shared content'}</p><small>Pinned by {pinned.pinned_by_name || 'a moderator'}</small></button>{/each}</div>{:else}<p class="pinned-message-empty">Important messages pinned in this conversation will appear here.</p>{/if}</div>{/if}</div><button class:active={conversationSearchOpen} class="collaboration-icon-button" onclick={() => { conversationSearchOpen = !conversationSearchOpen; if (!conversationSearchOpen) closeConversationSearch(); }} title="Search this conversation" aria-label="Search this conversation"><Search size={15} /></button><button class="collaboration-icon-button refresh-conversation" onclick={() => void refreshLive()} title="Refresh conversation" aria-label="Refresh conversation"><RefreshCw size={15} /></button><button class:active={detailsOpen} class="collaboration-icon-button details-toggle" onclick={toggleDetails} title="Conversation details" aria-label="Toggle conversation details"><Info size={16} /></button></div></header>
          {#if conversationSearchOpen}
            <section class="conversation-search-panel" aria-label="Search this conversation">
              <form onsubmit={runConversationSearch}><label><Search size={15} /><input bind:value={messageSearch} type="search" placeholder="Search messages and file names…" aria-label="Search messages and file names" /></label><button class="button primary small" disabled={messageSearching || !messageSearch.trim()}>{messageSearching ? 'Searching…' : 'Search'}</button><button class="icon-button" type="button" onclick={closeConversationSearch} aria-label="Close conversation search"><X size={16} /></button></form>
              {#if messageSearching}<div class="conversation-search-status">Searching the complete conversation…</div>
              {:else if messageSearch.trim() && !messageSearchResults.length}<div class="conversation-search-status">No messages or files match “{messageSearch.trim()}”.</div>
              {:else if messageSearchResults.length}<div class="conversation-search-results">{#each messageSearchResults as result}<button type="button" onclick={() => openSearchResult(result)}><span class="message-avatar"><ProfileAvatar userUid={result.sender_uid} name={result.sender_name} updatedAt={result.sender_profile_photo_updated_at} online={isUserOnline(result.sender_uid)} /></span><span><strong>{result.sender_name}<time>{clock(result.created_at)}</time></strong><small>{result.body}</small>{#if result.files.length}<em><Paperclip size={11} /> {result.files.map((file) => file.file_name).join(', ')}</em>{/if}</span><ChevronRight size={15} /></button>{/each}</div>{/if}
            </section>
          {/if}
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
            {:else}
            {#if loadingOlder}<div class="message-history-ghost" role="status" aria-label="Loading earlier messages"><i></i><span><b></b><b></b></span><small>Loading earlier messages…</small></div>{:else if hasMoreMessages}<div class="message-history-hint">Scroll upward for earlier messages</div>{:else}<div class="message-history-hint beginning">Beginning of conversation</div>{/if}
            {#each messages as message, index}
              {#if index === 0 || dayKey(messages[index - 1].created_at) !== dayKey(message.created_at)}<div class="message-day"><span>{dayLabel(message.created_at)}</span></div>{/if}
              {@const receipt = deliveryReceipt(message, index)}
              {@const sender = message.sender_uid === session.uid ? null : personByUid(message.sender_uid)}
              {@const replyPreview = replyPreviewFor(message)}
              {#if message.event_kind}
                <div data-message-uid={message.uid} class="message-system-event"><span>{message.body}</span><time>{clock(message.created_at)}</time></div>
              {:else}
              <article data-message-uid={message.uid} class:search-focused={focusedMessageUid === message.uid} class:mine={message.sender_uid === session.uid} class:deleted={!!message.deleted_at} class:long-message={isLongMessage(message)} class:media-message={message.files.some(isImageFile)} class:pinned-message={!!message.pinned_at} class="chat-message">
                {#if sender}
                  <button class="message-avatar person-trigger" type="button" onclick={() => openPersonCard(sender)} aria-label={`View ${sender.name}`}><ProfileAvatar userUid={sender.uid} name={sender.name} updatedAt={message.sender_profile_photo_updated_at ?? sender.profile_photo_updated_at} online={isUserOnline(sender.uid)} /></button>
                {:else}
                  <div class="message-avatar"><ProfileAvatar userUid={session.uid} name={session.name} updatedAt={session.profile_photo_updated_at} online={connected} /></div>
                {/if}
                <div class="message-content">
                  <header>
                    <span class="message-author">{#if sender}<button class="message-author-button" type="button" onclick={() => openPersonCard(sender)}>{message.sender_name}</button>{:else}<strong>{message.sender_name}</strong>{/if}<time>{clock(message.created_at)}{message.edited_at && !message.deleted_at ? ' · edited' : ''}</time>{#if message.pinned_at}<span class="message-pin-state" title={`Pinned by ${message.pinned_by_name || 'a moderator'}`}><Pin size={11} /> Pinned</span>{/if}</span>
                    {#if !message.deleted_at}
                      <span class="message-action-cluster">
                        <span class="message-reaction-action" data-reaction-picker={message.uid}>
                          <button class="message-action-trigger" type="button" onclick={() => reactionMenuUid = reactionMenuUid === message.uid ? '' : message.uid} aria-label={`React to message from ${message.sender_name}`} aria-expanded={reactionMenuUid === message.uid}><SmilePlus size={15} /></button>
                          {#if reactionMenuUid === message.uid}<span class="message-reaction-picker" role="menu" aria-label="Choose a reaction">{#each reactionChoices as emoji}<button type="button" role="menuitem" title={`React with ${emoji}`} onclick={() => reactToMessage(message, emoji)}>{emoji}</button>{/each}</span>{/if}
                        </span>
                        <span class="message-actions" data-message-actions={message.uid}>
                          <button class="message-action-trigger" type="button" onclick={() => messageMenuUid = messageMenuUid === message.uid ? '' : message.uid} aria-label={`Message actions for ${message.sender_name}`} aria-expanded={messageMenuUid === message.uid}><Ellipsis size={15} /></button>
                          {#if messageMenuUid === message.uid}<span class="message-action-menu"><button type="button" onclick={() => beginReply(message)}><Reply size={13} />Reply</button>{#if canPinMessages}<button type="button" onclick={() => toggleMessagePin(message)}><Pin size={13} />{message.pinned_at ? 'Unpin message' : 'Pin message'}</button>{/if}{#if message.sender_uid === session.uid}<button type="button" onclick={() => beginMessageEdit(message)}><Pencil size={13} />Edit message</button>{/if}{#if canDeleteMessage(message)}<button class="danger-text" type="button" onclick={() => { deleteCandidate = message; messageMenuUid = ''; }}><Trash2 size={13} />Delete message</button>{/if}</span>{/if}
                        </span>
                      </span>
                    {/if}
                  </header>
                  {#if replyPreview}<button class:unavailable={!!replyPreview.deleted_at} class="message-reply-quote" type="button" onclick={() => focusOriginalMessage(replyPreview.uid)} title="Go to original message"><Reply size={13} /><span><strong>{replyPreview.sender_name}</strong><small>{replyPreview.deleted_at ? 'Original message was deleted' : replyPreview.body}</small></span></button>{:else if message.reply_to_uid}<div class="message-reply-quote unavailable"><Reply size={13} /><span><strong>Original message</strong><small>Preview unavailable</small></span></div>{/if}
                  {#if message.deleted_at}
                    <p class="deleted-message"><Trash2 size={13} /><em>Message deleted</em></p>
                  {:else if editingMessageUid === message.uid}
                    <form class="message-edit-form" onsubmit={(event) => saveMessageEdit(event, message)}><textarea data-edit-message={message.uid} bind:value={editingBody} rows="3" maxlength="20000" aria-label="Edit message"></textarea><div><small>Edited messages remain marked as edited.</small><button class="button small" type="button" onclick={cancelMessageEdit}>Cancel</button><button class="button primary small" disabled={!editingBody.trim() || messageActionBusy === message.uid}>{messageActionBusy === message.uid ? 'Saving…' : 'Save'}</button></div></form>
                  {:else}
                    <p class="message-text">{#each messageTextSegments(message) as segment}{#if segment.mention}<button class:mine={segment.mention.uid === session.uid} class="message-mention" type="button" onclick={() => { const person = personByUid(segment.mention?.uid); if (person) openPersonCard(person); }} title={`View ${segment.mention.name}`}>{segment.text}</button>{:else}{segment.text}{/if}{/each}</p>
                    {#if message.record_links.length}<div class="shared-records">{#each message.record_links as record}<a href={recordHref(record)}><span><FileText size={15} /></span><div><strong>{record.label}</strong><small>{record.module_name} · Open {record.singular_name.toLowerCase()}</small></div><i><ChevronRight size={15} /></i></a>{/each}</div>{/if}
                    {#if message.files.length}<div class="shared-files">{#each message.files as file}{#if isImageFile(file)}<SharedImageThumbnail {file} onOpen={() => showSharedFile(file)} />{:else}<div class="shared-file-item"><button class="shared-file-preview" type="button" onclick={() => showSharedFile(file)} title={`Preview ${file.file_name}`}><span><FileText size={15} /></span><div><strong>{file.file_name}</strong><small>{file.mime_type} · {fileSize(file.size)} · Preview</small></div></button><button class="shared-file-download" type="button" onclick={() => downloadSharedFile(file)} aria-label={`Download ${file.file_name}`} title={`Download ${file.file_name}`}><DownloadIcon size={15} /></button></div>{/if}{/each}</div>{/if}
                    {#if (message.reactions || []).length}<div class="message-reactions">{#each message.reactions || [] as reaction}<button class:mine={reaction.users.some((user) => user.uid === session.uid)} type="button" title={reactionLabel(message, reaction.emoji)} aria-label={reactionLabel(message, reaction.emoji)} onclick={() => reactToMessage(message, reaction.emoji)}><span>{reaction.emoji}</span><b>{reaction.users.length}</b></button>{/each}</div>{/if}
                  {/if}
                  {#if receipt}<div class:seen={receipt.seen} class="message-receipt" title={receipt.detail}>{#if receipt.seen}<CheckCheck size={13} strokeWidth={2.4} />{:else}<Check size={13} strokeWidth={2.4} />{/if}<span>{receipt.label}</span></div>{/if}
                </div>
              </article>
              {/if}
            {/each}{/if}
          </div>
          <form class="message-composer" bind:this={composerForm} onsubmit={submit}>
            {#if replyingTo}<div class="composer-reply-context"><Reply size={15} /><span><strong>Replying to {replyingTo.sender_name}</strong><small>{replySnippet(replyingTo)}</small></span><button type="button" onclick={() => replyingTo = null} aria-label="Cancel reply"><X size={14} /></button></div>{/if}
            {#if pendingFiles.length || pendingRecordUids.length || pendingMentionUids.length}<div class="pending-shares">{#each pendingFiles as file, index}{#if isPendingImage(file)}<div class="pending-image-share"><img src={pendingImageUrl(file)} alt="" /><span><strong>{file.name}</strong><small>{fileSize(file.size)} · Image ready to send</small></span><button type="button" aria-label={`Remove ${file.name}`} onclick={() => removePendingFile(index)}><X size={13} /></button></div>{:else}<span><i><Paperclip size={12} /></i>{file.name} · {fileSize(file.size)}<button type="button" aria-label={`Remove ${file.name}`} onclick={() => removePendingFile(index)}><X size={12} /></button></span>{/if}{/each}{#each pendingRecordUids as uid}<span><i><FileText size={12} /></i>Record · {uid.slice(0, 12)}<button type="button" aria-label={`Remove record ${uid}`} onclick={() => pendingRecordUids = pendingRecordUids.filter((item) => item !== uid)}><X size={12} /></button></span>{/each}{#each pendingMentionUids as uid}<span><i><AtSign size={12} /></i>{personName(uid)}<button type="button" aria-label={`Remove mention ${personName(uid)}`} onclick={() => pendingMentionUids = pendingMentionUids.filter((item) => item !== uid)}><X size={12} /></button></span>{/each}</div>{/if}
            {#if toolsOpen}<div class="composer-context-tools"><label><span>Link an MX record</span><div><input bind:value={recordLinkInput} placeholder="Record UID or copied MX link" aria-label="Record to link" /><button class="button small" type="button" onclick={addRecordLink} disabled={!recordLinkInput.trim()}>Add</button></div></label><label><span>Mention someone</span><div><select bind:value={mentionUser} aria-label="Person to mention"><option value="">Choose a person…</option>{#each mentionablePeople.filter((user) => !pendingMentionUids.includes(user.uid)) as user}<option value={user.uid}>{user.name}</option>{/each}</select><button class="button small" type="button" onclick={addMention} disabled={!mentionUser}>Add</button></div></label></div>{/if}
            <div class="composer-entry" data-mention-composer>
              {#if mentionOpen}<div id="composer-mention-options" class="composer-mention-menu" role="listbox" aria-label="People in this conversation"><header><AtSign size={14} /><span><strong>Mention someone</strong><small>{mentionQuery ? `Results for “${mentionQuery}”` : 'People in this conversation'}</small></span></header>{#if mentionSuggestions.length}<div>{#each mentionSuggestions as user, index}<button class:active={mentionActiveIndex === index} type="button" role="option" aria-selected={mentionActiveIndex === index} onmouseenter={() => mentionActiveIndex = index} onmousedown={(event) => event.preventDefault()} onclick={() => chooseMention(user)}><span class="message-avatar"><ProfileAvatar userUid={user.uid} name={user.name} updatedAt={user.profile_photo_updated_at} online={isUserOnline(user.uid)} /></span><span><strong>{user.name}</strong><small class:online={isUserOnline(user.uid)}>{isUserOnline(user.uid) ? 'Online' : 'Offline'}</small></span><span class="mention-enter">↵</span></button>{/each}</div>{:else}<p>{membersLoaded || selected.kind === 'direct' ? 'No matching people in this conversation.' : 'Loading people…'}</p>{/if}</div>{/if}
              <textarea bind:this={composerTextarea} bind:value={body} oninput={composerInput} onkeydown={composerKeydown} onpaste={composerPaste} rows="2" placeholder={`Message ${selected.name}`} aria-label="Message" aria-autocomplete="list" aria-controls="composer-mention-options"></textarea>
              <div class="composer-commandbar"><div><label class="composer-action" title={`Attach files up to ${sharedFileLimitLabel()}`} aria-label={`Attach files up to ${sharedFileLimitLabel()}`}><span><Paperclip size={13} /></span><span class="composer-action-label">Attach</span><input type="file" multiple onchange={(event) => { if (event.currentTarget.files) queueSharedFiles(event.currentTarget.files); event.currentTarget.value = ''; }} /></label><button class:active={toolsOpen} class="composer-action" type="button" onclick={() => toolsOpen = !toolsOpen} title="Add context" aria-label="Add context"><span><Link2 size={13} /></span><span class="composer-action-label">Add context</span></button></div><span class="composer-hint">Type @ to mention · Enter to send · Shift + Enter for a new line</span><button class="button primary send-message icon-label" disabled={sending || (!body.trim() && !pendingFiles.length && !pendingRecordUids.length)}>{#if !sending}<Send size={13} />{/if}<span class="send-message-label">{sending ? 'Sending…' : 'Send'}</span></button></div>
            </div>
          </form>
        {:else}<div class="collaboration-empty"><div class="collaboration-empty-mark">MX</div><h2>Choose a conversation</h2><p>Keep work, documents, and record context together without leaving MX.</p><button class="button primary" onclick={() => createOpen = true}>Create a space</button></div>{/if}
      </main>

      {#if detailsOpen && selected}
        {@const detailsPerson = selected.kind === 'direct' ? personByUid(selected.direct_user_uid) : null}
        <button class="conversation-details-scrim" type="button" onclick={() => detailsOpen = false} aria-label="Close conversation details"></button>
        <aside class="conversation-details">
          <header><strong>Conversation details</strong><button onclick={() => detailsOpen = false} aria-label="Close details"><X size={15} /></button></header>
          <section class="details-identity">{#if detailsPerson}<button class="channel-glyph large direct person-trigger" type="button" onclick={() => openPersonCard(detailsPerson)} aria-label={`View ${detailsPerson.name}`}><ProfileAvatar userUid={detailsPerson.uid} name={detailsPerson.name} updatedAt={detailsPerson.profile_photo_updated_at} online={isDirectOnline(selected)} /></button><h3><button class="conversation-person-name" type="button" onclick={() => openPersonCard(detailsPerson)}>{selected.name}</button></h3>{:else}<span class:direct={selected.kind === 'direct'} class="channel-glyph large">{#if selected.kind === 'direct'}{initials(selected.name)}{:else if selected.kind === 'channel'}<Hash size={20} />{:else}<UsersRound size={20} />{/if}</span><h3>{selected.name}</h3>{/if}<p>{selected.kind === 'direct' ? `Private to you and ${selected.name}. Messages, files, and shared records are not visible to other MX accounts.` : selected.description || 'A shared MX workspace.'}</p>{#if canManageSpace}<button class="button small manage-space-button" onclick={() => openManageSpace(false)}><Pencil size={13} /> Manage space</button>{/if}</section>
          <dl>{#if selected.kind === 'direct'}<div><dt>Status</dt><dd class:online={isDirectOnline(selected)}>{isDirectOnline(selected) ? 'Online' : 'Offline'}</dd></div>{/if}<div><dt>Type</dt><dd>{selected.kind === 'direct' ? 'Direct message' : selected.kind === 'channel' ? 'Channel' : 'Group space'}</dd></div><div><dt>Members</dt><dd>{selected.member_count}</dd></div><div><dt>Your access</dt><dd>{selected.role}</dd></div>{#if selected.kind !== 'direct'}<div><dt>Privacy</dt><dd><LockKeyhole size={11} /> Private</dd></div><div><dt>Who can add</dt><dd>{selected.invite_policy === 'owner' ? 'Owner only' : selected.invite_policy === 'members' ? 'All members' : 'Owner and admins'}</dd></div>{/if}</dl>
          {#if selected.kind !== 'direct'}<section class="details-members"><header><strong>People</strong><div class="details-members-actions"><span>{selected.member_count}</span>{#if canAddPeople}<button type="button" onclick={() => openManageSpace(true)}><UserPlus size={13} /> Add person</button>{/if}</div></header>{#if !membersLoaded}<p>Loading space members…</p>{:else}<div>{#each channelMembers as member}{#if member.user_uid === session.uid}<div class="details-member"><span class="message-avatar"><ProfileAvatar userUid={member.user_uid} name={member.user_name} updatedAt={member.profile_photo_updated_at} online={connected} /></span><span><strong>{member.user_name} (you)</strong><small>{member.role}</small></span></div>{:else}{@const memberPerson = personByUid(member.user_uid)}<button class="details-member" type="button" onclick={() => memberPerson && openPersonCard(memberPerson)} disabled={!memberPerson}><span class="message-avatar"><ProfileAvatar userUid={member.user_uid} name={member.user_name} updatedAt={member.profile_photo_updated_at} online={isUserOnline(member.user_uid)} /></span><span><strong>{member.user_name}</strong><small class:online={isUserOnline(member.user_uid)}>{isUserOnline(member.user_uid) ? 'Online' : 'Offline'} · {member.role}</small></span><MessageCircle size={14} /></button>{/if}{/each}</div>{/if}</section>{/if}
          {#if selected.kind !== 'direct'}<section class="details-membership-action">{#if selected.role === 'owner'}<p><Shield size={13} /><span>Owners must transfer ownership before leaving a space.</span></p>{:else}<button class="button danger small" type="button" onclick={leaveCurrentSpace} disabled={manageBusy === 'leave'}><LogOut size={13} />{manageBusy === 'leave' ? 'Leaving…' : 'Leave space'}</button>{/if}</section>{/if}
          <section class="details-shared"><header><strong>Shared records</strong><span>{sharedRecords.length}</span></header>{#if sharedRecords.length}<div>{#each sharedRecords.slice(-6).reverse() as record}<a href={recordHref(record)}><i><FileText size={14} /></i><span><strong>{record.label}</strong><small>{record.module_name}</small></span></a>{/each}</div>{:else}<p>No records shared in the loaded messages.</p>{/if}</section>
          <section class="details-shared details-files"><header><strong>Shared files</strong><span>{channelFileTotal}</span></header><form class="details-file-search" onsubmit={searchSharedFiles}><label><Search size={13} /><input bind:value={fileSearch} type="search" placeholder="Search all files…" aria-label="Search shared files" /></label><button class="button small" disabled={filesLoading}>{filesLoading ? '…' : 'Search'}</button></form>{#if filesLoading && !channelFiles.length}<p>Searching the complete file history…</p>{:else if channelFiles.length}<div>{#each channelFiles as file}<div class="details-file-row"><button type="button" onclick={() => showSharedFile(file)} title={`Preview ${file.file_name}`}><i><FileText size={14} /></i><span><strong>{file.file_name}</strong><small>{fileSize(file.size)} · {file.sender_name} · Preview</small></span></button><button class="details-file-download" type="button" onclick={() => downloadSharedFile(file)} aria-label={`Download ${file.file_name}`} title={`Download ${file.file_name}`}><DownloadIcon size={14} /></button></div>{/each}</div>{#if channelFiles.length < channelFileTotal}<button class="button small details-load-more" onclick={() => loadSharedFiles(false)} disabled={filesLoading}>{filesLoading ? 'Loading…' : `Load more (${channelFileTotal - channelFiles.length})`}</button>{/if}{:else}<p>{fileSearch.trim() ? `No files match “${fileSearch.trim()}”.` : 'No files shared yet.'}</p>{/if}</section>
        </aside>
      {/if}
    </div>
  </div>
</section>

{#if profilePerson}
  {@const profileDirect = directChannelFor(profilePerson.uid)}
  <div class="collaboration-modal-backdrop person-card-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget && !directBusyUid) profilePerson = null; }}>
    <div class="dialog person-card-dialog" role="dialog" aria-modal="true" aria-labelledby="person-card-name">
      <button class="icon-button person-card-close" type="button" onclick={() => profilePerson = null} aria-label="Close person card"><X size={17} /></button>
      <div class="person-card-identity"><span class="person-card-avatar"><ProfileAvatar userUid={profilePerson.uid} name={profilePerson.name} updatedAt={profilePerson.profile_photo_updated_at} online={isUserOnline(profilePerson.uid)} /></span><div><p class="eyebrow">MX coworker</p><h2 id="person-card-name">{profilePerson.name}</h2><p class:online={isUserOnline(profilePerson.uid)}>{isUserOnline(profilePerson.uid) ? 'Online now' : 'Offline'}</p></div></div>
      <div class="person-card-privacy"><LockKeyhole size={16} /><p><strong>Private conversation</strong><span>Only you and {profilePerson.name} can access its messages, files, and shared record links.</span></p></div>
      <div class="dialog-actions"><button class="button" type="button" onclick={() => profilePerson = null} disabled={!!directBusyUid}>Close</button><button class="button primary icon-label" type="button" onclick={() => startDirectWith(profilePerson!)} disabled={!!directBusyUid}><MessageCircle size={15} />{directBusyUid === profilePerson.uid ? 'Opening…' : profileDirect ? 'Open private chat' : 'Start private chat'}</button></div>
    </div>
  </div>
{/if}

{#if previewFile}
  <FilePreview fileName={previewFile.file_name} load={() => prepareMessageFilePreview(previewFile!.uid, previewFile!.file_name)} onClose={() => previewFile = null} />
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

{#if manageOpen && selected}
  <div class="collaboration-modal-backdrop manage-space-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) closeManageSpace(); }}>
    <div class="dialog manage-space-dialog" role="dialog" aria-modal="true" aria-labelledby="manage-space-title">
      <header><div><p class="eyebrow">MX Connect</p><h2 id="manage-space-title">{canManageSpace ? 'Manage space' : 'Add people'}</h2><p>{canManageSpace ? 'Edit its identity, privacy, invitation policy, and member access.' : 'Invite another MX account to this private space.'}</p></div><button class="icon-button" type="button" onclick={closeManageSpace} aria-label="Close space management"><X size={18} /></button></header>
      {#if manageNotice}<div class="notice success manage-space-notice">{manageNotice}</div>{/if}{#if manageError}<div class="notice error manage-space-notice">{manageError}</div>{/if}
      {#if manageLoading}<div class="manage-space-loading"><div class="loader"></div><span>Loading members…</span></div>{:else}
        {#if canManageSpace}<form class="manage-space-identity" onsubmit={saveSpaceDetails}><label>Space name<input bind:value={manageName} required maxlength="80" /></label><label>Description<textarea bind:value={manageDescription} rows="2" maxlength="500"></textarea></label>{#if selected.role === 'owner'}<label>Who can add people<select bind:value={manageInvitePolicy} aria-label="Who can add people"><option value="owner">Owner only</option><option value="admins">Owner and admins</option><option value="members">Any member</option></select></label>{/if}<button class="button primary icon-label" disabled={manageBusy === 'details' || !manageName.trim()}>{#if manageBusy !== 'details'}<Save size={14} />{/if}{manageBusy === 'details' ? 'Saving…' : 'Save details'}</button></form>
        <section class="manage-members"><header><div><h3>Members</h3><p>{channelMembers.length} people can access this space.</p></div></header>
          <div class="manage-member-list">{#each channelMembers as member}<article><span class="message-avatar"><ProfileAvatar userUid={member.user_uid} name={member.user_name} updatedAt={member.profile_photo_updated_at} online={isUserOnline(member.user_uid)} /></span><div><strong>{member.user_name}{member.user_uid === session.uid ? ' (you)' : ''}</strong><small class:online={isUserOnline(member.user_uid)}>{isUserOnline(member.user_uid) ? 'Online' : 'Offline'} · {member.role}</small></div>{#if member.role === 'owner'}<span class="member-role-badge"><Shield size={12} /> Owner</span>{:else}<select value={member.role} aria-label={`Role for ${member.user_name}`} disabled={member.user_uid === session.uid || selected.role !== 'owner' || !!manageBusy} onchange={(event) => changeManagedRole(member, event.currentTarget.value as 'owner' | 'admin' | 'member')}><option value="member">Member</option><option value="admin">Administrator</option>{#if selected.role === 'owner'}<option value="owner">Transfer ownership</option>{/if}</select><button class="collaboration-icon-button danger-text" type="button" title={`Remove ${member.user_name}`} aria-label={`Remove ${member.user_name}`} disabled={member.user_uid === session.uid || (selected.role !== 'owner' && member.role === 'admin') || !!manageBusy} onclick={() => removeManagedMember(member)}><UserMinus size={15} /></button>{/if}</article>{/each}</div>
        </section>{/if}
        {#if canAddPeople}<section class="manage-add-member"><header><div><h3>Add people</h3><p>Search all MX accounts that are not already in this space.</p></div><UserPlus size={18} /></header><label class="conversation-search"><span><Search size={14} /></span><input bind:this={manageMemberSearchInput} bind:value={manageMemberSearch} onfocus={() => addMemberPickerOpen = true} oninput={() => { addMemberUid = ''; addMemberPickerOpen = true; }} placeholder="Find a person" aria-label="Find a person to add" /></label><div><div class="manage-person-picker" data-manage-person-picker><button class="manage-person-trigger" type="button" onclick={() => addMemberPickerOpen = !addMemberPickerOpen} aria-haspopup="listbox" aria-expanded={addMemberPickerOpen && availableMembers.length > 0}><span>{#if selectedAddMember}<span class="message-avatar"><ProfileAvatar userUid={selectedAddMember.uid} name={selectedAddMember.name} updatedAt={selectedAddMember.profile_photo_updated_at} online={isUserOnline(selectedAddMember.uid)} /></span><span><strong>{selectedAddMember.name}</strong><small>{isUserOnline(selectedAddMember.uid) ? 'Online' : 'Offline'}</small></span>{:else}<span class="picker-placeholder">{availableMembers.length ? 'Choose a person' : 'No matching people'}</span>{/if}</span><ChevronDown size={14} /></button>{#if addMemberPickerOpen && availableMembers.length}<div class="manage-person-options" role="listbox" aria-label="Person to add">{#each availableMembers as user}<button class:selected={user.uid === addMemberUid} type="button" role="option" aria-selected={user.uid === addMemberUid} onclick={() => { addMemberUid = user.uid; addMemberPickerOpen = false; }}><span class="message-avatar"><ProfileAvatar userUid={user.uid} name={user.name} updatedAt={user.profile_photo_updated_at} online={isUserOnline(user.uid)} /></span><span><strong>{user.name}</strong><small class:online={isUserOnline(user.uid)}>{isUserOnline(user.uid) ? 'Online' : 'Offline'}</small></span>{#if user.uid === addMemberUid}<Check size={14} />{/if}</button>{/each}</div>{/if}</div>{#if selected.role === 'owner'}<select bind:value={addMemberRole} aria-label="New member role"><option value="member">Member</option><option value="admin">Administrator</option></select>{/if}<button class="button primary icon-label" type="button" onclick={addManagedMember} disabled={!addMemberUid || !!manageBusy}><UserPlus size={14} /> Add</button></div></section>{:else}<section class="manage-add-member manage-add-restricted"><LockKeyhole size={17} /><div><h3>Invitations are restricted</h3><p>The space owner currently limits adding people to a more privileged role.</p></div></section>{/if}
      {/if}
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
        <fieldset class="space-member-picker"><legend>Add people <span>{selectedMembers.length} selected</span></legend><label class="conversation-search"><span><Search size={14} /></span><input bind:value={memberSearch} placeholder="Find a person" aria-label="Find a person" /></label><div>{#each visiblePeople as user}<label class="space-member"><input type="checkbox" checked={selectedMembers.includes(user.uid)} onchange={() => toggleMember(user.uid)} /><span class="message-avatar"><ProfileAvatar userUid={user.uid} name={user.name} updatedAt={user.profile_photo_updated_at} online={isUserOnline(user.uid)} /></span><strong>{user.name}</strong><small class:online={isUserOnline(user.uid)}>{isUserOnline(user.uid) ? 'Online' : 'Offline'}</small></label>{/each}{#if !visiblePeople.length}<p class="muted">No matching people.</p>{/if}</div></fieldset>
        <div class="dialog-actions"><button class="button" type="button" onclick={closeCreate}>Cancel</button><button class="button primary">Create space</button></div>
      </form>
    </div>
  </div>
{/if}
