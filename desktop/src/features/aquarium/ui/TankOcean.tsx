import * as React from "react";

import { Button } from "@/shared/ui/button";

import { addCreatureFromProfile, useAquariumStore } from "../store";
import type { CreatureProfile } from "../types";
import { CreateCreatureDialog } from "./CreateCreatureDialog";
import { CreatureSprite } from "./CreatureSprite";
import { MockSourceBadge } from "./MockSourceBadge";

const PROFILE_DRAG_TYPE = "application/x-aquarium-profile";

function OceanCard({
  onAdd,
  profile,
}: {
  onAdd: (profileId: string) => void;
  profile: CreatureProfile;
}) {
  return (
    <fieldset
      className="flex flex-col items-center gap-2 rounded-xl border border-border bg-background px-3 py-4"
      data-testid={`aquarium-ocean-card-${profile.id}`}
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(PROFILE_DRAG_TYPE, profile.id);
        event.dataTransfer.effectAllowed = "copy";
      }}
    >
      <legend className="sr-only">{profile.name}, saved mock creature</legend>
      <CreatureSprite
        animation="idle"
        applicationStatus="resting"
        color={profile.color}
        name={profile.name}
        species={profile.species}
      />
      <p className="text-sm font-medium">{profile.name}</p>
      <p className="text-center text-xs text-muted-foreground">
        {profile.description}
      </p>
      <Button
        data-testid={`aquarium-ocean-add-${profile.id}`}
        onClick={() => onAdd(profile.id)}
        size="sm"
        type="button"
        variant="secondary"
      >
        Add
      </Button>
    </fieldset>
  );
}

export function TankOcean({ tankId }: { tankId: string }) {
  const snapshot = useAquariumStore();
  const [createOpen, setCreateOpen] = React.useState(false);
  const [dropActive, setDropActive] = React.useState(false);

  const addProfile = React.useCallback(
    (profileId: string) => {
      addCreatureFromProfile(tankId, profileId);
    },
    [tankId],
  );

  return (
    <section
      aria-label="Ocean. Drop a saved creature here or use Add."
      className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-6 py-8"
      data-drop-active={dropActive ? "true" : "false"}
      data-testid="aquarium-ocean"
      onDragEnter={(event) => {
        event.preventDefault();
        setDropActive(true);
      }}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node)) return;
        setDropActive(false);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDrop={(event) => {
        event.preventDefault();
        setDropActive(false);
        const profileId = event.dataTransfer.getData(PROFILE_DRAG_TYPE);
        if (profileId) addProfile(profileId);
      }}
    >
      <div className="mb-4 flex items-center gap-2">
        <h2 className="text-base font-semibold">Ocean</h2>
        <MockSourceBadge source={snapshot.source} />
      </div>
      <p className="mb-6 max-w-md text-center text-sm text-muted-foreground">
        Drag or select a saved creature to add a fresh instance to this tank.
        The Ocean template stays put. Keyboard Add is equivalent to dragging.
      </p>
      <div className="grid w-full max-w-2xl grid-cols-1 gap-3 sm:grid-cols-3">
        {snapshot.profiles.map((profile) => (
          <OceanCard key={profile.id} onAdd={addProfile} profile={profile} />
        ))}
      </div>
      <Button
        className="mt-6"
        data-testid="aquarium-ocean-create"
        onClick={() => setCreateOpen(true)}
        type="button"
        variant="outline"
      >
        Create creature
      </Button>
      <CreateCreatureDialog
        onOpenChange={setCreateOpen}
        open={createOpen}
        tankId={tankId}
      />
    </section>
  );
}
