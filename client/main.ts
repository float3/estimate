// The estimation game, in a browser.
//
// One page with three screens: the one that opens a room, the host's, and
// the player's, which is almost always a phone.

import {
  api,
  byId,
  esc,
  fail,
  html,
  hide,
  joinLink,
  list,
  num,
  onClick,
  onSubmit,
  plural,
  qrInto,
  remember,
  remembered,
  route,
  setValue,
  show,
  text,
  value,
  watch,
} from "./rooms";

type Stage = "first" | "discussion" | "second" | "revealed";

interface Bound {
  low: number;
  high: number;
  at: number;
}

interface Extreme {
  name: string;
  value: number;
}

interface Stats {
  n: number;
  median_low: number;
  median_high: number;
  mean_low: number;
  mean_high: number;
  lowest: Extreme;
  highest: Extreme;
  widest: Extreme;
  tightest: Extreme;
  median_width: number;
  hits: number | null;
}

interface Change {
  name: string;
  before: Bound;
  after: Bound;
  moved: number;
  width_change: number;
  hit_before: boolean | null;
  hit_after: boolean | null;
}

interface Pull {
  name: string;
  towards: number;
}

interface Shift {
  n: number;
  moved: number;
  narrower: number;
  wider: number;
  median_middle_before: number;
  median_middle_after: number;
  median_width_before: number;
  median_width_after: number;
  biggest: Change | null;
  pulled: Pull | null;
  hits_before: number | null;
  hits_after: number | null;
  towards: number | null;
}

interface Asked {
  question: string;
  unit: string;
  stage: Stage;
  answer: number | null;
  asked: number;
}

interface Who {
  name: string;
  first: boolean;
  second: boolean;
}

interface Answer {
  name: string;
  low: number;
  high: number;
  width: number;
  hit: boolean | null;
}

interface Score {
  name: string;
  answered: number;
  scored: number;
  hits: number;
  tighter: number;
  changed_mind: number;
}

interface Past {
  question: string;
  unit: string;
  answer: number | null;
  n: number;
  hits: number | null;
  hits_before: number | null;
}

interface HostView {
  join: string;
  confidence: number;
  players: Who[];
  round: Asked | null;
  before: Stats | null;
  after: Stats | null;
  standing: Stats | null;
  shift: Shift | null;
  changes: Change[];
  answers: Answer[];
  history: Past[];
  scores: Score[];
}

interface PlayerView {
  name: string;
  confidence: number;
  round: Asked | null;
  mine_before: Bound | null;
  mine_after: Bound | null;
  submitted: number;
  players: number;
  standing: Stats | null;
  before: Stats | null;
  shift: Shift | null;
  hit: boolean | null;
  hit_before: boolean | null;
  mine_changed: Change | null;
  score: Score | null;
}

/** The unit of the current question, appended to every number shown. */
let unit = "";

const withUnit = (given: number): string =>
  unit ? `${num(given)} ${esc(unit)}` : num(given);
const range = (low: number, high: number): string =>
  `${num(low)} – ${withUnit(high)}`;
const ranged = (bound: Bound): string => range(bound.low, bound.high);
/** A width rather than a pair of bounds, labelled so it is not read as
 *  one. */
const span = (width: number): string => `${withUnit(width)} apart`;

const STAGES: Record<Stage, string> = {
  first: "First ranges",
  discussion: "Talking it over",
  second: "Second ranges",
  revealed: "Revealed",
};

function facts(rows: [string, string][]): string {
  return `<div class="facts">${rows
    .map(([name, said]) => `<div class="fact"><span>${name}</span>${said}</div>`)
    .join("")}</div>`;
}

/** The room's numbers for one pass. */
function statsHtml(stats: Stats, confidence: number): string {
  const rows: [string, string][] = [
    [
      "The middle of the room",
      `<b>${range(stats.median_low, stats.median_high)}<small>median of the lower and the upper bounds</small></b>`,
    ],
    ["On average", `<b>${range(stats.mean_low, stats.mean_high)}</b>`],
    [
      "As low as anyone went",
      `<b>${withUnit(stats.lowest.value)}<small>${esc(stats.lowest.name)}</small></b>`,
    ],
    [
      "As high as anyone went",
      `<b>${withUnit(stats.highest.value)}<small>${esc(stats.highest.name)}</small></b>`,
    ],
    [
      "Widest range",
      `<b>${span(stats.widest.value)}<small>${esc(stats.widest.name)}</small></b>`,
    ],
    [
      "Tightest range",
      `<b>${span(stats.tightest.value)}<small>${esc(stats.tightest.name)}</small></b>`,
    ],
    ["Middling width", `<b>${span(stats.median_width)}</b>`],
  ];
  if (stats.hits !== null) {
    const expected = Math.round((stats.n * confidence) / 100);
    rows.push([
      "Contained the answer",
      `<b>${stats.hits} of ${stats.n}<small>at ${confidence}%, about ${expected} should</small></b>`,
    ]);
  }
  return facts(rows);
}

/** What changed between the two passes. */
function shiftHtml(shift: Shift): string {
  const rows: [string, string][] = [
    [
      "Changed their range",
      `<b>${shift.moved} of ${shift.n}<small>${shift.narrower} narrowed, ${shift.wider} widened</small></b>`,
    ],
    [
      "The middle of the room",
      `<b>${num(shift.median_middle_before)} → ${withUnit(shift.median_middle_after)}</b>`,
    ],
    [
      "Middling width",
      `<b>${num(shift.median_width_before)} → ${span(shift.median_width_after)}</b>`,
    ],
  ];
  if (shift.biggest) {
    rows.push([
      "Moved furthest",
      `<b>${esc(shift.biggest.name)}<small>${ranged(shift.biggest.before)} → ${ranged(shift.biggest.after)}</small></b>`,
    ]);
  }
  if (shift.pulled) {
    rows.push([
      "The room moved towards",
      `<b>${esc(shift.pulled.name)}<small>everyone else ended ${withUnit(shift.pulled.towards)} closer to their first answer</small></b>`,
    ]);
  }
  if (shift.hits_before !== null && shift.hits_after !== null) {
    rows.push([
      "Contained the answer",
      `<b>${shift.hits_before} → ${shift.hits_after} of ${shift.n}</b>`,
    ]);
  }
  if (shift.towards !== null) {
    rows.push([
      "Moved towards the answer",
      `<b>${shift.towards} of ${shift.n}</b>`,
    ]);
  }
  return facts(rows);
}

// ---------------------------------------------------------------- opening

function start(): void {
  show("start");
  onClick("open", async () => {
    byId<HTMLButtonElement>("open").disabled = true;
    try {
      const keys = await api<{ host: string }>("POST", "/api/rooms");
      location.href = `/h/${keys.host}`;
    } catch (e) {
      byId<HTMLButtonElement>("open").disabled = false;
      throw e;
    }
  });
}

// ------------------------------------------------------------------- host

function host(key: string): void {
  show("host");
  const at = (path: string) => `/api/h/${encodeURIComponent(key)}${path}`;
  let link = "";
  let answerFor: number | null = null;

  text("where", "You are the host");

  onSubmit("ask-form", async () => {
    await api("POST", at("/ask"), {
      question: byId<HTMLInputElement>("question").value,
      unit: byId<HTMLInputElement>("unit").value,
      answer: value("answer"),
    });
    byId<HTMLInputElement>("question").value = "";
    byId<HTMLInputElement>("unit").value = "";
    byId<HTMLInputElement>("answer").value = "";
    fail("");
  });

  onSubmit("answer-form", async () => {
    await api("PUT", at("/answer"), { answer: value("true-answer") });
    fail("");
  });

  const moveTo = (stage: Stage) => api("PUT", at("/stage"), { stage });
  onClick("to-discussion", async () => {
    await moveTo("discussion");
    fail("");
  });
  onClick("to-second", async () => {
    await moveTo("second");
    fail("");
  });
  onClick("to-first", async () => {
    await moveTo("first");
    fail("");
  });
  onClick("reveal", async () => {
    await api("POST", at("/reveal"));
    fail("");
  });

  function drawRoom(view: HostView): void {
    const next = joinLink(view.join);
    if (next !== link) {
      link = next;
      const anchor = byId<HTMLAnchorElement>("join-link");
      anchor.textContent = next;
      anchor.href = next;
      qrInto("join-qr", next);
    }
    byId("room").classList.toggle("tight", view.players.length > 0);
    html(
      "who",
      view.players.length === 0
        ? "Nobody has joined yet."
        : `<span class="dot"></span> ${esc(plural(view.players.length, "person", "people"))} in the room`,
    );

    html(
      "levels",
      [50, 80, 90, 98]
        .map(
          (level) =>
            `<button type="button" class="chip" data-level="${level}" aria-pressed="${level === view.confidence}">${level}%</button>`,
        )
        .join(""),
    );
    for (const chip of Array.from(byId("levels").children)) {
      chip.addEventListener("click", () => {
        const level = Number((chip as HTMLElement).dataset.level);
        api("PUT", at("/confidence"), { confidence: level })
          .then(() => fail(""))
          .catch((e: unknown) =>
            fail(e instanceof Error ? e.message : String(e)),
          );
      });
    }
  }

  function drawWaiting(view: HostView, round: Asked): void {
    const taking = round.stage === "second" ? "second" : "first";
    const inYet = view.players.filter((who) =>
      taking === "second" ? who.second : who.first,
    );
    const missing = view.players
      .filter((who) => !(taking === "second" ? who.second : who.first))
      .map((who) => who.name);
    let said = `${inYet.length} of ${plural(view.players.length, "person", "people")} answered`;
    if (round.stage !== "revealed" && missing.length > 0 && missing.length <= 6) {
      said += ` · waiting on ${list(missing)}`;
    }
    if (round.answer === null) said += " · no answer entered yet";
    text("progress", said);
  }

  function drawNumbers(view: HostView): void {
    const round = view.round;
    if (!round) return;
    const parts: string[] = [];
    const twice = view.shift !== null;
    if (round.stage === "revealed" && twice) {
      parts.push(
        `<h3>What the discussion did</h3>${shiftHtml(view.shift as Shift)}`,
      );
    }
    if (view.standing) {
      parts.push(
        `<h3>${twice ? "Where the room ended up" : "What the room thinks"}</h3>${statsHtml(view.standing, view.confidence)}`,
      );
    }
    if (twice && view.before) {
      parts.push(
        `<details><summary>Before the discussion</summary>${statsHtml(view.before, view.confidence)}</details>`,
      );
    }
    if (!view.standing) parts.push(`<p class="note">No ranges yet.</p>`);
    html("numbers", parts.join(""));
  }

  function drawRanges(view: HostView): void {
    if (view.answers.length === 0) {
      html("ranges", "");
      return;
    }
    const scored = view.answers.some((answer) => answer.hit !== null);
    const changed = new Map(view.changes.map((change) => [change.name, change]));
    html(
      "ranges",
      `<h3>Everybody's ranges</h3><div class="scroll"><table><thead><tr>
        <th>Who</th><th>At least</th><th>At most</th><th>Moved</th>${scored ? "<th>In?</th>" : ""}
      </tr></thead><tbody>${view.answers
        .map((answer) => {
          const change = changed.get(answer.name);
          return `<tr>
            <td>${esc(answer.name)}</td><td>${num(answer.low)}</td><td>${num(answer.high)}</td>
            <td>${change ? num(change.moved) : "—"}</td>
            ${scored ? `<td class="${answer.hit ? "good" : "poor"}">${answer.hit ? "yes" : "no"}</td>` : ""}
          </tr>`;
        })
        .join("")}</tbody></table></div>`,
    );
  }

  function drawBoard(view: HostView): void {
    if (view.scores.length === 0) {
      html("board", "");
      return;
    }
    const scored = view.scores.some((score) => score.scored > 0);
    html(
      "board",
      `<h3>The evening so far</h3><div class="scroll"><table><thead><tr>
        <th>Who</th><th>Answered</th>${scored ? "<th>In</th>" : ""}<th>Tighter</th><th>Changed</th>
      </tr></thead><tbody>${view.scores
        .map(
          (score) => `<tr>
          <td>${esc(score.name)}</td><td>${score.answered}</td>
          ${scored ? `<td>${score.scored ? `${score.hits}/${score.scored} · ${Math.round((100 * score.hits) / score.scored)}%` : "—"}</td>` : ""}
          <td>${score.tighter}</td><td>${score.changed_mind}</td>
        </tr>`,
        )
        .join("")}</tbody></table></div>
      <p class="note">“In” counts the ranges that contained the answer.
        “Tighter” counts the questions where the range was
        narrower than the room's median range. “Changed” counts the questions
        where the second range differed from the first.</p>`,
    );
  }

  function drawHistory(view: HostView): void {
    if (view.history.length === 0) {
      html("history", "");
      return;
    }
    html(
      "history",
      `<h3>Asked already</h3>${view.history
        .map(
          (past) => `<div class="past">
        <span>${esc(past.question)}</span>
        <span>${past.answer === null ? "no answer" : esc(num(past.answer))}${
          past.hits === null
            ? ""
            : ` · ${past.hits_before === null ? "" : `${past.hits_before} → `}${past.hits} of ${past.n} in`
        }</span>
      </div>`,
        )
        .join("")}`,
    );
  }

  function draw(view: HostView): void {
    unit = view.round ? view.round.unit : "";
    drawRoom(view);
    const round = view.round;
    hide("asked", round === null);
    if (round) {
      text("h-state", STAGES[round.stage]);
      text("h-question", round.question);
      drawWaiting(view, round);
      // In the first pass the host can move to the discussion; in the
      // discussion, on to the second pass or back to the first; in the
      // second pass, back to the discussion.
      hide("to-discussion", round.stage !== "first" && round.stage !== "second");
      text(
        "to-discussion",
        round.stage === "second" ? "Back to the discussion" : "Stop and talk it over",
      );
      hide("to-second", round.stage !== "discussion");
      hide("to-first", round.stage !== "discussion");
      hide("reveal", round.stage === "revealed");
      // The answer can be entered when the question is asked, typed in
      // while it is being answered, or corrected after the reveal. It
      // stays editable until the next question replaces it.
      hide("answer-form", false);
      if (answerFor !== round.asked) {
        answerFor = round.asked;
        setValue("true-answer", round.answer);
      }
      text(
        "answer-label",
        round.answer === null ? "The answer" : "The answer, which you can still change",
      );
      drawNumbers(view);
      drawRanges(view);
    }
    drawBoard(view);
    drawHistory(view);
  }

  // Fetches the room once, then keeps it up to date from its event
  // stream; `watch` adds the "/events" itself.
  watch<HostView>(at(""), draw);
}

// ------------------------------------------------------------------- join

function join(key: string): void {
  show("join");
  const kept = remembered("estimate", key);
  if (kept) {
    location.replace(`/p/${kept}`);
    return;
  }

  api<{ players: number }>("GET", `/api/j/${encodeURIComponent(key)}`)
    .then((body) => {
      text(
        "join-count",
        body.players === 0
          ? "You are the first one in."
          : `${plural(body.players, "person", "people")} already in.`,
      );
    })
    .catch((e: unknown) => fail(e instanceof Error ? e.message : String(e)));

  onSubmit("name-form", async () => {
    const body = await api<{ player: string }>(
      "POST",
      `/api/j/${encodeURIComponent(key)}`,
      { name: byId<HTMLInputElement>("name").value },
    );
    remember("estimate", key, body.player);
    location.href = `/p/${body.player}`;
  });
}

// ----------------------------------------------------------------- player

function player(key: string): void {
  show("player");
  const at = (path: string) => `/api/p/${encodeURIComponent(key)}${path}`;
  /** Which pass the fields were last filled in for. */
  let filledFor = "";

  onSubmit("bounds-form", async () => {
    const low = value("low");
    const high = value("high");
    if (low === null || high === null) {
      fail("Both bounds need a number.");
      return;
    }
    const body = await api<{ view: PlayerView }>("PUT", at("/bounds"), {
      low,
      high,
    });
    draw(body.view);
    fail("");
  });

  /** This player's two answers and the difference between them. */
  function mineHtml(view: PlayerView): string {
    const change = view.mine_changed;
    if (!change) {
      const standing = view.mine_after ?? view.mine_before;
      return standing
        ? facts([["Your range", `<b>${ranged(standing)}</b>`]])
        : "";
    }
    const rows: [string, string][] = [
      ["Your first range", `<b>${ranged(change.before)}</b>`],
      ["After the discussion", `<b>${ranged(change.after)}</b>`],
    ];
    rows.push([
      "Your middle moved",
      change.moved === 0 && change.width_change === 0
        ? "<b>not at all</b>"
        : `<b>${withUnit(change.moved)}<small>${
            change.width_change === 0
              ? "same width"
              : change.width_change < 0
                ? `${num(-change.width_change)} narrower`
                : `${num(change.width_change)} wider`
          }</small></b>`,
    ]);
    if (change.hit_before !== null && change.hit_after !== null) {
      rows.push([
        "Contained the answer",
        `<b>${change.hit_before ? "yes" : "no"} → <span class="${change.hit_after ? "good" : "poor"}">${change.hit_after ? "yes" : "no"}</span></b>`,
      ]);
    }
    return facts(rows);
  }

  function resultHtml(view: PlayerView): string {
    const round = view.round;
    if (!round || round.stage !== "revealed") return "";
    const verdict =
      view.hit === null
        ? ""
        : view.hit
          ? `<p class="good">Your range contained it.</p>`
          : `<p class="poor">Your range missed it.</p>`;
    const answer =
      round.answer === null
        ? `<p class="note">The host has not entered the answer.</p>`
        : `<p class="eyebrow">The answer</p><p class="big">${withUnit(round.answer)}</p>${verdict}`;
    const room = view.standing
      ? `<h3>The room</h3>${statsHtml(view.standing, view.confidence)}`
      : "";
    const shift = view.shift
      ? `<h3>What the discussion did</h3>${shiftHtml(view.shift)}`
      : "";
    const evening =
      view.score && view.score.scored
        ? `<p class="note">Your evening: ${view.score.hits} of ${view.score.scored}
           contained the answer, ${view.score.tighter} of ${view.score.answered}
           ranges came in tighter than the room's, and the discussion moved you
           on ${plural(view.score.changed_mind, "question", "questions")}.</p>`
        : "";
    return `<div class="panel">${answer}${mineHtml(view)}${shift}${room}${evening}</div>`;
  }

  function draw(view: PlayerView): void {
    unit = view.round ? view.round.unit : "";
    html("you", `<span class="dot"></span> You are ${esc(view.name)}`);
    text("where", `${view.confidence}% sure`);
    const round = view.round;
    if (!round) {
      text("p-state", "Waiting");
      text("p-question", "Waiting for the first question");
      text("p-unit", "");
      hide("bounds-form", true);
      hide("p-closed", true);
      html("p-result", "");
      return;
    }

    const taking = round.stage === "first" || round.stage === "second";
    text("p-state", STAGES[round.stage]);
    text("p-question", round.question);
    text(
      "p-unit",
      (round.unit ? `Answer in ${round.unit}. ` : "") +
        (taking
          ? `Give the range you are ${view.confidence}% sure about.`
          : ""),
    );
    hide("bounds-form", !taking);
    hide("p-closed", taking);
    byId("send").textContent =
      round.stage === "second" ? "Send the new range" : "Send";

    if (!taking) {
      const standing = view.mine_after ?? view.mine_before;
      text(
        "p-closed",
        round.stage === "discussion"
          ? standing
            ? `You said ${ranged(standing)}. Talk it over; the host will ask again.`
            : "You did not answer. Talk it over; the host will ask again."
          : standing
            ? `Your range: ${ranged(standing)}.`
            : "You did not answer this one.",
      );
    }

    // The second pass starts from the first answer, so that submitting
    // it unchanged counts as an answer.
    const fillFor = `${round.asked}:${round.stage}`;
    if (taking && filledFor !== fillFor) {
      filledFor = fillFor;
      const from = round.stage === "second" ? view.mine_before : null;
      const mine = (round.stage === "second" ? view.mine_after : view.mine_before) ?? from;
      setValue("low", mine ? mine.low : null);
      setValue("high", mine ? mine.high : null);
    }

    const mine = round.stage === "second" ? view.mine_after : view.mine_before;
    text(
      "mine",
      `${
        mine ? `Sent: ${ranged(mine)}. You can change it until the host moves on. ` : ""
      }${view.submitted} of ${plural(view.players, "person", "people")} have answered.`,
    );
    if (round.stage === "second" && view.mine_before) {
      text(
        "mine-before",
        `You said ${ranged(view.mine_before)} before the discussion.`,
      );
    }
    hide("mine-before", round.stage !== "second" || !view.mine_before);

    html("p-result", resultHtml(view));
  }

  // Fetches the room once, then keeps it up to date from its event
  // stream; `watch` adds the "/events" itself.
  watch<PlayerView>(at(""), draw);
}

const where = route();
if (where.kind === "host") host(where.key);
else if (where.kind === "join") join(where.key);
else if (where.kind === "player") player(where.key);
else start();
