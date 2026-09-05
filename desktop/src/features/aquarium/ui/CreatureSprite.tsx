import { Crown } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/shared/lib/cn";

import type {
  CreatureAnimation,
  CreatureApplicationStatus,
  CreatureSpecies,
} from "../types";
import "./creatureSprite.css";

type PixelGrid = readonly string[];

const FISH_A: PixelGrid = [
  "......##......",
  "....######....",
  "...##....##...",
  "..##.##...##..",
  ".##########...",
  "..########.#..",
  "...######.....",
  ".....##.......",
];
const FISH_B: PixelGrid = [
  "......##......",
  "....######....",
  "...##....##...",
  "..##.##...##..",
  ".##########...",
  "#.########....",
  "...######.....",
  ".....##.......",
];
const OCTO_A: PixelGrid = [
  "....####......",
  "...######.....",
  "..##.##.##....",
  "..########....",
  "...######.....",
  "..#.#.#.#.....",
  ".#.#.#.#......",
  "#.#...#.#.....",
];
const OCTO_B: PixelGrid = [
  "....####......",
  "...######.....",
  "..##.##.##....",
  "..########....",
  "...######.....",
  "...#.#.#......",
  "..#.#.#.#.....",
  ".#.#.#.#......",
];
const HORSE_A: PixelGrid = [
  "......##......",
  ".....###......",
  "....##.#......",
  "...#####......",
  ".....###......",
  ".....##.......",
  ".....###......",
  "......##......",
];
const HORSE_B: PixelGrid = [
  "......##......",
  ".....###......",
  "....##.#......",
  "...#####......",
  ".....###......",
  ".....##.#.....",
  ".....##.......",
  "......##......",
];

const FRAMES: Record<CreatureSpecies, { a: PixelGrid; b: PixelGrid }> = {
  fish: { a: FISH_A, b: FISH_B },
  octopus: { a: OCTO_A, b: OCTO_B },
  seahorse: { a: HORSE_A, b: HORSE_B },
};

function PixelFrame({
  className,
  color,
  grid,
}: {
  className?: string;
  color: string;
  grid: PixelGrid;
}) {
  const cells: ReactNode[] = [];
  for (const [y, row] of grid.entries()) {
    for (const [x, cell] of [...row].entries()) {
      if (cell !== "#") continue;
      cells.push(
        <rect
          fill={color}
          height="1"
          key={`cell-${String(x)}-${String(y)}`}
          width="1"
          x={x}
          y={y}
        />,
      );
    }
  }
  return (
    <svg
      aria-hidden="true"
      className={className}
      shapeRendering="crispEdges"
      viewBox="0 0 14 8"
    >
      {cells}
    </svg>
  );
}

export function CreatureSprite({
  animation,
  applicationStatus,
  className,
  color,
  leader,
  name,
  species,
}: {
  animation: CreatureAnimation;
  applicationStatus: CreatureApplicationStatus;
  className?: string;
  color: string;
  leader?: boolean;
  name: string;
  species: CreatureSpecies;
}) {
  const frames = FRAMES[species];
  return (
    <span
      className={cn(
        "relative inline-flex size-8 items-center justify-center",
        className,
      )}
      data-testid={`aquarium-creature-${name}`}
    >
      <span
        className="aquarium-creature-motion relative block size-7"
        data-animation={animation}
        data-status={applicationStatus}
      >
        <PixelFrame
          className="absolute inset-0 size-full"
          color={color}
          grid={frames.a}
        />
        <PixelFrame
          className="aquarium-creature-frame-b absolute inset-0 size-full"
          color={color}
          grid={frames.b}
        />
      </span>
      {leader ? (
        <Crown
          aria-label="Leader"
          className="absolute -top-1 right-0 size-3 text-amber-500"
          data-testid="aquarium-leader-crown"
        />
      ) : null}
    </span>
  );
}
