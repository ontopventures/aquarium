import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const TankWorkspace = React.lazy(async () => {
  const module = await import("@/features/aquarium/ui/TankWorkspace");
  return { default: module.TankWorkspace };
});

export const Route = createFileRoute("/tanks/$tankId")({
  component: TankRouteComponent,
});

function TankRouteComponent() {
  const { tankId } = Route.useParams();
  return (
    <React.Suspense
      fallback={<ViewLoadingFallback includeHeader kind="channel" />}
    >
      <TankWorkspace tankId={tankId} />
    </React.Suspense>
  );
}
