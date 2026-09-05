import { RightAuxiliaryPane } from "@/features/channels/ui/RightAuxiliaryPane";
import {
  AuxiliaryPanel,
  AuxiliaryPanelBody,
  AuxiliaryPanelHeader,
  AuxiliaryPanelHeaderActions,
  AuxiliaryPanelHeaderGroup,
  AuxiliaryPanelTitle,
} from "@/shared/layout/AuxiliaryPanel";
import { useThreadPanelWidth } from "@/shared/hooks/useThreadPanelWidth";
import { SegmentedControl } from "@/shared/ui/segmented-control";

import {
  creaturesForTank,
  issueForTank,
  setContextSection,
  tankById,
  useAquariumStore,
} from "../store";
import type { ContextSection } from "../types";
import { CreatureSprite } from "./CreatureSprite";
import { MockSourceBadge } from "./MockSourceBadge";

const SECTIONS: { value: ContextSection; label: string }[] = [
  { value: "creatures", label: "Creatures" },
  { value: "environment", label: "Env" },
  { value: "changes", label: "Changes" },
  { value: "device", label: "Device" },
];

export function TankContextPanel({
  onClose,
  tankId,
}: {
  onClose: () => void;
  tankId: string;
}) {
  const snapshot = useAquariumStore();
  const tank = tankById(tankId, snapshot);
  const issue = tank ? issueForTank(tank, snapshot) : undefined;
  const creatures = creaturesForTank(tankId, snapshot);
  const device = snapshot.devices.find(
    (item) => item.device_id === tank?.device_id,
  );
  const repository = snapshot.repositories.find(
    (item) => item.id === tank?.repository_id,
  );
  const { canReset, onResetWidth, onResizeStart, widthPx } =
    useThreadPanelWidth(undefined, {
      sessionKey: "buzz.desktop.aquarium-tank-context-width",
    });
  if (!tank) return null;

  return (
    <RightAuxiliaryPane
      canResetWidth={canReset}
      onResetWidth={onResetWidth}
      onResizeStart={onResizeStart}
      testId="aquarium-context-panel"
      widthPx={widthPx}
    >
      <AuxiliaryPanel
        isSinglePanelView={false}
        layout="split"
        onClose={onClose}
        testId="aquarium-context-panel-inner"
        widthPx={widthPx}
        header={
          <AuxiliaryPanelHeader transparent>
            <AuxiliaryPanelHeaderGroup>
              <AuxiliaryPanelTitle>Tank context</AuxiliaryPanelTitle>
            </AuxiliaryPanelHeaderGroup>
            <AuxiliaryPanelHeaderActions includeCloseAction>
              <MockSourceBadge source={tank.source} />
            </AuxiliaryPanelHeaderActions>
          </AuxiliaryPanelHeader>
        }
      >
        <AuxiliaryPanelBody className="overflow-y-auto overflow-x-hidden overscroll-contain px-4 pb-8">
          <div className="space-y-4">
            <SegmentedControl
              legend="Tank context section"
              onValueChange={setContextSection}
              optionTestIdPrefix="aquarium-context"
              options={SECTIONS}
              size="wide"
              testId="aquarium-context-sections"
              value={snapshot.contextSection}
            />
            {snapshot.contextSection === "creatures" ? (
              <ul
                className="space-y-3"
                data-testid="aquarium-context-creatures"
              >
                {creatures.length === 0 ? (
                  <li className="text-sm text-muted-foreground">
                    No creatures yet. Use Ocean to add one.
                  </li>
                ) : (
                  creatures.map((creature) => (
                    <li
                      className="flex items-start gap-2"
                      key={creature.instance_id}
                    >
                      <CreatureSprite
                        animation={creature.animation}
                        applicationStatus={creature.applicationStatus}
                        color={creature.color}
                        leader={creature.leader}
                        name={creature.name}
                        species={creature.species}
                      />
                      <div className="min-w-0">
                        <p className="text-sm font-medium">{creature.name}</p>
                        <p className="text-xs capitalize text-muted-foreground">
                          {creature.applicationStatus}
                          {creature.leader ? " · leader" : ""}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Fresh instance of {creature.profile_id}
                        </p>
                      </div>
                    </li>
                  ))
                )}
              </ul>
            ) : null}
            {snapshot.contextSection === "environment" ? (
              <div
                className="space-y-2 text-sm"
                data-testid="aquarium-context-environment"
              >
                <p>Repository: {repository?.name ?? tank.repository_id}</p>
                <p>Branch: {tank.branch ?? "not provisioned"}</p>
                <p>
                  Worktree: {tank.worktree_path ?? "mock — no live checkout"}
                </p>
              </div>
            ) : null}
            {snapshot.contextSection === "changes" ? (
              <p
                className="text-sm text-muted-foreground"
                data-testid="aquarium-context-changes"
              >
                Mock tanks do not show a live git diff. Changes appear here when
                a device adapter is bound.
              </p>
            ) : null}
            {snapshot.contextSection === "device" ? (
              <div
                className="space-y-2 text-sm"
                data-testid="aquarium-context-device"
              >
                <p>{device?.displayName ?? tank.device_id}</p>
                <p>
                  {device?.online ? "Online" : "Offline"} · readiness{" "}
                  {device?.setup_readiness ?? "unknown"}
                </p>
                <p>Harnesses: {device?.harnesses.join(", ") || "none"}</p>
                {issue ? (
                  <p>
                    Issue {issue.identifier}: {issue.title}
                  </p>
                ) : (
                  <p>No linked issue</p>
                )}
              </div>
            ) : null}
          </div>
        </AuxiliaryPanelBody>
      </AuxiliaryPanel>
    </RightAuxiliaryPane>
  );
}
