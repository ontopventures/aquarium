/**
 * Host-local Linear personal-API-key storage.
 *
 * Orca stores the key on the active runtime (Electron encrypted storage /
 * remote runtime). Buzz already has an OS keyring blob for nsec
 * (`desktop/src-tauri/src/secret_store.rs`). This UI lane cannot add Tauri
 * commands; the intended invoke names are listed for the backend mailbox.
 *
 * Until those commands exist: keep the key in process memory only. Never write
 * `lin_api_` material to localStorage. Never call Linear over the network
 * without authorized access plus a bound real adapter.
 */

export const LINEAR_PERSONAL_KEY_SETTINGS_URL =
  "https://linear.app/settings/account/security";

export const LINEAR_HOST_LOCAL_BACKEND_PATHS = [
  "desktop/src-tauri/src/secret_store.rs — extend the existing OS-keyring JSON blob with a Linear personal-key slot (do not reuse the nsec key name)",
  "desktop/src-tauri invoke: aquarium_linear_secret_set | aquarium_linear_secret_get | aquarium_linear_secret_clear",
  "Must stay host-local; do not sync via relay or localStorage",
] as const;

const LINEAR_KEY_PREFIX = "lin_api_";
const LOCAL_STORAGE_PROBE_KEY = "aquarium.linear.apiKey";

let memoryKey: string | null = null;

export function looksLikeLinearPersonalKey(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith(LINEAR_KEY_PREFIX) && trimmed.length >= 16;
}

export function peekLinearKeyFromWebStorage(): string | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage.getItem(LOCAL_STORAGE_PROBE_KEY);
  } catch {
    return null;
  }
}

export async function storeLinearKeyHostLocal(
  apiKey: string,
): Promise<{ ok: true; persisted: false } | { ok: false; error: string }> {
  const trimmed = apiKey.trim();
  if (!trimmed) {
    return { ok: false, error: "Paste a Personal API key." };
  }
  if (typeof window !== "undefined") {
    try {
      window.localStorage.removeItem(LOCAL_STORAGE_PROBE_KEY);
    } catch {
      // Ignore quota / privacy-mode failures; we still refuse to persist.
    }
  }
  memoryKey = trimmed;
  return { ok: true, persisted: false };
}

export async function loadLinearKeyHostLocal(): Promise<string | null> {
  return memoryKey;
}

export async function clearLinearKeyHostLocal(): Promise<void> {
  memoryKey = null;
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(LOCAL_STORAGE_PROBE_KEY);
  } catch {
    // Same as store: absence of web storage is not a persist path.
  }
}

export function resetLinearKeyMemoryForTests(): void {
  memoryKey = null;
}
