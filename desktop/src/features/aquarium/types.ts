export type AdapterSource = "mock" | "device" | "linear";

export type DeviceReadiness = "ready" | "needs-setup" | "offline" | "unknown";

export type DeviceOpStatus =
  | "accepted"
  | "executing"
  | "succeeded"
  | "failed"
  | "rejected"
  | "conflict"
  | "uncertain";

export type CreatureAnimation = "idle" | "swim";

export type CreatureApplicationStatus =
  | "working"
  | "paused"
  | "resting"
  | "offline"
  | "attention";

export type CreatureSpecies = "fish" | "octopus" | "seahorse";

export type TankStatus = "active" | "completed" | "error";

export type ContextSection = "environment" | "changes" | "creatures" | "device";

export type DeviceCapabilities = {
  source: AdapterSource;
  device_id: string;
  device_pubkey: string;
  displayName: string;
  online: boolean;
  protocol_version: string;
  harnesses: string[];
  setup_readiness: DeviceReadiness;
  grant_generation: number;
};

export type DeviceOpResult = {
  source: AdapterSource;
  status: DeviceOpStatus;
  request_id?: string;
  session_id?: string;
  worktree_path?: string;
  branch?: string;
  head?: string;
  message: string;
};

export type LinearConnection = {
  source: AdapterSource;
  connected: boolean;
  workspaceName?: string;
  message: string;
};

export type LinearIssue = {
  source: AdapterSource;
  id: string;
  identifier: string;
  title: string;
  status: string;
  projectName?: string;
  url?: string;
  tank_id?: string | null;
};

export type RepositoryOption = {
  source: AdapterSource;
  id: string;
  name: string;
  defaultBranch: string;
};

export type CreatureProfile = {
  source: AdapterSource;
  id: string;
  name: string;
  description: string;
  species: CreatureSpecies;
  color: string;
  harness: string;
};

export type CreatureInstance = {
  source: AdapterSource;
  instance_id: string;
  profile_id: string;
  tank_id: string;
  name: string;
  description: string;
  species: CreatureSpecies;
  color: string;
  harness: string;
  animation: CreatureAnimation;
  applicationStatus: CreatureApplicationStatus;
  leader: boolean;
  session_id?: string;
};

export type TankMessage = {
  source: AdapterSource;
  id: string;
  tank_id: string;
  author: "human" | "creature";
  instance_id?: string;
  body: string;
  createdAt: number;
};

export type Tank = {
  source: AdapterSource;
  id: string;
  title: string;
  description?: string;
  status: TankStatus;
  issue_id?: string | null;
  repository_id: string;
  device_id: string;
  conversation_id?: string | null;
  branch?: string;
  worktree_path?: string;
  request_id?: string;
  errorMessage?: string;
  oceanCollapsed: boolean;
  createdAt: number;
};

export type ProvisionTankInput = {
  title: string;
  description?: string;
  issue_id?: string | null;
  repository_id: string;
  device_id: string;
};

export type ProvisionTankResult =
  | {
      ok: true;
      source: AdapterSource;
      tank: Tank;
      existing: boolean;
    }
  | {
      ok: false;
      source: AdapterSource;
      error: string;
      existingTankId?: string;
    };

export type AquariumState = {
  source: AdapterSource;
  tanks: Tank[];
  profiles: CreatureProfile[];
  creatures: CreatureInstance[];
  messages: TankMessage[];
  issues: LinearIssue[];
  devices: DeviceCapabilities[];
  repositories: RepositoryOption[];
  linear: LinearConnection;
  selectedCreatureId: string | null;
  contextSection: ContextSection;
  provisioning: boolean;
};
