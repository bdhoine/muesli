// Muesli's own updates in both skins: the menu, the icon's dot, the banner, the notification,
// Settings, and the first start after an update.
//   cargo build -p muesli-sim && node --test apps/sim/tests/*.test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, test } from "node:test";
import { eventually, startSim } from "./sim.mjs";

const VERSION = readFileSync(new URL("../../../Cargo.toml", import.meta.url), "utf8").match(/^version = "([^"]+)"/m)[1];

for (const skin of ["mac", "windows"]) {
  const mac = skin === "mac";

  describe(`${skin} skin: updates`, () => {
    test("an update ready while idle: menu, dot, banner and one notification", async () => {
      const sim = await startSim({ scenario: "update-ready", skin });
      try {
        await eventually(async () => (await sim.state()).menu.updateReady, "the update dot");
        const state = await sim.state();
        assert.equal(state.menu.items[0].label, "Restart to Update to 0.2.0");
        assert.equal(state.menu.transcribing, false);
        assert.equal(state.notice.title, "Muesli 0.2.0 is ready to install");
        assert.equal(state.notice.action, "Restart");

        const page = await sim.open("main");
        const banner = page.getByTestId("update-banner");
        await banner.getByText("Muesli 0.2.0 is ready to install").waitFor();
        await banner.getByRole("button", { name: "What's new" }).click();
        await eventually(
          async () => (await sim.state()).log.some((l) => l.includes("https://github.com/bdhoine/muesli/releases/tag/v0.2.0")),
          "release notes opened",
        );
        await banner.getByRole("button", { name: "Not now" }).click();
        await eventually(async () => (await page.getByTestId("update-banner").count()) === 0, "banner put away");
        const after = await sim.state();
        assert.equal(after.menu.items[0].label, "Restart to Update to 0.2.0", "the menu item stays");
        assert.equal(after.menu.updateReady, true, "the dot stays");

        const settings = await sim.open("settings");
        await settings.getByTestId("update-status").getByText("0.2.0 is ready").waitFor();
        await settings.getByTestId("update-restart").click();
        await eventually(async () => (await sim.state()).updatesInstalled.now === 1, "installed and restarted");
      } finally {
        await sim.stop();
      }
    });

    test("an update ready while transcribing waits until Stop and Save", async () => {
      const sim = await startSim({ scenario: "update-while-transcribing", skin });
      try {
        const settings = await sim.open("settings");
        await settings.getByTestId("update-status").getByText("0.2.0 is ready").waitFor();
        await eventually(async () => await settings.getByTestId("update-restart").isDisabled(), "Restart is unavailable");
        const state = await sim.state();
        assert.equal(state.menu.transcribing, true);
        assert.equal(state.menu.updateReady, false, "the red dot wins");
        assert.notEqual(state.menu.items[0].label, "Restart to Update to 0.2.0");
        assert.equal(state.notice, null);

        const page = await sim.open("main");
        await page.getByTestId("live-badge").waitFor();
        assert.equal(await page.getByTestId("update-banner").count(), 0);
        await page.getByTestId("stop").click();
        await eventually(async () => !(await sim.state()).menu.transcribing, "saved");
        await sim.act({ do: "advance", seconds: 1 });
        await page.getByTestId("update-banner").getByText("Muesli 0.2.0 is ready to install").waitFor();
        const after = await sim.state();
        assert.equal(after.menu.items[0].label, "Restart to Update to 0.2.0");
        assert.equal(after.menu.updateReady, true);
        assert.equal(after.notice.title, "Muesli 0.2.0 is ready to install");
        await eventually(async () => !(await settings.getByTestId("update-restart").isDisabled()), "Restart available");
      } finally {
        await sim.stop();
      }
    });

    test("Check for Updates: up to date", async () => {
      const sim = await startSim({ scenario: "update-up-to-date", skin });
      try {
        const page = await sim.open("settings");
        await page.getByTestId("version").getByTestId("version-number").getByText(VERSION).waitFor();
        await page.getByTestId("last-checked").getByText("Not checked yet").waitFor();
        assert.equal(await page.getByTestId("auto-update").isChecked(), true, "on by default");
        await page.getByTestId("check-updates").click();
        await page.getByTestId("update-status").getByText("Muesli is up to date").waitFor();
        await page.getByTestId("last-checked").getByText("Last checked:").waitFor();
        await page.getByTestId("version").getByRole("button", { name: "What's new" }).click();
        await eventually(
          async () => (await sim.state()).log.some((l) => l.includes(`releases/tag/v${VERSION}`)),
          "release notes of this version",
        );
        await page.getByTestId("auto-update").uncheck();
        await eventually(async () => (await sim.view("settings")).updates.autoCheck === false, "switched off");
      } finally {
        await sim.stop();
      }
    });

    test("Check for Updates: no connection, with Try again", async () => {
      const sim = await startSim({ scenario: "update-check-failed", skin });
      try {
        const page = await sim.open("settings");
        await page.getByTestId("check-updates").click();
        await page.getByTestId("update-status").getByText("Couldn't check for updates: Could not connect to github.com (timed out after 15 s).").waitFor();
        await page.getByRole("button", { name: "Try again" }).waitFor();
        await sim.act({ do: "release", version: "0.2.0" });
        await page.getByRole("button", { name: "Try again" }).click();
        await page.getByTestId("update-status").getByText("0.2.0 is ready").waitFor();
      } finally {
        await sim.stop();
      }
    });

    test("the first start after an update", async () => {
      const sim = await startSim({ scenario: "updated-permissions-lost", skin });
      try {
        const state = await sim.state();
        assert.equal(state.notice.title, `Muesli updated to ${VERSION}`);
        assert.equal(state.notice.action, "What's new");
        if (!mac) {
          assert.deepEqual(state.windows, [], "Windows keeps its permissions");
          return;
        }
        assert.deepEqual(state.windows, ["setup"]);
        const page = await sim.open("setup");
        await page.getByText("Muesli was updated, and macOS asks for these permissions again.").waitFor();
        await page.getByTestId("permission-microphone").waitFor();
        assert.equal((await page.locator(".counter").textContent()).trim(), "");
        assert.equal(await page.getByRole("button", { name: "Back" }).count(), 0);
        assert.equal(await page.getByTestId("setup-primary").isDisabled(), true);
        await page.getByTestId("permission-microphone").getByRole("button", { name: "Allow…" }).click();
        await page.getByTestId("permission-systemAudio").getByRole("button", { name: "Allow…" }).click();
        await eventually(async () => !(await page.getByTestId("setup-primary").isDisabled()), "Continue enabled");
        await page.getByTestId("setup-primary").click();
        await eventually(async () => (await sim.state()).windows.length === 0, "setup closed, nothing else opened");
      } finally {
        await sim.stop();
      }
    });
  });
}
