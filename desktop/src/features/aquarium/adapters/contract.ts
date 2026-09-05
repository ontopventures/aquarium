import type {
  AdapterSource,
  DeviceCapabilities,
  DeviceOpResult,
  LinearConnection,
  LinearIssue,
} from "../types";

export type DeviceAdapter = {
  inspectCapabilities(deviceId: string): Promise<DeviceCapabilities>;
  createCheckout(input: {
    tank_id: string;
    device_id: string;
    repository_id: string;
    branch: string;
    relpath: string;
    request_id: string;
  }): Promise<DeviceOpResult>;
  inspectRequest(request_id: string): Promise<DeviceOpResult>;
  startSession(input: {
    tank_id: string;
    device_id: string;
    checkout_path: string;
    instance_id: string;
    request_id: string;
  }): Promise<DeviceOpResult>;
  cancelSession(input: {
    device_id: string;
    session_id: string;
    request_id: string;
  }): Promise<DeviceOpResult>;
};

export type LinearAdapter = {
  connection(): LinearConnection;
  connectApiKey(apiKey: string): Promise<LinearConnection>;
  disconnect(): Promise<LinearConnection>;
  searchIssues(query: string): Promise<LinearIssue[]>;
  getIssue(id: string): Promise<LinearIssue | null>;
};

export type AquariumAdapters = {
  source: AdapterSource;
  device: DeviceAdapter;
  linear: LinearAdapter;
};
