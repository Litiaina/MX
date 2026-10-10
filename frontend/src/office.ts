import { mount } from 'svelte';
import OfficeEditor from './lib/components/OfficeEditor.svelte';
mount(OfficeEditor, { target: document.getElementById('office-app')! });
