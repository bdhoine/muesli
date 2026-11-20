// Live text follows pauses: the scenarios for a long monologue, a rapid back-and-forth and a
// silent stretch, through the real engine with the stand-in voice detector. And a call through
// the speakers, with the stand-in echo canceller.
//   cargo build -p muesli-sim && node --test apps/sim/tests/*.test.mjs
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { eventually, startSim } from "./sim.mjs";

async function lines(sim) {
  const view = await sim.view("main");
  return view.reader?.lines ?? [];
}

describe("live text follows pauses", () => {
  test("rapid back-and-forth: each sentence, in turn, before anyone stops", async () => {
    const sim = await startSim({ scenario: "back-and-forth" });
    try {
      await eventually(async () => (await lines(sim)).length === 4, "four sentences");
      const got = await lines(sim);
      assert.deepEqual(
        got.map((l) => l.speaker),
        ["You", "Others", "You", "Others"],
      );
      assert.ok((await sim.view("main")).live, "still transcribing");
    } finally {
      await sim.stop();
    }
  });

  test("a long monologue comes in blocks of at most ten seconds", async () => {
    const sim = await startSim({ scenario: "long-monologue" });
    try {
      await eventually(async () => (await lines(sim)).length >= 5, "five blocks for 45 s");
      const got = await lines(sim);
      assert.ok(got.every((l) => l.speaker === "Others"));
      // Each block starts where the one before was cut, at most ten seconds later.
      const seconds = got.map((l) => Number(l.time.slice(0, 2)) * 60 + Number(l.time.slice(3)));
      assert.equal(seconds[0], 0);
      for (let i = 1; i < seconds.length; i++) assert.ok(seconds[i] - seconds[i - 1] <= 10, `${got[i - 1].time} → ${got[i].time}`);
    } finally {
      await sim.stop();
    }
  });

  test("a silent stretch: nothing for ten minutes, then the sentence at its own time", async () => {
    const sim = await startSim({ scenario: "silent-stretch" });
    try {
      await eventually(async () => (await lines(sim)).length === 1, "one sentence");
      const [line] = await lines(sim);
      assert.equal(line.speaker, "You");
      assert.equal(line.time, "09:59");
    } finally {
      await sim.stop();
    }
  });

  test("through the speakers: the others once, as Others, and your replies as You", async () => {
    const sim = await startSim({ scenario: "through-the-speakers" });
    try {
      await eventually(async () => (await lines(sim)).length === 3, "three sentences");
      // Give an echo the time it would need to show up as well.
      await new Promise((resolve) => setTimeout(resolve, 1500));
      const got = await lines(sim);
      assert.deepEqual(
        got.map((l) => l.speaker),
        ["Others", "You", "Others"],
      );
    } finally {
      await sim.stop();
    }
  });
});
