const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  openExternal: (url) => ipcRenderer.invoke('open-external', url),
  getPlatform: () => ipcRenderer.invoke('get-platform'),
  pickFolder: () => ipcRenderer.invoke('pick-folder'),
  getBackendUrl: () => ipcRenderer.invoke('get-backend-url'),
  isElectron: true
});
