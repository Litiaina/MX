import { mount } from 'svelte';
import App from './App.svelte';
import './styles/tokens.css';
import './styles/app.css';
import './styles/foundation.css';
import './styles/shell.css';
import './styles/workspaces.css';

const target = document.getElementById('app');

if (!target) {
  throw new Error('MX frontend mount point was not found');
}

mount(App, { target });
