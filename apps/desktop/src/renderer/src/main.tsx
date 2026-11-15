import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';

// The macOS window has no title bar of its own (see `createWindow`), so the styles make room for one.
if (navigator.userAgent.includes('Macintosh')) document.documentElement.dataset.platform = 'mac';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
