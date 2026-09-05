import { CircleDot } from "lucide-react";
import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { ProjectSectionHeader } from "@/features/projects/ui/ProjectSectionHeader";
import {
  PROJECT_LIST_CONTAINER_CLASS,
  PROJECT_LIST_ROW_CLASS,
  PROJECT_LIST_ROW_CONTENT_CLASS,
  PROJECT_LIST_ROW_META_CLASS,
  PROJECT_LIST_ROW_TITLE_CLASS,
  PROJECT_LIST_ROW_TRAILING_CLASS,
} from "@/features/projects/ui/projectListRowStyles";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import { useAquariumStore } from "../store";
import { LinearConnectDialog } from "./LinearConnectDialog";
import { MockSourceBadge } from "./MockSourceBadge";

export function AquariumProjectsDashboard() {
  const snapshot = useAquariumStore();
  const { goTank } = useAppNavigation();
  const [linearOpen, setLinearOpen] = React.useState(false);

  return (
    <div
      className="flex min-h-0 shrink-0 flex-col border-b border-border"
      data-testid="aquarium-projects-dashboard"
    >
      <ProjectSectionHeader
        icon={CircleDot}
        testId="aquarium-projects-section-header"
        title="Issues and tanks"
        trailing={
          <div className="flex items-center gap-2">
            <MockSourceBadge source={snapshot.source} />
            <Button
              data-testid="aquarium-open-linear"
              onClick={() => setLinearOpen(true)}
              size="sm"
              type="button"
              variant="outline"
            >
              {snapshot.linear.connected ? "Linear (mock)" : "Connect Linear"}
            </Button>
          </div>
        }
      />
      <div className={cn("mx-4 mb-4", PROJECT_LIST_CONTAINER_CLASS)}>
        {snapshot.issues.map((issue) => {
          const tank = snapshot.tanks.find((item) => item.id === issue.tank_id);
          const device = snapshot.devices.find(
            (item) => item.device_id === tank?.device_id,
          );
          return (
            <div
              className={PROJECT_LIST_ROW_CLASS}
              data-testid={`aquarium-dashboard-issue-${issue.identifier}`}
              key={issue.id}
            >
              <div className={PROJECT_LIST_ROW_CONTENT_CLASS}>
                <div className="min-w-0 flex-1">
                  <p className={PROJECT_LIST_ROW_TITLE_CLASS}>
                    {issue.identifier} {issue.title}
                  </p>
                  <p className={PROJECT_LIST_ROW_META_CLASS}>
                    <span>{issue.status}</span>
                    <span>{device?.displayName ?? "No device"}</span>
                    <span>PR none (mock)</span>
                  </p>
                </div>
                <div className={PROJECT_LIST_ROW_TRAILING_CLASS}>
                  {tank ? (
                    <Button
                      data-testid={`aquarium-dashboard-open-${tank.id}`}
                      onClick={() => void goTank(tank.id)}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      {tank.title}
                    </Button>
                  ) : (
                    <span className="text-xs text-muted-foreground">
                      No tank yet
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <LinearConnectDialog onOpenChange={setLinearOpen} open={linearOpen} />
    </div>
  );
}
