import { Button } from "@/shared/ui/button";

import {
  creaturesForTank,
  reopenOcean,
  selectCreature,
  useAquariumStore,
} from "../store";
import { CreatureSprite } from "./CreatureSprite";

function statusLabel(status: string, leader: boolean): string {
  const pieces = [status];
  if (leader) pieces.push("leader");
  return pieces.join(", ");
}

export function CreatureShelf({ tankId }: { tankId: string }) {
  const snapshot = useAquariumStore();
  const creatures = creaturesForTank(tankId, snapshot);
  if (creatures.length === 0) {
    return (
      <div
        className="flex items-center justify-between gap-2 px-5 py-2"
        data-testid="aquarium-creature-shelf"
      >
        <p className="text-xs text-muted-foreground">
          No creatures in this tank
        </p>
        <Button
          data-testid="aquarium-reopen-ocean"
          onClick={() => reopenOcean(tankId)}
          size="sm"
          type="button"
          variant="ghost"
        >
          Ocean
        </Button>
      </div>
    );
  }

  return (
    <div
      className="flex items-center gap-2 overflow-x-auto px-5 py-2"
      data-testid="aquarium-creature-shelf"
    >
      {creatures.map((creature) => {
        const selected = snapshot.selectedCreatureId === creature.instance_id;
        return (
          <button
            aria-label={`${creature.name}, ${statusLabel(creature.applicationStatus, creature.leader)}`}
            className="flex shrink-0 items-center gap-2 rounded-lg px-2 py-1 text-left hover:bg-muted/60 data-[selected=true]:bg-muted"
            data-selected={selected ? "true" : "false"}
            data-testid={`aquarium-shelf-${creature.instance_id}`}
            key={creature.instance_id}
            onClick={() =>
              selectCreature(selected ? null : creature.instance_id)
            }
            type="button"
          >
            <CreatureSprite
              animation={creature.animation}
              applicationStatus={creature.applicationStatus}
              color={creature.color}
              leader={creature.leader}
              name={creature.name}
              species={creature.species}
            />
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-xs font-medium">
                {creature.name}
              </span>
              <span className="text-2xs capitalize text-muted-foreground">
                {statusLabel(creature.applicationStatus, creature.leader)}
              </span>
            </span>
          </button>
        );
      })}
      <Button
        className="ml-auto shrink-0"
        data-testid="aquarium-reopen-ocean"
        onClick={() => reopenOcean(tankId)}
        size="sm"
        type="button"
        variant="ghost"
      >
        Ocean
      </Button>
    </div>
  );
}
