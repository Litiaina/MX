import { mount } from 'svelte';
import OfficeEditor from './lib/components/OfficeEditor.svelte';
import AttachmentOfficeViewer from './lib/components/AttachmentOfficeViewer.svelte';
const target = document.getElementById('office-app')!;
const params = new URLSearchParams(location.hash.slice(1));
if (params.has('message_file') || params.has('record') || params.has('attachment')) mount(AttachmentOfficeViewer, { target });
else mount(OfficeEditor, { target });
