import { mount } from 'svelte';
import App from './App.svelte';
import './styles/app.css';

const target = document.getElementById('app');

if (!target) {
  throw new Error('MX frontend mount point was not found');
}

mount(App, { target });
