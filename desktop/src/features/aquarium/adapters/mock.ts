import { createMockState } from "../mock/fixtures";
import type {
  DeviceCapabilities,
  DeviceOpResult,
  LinearConnection,
  LinearIssue,
} from "../types";
import type { DeviceAdapter, LinearAdapter } from "./contract";

function mockFailure(message: string): DeviceOpResult {
  return { source: "mock", status: "failed", message };
}

export function createMockDeviceAdapter(
  getDevices: () => DeviceCapabilities[],
): DeviceAdapter {
  const journal = new Map<string, DeviceOpResult>();
  return {
    async inspectCapabilities(deviceId) {
      const device = getDevices().find((item) => item.device_id === deviceId);
      if (!device) {
        return {
          source: "mock",
          device_id: deviceId,
          device_pubkey: "",
          displayName: "Unknown mock device",
          online: false,
          protocol_version: "1",
          harnesses: [],
          setup_readiness: "unknown",
          grant_generation: 0,
        };
      }
      return device;
    },
    async createCheckout(input) {
      const device = getDevices().find(
        (item) => item.device_id === input.device_id,
      );
      if (!device?.online || device.setup_readiness !== "ready") {
        return mockFailure(
          "Execution device is offline or not ready. Create does not fall back to this machine. (mock)",
        );
      }
      const result: DeviceOpResult = {
        source: "mock",
        status: "succeeded",
        request_id: input.request_id,
        worktree_path: `/mock/tanks/${input.tank_id}`,
        branch: input.branch,
        head: "mock-head",
        message: "Mock checkout recorded. Not a live git worktree.",
      };
      journal.set(input.request_id, result);
      return result;
    },
    async inspectRequest(request_id) {
      return (
        journal.get(request_id) ?? {
          source: "mock",
          status: "uncertain",
          request_id,
          message: "Mock request inspect. Not a live journal row.",
        }
      );
    },
    async startSession(input) {
      const result: DeviceOpResult = {
        source: "mock",
        status: "succeeded",
        request_id: input.request_id,
        session_id: `session-mock-${input.instance_id}`,
        message: "Mock session id. Not a live harness process.",
      };
      journal.set(input.request_id, result);
      return result;
    },
    async cancelSession(input) {
      const result: DeviceOpResult = {
        source: "mock",
        status: "succeeded",
        request_id: input.request_id,
        session_id: input.session_id,
        message: "Mock cancel. No process was signalled.",
      };
      journal.set(input.request_id, result);
      return result;
    },
  };
}

export function createMockLinearAdapter(
  getIssues: () => LinearIssue[],
  getConnection: () => LinearConnection,
  setConnection: (next: LinearConnection) => void,
): LinearAdapter {
  return {
    connection: getConnection,
    async connectApiKey(apiKey) {
      const trimmed = apiKey.trim();
      if (trimmed.length < 8) {
        const next: LinearConnection = {
          source: "mock",
          connected: false,
          message: "Mock Linear key must be at least 8 characters. Not sent.",
        };
        setConnection(next);
        return next;
      }
      const next: LinearConnection = {
        source: "mock",
        connected: true,
        workspaceName: "Mock workspace",
        message: "Mock Linear connection — not a live workspace.",
      };
      setConnection(next);
      return next;
    },
    async disconnect() {
      const next: LinearConnection = {
        source: "mock",
        connected: false,
        message: "Mock Linear disconnected.",
      };
      setConnection(next);
      return next;
    },
    async searchIssues(query) {
      if (!getConnection().connected) return [];
      const needle = query.trim().toLowerCase();
      const issues = getIssues();
      if (!needle) return issues;
      return issues.filter(
        (issue) =>
          issue.identifier.toLowerCase().includes(needle) ||
          issue.title.toLowerCase().includes(needle),
      );
    },
    async getIssue(id) {
      return getIssues().find((issue) => issue.id === id) ?? null;
    },
  };
}

export function unusedFixtureSeed(): number {
  return createMockState().tanks.length;
}
