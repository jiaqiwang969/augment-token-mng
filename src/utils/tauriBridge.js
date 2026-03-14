export const hasTauriBridge = (target = globalThis.window) => {
  const internals = target?.__TAURI_INTERNALS__
  return typeof internals?.invoke === 'function'
}
