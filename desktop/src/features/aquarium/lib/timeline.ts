import type { TimelineMessage } from "@/features/messages/types";

import type { TankMessage } from "../types";

/** Map mock tank messages onto the Buzz timeline row contract. */
export function tankMessageToTimeline(message: TankMessage): TimelineMessage {
  const isCreature = message.author === "creature";
  return {
    id: message.id,
    createdAt: message.createdAt,
    author: isCreature ? "Creature" : "You",
    isAgent: isCreature,
    time: "",
    body: message.body,
    depth: 0,
  };
}
