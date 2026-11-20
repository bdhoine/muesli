// The macOS skin against the design canvas ("Muesli – macOS app"): sizes, colours and what is
// shown, measured on the rendered page. When the design changes, change the numbers here with it.
import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { eventually, startSim } from "./sim.mjs";

const OAT = "rgb(234, 223, 200)";
const MILK = "rgb(246, 241, 231)";
const CARD = "rgb(255, 253, 248)";
const LINE = "rgb(220, 208, 184)";
const SEED = "rgb(91, 122, 58)";
const MUTED = "rgb(107, 90, 75)";
const SIDEBAR = "rgb(239, 231, 216)";
const CONTROL_LINE = "rgb(201, 185, 148)";

/** The computed style properties of the first element matching `selector`, and its box. */
async function look(page, selector, props) {
  return page.locator(selector).first().evaluate((el, props) => {
    const cs = getComputedStyle(el);
    const box = el.getBoundingClientRect();
    const out = { width: box.width, height: box.height, right: box.right, bottom: box.bottom };
    for (const p of props) out[p] = cs.getPropertyValue(p);
    return out;
  }, props);
}

describe("macOS skin matches the design", () => {
  test("setup", async () => {
    const sim = await startSim({ scenario: "first-launch", skin: "mac" });
    try {
      const page = await sim.open("setup");
      await page.setViewportSize({ width: 720, height: 600 });
      await page.getByTestId("setup-title").waitFor();

      // Welcome: mark tile, no counter, no step dots, no download line, three promise cards.
      const tile = await look(page, ".mark-tile", ["background-color", "border-radius"]);
      assert.deepEqual([tile.width, tile.height, tile["background-color"], tile["border-radius"]], [56, 56, OAT, "13px"]);
      assert.equal((await page.locator(".counter").textContent()).trim(), "");
      assert.equal(await page.locator(".dots").count(), 0);
      assert.equal(await page.getByTestId("setup-model").count(), 0);
      const h1 = await look(page, "h1", ["font-size", "font-weight"]);
      assert.deepEqual([h1["font-size"], h1["font-weight"]], ["30px", "500"]);
      const sub = await look(page, ".subtitle", ["font-size", "color"]);
      assert.deepEqual([sub["font-size"], sub.color], ["16px", MUTED]);
      assert.equal(await page.locator(".promises li").count(), 3);
      const card = await look(page, ".promises li", ["background-color", "border-top-color", "border-radius", "padding-left"]);
      assert.deepEqual([card["background-color"], card["border-top-color"], card["border-radius"], card["padding-left"]], [CARD, LINE, "12px", "16px"]);
      const icon = await look(page, ".promises .icon", ["border-radius"]);
      assert.deepEqual([icon.width, icon.height, icon["border-radius"]], [36, 36, "18px"]);
      const primary = await look(page, "[data-testid=setup-primary]", ["background-color", "border-radius", "font-weight"]);
      assert.deepEqual([primary.height, primary["background-color"], primary["border-radius"], primary["font-weight"]], [36, SEED, "7px", "600"]);
      const body = await look(page, ".setup-body", ["padding-left", "padding-top"]);
      assert.deepEqual([body["padding-left"], body["padding-top"]], ["56px", "28px"]);

      // Languages: "Step 1 of 2" and the download line in the footer.
      await page.getByTestId("setup-primary").click();
      await page.getByTestId("chip-en").waitFor();
      assert.equal((await page.locator(".counter").textContent()).trim(), "Step 1 of 2");
      await page.getByTestId("setup-model").getByText("Speech model downloading in the background").waitFor();
      const chip = await look(page, ".chip", ["border-radius"]);
      assert.deepEqual([chip.height, chip["border-radius"]], [36, "18px"]);
      const back = await look(page, ".buttons .btn:not(.primary)", ["border-top-color"]);
      assert.deepEqual([back.height, back["border-top-color"]], [36, CONTROL_LINE]);
    } finally {
      await sim.stop();
    }
  });

  test("main window", async () => {
    const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
    try {
      const page = await sim.open("main");
      await page.setViewportSize({ width: 1080, height: 680 });
      await page.getByTestId("meeting-row").first().click();
      await page.getByTestId("reader-title").waitFor();

      // The list: 360 px, oat-tinted, a bare "Transcripts" header, no toolbar.
      const sidebar = await look(page, ".sidebar", ["background-color"]);
      assert.deepEqual([sidebar.width, sidebar["background-color"]], [360, SIDEBAR]);
      assert.equal(await page.locator(".toolbar").count(), 0);
      assert.equal(await page.locator(".list-head button").count(), 0);
      const head = await look(page, ".list-head h1", ["font-size"]);
      assert.equal(head["font-size"], "20px");
      const selected = await look(page, ".row.selected", ["background-color", "border-radius"]);
      assert.deepEqual([selected["background-color"], selected["border-radius"]], ["rgba(91, 122, 58, 0.16)", "10px"]);

      // The transcript: 26 px title, 15 px lines with a 44 px time column, milk background.
      const reader = await look(page, ".reader", ["background-color"]);
      assert.equal(reader["background-color"], MILK);
      const title = await look(page, "[data-testid=reader-title]", ["font-size"]);
      assert.equal(title["font-size"], "26px");
      const time = await look(page, ".line .time", []);
      assert.equal(time.width, 44);
      const line = await look(page, ".line", ["font-size"]);
      assert.equal(line["font-size"], "15px");

      // Start transcribing: a 56 px round button, 28 px from the bottom right corner.
      const fab = await look(page, ".fab", ["background-color", "border-radius", "font-size"]);
      assert.deepEqual([fab.height, fab["background-color"], fab["border-radius"], fab["font-size"]], [56, SEED, "28px", "16px"]);
      assert.deepEqual([1080 - fab.right, 680 - fab.bottom], [28, 28]);
      await page.getByRole("button", { name: "Start transcribing" }).waitFor();
    } finally {
      await sim.stop();
    }
  });

  test("main window while transcribing", async () => {
    const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
    try {
      const page = await sim.open("main");
      await page.setViewportSize({ width: 1080, height: 680 });
      await page.getByTestId("meeting-row").first().waitFor();
      await sim.act({ do: "call", teams: true, seconds: 20 });
      await sim.act({ do: "answer", accept: true });
      await page.getByTestId("live-row").click();
      await sim.act({ do: "talk", seconds: 31 });
      await page.getByTestId("transcript").getByText("You").first().waitFor();
      // Text comes in a block per pause (at most 10 s), so wait until the last block is in.
      let count = -1;
      await eventually(async () => {
        await page.waitForTimeout(400);
        const now = await page.locator(".transcript .line").count();
        const settled = now === count;
        count = now;
        return settled;
      }, "the last block", 10000);

      // No Start button while transcribing; Stop and Save in the header instead.
      assert.equal(await page.locator(".fab").count(), 0);
      await page.getByTestId("stop").waitFor();
      // The newest line is faded while it settles, the others are not.
      const lines = page.locator(".transcript .line");
      const last = (await lines.count()) - 1;
      const opacity = (i) => lines.nth(i).evaluate((el) => getComputedStyle(el).opacity);
      await page.waitForTimeout(1300); // the fade-in takes 1.2 s
      assert.equal(await opacity(last), "0.6");
      if (last > 0) assert.equal(await opacity(0), "1");
      // "Listening" with three dots, 58 px in so it lines up with the text after the time column.
      await page.getByRole("status").getByText("Listening · text follows each pause").waitFor();
      assert.equal(await page.locator(".listening .bounce span").count(), 3);
      const listening = await look(page, ".listening", ["padding-left", "font-size"]);
      assert.deepEqual([listening["padding-left"], listening["font-size"]], ["58px", "13.5px"]);
      const dot = await look(page, ".bounce span", ["background-color", "border-radius"]);
      assert.deepEqual([dot.width, dot.height, dot["background-color"], dot["border-radius"]], [6, 6, SEED, "3px"]);
    } finally {
      await sim.stop();
    }
  });

  test("settings", async () => {
    const sim = await startSim({ scenario: "a-day-of-meetings", skin: "mac" });
    try {
      const page = await sim.open("settings");
      await page.setViewportSize({ width: 760, height: 1060 });
      await page.getByTestId("folder").waitFor();

      // One page: uppercase headings above white cards, rows 18 px in, 44 × 26 switches.
      const headings = (await page.locator(".settings > h2").allTextContents()).map((t) => t.trim());
      assert.deepEqual(headings, ["General", "Languages", "Vocabulary", "AI assistants"]); // Vocabulary is ours, after the design
      const h2 = await look(page, ".settings > h2", ["font-size", "text-transform", "color", "letter-spacing"]);
      assert.deepEqual([h2["font-size"], h2["text-transform"], h2.color], ["12.5px", "uppercase", MUTED]);
      const card = await look(page, ".card", ["background-color", "border-top-color", "border-radius"]);
      assert.deepEqual([card["background-color"], card["border-top-color"], card["border-radius"]], [CARD, LINE, "12px"]);
      const row = await look(page, ".setting", ["padding-left", "padding-top"]);
      assert.deepEqual([row["padding-left"], row["padding-top"]], ["18px", "13px"]);
      const title = await look(page, ".setting .title", ["font-size", "font-weight"]);
      assert.deepEqual([title["font-size"], title["font-weight"]], ["15px", "500"]);
      const toggle = await look(page, "[data-testid=login]", []);
      assert.deepEqual([toggle.width, toggle.height], [44, 26]);
      const button = await look(page, ".setting .btn", ["border-top-color", "border-radius"]);
      assert.deepEqual([button.height, button["border-top-color"], button["border-radius"]], [32, CONTROL_LINE, "7px"]);

      // Advanced and Other apps… fold, with an arrow that turns when open.
      const turn = (sel) => page.locator(`${sel} > summary .chevron`).evaluate((el) => getComputedStyle(el).transform);
      for (const sel of [".advanced"]) {
        const arrow = await look(page, `${sel} > summary .chevron`, []);
        assert.deepEqual([arrow.width, arrow.height], [10, 10]);
        assert.equal(await turn(sel), "none");
      }
      // Advanced: the speech model as "Speech model" with its size, and the permissions.
      await page.locator(".advanced summary").click();
      await page.waitForTimeout(200);
      assert.equal(await turn(".advanced"), "matrix(0, 1, -1, 0, 0, 0)");
      await page.getByTestId("model").getByText("Speech model").waitFor();
      await page.getByTestId("model").getByText("574 MB, on this Mac").waitFor();
      await page.getByTestId("permission-microphone").getByText("Your side of the conversation").waitFor();
    } finally {
      await sim.stop();
    }
  });
});
