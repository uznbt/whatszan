const { contextBridge, ipcRenderer, webFrame } = require("electron");

// const { isDebug } = require("../src/util.mjs");
const isDebug = process?.env?.DEBUG == 1;

contextBridge.exposeInMainWorld("ipc", {
  debug: isDebug,
  notifyEv: (args) => ipcRenderer.invoke("notifyEv", args),
  open: (url) => ipcRenderer.invoke("open", url),
  stateGet: (name) => ipcRenderer.invoke("stateGet", name),
  windowToggle: () => ipcRenderer.invoke("windowToggle"),
  escapePressed: () => ipcRenderer.invoke("escapePressed"),
  ...(isDebug && {
    ping: () => ipcRenderer.invoke("ping"),
  }),
  logToMain: (msg) => ipcRenderer.invoke("log-to-main", msg),
  insertText: (text) => ipcRenderer.invoke("insert-text", text),
  updateRecentChats: (chats) => ipcRenderer.send("update-recent-chats", chats),
  onJumpListAction: (callback) => ipcRenderer.on("jump-list-action", (event, data) => callback(data)),
  onSettingsSaved: (callback) => ipcRenderer.on("settings-saved", () => callback()),
  notifySend: (id, title, body, iconDataUrl) => ipcRenderer.invoke("notifySend", id, title, body, iconDataUrl),
  onNotifyClick: (callback) => ipcRenderer.on("notify-click", (event, id) => callback(id)),
  onNotifyReply: (callback) => ipcRenderer.on("notify-reply", (event, id, reply) => callback(id, reply)),
});

// Bridge untuk screen-picker.html: kirim pilihan layar ke main process
contextBridge.exposeInMainWorld("screenPickerIpc", {
  selectSource: (sourceId) => ipcRenderer.invoke("screen-share-select", sourceId),
});

webFrame.setVisualZoomLevelLimits(1, 3);

