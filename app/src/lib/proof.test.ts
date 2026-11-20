import { describe, expect, it } from "vitest";
import { ledger, type Ledger } from "./ledger";
import {
  CLEAR_MS,
  HOLD_MS,
  ProofWatch,
  REFRESH_MS,
  SHOW_AFTER_MS,
  grade,
  proofOf,
  rise,
  type Proof,
} from "./proof";
import type { DirStats, Measured, StatsTick } from "./types";

const M = 1_000_000;

/** One direction: Steam moving `bps` of a 20 Mbps line. */
function dir(bps: number): DirStats {
  return {
    shaped_bps: 20 * M,
    classes: [{ name: "everything else", bps, drops: 0, queued_bytes: 0 }],
    families: [{ family: "steam", name: "Steam", bps, unidentified_bps: 0 }],
    sites: [],
  };
}
const book = (bps: number): Ledger => ledger([], dir(bps), (t) => t.id);
const full = book(19.5 * M);
const calm = book(4 * M);

function tick(ping: number | null, state: StatsTick["state"] = "Shaping"): StatsTick {
  return {
    t_ms: 0,
    state,
    down: null,
    up: null,
    names: null,
    ...(ping === null ? {} : { ping: { now_ms: ping, low_ms: 12 } }),
  };
}

const measured: Measured = { at: 1_763_000_000, down_mbps: 24.6, up_mbps: 4.8, idle_ms: 14, loaded_ms: 142, loaded_up_ms: 260 };

describe("grade", () => {
  it("follows the A–F scale on each edge", () => {
    expect(grade(0)).toBe("A");
    expect(grade(30)).toBe("A");
    expect(grade(30.1)).toBe("B");
    expect(grade(60)).toBe("B");
    expect(grade(60.1)).toBe("C");
    expect(grade(200)).toBe("C");
    expect(grade(200.1)).toBe("D");
    expect(grade(400)).toBe("D");
    expect(grade(400.1)).toBe("F");
  });

  it("rise is loaded − idle, never negative, null when a ping is missing", () => {
    expect(rise(14, 142)).toBe(128);
    expect(rise(20, 15)).toBe(0);
    expect(rise(undefined, 142)).toBeNull();
    expect(rise(14, undefined)).toBeNull();
  });
});

describe("proofOf", () => {
  it("is null without a ping", () => {
    expect(proofOf(tick(null), full, null, measured)).toBeNull();
  });

  it("is null on a quiet line", () => {
    expect(proofOf(tick(19), calm, calm, measured)).toBeNull();
  });

  it("is null unless shaping", () => {
    expect(proofOf(tick(19, "Measuring"), full, null, measured)).toBeNull();
    expect(proofOf(tick(19, { Paused: { until: null } }), full, null, measured)).toBeNull();
  });

  it("compares a full download with the test's loaded ping", () => {
    expect(proofOf(tick(19), full, calm, measured)).toEqual<Proof>({
      now: 19,
      without: 142,
      at: measured.at,
      missing: false,
      name: "Steam",
    });
  });

  it("uses the upload ping when only upload is full", () => {
    expect(proofOf(tick(19), calm, full, measured)?.without).toBe(260);
  });

  it("says less when an old test has no upload ping", () => {
    const old: Measured = { ...measured, loaded_up_ms: undefined };
    const p = proofOf(tick(19), calm, full, old);
    expect(p?.without).toBeNull();
    expect(p?.missing).toBe(true);
  });

  it("hides 'without' when it isn't clearly worse", () => {
    const close = proofOf(tick(142 - CLEAR_MS + 1), full, null, measured);
    expect(close?.without).toBeNull();
    expect(close?.missing).toBe(false);
    expect(proofOf(tick(142 - CLEAR_MS), full, null, measured)?.without).toBe(142);
  });

  it("offers to measure when there's no test on this network", () => {
    const p = proofOf(tick(19), full, null, undefined);
    expect(p).toMatchObject({ now: 19, without: null, at: null, missing: true });
  });
});

describe("ProofWatch", () => {
  const p = (now: number): Proof => ({ now, without: 142, at: 1, missing: false, name: "Steam" });
  /** Feed `proof` every 250 ms (the service's rate) from `from` to `to`
   *  inclusive; returns what the last tick showed. */
  const run = (w: ProofWatch, proof: Proof | null, from: number, to: number): Proof | null => {
    let out: Proof | null = null;
    for (let t = from; t <= to; t += 250) out = w.step(proof, t);
    return out;
  };
  /** A watch already showing 19 ms at `SHOW_AFTER_MS`. */
  const showing = (): ProofWatch => {
    const w = new ProofWatch();
    run(w, p(19), 0, SHOW_AFTER_MS);
    return w;
  };

  it("shows after the line has been full for a while", () => {
    const w = new ProofWatch();
    expect(run(w, p(19), 0, SHOW_AFTER_MS - 250)).toBeNull();
    expect(w.step(p(19), SHOW_AFTER_MS)?.now).toBe(19);
  });

  it("rides out a short dip, but starts the wait again after a real gap", () => {
    const w = new ProofWatch();
    run(w, p(19), 0, 1_000);
    w.step(null, 1_250);
    expect(run(w, p(19), 1_500, SHOW_AFTER_MS)?.now).toBe(19);

    const v = new ProofWatch();
    run(v, p(19), 0, 1_000);
    run(v, null, 1_250, 2_500);
    expect(run(v, p(19), 2_750, 2_750 + SHOW_AFTER_MS - 250)).toBeNull();
    expect(v.step(p(19), 2_750 + SHOW_AFTER_MS)?.now).toBe(19);
  });

  it("holds after the line calms down, then lets go", () => {
    const w = showing();
    const end = SHOW_AFTER_MS + HOLD_MS;
    expect(run(w, null, SHOW_AFTER_MS + 250, end - 250)?.now).toBe(19);
    expect(w.step(null, end)).toBeNull();
  });

  it("goes at once when paused or measuring", () => {
    const w = showing();
    expect(w.step(null, SHOW_AFTER_MS + 250, true)).toBeNull();
    expect(w.step(p(19), SHOW_AFTER_MS + 500)).toBeNull();
  });

  it("keeps the number still through small moves, and follows big ones", () => {
    const w = showing();
    const t0 = SHOW_AFTER_MS;
    expect(w.step(p(20), t0 + 250)?.now).toBe(19);
    expect(w.step(p(18), t0 + 500)?.now).toBe(19);
    expect(w.step(p(21), t0 + 750)?.now).toBe(21);
    expect(w.step(p(22), t0 + 1_000)?.now).toBe(21);
    expect(run(w, p(22), t0 + 1_250, t0 + 750 + REFRESH_MS - 250)?.now).toBe(21);
    expect(w.step(p(22), t0 + 750 + REFRESH_MS)?.now).toBe(22);
  });
});
