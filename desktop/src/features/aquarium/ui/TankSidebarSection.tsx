import { Fish } from "lucide-react";
import { useLocation } from "@tanstack/react-router";
import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import {
  ChannelSectionHeader,
  SectionQuickAction,
} from "@/features/sidebar/ui/CustomChannelSection";
import { cn } from "@/shared/lib/cn";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/shared/ui/sidebar";
import { SidebarMenuLabel } from "@/shared/ui/sidebar-menu-label";

import { activeTanks, useAquariumStore } from "../store";

const TANKS_CONTENT_ID = "sidebar-aquarium-tank-list";

export function TankSidebarSection({
  onCreateTank,
}: {
  onCreateTank: () => void;
}) {
  const snapshot = useAquariumStore();
  const tanks = activeTanks(snapshot);
  const { goTank } = useAppNavigation();
  const pathname = useLocation({ select: (location) => location.pathname });
  const [collapsed, setCollapsed] = React.useState(false);

  return (
    <SidebarGroup
      className="group/sidebar-section select-none"
      data-testid="aquarium-tanks-section"
    >
      <ChannelSectionHeader
        actions={
          <SectionQuickAction
            label="New tank"
            onClick={onCreateTank}
            testId="aquarium-tanks-create"
          />
        }
        contentId={TANKS_CONTENT_ID}
        isCollapsed={collapsed}
        onToggleCollapsed={() => setCollapsed((value) => !value)}
        testId="aquarium-tank-list"
        title="Tanks"
      />
      {collapsed ? null : (
        <SidebarGroupContent id={TANKS_CONTENT_ID}>
          <SidebarMenu data-testid="aquarium-tank-list">
            {tanks.length === 0 ? (
              <p className="px-3 py-1 text-xs text-muted-foreground">
                No active tanks
              </p>
            ) : (
              tanks.map((tank) => {
                const isActive = pathname === `/tanks/${tank.id}`;
                return (
                  <SidebarMenuItem key={tank.id}>
                    <SidebarMenuButton
                      className={cn(
                        "data-[active=true]:font-normal",
                        isActive
                          ? "group-hover/menu-item:bg-sidebar-active group-hover/menu-item:text-sidebar-active-foreground"
                          : "group-hover/menu-item:bg-sidebar-accent group-hover/menu-item:text-sidebar-foreground",
                      )}
                      data-testid={`aquarium-tank-${tank.id}`}
                      isActive={isActive}
                      onClick={() => void goTank(tank.id)}
                      tooltip={tank.title}
                      type="button"
                    >
                      <Fish className="size-4 shrink-0" />
                      <SidebarMenuLabel>{tank.title}</SidebarMenuLabel>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })
            )}
          </SidebarMenu>
        </SidebarGroupContent>
      )}
    </SidebarGroup>
  );
}
