import assert from "node:assert/strict";
import test from "node:test";

import { tankMessageToTimeline } from "./timeline.ts";

test("tank messages keep mock source semantics on the Buzz timeline contract", () => {
  const row = tankMessageToTimeline({
    source: "mock",
    id: "msg-mock-1",
    tank_id: "tank-mock-auth-login",
    author: "human",
    body: "Please tighten the login empty state.",
    createdAt: 1_700_000_000_000,
  });
  assert.equal(row.id, "msg-mock-1");
  assert.equal(row.body, "Please tighten the login empty state.");
  assert.equal(row.author, "You");
  assert.equal(row.isAgent, false);
  assert.equal(row.pending, undefined);
  assert.equal(row.depth, 0);
});

test("creature tank messages render as agent authors", () => {
  const row = tankMessageToTimeline({
    source: "mock",
    id: "msg-mock-2",
    tank_id: "tank-mock-auth-login",
    author: "creature",
    instance_id: "instance-mock-coral-1",
    body: "Working on it.",
    createdAt: 1_700_000_000_100,
  });
  assert.equal(row.author, "Creature");
  assert.equal(row.isAgent, true);
});
