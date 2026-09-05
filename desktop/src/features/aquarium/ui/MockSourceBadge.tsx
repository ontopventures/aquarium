import { cn } from "@/shared/lib/cn";

import type { AdapterSource } from "../types";

export function MockSourceBadge({
  className,
  source,
}: {
  className?: string;
  source: AdapterSource;
}) {
  if (source !== "mock") return null;
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md bg-muted px-1.5 py-0.5 text-2xs font-medium uppercase tracking-wide text-muted-foreground",
        className,
      )}
      data-aquarium-source="mock"
      data-testid="aquarium-mock-badge"
    >
      Mock data
    </span>
  );
}
