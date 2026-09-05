import type { ReactNode } from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import type { ChannelVisibility } from "@/shared/api/types";
import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import { Dialog } from "@/shared/ui/dialog";

import {
  type CreateChannelInput,
  type CreateChannelKind,
  useCreateChannelForm,
} from "@/features/sidebar/lib/useCreateChannelForm";
import {
  CREATE_CHANNEL_FORM_ID,
  CreateChannelFormFields,
  CreateChannelFormFooter,
} from "@/features/sidebar/ui/CreateChannelFormFields";

type CreateChannelDialogProps = {
  /** Which kind of channel to create, or null when closed. */
  channelKind: CreateChannelKind | null;
  children?: ReactNode;
  description?: string;
  isCreating: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (input: {
    name: string;
    description?: string;
    visibility: ChannelVisibility;
    ttlSeconds?: number;
    templateId?: string;
  }) => Promise<void>;
  testId?: string;
  title?: string;
};

export function CreateChannelDialog({
  channelKind,
  children,
  description,
  isCreating,
  onOpenChange,
  onCreate,
  testId = "create-channel-dialog",
  title,
}: CreateChannelDialogProps) {
  const open = channelKind !== null;
  const { goTank } = useAppNavigation();

  const form = useCreateChannelForm({
    channelKind: channelKind ?? "stream",
    allowTankKind: true,
    active: open,
    isCreating,
    onCreate: onCreate as (input: CreateChannelInput) => Promise<void>,
    onCreated: () => onOpenChange(false),
    onTankCreated: (tankId) => {
      void goTank(tankId);
    },
  });

  const kindLabel =
    form.channelKind === "forum"
      ? "forum"
      : form.channelKind === "tank"
        ? "tank"
        : "channel";

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && (isCreating || form.isCreating)) return;
        onOpenChange(nextOpen);
      }}
    >
      <ChooserDialogContent
        className="max-w-lg"
        contentClassName="pt-3"
        data-testid={testId}
        footerClassName="border-t-0 pt-0"
        headerClassName="pb-2"
        title={title ?? `Create a new ${kindLabel}`}
        headerSubtitle={
          description ??
          (form.channelKind === "forum"
            ? "Forums organize threaded discussions around a topic."
            : form.channelKind === "tank"
              ? "Tanks are task spaces with a conversation, creatures, and an execution device. Mock data until a real adapter is bound."
              : "Channels are real-time streams for team conversation.")
        }
        footer={<CreateChannelFormFooter form={form} />}
      >
        <form
          className="space-y-5"
          id={CREATE_CHANNEL_FORM_ID}
          onSubmit={form.handleSubmit}
        >
          {children}
          <CreateChannelFormFields form={form} />
        </form>
      </ChooserDialogContent>
    </Dialog>
  );
}
