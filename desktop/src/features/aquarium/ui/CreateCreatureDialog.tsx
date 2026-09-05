import * as React from "react";

import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import { Button } from "@/shared/ui/button";
import { Dialog } from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

import { addCreatureFromProfile, createCreatureProfile } from "../store";
import type { CreatureSpecies } from "../types";
import { MockSourceBadge } from "./MockSourceBadge";

const SPECIES: { value: CreatureSpecies; label: string }[] = [
  { value: "fish", label: "Fish" },
  { value: "octopus", label: "Octopus" },
  { value: "seahorse", label: "Seahorse" },
];

export function CreateCreatureDialog({
  onOpenChange,
  open,
  tankId,
}: {
  onOpenChange: (open: boolean) => void;
  open: boolean;
  tankId: string;
}) {
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [species, setSpecies] = React.useState<CreatureSpecies>("fish");
  const [color, setColor] = React.useState("#7dc4e4");

  React.useEffect(() => {
    if (!open) return;
    setName("");
    setDescription("");
    setSpecies("fish");
    setColor("#7dc4e4");
  }, [open]);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <ChooserDialogContent
        data-testid="aquarium-create-creature-dialog"
        footer={
          <div className="flex w-full justify-end">
            <Button
              data-testid="aquarium-create-creature-submit"
              disabled={!name.trim()}
              onClick={() => {
                const profile = createCreatureProfile({
                  color,
                  description,
                  name,
                  species,
                });
                addCreatureFromProfile(tankId, profile.id);
                onOpenChange(false);
              }}
              type="button"
            >
              Create creature
            </Button>
          </div>
        }
        title="Create a creature"
      >
        <div className="space-y-4">
          <MockSourceBadge source="mock" />
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="aquarium-create-creature-name"
            >
              Name
            </label>
            <Input
              data-testid="aquarium-create-creature-name"
              id="aquarium-create-creature-name"
              onChange={(event) => setName(event.target.value)}
              value={name}
            />
          </div>
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="aquarium-create-creature-description"
            >
              Instructions
            </label>
            <Textarea
              data-testid="aquarium-create-creature-description"
              id="aquarium-create-creature-description"
              onChange={(event) => setDescription(event.target.value)}
              rows={3}
              value={description}
            />
          </div>
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="aquarium-create-creature-species"
            >
              Appearance
            </label>
            <select
              className="flex h-10 w-full rounded-xl border border-input bg-background px-3 text-sm"
              data-testid="aquarium-create-creature-species"
              id="aquarium-create-creature-species"
              onChange={(event) =>
                setSpecies(event.target.value as CreatureSpecies)
              }
              value={species}
            >
              {SPECIES.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="aquarium-create-creature-color"
            >
              Color
            </label>
            <Input
              data-testid="aquarium-create-creature-color"
              id="aquarium-create-creature-color"
              onChange={(event) => setColor(event.target.value)}
              type="color"
              value={color}
            />
          </div>
        </div>
      </ChooserDialogContent>
    </Dialog>
  );
}
