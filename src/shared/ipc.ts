/** Channel names shared by the main process and the preload script. */
export const IPC = {
  getState: 'library:get-state',
  openFolder: 'library:open-folder',
  stateChanged: 'library:state-changed',
} as const;
