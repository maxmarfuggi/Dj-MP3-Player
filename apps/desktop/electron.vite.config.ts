import react from '@vitejs/plugin-react';
import { defineConfig } from 'electron-vite';

// Entry points use electron-vite's defaults: src/main/index.ts,
// src/preload/index.ts and src/renderer/index.html.
export default defineConfig({
  main: {
    // The Rust core is loaded at runtime. The native addon cannot be bundled.
    build: {
      rollupOptions: {
        external: ['djusb-node'],
      },
    },
  },
  preload: {},
  renderer: {
    plugins: [react()],
  },
});
