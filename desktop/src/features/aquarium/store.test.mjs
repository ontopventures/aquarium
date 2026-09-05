import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  MOCK_DEVICE_OFFLINE_ID,
  MOCK_DEVICE_READY_ID,
  MOCK_ISSUE_AUTH_ID,
  MOCK_ISSUE_DASHBOARD_ID,
  MOCK_PROFILE_INK_ID,
  MOCK_REPO_ID,
  MOCK_TANK_AUTH_ID,
} from "./mock/fixtures.ts";
import {
  addCreatureFromProfile,
  createCreatureProfile,
  getAquariumSnapshot,
  provisionTank,
  resetAquariumStore,
  sendTankMessage,
} from "./store.ts";

beforeEach(() => {
  resetAquariumStore();
});

test("fixtures are labelled mock, never as live Linear or device data", () => {
  const snapshot = getAquariumSnapshot();
  assert.equal(snapshot.source, "mock");
  assert.equal(snapshot.linear.source, "mock");
  assert.match(snapshot.linear.message, /not a live/i);
  for (const tank of snapshot.tanks) assert.equal(tank.source, "mock");
  for (const issue of snapshot.issues) assert.equal(issue.source, "mock");
  for (const device of snapshot.devices) assert.equal(device.source, "mock");
});

test("sidebar and dashboard resolve the same canonical tank id", () => {
  const snapshot = getAquariumSnapshot();
  const sidebarTank = snapshot.tanks.find(
    (tank) => tank.id === MOCK_TANK_AUTH_ID,
  );
  const dashboardIssue = snapshot.issues.find(
    (issue) => issue.id === MOCK_ISSUE_AUTH_ID,
  );
  assert.ok(sidebarTank);
  assert.equal(dashboardIssue?.tank_id, sidebarTank.id);
});

test("viewing an issue does not create a tank", () => {
  const before = getAquariumSnapshot().tanks.length;
  const issue = getAquariumSnapshot().issues.find(
    (item) => item.id === MOCK_ISSUE_DASHBOARD_ID,
  );
  assert.equal(issue?.tank_id, null);
  assert.equal(getAquariumSnapshot().tanks.length, before);
});

test("provisionTank from a fixture issue creates one canonical tank", async () => {
  const created = await provisionTank({
    title: "Dashboard empty state",
    issue_id: MOCK_ISSUE_DASHBOARD_ID,
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_READY_ID,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  assert.equal(created.source, "mock");
  assert.equal(created.existing, false);
  const again = await provisionTank({
    title: "Dashboard empty state",
    issue_id: MOCK_ISSUE_DASHBOARD_ID,
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_READY_ID,
  });
  assert.equal(again.ok, true);
  if (!again.ok) return;
  assert.equal(again.existing, true);
  assert.equal(again.tank.id, created.tank.id);
  const linked = getAquariumSnapshot().issues.find(
    (issue) => issue.id === MOCK_ISSUE_DASHBOARD_ID,
  );
  assert.equal(linked?.tank_id, created.tank.id);
});

test("offline device does not fall back locally", async () => {
  const before = getAquariumSnapshot().tanks.map((tank) => tank.id);
  const result = await provisionTank({
    title: "Should not create",
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_OFFLINE_ID,
  });
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.match(result.error, /does not fall back/i);
  assert.equal(result.source, "mock");
  const after = getAquariumSnapshot().tanks.map((tank) => tank.id);
  assert.deepEqual(after, before);
});

test("adding a saved profile to a tank that already has a leader creates a fresh non-leader instance", () => {
  const existing = getAquariumSnapshot().creatures.find(
    (creature) => creature.tank_id === MOCK_TANK_AUTH_ID && creature.leader,
  );
  assert.ok(existing);
  const added = addCreatureFromProfile(MOCK_TANK_AUTH_ID, MOCK_PROFILE_INK_ID);
  assert.ok(added);
  assert.equal(added.source, "mock");
  assert.notEqual(added.instance_id, MOCK_PROFILE_INK_ID);
  assert.notEqual(added.instance_id, existing.instance_id);
  assert.equal(added.profile_id, MOCK_PROFILE_INK_ID);
  assert.equal(added.leader, false);
});

test("first creature in a new tank becomes leader; second does not", async () => {
  const created = await provisionTank({
    title: "Empty for leader test",
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_READY_ID,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const leader = addCreatureFromProfile(created.tank.id, MOCK_PROFILE_INK_ID);
  assert.equal(leader?.leader, true);
  const second = addCreatureFromProfile(created.tank.id, MOCK_PROFILE_INK_ID);
  assert.equal(second?.leader, false);
  assert.notEqual(second?.instance_id, leader?.instance_id);
});

test("createCreatureProfile stays a template; adding copies into a new instance", async () => {
  const result = await provisionTank({
    title: "New mock tank",
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_READY_ID,
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  const profile = createCreatureProfile({
    name: "Nori",
    description: "Mock instructions",
    species: "fish",
    color: "#cad3f5",
  });
  const instance = addCreatureFromProfile(result.tank.id, profile.id);
  assert.ok(instance);
  assert.equal(instance.profile_id, profile.id);
  assert.notEqual(instance.instance_id, profile.id);
  assert.equal(
    getAquariumSnapshot().profiles.some((item) => item.id === profile.id),
    true,
  );
});

test("first message collapses ocean without requiring a creature", async () => {
  const result = await provisionTank({
    title: "Creature-less",
    repository_id: MOCK_REPO_ID,
    device_id: MOCK_DEVICE_READY_ID,
  });
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.tank.oceanCollapsed, false);
  sendTankMessage(result.tank.id, "hello from mock");
  const tank = getAquariumSnapshot().tanks.find(
    (item) => item.id === result.tank.id,
  );
  assert.equal(tank?.oceanCollapsed, true);
  assert.equal(
    getAquariumSnapshot().creatures.some(
      (creature) => creature.tank_id === result.tank.id,
    ),
    false,
  );
});
