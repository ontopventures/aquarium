import type { AquariumState } from "../types";

/** All fixture ids are mock. Do not treat them as live Linear/device records. */
export const MOCK_TANK_AUTH_ID = "tank-mock-auth-login";
export const MOCK_ISSUE_AUTH_ID = "issue-mock-12";
export const MOCK_ISSUE_DASHBOARD_ID = "issue-mock-18";
export const MOCK_DEVICE_READY_ID = "device-mock-ready";
export const MOCK_DEVICE_OFFLINE_ID = "device-mock-offline";
export const MOCK_REPO_ID = "repo-mock-aquarium";
export const MOCK_PROFILE_CORAL_ID = "profile-mock-coral";
export const MOCK_PROFILE_INK_ID = "profile-mock-ink";
export const MOCK_PROFILE_KELP_ID = "profile-mock-kelp";
export const MOCK_INSTANCE_CORAL_ID = "instance-mock-coral-1";

export function createMockState(): AquariumState {
  return {
    source: "mock",
    provisioning: false,
    selectedCreatureId: null,
    contextSection: "creatures",
    linear: {
      source: "mock",
      connected: true,
      workspaceName: "Mock workspace",
      message: "Mock Linear connection — not a live workspace.",
    },
    repositories: [
      {
        source: "mock",
        id: MOCK_REPO_ID,
        name: "aquarium (mock)",
        defaultBranch: "main",
      },
    ],
    devices: [
      {
        source: "mock",
        device_id: MOCK_DEVICE_READY_ID,
        device_pubkey: "mock-device-pubkey-ready",
        displayName: "Taylor Mac mini (mock)",
        online: true,
        protocol_version: "1",
        harnesses: ["fixture-agent"],
        setup_readiness: "ready",
        grant_generation: 1,
      },
      {
        source: "mock",
        device_id: MOCK_DEVICE_OFFLINE_ID,
        device_pubkey: "mock-device-pubkey-offline",
        displayName: "Offline laptop (mock)",
        online: false,
        protocol_version: "1",
        harnesses: [],
        setup_readiness: "offline",
        grant_generation: 1,
      },
    ],
    issues: [
      {
        source: "mock",
        id: MOCK_ISSUE_AUTH_ID,
        identifier: "MOCK-12",
        title: "Auth login polish",
        status: "In Progress",
        projectName: "Aquarium (mock)",
        tank_id: MOCK_TANK_AUTH_ID,
      },
      {
        source: "mock",
        id: MOCK_ISSUE_DASHBOARD_ID,
        identifier: "MOCK-18",
        title: "Dashboard empty state",
        status: "Todo",
        projectName: "Aquarium (mock)",
        tank_id: null,
      },
    ],
    profiles: [
      {
        source: "mock",
        id: MOCK_PROFILE_CORAL_ID,
        name: "Coral",
        description: "Careful reviewer. Mock profile, not a live harness.",
        species: "fish",
        color: "#e78a4e",
        harness: "fixture-agent",
      },
      {
        source: "mock",
        id: MOCK_PROFILE_INK_ID,
        name: "Ink",
        description: "Explores edge cases. Mock profile, not a live harness.",
        species: "octopus",
        color: "#8aadf4",
        harness: "fixture-agent",
      },
      {
        source: "mock",
        id: MOCK_PROFILE_KELP_ID,
        name: "Kelp",
        description: "Keeps the branch tidy. Mock profile, not a live harness.",
        species: "seahorse",
        color: "#a6da95",
        harness: "fixture-agent",
      },
    ],
    tanks: [
      {
        source: "mock",
        id: MOCK_TANK_AUTH_ID,
        title: "Auth login polish",
        description: "Mock tank bound to MOCK-12. Not a live worktree.",
        status: "active",
        issue_id: MOCK_ISSUE_AUTH_ID,
        repository_id: MOCK_REPO_ID,
        device_id: MOCK_DEVICE_READY_ID,
        conversation_id: null,
        branch: "aquarium/mock-auth-login",
        oceanCollapsed: true,
        createdAt: 1,
      },
    ],
    creatures: [
      {
        source: "mock",
        instance_id: MOCK_INSTANCE_CORAL_ID,
        profile_id: MOCK_PROFILE_CORAL_ID,
        tank_id: MOCK_TANK_AUTH_ID,
        name: "Coral",
        description: "Careful reviewer. Mock profile, not a live harness.",
        species: "fish",
        color: "#e78a4e",
        harness: "fixture-agent",
        animation: "swim",
        applicationStatus: "working",
        leader: true,
      },
    ],
    messages: [
      {
        source: "mock",
        id: "msg-mock-1",
        tank_id: MOCK_TANK_AUTH_ID,
        author: "human",
        body: "Please tighten the login empty state. This is mock conversation data.",
        createdAt: 1,
      },
    ],
  };
}
