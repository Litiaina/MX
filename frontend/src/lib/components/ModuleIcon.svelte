<script lang="ts">
  import BriefcaseBusiness from '@lucide/svelte/icons/briefcase-business';
  import Building2 from '@lucide/svelte/icons/building-2';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import ClipboardList from '@lucide/svelte/icons/clipboard-list';
  import Database from '@lucide/svelte/icons/database';
  import FileText from '@lucide/svelte/icons/file-text';
  import Folder from '@lucide/svelte/icons/folder';
  import GraduationCap from '@lucide/svelte/icons/graduation-cap';
  import HeartPulse from '@lucide/svelte/icons/heart-pulse';
  import Inbox from '@lucide/svelte/icons/inbox';
  import Landmark from '@lucide/svelte/icons/landmark';
  import Package from '@lucide/svelte/icons/package';
  import Scale from '@lucide/svelte/icons/scale';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Users from '@lucide/svelte/icons/users';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { normaliseModuleIcon } from '../util/moduleIcons';

  let { name, size = 16, strokeWidth = 2 }:
    { name?: string | null; size?: number; strokeWidth?: number } = $props();
  const icon = $derived(normaliseModuleIcon(name));
</script>

{#if icon === 'folder'}<Folder {size} {strokeWidth} />
{:else if icon === 'inbox'}<Inbox {size} {strokeWidth} />
{:else if icon === 'clipboard-list'}<ClipboardList {size} {strokeWidth} />
{:else if icon === 'database'}<Database {size} {strokeWidth} />
{:else if icon === 'users'}<Users {size} {strokeWidth} />
{:else if icon === 'building-2'}<Building2 {size} {strokeWidth} />
{:else if icon === 'briefcase-business'}<BriefcaseBusiness {size} {strokeWidth} />
{:else if icon === 'calendar-days'}<CalendarDays {size} {strokeWidth} />
{:else if icon === 'package'}<Package {size} {strokeWidth} />
{:else if icon === 'landmark'}<Landmark {size} {strokeWidth} />
{:else if icon === 'scale'}<Scale {size} {strokeWidth} />
{:else if icon === 'heart-pulse'}<HeartPulse {size} {strokeWidth} />
{:else if icon === 'graduation-cap'}<GraduationCap {size} {strokeWidth} />
{:else if icon === 'shield-check'}<ShieldCheck {size} {strokeWidth} />
{:else if icon === 'wrench'}<Wrench {size} {strokeWidth} />
{:else}<FileText {size} {strokeWidth} />{/if}
