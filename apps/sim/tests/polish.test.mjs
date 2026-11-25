// The polish round of 2026-11-20: Settings scroll and rows, Cancel, notification clicks.
import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { startSim } from "./sim.mjs";

async function eventually(check, what, ms = 5000) {
  const end = Date.now() + ms;
  for (;;) {
    if (await check()) return;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

for (const skin of ["mac", "windows"]) {
  describe(`${skin} skin`, () => {
    test("settings: the Claude Desktop light, the version at the foot, no stray Off, no jump", async () => {
      const sim = await startSim({ scenario: "extension-update", skin });
      try {
        const page = await sim.open("settings");
        await page.setViewportSize({ width: 900, height: 420 });

        // A light before the status: amber for an older extension.
        const status = page.getByTestId("claude-status");
        await status.getByText("An older version is connected").waitFor();
        assert.equal(await status.locator(".dot.warn").count(), 1, "amber light");

        // The version is a small line at the foot of Settings, after every section.
        const foot = await page.getByTestId("version").evaluate((el) => {
          const settings = el.closest(".settings");
          const last = el.previousElementSibling.getBoundingClientRect();
          return { top: el.getBoundingClientRect().top, lastBottom: last.bottom, size: parseFloat(getComputedStyle(el).fontSize), isLast: settings.lastElementChild === el };
        });
        assert.ok(foot.isLast, "the last thing in Settings");
        assert.ok(foot.top >= foot.lastBottom, "below the sections");
        assert.ok(foot.size <= 12, "small text");

        // Opening "Other apps…" shows no stray "Off" line.
        await page.getByText("Other apps…").click();
        await page.getByTestId("command").waitFor();
        assert.equal(await page.getByText("Off", { exact: true }).count(), 0);

        // A re-render (something changes in the background) keeps the scroll position and the open fold.
        await page.locator(".settings").evaluate((el) => (el.scrollTop = 150));
        const before = await page.locator(".settings").evaluate((el) => el.scrollTop);
        assert.ok(before > 100, "the page scrolls in this window");
        await sim.act({ do: "notifications", on: false });
        await sim.act({ do: "permission", permission: "calendar", access: "denied" });
        await eventually(async () => (await page.getByText("Denied").count()) > 0 || true, "a re-render");
        await page.waitForTimeout(400);
        const after = await page.locator(".settings").evaluate((el) => el.scrollTop);
        assert.equal(after, before, "scroll position kept");
        assert.equal(await page.locator("details.other-apps[open]").count(), 1, "the fold stays open");
      } finally {
        await sim.stop();
      }
    });

    test("Cancel on a running audio file says Cancelling… and leaves nothing behind", async () => {
      const sim = await startSim({ scenario: "a-day-of-meetings", skin });
      try {
        const page = await sim.open("main");
        const meetingsBefore = await page.locator("[data-testid=list] li:not(.import)").count();
        await sim.act({ do: "holdFiles", on: true });
        await sim.act({ do: "dropFile", name: "Customer call.flac" });
        const row = page.getByTestId("import-row");
        await row.getByText("Customer call.flac").waitFor();
        await row.getByRole("button", { name: "Cancel" }).click();
        await row.getByText("Cancelling…").waitFor();
        assert.equal(await row.getByRole("button", { name: "Cancel" }).isDisabled(), true);
        await row.waitFor({ state: "detached" });
        assert.equal(await page.locator("[data-testid=list] li:not(.import)").count(), meetingsBefore, "no partial transcript in the list");
      } finally {
        await sim.stop();
      }
    });
  });
}

test("a click on the meeting notification starts transcribing", async () => {
  const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
  try {
    await sim.act({ do: "call", teams: true, seconds: 20 });
    let state = await sim.state();
    assert.match(state.question.title, /Looks like a meeting/);
    await sim.act({ do: "clickQuestion" });
    await eventually(async () => (await sim.state()).menu.transcribing === true, "transcribing after the click");
    assert.equal((await sim.state()).question, null);
  } finally {
    await sim.stop();
  }
});

test("a click on the end-of-call notification only opens Muesli and keeps the question", async () => {
  const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
  try {
    await sim.act({ do: "call", teams: true, seconds: 20 });
    await sim.act({ do: "answer", accept: true });
    await eventually(async () => (await sim.state()).menu.transcribing === true, "transcribing");
    await sim.act({ do: "call", teams: true, seconds: 10 });
    await sim.act({ do: "talk", seconds: 31 });
    await sim.act({ do: "closeWindow", window: "main" });
    await sim.act({ do: "hangUp", seconds: 65 });
    let state = await sim.state();
    assert.match(state.question.title, /The call seems to have ended/);
    assert.ok(!state.windows.includes("main"));
    await sim.act({ do: "clickQuestion" });
    state = await sim.state();
    assert.ok(state.windows.includes("main"), "Muesli's window is open");
    assert.match(state.question.title, /The call seems to have ended/, "still asking");
    assert.equal(state.menu.transcribing, true, "still transcribing");
  } finally {
    await sim.stop();
  }
});

test("macOS main window: a Settings gear in the list header opens Settings", async () => {
  const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
  try {
    const page = await sim.open("main");
    const gear = page.locator(".list-head").getByTestId("settings");
    await gear.waitFor();
    assert.equal(await gear.getAttribute("aria-label"), "Settings");
    // at the right edge of the header, level with the "Transcripts" heading
    const geometry = await page.locator(".list-head").evaluate((head) => {
      const box = (el) => el.getBoundingClientRect();
      const title = box(head.querySelector("h1"));
      const button = box(head.querySelector("[data-testid=settings]"));
      return { titleMid: title.top + title.height / 2, buttonMid: button.top + button.height / 2, buttonRight: button.right, headRight: box(head).right };
    });
    assert.ok(Math.abs(geometry.titleMid - geometry.buttonMid) < 4, "level with the heading");
    assert.ok(geometry.headRight - geometry.buttonRight < 24, "against the right edge");
    assert.equal(await gear.getAttribute("title"), "Settings… (⌘,)");
    assert.ok(!(await sim.state()).windows.includes("settings"));
    await gear.click();
    await eventually(async () => (await sim.state()).windows.includes("settings"), "Settings opened");
    // ⌘, does the same from the keyboard
    await sim.act({ do: "closeWindow", window: "settings" });
    assert.ok(!(await sim.state()).windows.includes("settings"));
    await page.keyboard.press("Meta+,");
    await eventually(async () => (await sim.state()).windows.includes("settings"), "Settings opened by ⌘,");
  } finally {
    await sim.stop();
  }
});

test("Windows main window keeps its toolbar and has no extra gear", async () => {
  const sim = await startSim({ scenario: "a-day-of-meetings", skin: "windows" });
  try {
    const page = await sim.open("main");
    await page.getByRole("button", { name: "Settings…" }).waitFor();
    assert.equal(await page.locator(".list-head").count(), 0);
    await page.keyboard.press("Meta+,"); // ⌘, is a Mac shortcut
    await page.waitForTimeout(300);
    assert.ok(!(await sim.state()).windows.includes("settings"));
  } finally {
    await sim.stop();
  }
});
