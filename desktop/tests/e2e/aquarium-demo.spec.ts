import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, openCreateChannelDialog } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "buzz-feature-overrides-v1",
      JSON.stringify({ projects: true }),
    );
  });
  await installMockBridge(page);
});

test("Channels stay separate from Tanks; dashboard opens the canonical tank", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
  await expect(page.getByTestId("stream-list")).toBeVisible();
  await expect(page.getByTestId("aquarium-tanks-section")).toBeVisible();

  await page.getByTestId("open-projects-view").click();
  await expect(page.getByTestId("aquarium-projects-dashboard")).toBeVisible();
  await expect(page.getByTestId("aquarium-mock-badge").first()).toBeVisible();
  await page
    .getByTestId("aquarium-dashboard-open-tank-mock-auth-login")
    .click();
  await expect(page.getByTestId("aquarium-tank-workspace")).toBeVisible();
  await expect(page.getByTestId("chat-title")).toHaveText("Auth login polish");
  await expect(page.getByTestId("message-composer")).toBeVisible();
  await expect(
    page.getByTestId("channel-shared-header-backdrop"),
  ).toBeVisible();
});

test("Tanks plus opens shared create UI with Tank selected", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId("aquarium-tanks-create").click();
  await expect(page.getByTestId("create-channel-dialog")).toBeVisible();
  await expect(page.getByTestId("create-space-kind")).toBeVisible();
  await expect(page.getByTestId("aquarium-tank-fields")).toBeVisible();
  await expect(page.getByTestId("create-channel-template")).toHaveCount(0);
  await expect(page.getByTestId("create-tank-device-offline")).toHaveCount(0);
});

test("shared create dialog still exposes channel fields for Channel", async ({
  page,
}) => {
  await page.goto("/");
  await openCreateChannelDialog(page);
  await expect(page.getByTestId("create-channel-dialog")).toBeVisible();
  await expect(page.getByTestId("create-space-kind-stream")).toBeVisible();
  await expect(page.getByTestId("create-channel-name")).toBeVisible();
  await expect(page.getByTestId("create-channel-template")).toBeVisible();
  await expect(page.getByTestId("aquarium-tank-fields")).toHaveCount(0);
});

test("new tank shows Ocean; adding a saved profile creates a leader instance", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId("aquarium-tanks-create").click();
  await page.getByTestId("create-channel-name").fill("Fresh mock tank");
  await page.getByTestId("create-channel-submit").click();
  await expect(page.getByTestId("aquarium-ocean")).toBeVisible();
  await waitForAnimations(page);
  await page.getByTestId(/aquarium-ocean-add-profile-mock-ink/).click();
  await expect(page.getByTestId("aquarium-creature-shelf")).toBeVisible();
  await expect(page.getByTestId("aquarium-leader-crown")).toBeVisible();
  await expect(page.getByTestId("aquarium-ocean")).toHaveCount(0);
  await expect(page.getByTestId("message-composer")).toBeVisible();
});
