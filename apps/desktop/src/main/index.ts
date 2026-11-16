import path from 'node:path';
import { BrowserWindow, app, dialog, ipcMain, nativeTheme } from 'electron';
import type { LibraryState, OpenFolderResult } from '../shared/types';
import { IPC } from '../shared/ipc';
import { handleMediaProtocol, registerMediaScheme } from './mediaProtocol';
import { loadCore } from './nativeCore';

registerMediaScheme();
// The UI is dark only, so dialogs, menus and the Windows title bar should be too.
nativeTheme.themeSource = 'dark';

let mainWindow: BrowserWindow | null = null;

const { LibraryManager } = loadCore();
const libraries = new LibraryManager((state: LibraryState) => {
  mainWindow?.webContents.send(IPC.stateChanged, state);
});

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1100,
    height: 720,
    minWidth: 760,
    minHeight: 480,
    backgroundColor: '#0d0e12',
    title: 'DJ MP3 Player',
    // On macOS the UI runs up to the top edge, with the window buttons over the
    // sidebar. The renderer leaves room for them and marks where to drag.
    ...(process.platform === 'darwin'
      ? { titleBarStyle: 'hiddenInset' as const, trafficLightPosition: { x: 18, y: 16 } }
      : {}),
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      sandbox: true,
    },
  });

  // The UI is a single page. Without this, dropping a file on the window would
  // navigate away from it. Reloads (same URL) stay allowed for development.
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', (event, url) => {
    if (url !== window.webContents.getURL()) event.preventDefault();
  });

  // electron-vite sets this to the dev server's URL when running `npm run dev`.
  const devServerUrl = process.env.ELECTRON_RENDERER_URL;
  if (!app.isPackaged && devServerUrl) {
    void window.loadURL(devServerUrl);
  } else {
    void window.loadFile(path.join(__dirname, '../renderer/index.html'));
  }
  window.on('closed', () => {
    mainWindow = null;
  });
  return window;
}

async function openFolder(): Promise<OpenFolderResult> {
  if (!mainWindow) return { status: 'cancelled' };
  const { canceled, filePaths } = await dialog.showOpenDialog(mainWindow, {
    title: 'Open a DJ USB or a copy of one',
    properties: ['openDirectory'],
  });
  const folderPath = filePaths[0];
  if (canceled || folderPath === undefined) return { status: 'cancelled' };
  return libraries.addFolder(folderPath) ? { status: 'opened' } : { status: 'no-library', folderPath };
}

void app.whenReady().then(() => {
  handleMediaProtocol(libraries);
  ipcMain.handle(IPC.getState, () => libraries.getState());
  ipcMain.handle(IPC.openFolder, openFolder);
  mainWindow = createWindow();
  libraries.start();
});

app.on('window-all-closed', () => {
  libraries.stop();
  app.quit();
});
