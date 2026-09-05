import { useSyncExternalStore } from "react";

import {
  createMockDeviceAdapter,
  createMockLinearAdapter,
  type AquariumAdapters,
} from "./adapters";
import {
  clearLinearKeyHostLocal,
  loadLinearKeyHostLocal,
  storeLinearKeyHostLocal,
} from "./adapters/linearSecret";
import { createDeviceRequestId, createLocalId, slugBranch } from "./lib/ids";
import { createMockState } from "./mock/fixtures";
import type {
  AquariumState,
  ContextSection,
  CreatureInstance,
  CreatureProfile,
  DeviceOpResult,
  LinearConnection,
  ProvisionTankInput,
  ProvisionTankResult,
  Tank,
} from "./types";

let state: AquariumState = createMockState();
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

function update(
  patch: Partial<AquariumState> | ((current: AquariumState) => AquariumState),
): void {
  state = typeof patch === "function" ? patch(state) : { ...state, ...patch };
  emit();
}

export function getAquariumSnapshot(): AquariumState {
  return state;
}

export function subscribeAquarium(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function bindMockAdapters(): AquariumAdapters {
  return {
    source: "mock",
    device: createMockDeviceAdapter(() => state.devices),
    linear: createMockLinearAdapter(
      () => state.issues,
      () => state.linear,
      replaceLinearConnection,
    ),
  };
}

let adapters: AquariumAdapters = bindMockAdapters();

export function resetAquariumStore(): void {
  void clearLinearKeyHostLocal();
  state = createMockState();
  adapters = bindMockAdapters();
  emit();
}

/** Test/future seam: replace mock adapters. Real adapters must keep source honest. */
export function bindAquariumAdapters(next: AquariumAdapters): void {
  adapters = next;
}

export function getBoundAquariumAdapters(): AquariumAdapters {
  return adapters;
}

export function replaceLinearConnection(next: LinearConnection): void {
  update({ linear: next });
}

export function useAquariumStore(): AquariumState {
  return useSyncExternalStore(
    subscribeAquarium,
    getAquariumSnapshot,
    getAquariumSnapshot,
  );
}

export function activeTanks(snapshot: AquariumState = state): Tank[] {
  return snapshot.tanks.filter((tank) => tank.status !== "completed");
}

export function tankById(
  tankId: string,
  snapshot: AquariumState = state,
): Tank | undefined {
  return snapshot.tanks.find((tank) => tank.id === tankId);
}

export function creaturesForTank(
  tankId: string,
  snapshot: AquariumState = state,
): CreatureInstance[] {
  return snapshot.creatures.filter((creature) => creature.tank_id === tankId);
}

export function messagesForTank(
  tankId: string,
  snapshot: AquariumState = state,
): AquariumState["messages"] {
  return snapshot.messages
    .filter((message) => message.tank_id === tankId)
    .slice()
    .sort((a, b) => a.createdAt - b.createdAt);
}

export function issueForTank(
  tank: Tank,
  snapshot: AquariumState = state,
): AquariumState["issues"][number] | undefined {
  if (!tank.issue_id) return undefined;
  return snapshot.issues.find((issue) => issue.id === tank.issue_id);
}

export async function searchMockIssues(query: string) {
  return adapters.linear.searchIssues(query);
}

export async function connectMockLinear(apiKey: string) {
  const trimmed = apiKey.trim();
  if (trimmed) {
    const stored = await storeLinearKeyHostLocal(trimmed);
    if (!stored.ok) {
      return {
        source: "mock" as const,
        connected: false,
        message: stored.error,
      };
    }
  } else {
    const existing = await loadLinearKeyHostLocal();
    if (!existing) {
      return adapters.linear.connection();
    }
  }
  return adapters.linear.connectApiKey(trimmed || "recheck");
}

export async function disconnectMockLinear() {
  await clearLinearKeyHostLocal();
  return adapters.linear.disconnect();
}

function replaceTank(tank: Tank): void {
  update((current) => ({
    ...current,
    tanks: current.tanks.map((item) => (item.id === tank.id ? tank : item)),
    issues: current.issues.map((issue) =>
      issue.id === tank.issue_id ? { ...issue, tank_id: tank.id } : issue,
    ),
  }));
}

function recoverTank(existing: Tank, checkout: DeviceOpResult): Tank {
  return {
    ...existing,
    source: checkout.source,
    status: "active",
    branch: checkout.branch ?? existing.branch,
    worktree_path: checkout.worktree_path ?? existing.worktree_path,
    request_id: checkout.request_id ?? existing.request_id,
    errorMessage: undefined,
  };
}

async function applyCheckout(
  tankId: string,
  input: ProvisionTankInput,
  request_id: string,
  opts?: { replace?: boolean; createdAt?: number },
): Promise<ProvisionTankResult> {
  const title = input.title.trim();
  const checkout = await adapters.device.createCheckout({
    tank_id: tankId,
    device_id: input.device_id,
    repository_id: input.repository_id,
    branch: slugBranch(title),
    relpath: `tanks/${tankId}`,
    request_id,
  });
  if (checkout.status !== "succeeded") {
    const failed: Tank = {
      source: checkout.source,
      id: tankId,
      title,
      description: input.description,
      status: "error",
      issue_id: input.issue_id ?? null,
      repository_id: input.repository_id,
      device_id: input.device_id,
      request_id,
      oceanCollapsed: false,
      errorMessage: checkout.message,
      createdAt: opts?.createdAt ?? Date.now(),
    };
    if (opts?.replace) {
      replaceTank(failed);
    } else {
      update((current) => ({ ...current, tanks: [...current.tanks, failed] }));
    }
    return {
      ok: false,
      source: checkout.source,
      error: checkout.message,
      existingTankId: tankId,
    };
  }

  const tank: Tank = {
    source: checkout.source,
    id: tankId,
    title,
    description: input.description,
    status: "active",
    issue_id: input.issue_id ?? null,
    repository_id: input.repository_id,
    device_id: input.device_id,
    branch: checkout.branch,
    worktree_path: checkout.worktree_path,
    request_id: checkout.request_id ?? request_id,
    oceanCollapsed: false,
    createdAt: opts?.createdAt ?? Date.now(),
  };
  if (opts?.replace) {
    replaceTank(tank);
  } else {
    update((current) => ({
      ...current,
      tanks: [...current.tanks, tank],
      issues: current.issues.map((issue) =>
        issue.id === input.issue_id ? { ...issue, tank_id: tank.id } : issue,
      ),
    }));
  }
  return {
    ok: true,
    source: tank.source,
    tank,
    existing: Boolean(opts?.replace),
  };
}

/**
 * Production create seam used by the shared Buzz creation dialog.
 * Does not call the channel-metadata callback.
 */
export async function provisionTank(
  input: ProvisionTankInput,
): Promise<ProvisionTankResult> {
  const title = input.title.trim();
  if (!title) {
    return {
      ok: false,
      source: adapters.source,
      error: "A tank needs a title.",
    };
  }
  if (!input.repository_id) {
    return {
      ok: false,
      source: adapters.source,
      error: "Select a repository.",
    };
  }
  if (!input.device_id) {
    return {
      ok: false,
      source: adapters.source,
      error: "Select an execution device.",
    };
  }

  update({ provisioning: true });
  try {
    const capabilities = await adapters.device.inspectCapabilities(
      input.device_id,
    );
    if (!capabilities.online || capabilities.setup_readiness !== "ready") {
      return {
        ok: false,
        source: capabilities.source,
        error:
          "Execution device is offline or not ready. Create does not fall back to this machine.",
      };
    }

    if (input.issue_id) {
      const existing = state.tanks.find(
        (tank) => tank.issue_id === input.issue_id,
      );
      if (existing && existing.status !== "error") {
        return {
          ok: true,
          source: existing.source,
          tank: existing,
          existing: true,
        };
      }
      if (existing?.status === "error" && existing.request_id) {
        const inspected = await adapters.device.inspectRequest(
          existing.request_id,
        );
        if (inspected.status === "succeeded") {
          const recovered = recoverTank(existing, inspected);
          replaceTank(recovered);
          return {
            ok: true,
            source: recovered.source,
            tank: recovered,
            existing: true,
          };
        }
        return applyCheckout(
          existing.id,
          {
            title: existing.title,
            description: existing.description,
            issue_id: existing.issue_id,
            repository_id: existing.repository_id,
            device_id: existing.device_id,
          },
          existing.request_id,
          { replace: true, createdAt: existing.createdAt },
        );
      }
    }

    const tankId = createLocalId("tank-mock");
    return applyCheckout(tankId, input, createDeviceRequestId());
  } finally {
    update({ provisioning: false });
  }
}

export function addCreatureFromProfile(
  tankId: string,
  profileId: string,
): CreatureInstance | null {
  const tank = tankById(tankId);
  const profile = state.profiles.find((item) => item.id === profileId);
  if (!tank || !profile) return null;

  const hasLeader = creaturesForTank(tankId).some(
    (creature) => creature.leader,
  );
  const instance: CreatureInstance = {
    source: profile.source,
    instance_id: createLocalId("instance-mock"),
    profile_id: profile.id,
    tank_id: tankId,
    name: profile.name,
    description: profile.description,
    species: profile.species,
    color: profile.color,
    harness: profile.harness,
    animation: "idle",
    applicationStatus: "resting",
    leader: !hasLeader,
  };
  update((current) => ({
    ...current,
    creatures: [...current.creatures, instance],
    selectedCreatureId: instance.instance_id,
    tanks: current.tanks.map((item) =>
      item.id === tankId ? { ...item, oceanCollapsed: true } : item,
    ),
  }));
  return instance;
}

export function createCreatureProfile(input: {
  name: string;
  description: string;
  species: CreatureProfile["species"];
  color: string;
}): CreatureProfile {
  const profile: CreatureProfile = {
    source: "mock",
    id: createLocalId("profile-mock"),
    name: input.name.trim() || "New creature",
    description: input.description.trim() || "Mock creature profile.",
    species: input.species,
    color: input.color,
    harness: "fixture-agent",
  };
  update((current) => ({
    ...current,
    profiles: [...current.profiles, profile],
  }));
  return profile;
}

export function sendTankMessage(tankId: string, body: string): void {
  const trimmed = body.trim();
  if (!trimmed || !tankById(tankId)) return;
  update((current) => ({
    ...current,
    messages: [
      ...current.messages,
      {
        source: "mock",
        id: createLocalId("msg-mock"),
        tank_id: tankId,
        author: "human",
        body: trimmed,
        createdAt: Date.now(),
      },
    ],
    tanks: current.tanks.map((tank) =>
      tank.id === tankId ? { ...tank, oceanCollapsed: true } : tank,
    ),
  }));
}

export function selectCreature(instanceId: string | null): void {
  update({ selectedCreatureId: instanceId });
}

export function setOceanCollapsed(tankId: string, collapsed: boolean): void {
  update((current) => ({
    ...current,
    tanks: current.tanks.map((tank) =>
      tank.id === tankId ? { ...tank, oceanCollapsed: collapsed } : tank,
    ),
  }));
}

export function setContextSection(section: ContextSection): void {
  update({ contextSection: section });
}

export function reopenOcean(tankId: string): void {
  setOceanCollapsed(tankId, false);
  update({ selectedCreatureId: null });
}
