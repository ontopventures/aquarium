import * as React from "react";

import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import { Button } from "@/shared/ui/button";
import { Dialog } from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";

import {
  LINEAR_PERSONAL_KEY_SETTINGS_URL,
  looksLikeLinearPersonalKey,
} from "../adapters/linearSecret";
import {
  connectMockLinear,
  disconnectMockLinear,
  useAquariumStore,
} from "../store";
import { MockSourceBadge } from "./MockSourceBadge";

export function LinearConnectDialog({
  onOpenChange,
  open,
}: {
  onOpenChange: (open: boolean) => void;
  open: boolean;
}) {
  const snapshot = useAquariumStore();
  const [apiKey, setApiKey] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [pending, setPending] = React.useState(false);
  const connected = snapshot.linear.connected;
  const submitLabel = connected ? "Update access" : "Connect";

  React.useEffect(() => {
    if (open) return;
    setApiKey("");
    setError(null);
    setPending(false);
  }, [open]);

  return (
    <Dialog
      onOpenChange={(next) => {
        if (!next && pending) return;
        onOpenChange(next);
      }}
      open={open}
    >
      <ChooserDialogContent
        data-testid="aquarium-linear-connect-dialog"
        headerSubtitle="Paste a Personal API key (Account > Security & Access). Orca-style connect / update-access / re-check. The key stays on this host; it is not sent to Linear until a desktop-safe client is bound."
        footer={
          <div className="flex w-full flex-wrap items-center justify-end gap-2">
            {connected ? (
              <>
                <Button
                  data-testid="aquarium-linear-recheck"
                  disabled={pending}
                  onClick={() => {
                    setPending(true);
                    void connectMockLinear("").then((result) => {
                      setPending(false);
                      setError(result.connected ? null : result.message);
                    });
                  }}
                  type="button"
                  variant="ghost"
                >
                  Re-check connection
                </Button>
                <Button
                  data-testid="aquarium-linear-disconnect"
                  disabled={pending}
                  onClick={() => {
                    setPending(true);
                    void disconnectMockLinear().finally(() =>
                      setPending(false),
                    );
                  }}
                  type="button"
                  variant="outline"
                >
                  Disconnect
                </Button>
              </>
            ) : null}
            <Button
              data-testid="aquarium-linear-connect"
              disabled={pending || (!connected && apiKey.trim().length < 8)}
              onClick={() => {
                setPending(true);
                setError(null);
                void connectMockLinear(apiKey).then((result) => {
                  setPending(false);
                  if (!result.connected) setError(result.message);
                  else onOpenChange(false);
                });
              }}
              type="button"
            >
              {pending ? "Connecting…" : submitLabel}
            </Button>
          </div>
        }
        title={connected ? "Update Linear access" : "Add Linear access"}
      >
        <div className="space-y-4">
          <MockSourceBadge source={snapshot.linear.source} />
          <p className="text-sm text-muted-foreground">
            {snapshot.linear.message}
          </p>
          <p className="text-xs text-muted-foreground">
            Create a Personal API key from{" "}
            <a
              className="underline"
              href={LINEAR_PERSONAL_KEY_SETTINGS_URL}
              rel="noreferrer"
              target="_blank"
            >
              Linear account security
            </a>
            . Prefer full access for every team the account can see. Restricted
            keys only expose permitted teams. Keys stay in this process only;
            they are never written to localStorage and are not sent to
            Linear.app. OS keyring persistence is a backend follow-up.
          </p>
          {connected && !apiKey ? null : (
            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="aquarium-linear-api-key"
              >
                Personal API key
              </label>
              <Input
                autoComplete="off"
                data-testid="aquarium-linear-api-key"
                disabled={pending}
                id="aquarium-linear-api-key"
                onChange={(event) => setApiKey(event.target.value)}
                placeholder="lin_api_…"
                type="password"
                value={apiKey}
              />
              {looksLikeLinearPersonalKey(apiKey) ? (
                <p
                  className="text-xs text-muted-foreground"
                  data-testid="aquarium-linear-key-held"
                >
                  Looks like a Linear personal key. It will be held in host
                  memory only and will not be sent to Linear.app (no authorized
                  live client).
                </p>
              ) : null}
            </div>
          )}
          {error ? <p className="text-sm text-destructive">{error}</p> : null}
        </div>
      </ChooserDialogContent>
    </Dialog>
  );
}
