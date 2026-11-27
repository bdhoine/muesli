// Setup while the models download, and its AI assistants screen without Claude Desktop.
import assert from "node:assert/strict";
import { test } from "node:test";
import { eventually, startSim } from "./sim.mjs";

test("a change while the mouse button is down does not swallow the click", async () => {
  const sim = await startSim({ scenario: "first-launch", skin: "mac" });
  try {
    const page = await sim.open("setup");
    await page.getByTestId("setup-primary").click(); // Get started
    await page.getByTestId("chip-en").waitFor();

    // Press Continue, change the view underneath (as download progress does), then let go.
    const box = await page.getByTestId("setup-primary").boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await sim.intent({ type: "addLanguage", code: "de" });
    await page.waitForTimeout(500); // the simulator's page polls every 200 ms
    assert.equal(await page.getByTestId("chip-de").count(), 0, "not rendered while the button is held");
    await page.mouse.up();
    await page.getByTestId("setup-title").getByText("Allow Muesli to listen").waitFor();

    // A focused drop-down is not re-rendered (it would close); choosing renders.
    await page.getByTestId("setup-back").or(page.getByRole("button", { name: "Back" })).first().click();
    await page.getByTestId("chip-de").waitFor();
    await page.getByTestId("add-language").focus();
    await sim.intent({ type: "addLanguage", code: "fr" });
    await page.waitForTimeout(500);
    assert.equal(await page.getByTestId("chip-fr").count(), 0, "held back while the drop-down has focus");
    await page.getByTestId("add-language").selectOption("es");
    await page.getByTestId("chip-es").waitFor();
    await page.getByTestId("chip-fr").waitFor();
  } finally {
    await sim.stop();
  }
});

test("without Claude Desktop the assistants screen explains the other apps", async () => {
  const sim = await startSim({ scenario: "first-launch-no-claude", skin: "windows" });
  try {
    const page = await sim.open("setup");
    await page.getByTestId("setup-primary").click();
    await page.getByTestId("setup-primary").click();
    await page.getByTestId("permission-microphone").getByRole("button", { name: "Allow…" }).click();
    await eventually(async () => !(await page.getByTestId("setup-primary").isDisabled()), "Continue enabled");
    await page.getByTestId("setup-primary").click();

    await page.getByTestId("setup-title").getByText("Use Muesli with your AI assistant").waitFor();
    assert.equal(await page.getByTestId("claude-desktop").count(), 0);
    await page.getByTestId("other-apps").getByTestId("command").waitFor();
    await page.getByRole("tab", { name: "Codex" }).click();
    await page.getByTestId("command").getByText("codex").waitFor();
    await page.getByText("Claude Desktop is not on this computer").waitFor();

    // Switched off: no apps, no Skip; Continue goes on and it stays off.
    await page.getByTestId("assistants").uncheck();
    await eventually(async () => (await page.getByTestId("other-apps").count()) === 0, "apps hidden");
    assert.equal(await page.getByTestId("setup-skip").count(), 0);
    await page.getByTestId("setup-primary").click();
    await page.getByRole("button", { name: "Open Muesli" }).waitFor();
    assert.equal((await sim.view("settings")).assistants.enabled, false);
  } finally {
    await sim.stop();
  }
});
