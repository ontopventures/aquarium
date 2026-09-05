import * as React from "react";

import { FocusThreadDrawer } from "@/features/channels/ui/FocusThreadDrawer";
import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { ComposerDockBackdrop } from "@/features/messages/ui/ComposerDockBackdrop";
import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageRow } from "@/features/messages/ui/MessageRow";
import { channelChrome } from "@/shared/layout/chromeLayout";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

import { tankMessageToTimeline } from "../lib/timeline";
import {
  creaturesForTank,
  messagesForTank,
  selectCreature,
  sendTankMessage,
  tankById,
  useAquariumStore,
} from "../store";
import { CreatureShelf } from "./CreatureShelf";
import { MockSourceBadge } from "./MockSourceBadge";
import { TankContextPanel } from "./TankContextPanel";
import { TankOcean } from "./TankOcean";

export function TankWorkspace({ tankId }: { tankId: string }) {
  const snapshot = useAquariumStore();
  const tank = tankById(tankId, snapshot);
  const messages = messagesForTank(tankId, snapshot);
  const creatures = creaturesForTank(tankId, snapshot);
  const [contextOpen, setContextOpen] = React.useState(true);
  const selected = creatures.find(
    (creature) => creature.instance_id === snapshot.selectedCreatureId,
  );
  const showOcean = Boolean(tank && !tank.oceanCollapsed);
  const timelineMessages = React.useMemo(
    () => messages.map(tankMessageToTimeline),
    [messages],
  );

  const handleSend = React.useCallback(
    async (content: string) => {
      sendTankMessage(tankId, content);
    },
    [tankId],
  );

  if (!tank) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        Tank not found.
      </div>
    );
  }

  const timeline = (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-y-auto px-5 py-4"
      data-testid="aquarium-tank-timeline"
    >
      {timelineMessages.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No messages yet. A tank can stay creature-less.
        </p>
      ) : (
        timelineMessages.map((message) => (
          <div data-testid={`aquarium-message-${message.id}`} key={message.id}>
            <MessageRow
              channelId={tank.id}
              hideAgentAccessBadge
              message={message}
              showDepthGuides={false}
            />
          </div>
        ))
      )}
    </div>
  );

  return (
    <div
      className="relative flex min-h-0 min-w-0 flex-1 flex-row overflow-hidden"
      data-testid="aquarium-tank-workspace"
    >
      <div
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute inset-x-0 top-0 z-30 bg-background/80 backdrop-blur-md supports-backdrop-filter:bg-background/70 dark:bg-background/70 dark:backdrop-blur-xl dark:supports-backdrop-filter:bg-background/55",
          channelChrome.headerHeight,
        )}
        data-testid="channel-shared-header-backdrop"
      />
      <section
        aria-label="Tank conversation"
        className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
      >
        <ChatHeader
          actions={
            <div className="flex items-center gap-2">
              <MockSourceBadge source={tank.source} />
              <Button
                data-testid="aquarium-open-context"
                onClick={() => setContextOpen((open) => !open)}
                size="sm"
                type="button"
                variant="ghost"
              >
                Context
              </Button>
            </div>
          }
          description={tank.description}
          mode="channel"
          title={tank.title}
          transparentChrome
        />
        {tank.status === "error" && tank.errorMessage ? (
          <p
            className="border-b border-border px-5 py-2 text-sm text-destructive"
            data-testid="aquarium-tank-error"
          >
            {tank.errorMessage}
          </p>
        ) : null}
        {showOcean ? <TankOcean tankId={tank.id} /> : timeline}
        <div
          className="pointer-events-none relative z-40 isolate before:absolute before:inset-x-0 before:bottom-0 before:-z-10 before:h-24 before:bg-gradient-to-b before:from-transparent before:to-background before:content-[''] after:absolute after:inset-x-0 after:bottom-0 after:-z-10 after:h-12 after:bg-background after:content-['']"
          data-testid="aquarium-tank-composer"
        >
          <div className="pointer-events-auto">
            <CreatureShelf tankId={tank.id} />
          </div>
          <div className="composer-dock composer-overlay-corner-masks relative pointer-events-auto">
            <ComposerDockBackdrop gutterClassName="inset-x-5" />
            <MessageComposer
              channelId={tank.id}
              channelName={tank.title}
              channelType="stream"
              containerClassName="px-5 pb-0"
              disabled={tank.status === "error"}
              draftKey={`aquarium:${tank.id}`}
              layoutMode="dock"
              onSend={handleSend}
              placeholder={`Message ${tank.title}`}
              showBackgroundUploadProgress={false}
              showTopBorder={false}
            />
          </div>
        </div>
        {selected ? (
          <FocusThreadDrawer
            channelName={tank.title}
            label={`Direct work with ${selected.name}`}
            onClose={() => selectCreature(null)}
          >
            <div
              className="flex h-full min-h-0 flex-col bg-background"
              data-testid="aquarium-selected-creature"
            >
              <p className="px-5 py-3 text-sm text-muted-foreground">
                Direct work with {selected.name}. Close returns to the tank.
                Mock data — not a live harness session.
              </p>
              {timeline}
            </div>
          </FocusThreadDrawer>
        ) : null}
      </section>
      {contextOpen ? (
        <TankContextPanel
          onClose={() => setContextOpen(false)}
          tankId={tank.id}
        />
      ) : null}
    </div>
  );
}
