// The UI against the simulator (real engine, fake adapters), in both skins.
//   cargo build -p muesli-sim && node --test apps/sim/tests/*.test.mjs
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { eventually, startSim } from "./sim.mjs";

for (const skin of ["mac", "windows"]) {
  const mac = skin === "mac";

  describe(`${skin} skin`, () => {
    test("setup: four screens, the model downloads on the way, Open Muesli", async () => {
      const sim = await startSim({ scenario: "first-launch", skin });
      try {
        assert.deepEqual((await sim.state()).windows, ["setup"]);
        const page = await sim.open("setup");
        assert.equal(await page.getAttribute("html", "data-skin"), skin);
        await page.getByTestId("setup-title").getByText("Meet Muesli").waitFor();
        await page.getByText(`Runs entirely on this ${mac ? "Mac" : "PC"}.`).waitFor();
        assert.equal(await page.locator("[data-testid=dots] .dot").count(), 4);
        await page.getByTestId("setup-primary").click(); // Get started

        await page.getByTestId("chip-nl").waitFor();
        await page.getByTestId("chip-en").waitFor();
        await page.getByTestId("setup-model").getByText("Downloading").waitFor();
        await page.getByTestId("add-language").selectOption("fr");
        await page.getByTestId("chip-fr").waitFor();
        await page.getByTestId("setup-primary").click(); // Continue

        if (mac) {
          await page.getByTestId("permission-microphone").waitFor();
          await page.getByTestId("permission-calendar").getByText("Optional").waitFor();
          assert.equal(await page.getByTestId("setup-primary").isDisabled(), true, "Continue waits for the required ones");
          await page.getByTestId("permission-microphone").getByRole("button", { name: "Allow…" }).click();
          await page.getByTestId("permission-systemAudio").getByRole("button", { name: "Allow…" }).click();
          await eventually(async () => !(await page.getByTestId("setup-primary").isDisabled()), "Continue enabled");
        } else {
          await page.getByText("Call audio needs no permission").waitFor();
          assert.equal(await page.getByTestId("permission-systemAudio").count(), 0);
          await page.getByTestId("permission-microphone").getByRole("button", { name: "Allow…" }).click();
          await eventually(async () => !(await page.getByTestId("setup-primary").isDisabled()), "Continue enabled");
        }
        await page.getByTestId("setup-primary").click();
        await page.getByText(`Muesli is in your ${mac ? "menu bar" : "system tray"}`).waitFor();
        await page.getByText("Tell people you are transcribing").waitFor();
        await page.getByRole("button", { name: "Open Muesli" }).click();
        await eventually(async () => (await sim.state()).windows.join() === "main", "main window open, setup closed");
      } finally {
        await sim.stop();
      }
    });

    test("a call: asked, transcribed live, stopped and saved", async () => {
      const sim = await startSim({ scenario: "a-day-of-meetings", skin });
      try {
        const page = await sim.open("main");
        await page.getByTestId("meeting-row").first().waitFor();
        assert.equal(await page.getByTestId("meeting-row").count(), 3);
        await page.getByRole("button", { name: mac ? "Show in Finder" : "Show in Explorer" }).waitFor();

        await sim.act({ do: "call", teams: true, seconds: 20 });
        const { question } = await sim.state();
        assert.equal(question.title, "Looks like a meeting: Teams call");
        assert.equal(question.body, "Detected via teams. Start transcribing?");
        await sim.act({ do: "answer", accept: true });

        await page.getByTestId("live-row").waitFor();
        await page.getByTestId("live-badge").getByText("Transcribing · live, read-only").waitFor();
        assert.equal(await page.getByTestId("rename").count(), 0, "a live meeting is read-only");
        const menu = (await sim.state()).menu;
        assert.equal(menu.transcribing, true);
        assert.ok(menu.items.some((i) => i.label === "Transcribing · Teams call"));

        await sim.act({ do: "talk", seconds: 31 });
        await page.getByTestId("transcript").getByText("You").first().waitFor();
        await page.getByTestId("stop").click();
        await eventually(async () => (await page.getByTestId("live-row").count()) === 0, "live row gone");
        await eventually(async () => (await page.getByTestId("meeting-row").count()) === 4, "four meetings");
      } finally {
        await sim.stop();
      }
    });

    test("rename, delete with undo, then delete for good", async () => {
      const sim = await startSim({ scenario: "a-day-of-meetings", skin });
      try {
        const page = await sim.open("main");
        await page.getByTestId("meeting-row").filter({ hasText: "Weekly sync" }).click();
        await page.getByTestId("reader-title").getByText("Weekly sync").waitFor();
        await page.getByTestId("rename").click();
        await page.getByTestId("rename-form").locator("input").fill("Weekly sync, release");
        await page.getByTestId("rename-form").locator("input").press("Enter");
        await page.getByTestId("reader-title").getByText("Weekly sync, release").waitFor();
        await page.getByTestId("meeting-row").filter({ hasText: "Weekly sync, release" }).waitFor();

        await page.getByTestId("delete").click();
        await page.getByTestId("toast").getByText(`moved to the ${mac ? "Trash" : "Recycle Bin"}`).waitFor();
        await eventually(async () => (await page.getByTestId("meeting-row").count()) === 2, "hidden at once");
        await page.getByTestId("undo").click();
        await eventually(async () => (await page.getByTestId("meeting-row").count()) === 3, "back after Undo");

        await page.getByTestId("delete").click();
        await sim.act({ do: "advance", seconds: 7 });
        await eventually(async () => (await page.getByTestId("toast").count()) === 0, "toast gone");
        assert.equal((await sim.view("main")).meetings.length, 2);
      } finally {
        await sim.stop();
      }
    });

    test("without notifications the prompt window asks", async () => {
      const sim = await startSim({ scenario: "notifications-off", skin });
      try {
        await eventually(async () => (await sim.state()).windows.includes("prompt"), "prompt window open");
        const page = await sim.open("prompt");
        await page.getByTestId("prompt").getByText("Looks like a meeting: Teams call").waitFor();
        await page.getByRole("button", { name: "Start transcribing" }).click();
        await eventually(async () => (await sim.view("main")).transcribing, "transcribing");
        assert.ok(!(await sim.state()).windows.includes("prompt"));
      } finally {
        await sim.stop();
      }
    });

    test("settings: one page, platform words, languages and assistants", async () => {
      const sim = await startSim({ scenario: "download-failed", skin });
      try {
        const page = await sim.open("settings");
        await page.getByText(mac ? "Start at login" : "Start when I sign in").waitFor();
        await page.getByRole("button", { name: mac ? "Show in Finder" : "Show in Explorer" }).waitFor();
        await page.getByTestId("add-language").selectOption("de");
        await page.getByTestId("chip-de").waitFor();
        await page.getByRole("button", { name: "Reset" }).click();
        await eventually(async () => (await page.getByTestId("chip-de").count()) === 0, "reset");

        assert.equal(await page.getByTestId("command").count(), 0, "assistants are off by default");
        await page.getByTestId("assistants").check();
        await page.getByText("Connect Claude Desktop").waitFor();
        await page.getByText("Other apps…").click();
        await page.getByTestId("command").getByText("muesli-mcp").waitFor();

        await page.getByText("Advanced").click();
        await page.getByTestId("model").getByText("Download failed").waitFor();
        await page.getByTestId("model").getByText("huggingface.co").first().waitFor();
      } finally {
        await sim.stop();
      }
    });

    test("dark mode follows the system", async () => {
      const sim = await startSim({ scenario: "a-day-of-meetings", skin, colorScheme: "dark" });
      try {
        const page = await sim.open("main");
        await page.getByTestId("meeting-row").first().waitFor();
        const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
        assert.equal(bg, "rgb(30, 22, 16)");
      } finally {
        await sim.stop();
      }
    });
  });
}

test("the simulator page shows the tray, a notification and Muesli's windows", async () => {
  const sim = await startSim({ scenario: "call-ends-no-answer" });
  try {
    const page = await sim.simPage();
    await page.getByTestId("tray").getByText("Transcribing · Teams call").waitFor();
    await page.getByTestId("notification").getByText("The call seems to have ended").waitFor();
    await page.getByTestId("plus-five").click();
    await page.getByTestId("notification").getByText("Saved: Teams call").waitFor();
    await page.getByRole("button", { name: "Open" }).click();
    await page.frameLocator("iframe[name=main]").getByTestId("reader-title").getByText("Teams call").waitFor();
  } finally {
    await sim.stop();
  }
});
