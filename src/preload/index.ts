import { type IpcRendererEvent, contextBridge, ipcRenderer } from 'electron';
import { IPC } from '../shared/ipc';
import type { DjUsbApi, LibraryState } from '../shared/types';

const api: DjUsbApi = {
  getState: () => ipcRenderer.invoke(IPC.getState),
  openFolder: () => ipcRenderer.invoke(IPC.openFolder),
  onStateChanged(listener) {
    const handler = (_event: IpcRendererEvent, state: LibraryState) => listener(state);
    ipcRenderer.on(IPC.stateChanged, handler);
    return () => {
      ipcRenderer.removeListener(IPC.stateChanged, handler);
    };
  },
};

contextBridge.exposeInMainWorld('djusb', api);
