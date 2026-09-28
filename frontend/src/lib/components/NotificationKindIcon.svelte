<script lang="ts">
  import AtSign from '@lucide/svelte/icons/at-sign';
  import Bell from '@lucide/svelte/icons/bell';
  import FileCheck2 from '@lucide/svelte/icons/file-check-2';
  import FilePenLine from '@lucide/svelte/icons/file-pen-line';
  import FilePlus2 from '@lucide/svelte/icons/file-plus-2';
  import FileX2 from '@lucide/svelte/icons/file-x-2';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import type { NotificationCategory } from '../util/notificationKinds';

  let { category, size = 17 }: { category: NotificationCategory; size?: number } = $props();
</script>

{#if category === 'mention'}<AtSign {size} />
{:else if category === 'message'}<MessageSquare {size} />
{:else if category === 'file'}<Paperclip {size} />
{:else if category === 'collaboration'}<UsersRound {size} />
{:else if category === 'record-created'}<FilePlus2 {size} />
{:else if category === 'record-updated'}<FilePenLine {size} />
{:else if category === 'record-deleted'}<FileX2 {size} />
{:else if category === 'assignment'}<FileCheck2 {size} />
{:else if category === 'workflow'}<GitBranch {size} />
{:else if category === 'task'}<ListChecks {size} />
{:else if category === 'security'}<ShieldCheck {size} />
{:else}<Bell {size} />{/if}
