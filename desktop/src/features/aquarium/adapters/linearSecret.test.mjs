import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import {
  clearLinearKeyHostLocal,
  looksLikeLinearPersonalKey,
  peekLinearKeyFromWebStorage,
  resetLinearKeyMemoryForTests,
  storeLinearKeyHostLocal,
} from "./linearSecret.ts";

afterEach(async () => {
  resetLinearKeyMemoryForTests();
  await clearLinearKeyHostLocal();
});

test("does not treat short mock tokens as Linear personal keys", () => {
  assert.equal(looksLikeLinearPersonalKey("mock-key"), false);
  assert.equal(
    looksLikeLinearPersonalKey("lin_api_abcdefghijklmnopqrstuvwxyz"),
    true,
  );
});

test("storeLinearKeyHostLocal never writes the key to localStorage", async () => {
  const store = new Map();
  const setItemCalls = [];
  const previousWindow = globalThis.window;
  globalThis.window = {
    localStorage: {
      getItem: (key) => store.get(key) ?? null,
      setItem: (key, value) => {
        setItemCalls.push([key, value]);
        store.set(key, value);
      },
      removeItem: (key) => {
        store.delete(key);
      },
    },
  };
  try {
    const result = await storeLinearKeyHostLocal(
      "lin_api_this_is_not_sent_anywhere",
    );
    assert.equal(result.ok, true);
    if (result.ok) assert.equal(result.persisted, false);
    assert.equal(peekLinearKeyFromWebStorage(), null);
    assert.equal(store.has("aquarium.linear.apiKey"), false);
    assert.deepEqual(setItemCalls, []);
    assert.equal(
      setItemCalls.some(
        ([key, value]) =>
          String(key).includes("lin_api_") ||
          String(value).includes("lin_api_"),
      ),
      false,
    );
  } finally {
    globalThis.window = previousWindow;
  }
});
