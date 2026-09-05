export function createLocalId(prefix: string): string {
  const random =
    globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
  return `${prefix}-${random}`;
}

/** Device request ids: 13-digit epoch ms, hyphen, 32 lowercase hex. */
export const DEVICE_REQUEST_ID_PATTERN = /^(\d{13})-[0-9a-f]{32}$/;

export function createDeviceRequestId(now = Date.now()): string {
  const ts = now.toString().padStart(13, "0").slice(-13);
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  const hex = Array.from(bytes, (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  return `${ts}-${hex}`;
}

export function slugBranch(title: string): string {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  return `aquarium/${slug || "tank"}`;
}
