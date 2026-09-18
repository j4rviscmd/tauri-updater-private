// Pure re-export of the official updater JS API so that the conventional
// `npm i tauri-updater-private` install works. Private-specific helpers can
// be added here later.
//
// Usage rule (see README): never pass `headers` to check/download/downloadAndInstall
// from the frontend — download-side headers replace the preset Authorization map.
export * from '@tauri-apps/plugin-updater'
