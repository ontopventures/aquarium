import { ChevronDown } from "lucide-react";
import * as React from "react";

import {
  CHANNEL_FORM_FIELD_CONTROL_CLASS,
  CHANNEL_FORM_FIELD_SHELL_CLASS,
} from "@/features/channels/ui/channelFormStyles";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Input } from "@/shared/ui/input";

import { searchMockIssues, useAquariumStore } from "../store";
import type { LinearIssue } from "../types";
import { MockSourceBadge } from "./MockSourceBadge";

export type TankCreateFieldState = {
  issueId: string | null;
  issueQuery: string;
  repositoryId: string;
  deviceId: string;
  setIssueId: (id: string | null) => void;
  setIssueQuery: (value: string) => void;
  setRepositoryId: (id: string) => void;
  setDeviceId: (id: string) => void;
  reset: () => void;
};

export function useTankCreateFields(): TankCreateFieldState {
  const snapshot = useAquariumStore();
  const defaultRepo = snapshot.repositories[0]?.id ?? "";
  const defaultDevice =
    snapshot.devices.find((device) => device.online)?.device_id ??
    snapshot.devices[0]?.device_id ??
    "";
  const [issueId, setIssueId] = React.useState<string | null>(null);
  const [issueQuery, setIssueQuery] = React.useState("");
  const [repositoryId, setRepositoryId] = React.useState(defaultRepo);
  const [deviceId, setDeviceId] = React.useState(defaultDevice);
  const reset = React.useCallback(() => {
    setIssueId(null);
    setIssueQuery("");
    setRepositoryId(defaultRepo);
    setDeviceId(defaultDevice);
  }, [defaultDevice, defaultRepo]);
  return {
    issueId,
    issueQuery,
    repositoryId,
    deviceId,
    setIssueId,
    setIssueQuery,
    setRepositoryId,
    setDeviceId,
    reset,
  };
}

function FormDropdown({
  disabled,
  id,
  label,
  onValueChange,
  options,
  testId,
  value,
}: {
  disabled?: boolean;
  id: string;
  label: string;
  onValueChange: (value: string) => void;
  options: { label: string; value: string }[];
  testId: string;
  value: string;
}) {
  const selected = options.find((option) => option.value === value);
  return (
    <div
      className="flex min-h-12 items-center justify-between gap-4 rounded-xl border border-input bg-background px-3 py-3"
      data-testid={`${testId}-container`}
    >
      <label className="text-sm font-medium text-foreground" htmlFor={id}>
        {label}
      </label>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            className="-mr-2.5 ml-auto h-9 min-w-0 max-w-[60%] justify-end px-2.5 text-right text-sm font-medium text-foreground hover:bg-muted/50"
            data-testid={testId}
            disabled={disabled}
            id={id}
            type="button"
            variant="ghost"
          >
            <span className="truncate text-right">
              {selected?.label ?? "Select"}
            </span>
            <ChevronDown className="size-4 shrink-0 text-muted-foreground/70" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="end"
          onCloseAutoFocus={(event) => event.preventDefault()}
          style={{ minWidth: "var(--radix-dropdown-menu-trigger-width)" }}
        >
          <DropdownMenuRadioGroup onValueChange={onValueChange} value={value}>
            {options.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value}>
                {option.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

export function TankCreateFields({
  disabled,
  fields,
}: {
  disabled?: boolean;
  fields: TankCreateFieldState;
}) {
  const snapshot = useAquariumStore();
  const [matches, setMatches] = React.useState<LinearIssue[]>([]);

  React.useEffect(() => {
    let cancelled = false;
    void searchMockIssues(fields.issueQuery).then((issues) => {
      if (!cancelled) setMatches(issues);
    });
    return () => {
      cancelled = true;
    };
  }, [fields.issueQuery]);

  const selectedIssue =
    snapshot.issues.find((issue) => issue.id === fields.issueId) ?? null;
  const selectedDevice = snapshot.devices.find(
    (device) => device.device_id === fields.deviceId,
  );

  return (
    <div className="space-y-5" data-testid="aquarium-tank-fields">
      <MockSourceBadge source="mock" />
      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="create-tank-issue"
        >
          Linear issue
          <span className="ml-1 text-xs font-normal text-muted-foreground/50">
            Optional
          </span>
        </label>
        <div className={CHANNEL_FORM_FIELD_SHELL_CLASS}>
          <Input
            className={cn(
              "h-8 px-3 py-0 leading-6",
              CHANNEL_FORM_FIELD_CONTROL_CLASS,
            )}
            data-testid="create-tank-issue"
            disabled={disabled}
            id="create-tank-issue"
            onChange={(event) => {
              fields.setIssueQuery(event.target.value);
              fields.setIssueId(null);
            }}
            placeholder="Search mock issues"
            value={selectedIssue ? selectedIssue.identifier : fields.issueQuery}
          />
        </div>
        {selectedIssue?.tank_id ? (
          <p
            className="text-xs text-muted-foreground"
            data-testid="create-tank-existing"
          >
            This mock issue already has tank {selectedIssue.tank_id}. Create
            opens it instead of provisioning again.
          </p>
        ) : null}
        {!selectedIssue && matches.length > 0 ? (
          <ul className="space-y-1">
            {matches.map((issue) => (
              <li key={issue.id}>
                <Button
                  className="h-auto w-full justify-start px-2 py-1.5 text-left text-sm"
                  data-testid={`create-tank-issue-${issue.identifier}`}
                  onClick={() => {
                    fields.setIssueId(issue.id);
                    fields.setIssueQuery(issue.identifier);
                  }}
                  type="button"
                  variant="ghost"
                >
                  <span className="font-medium">{issue.identifier}</span>
                  <span className="ml-2 text-muted-foreground">
                    {issue.title}
                  </span>
                </Button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>

      <FormDropdown
        disabled={disabled}
        id="create-tank-repository"
        label="Repository"
        onValueChange={fields.setRepositoryId}
        options={snapshot.repositories.map((repository) => ({
          label: repository.name,
          value: repository.id,
        }))}
        testId="create-tank-repository"
        value={fields.repositoryId}
      />

      <FormDropdown
        disabled={disabled}
        id="create-tank-device"
        label="Execution device"
        onValueChange={fields.setDeviceId}
        options={snapshot.devices.map((device) => ({
          label: `${device.displayName}${device.online ? "" : " — offline"}`,
          value: device.device_id,
        }))}
        testId="create-tank-device"
        value={fields.deviceId}
      />
      {selectedDevice && !selectedDevice.online ? (
        <p
          className="text-sm text-destructive"
          data-testid="create-tank-device-offline"
        >
          This mock device is offline. Create will not fall back to this
          machine.
        </p>
      ) : null}
    </div>
  );
}
