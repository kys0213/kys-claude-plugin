// The atelier session kanban as a herdr-app panel: every atelier session by what it waits on,
// a click goes to its terminal, and a waiting session can be answered from here.

import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { agents, checking, exec, run, terminals, theme, watchFolder } from "herdr";
import { boardKey, decideArgs, layout, moveTarget, outcome, parseStatus, permitArgs, recordsDir, replyArgs } from "./board.js";

// The board is read when a session's record changes; the slow timer only moves the ages
// on, and the fast one stands in for the watch where the records folder can't be watched.
const AGE_MS = 30000;
const POLL_MS = 2000;
const CLI_TIMEOUT_MS = 10000;

export default class AtelierBoard extends View {
  init(_props, cx) {
    this.sessions = null;
    this.error = null;
    this.shown = null;
    this.loading = false;
    this.again = false;
    this.againAges = false;
    this.watching = false;
    this.picks = {};
    this.notes = {};
    if (checking()) return;
    cx.timer.every(AGE_MS, (cx) => this.refresh(cx, true));
    cx.timer.every(POLL_MS, (cx) => { if (!this.watching) this.refresh(cx); });
    this.refresh(cx);
    this.watch(cx);
  }

  watch(cx) {
    cx.spawn(async (cx) => {
      let dir = null;
      try {
        dir = recordsDir(await exec("atelier", ["kanban", "record-path", "--session", "board"], { timeout: CLI_TIMEOUT_MS }));
      } catch {
        dir = null;
      }
      if (!dir) return;
      this.watching = true;
      for (;;) {
        try {
          await watchFolder(dir);
        } catch {
          this.watching = false;
          return;
        }
        this.refresh(cx);
      }
    });
  }

  // `ages`: redraw even when only the ages moved on.
  refresh(cx, ages = false) {
    if (this.loading) {
      this.again = true;
      this.againAges = this.againAges || ages;
      return;
    }
    this.loading = true;
    cx.spawn(async (cx) => {
      let read;
      try {
        read = parseStatus(await exec("atelier", ["kanban", "status", "--format", "json"], { timeout: CLI_TIMEOUT_MS }));
      } catch (e) {
        read = { error: `atelier CLI 를 실행하지 못했어요: ${e?.message ?? e}` };
      }
      this.sessions = read.sessions ?? this.sessions;
      const key = boardKey(this.sessions);
      const changed = ages || key !== this.shown || (read.error ?? null) !== this.error;
      this.shown = key;
      this.error = read.error ?? null;
      this.loading = false;
      if (changed) cx.notify();
      if (this.again) {
        const againAges = this.againAges;
        this.again = false;
        this.againAges = false;
        this.refresh(cx, againAges);
      }
    });
  }

  send(cx, s, args) {
    if (!args) return;
    this.notes[s.session_id] = "보내는 중…";
    cx.notify();
    cx.spawn(async (cx) => {
      let done;
      try {
        done = outcome(await exec("atelier", args, { timeout: CLI_TIMEOUT_MS }));
      } catch (e) {
        done = { error: `${e?.message ?? e}` };
      }
      this.notes[s.session_id] = done.ok ? "보냈어요" : done.error;
      if (done.ok) delete this.picks[s.session_id];
      cx.notify();
      this.refresh(cx);
    });
  }

  go(cx, s) {
    const target = moveTarget(s.hc_terminal, agents(), terminals());
    if (target) run(target.command, target.args);
    else this.notes[s.session_id] = s.hc_terminal ? "터미널이 끝났어요" : "이동할 터미널이 없어요";
    cx.notify();
  }

  button(t, id, label, onPress, on = false) {
    return div()
      .id(id)
      .px(8)
      .py(3)
      .rounded(5)
      .border(1)
      .border_color(on ? t.accent : t.border)
      .when(on, (el) => el.bg(t.selected))
      .hover((el) => el.bg(t.hover))
      .cursor_pointer()
      .on_click((_e, cx) => onPress(cx))
      .child(label);
  }

  pick(cx, s, qi, q, label) {
    if (!this.picks[s.session_id]) this.picks[s.session_id] = {};
    const all = this.picks[s.session_id];
    const now = all[qi] ?? [];
    all[qi] = q.multi_select ? (now.includes(label) ? now.filter((l) => l !== label) : [...now, label]) : [label];
    cx.notify();
  }

  waitBlock(t, s) {
    const w = s.wait;
    const can = (action) => (s.actions ?? []).includes(action);
    const id = s.session_id;
    const box = v_flex().gap(5).pt(4);

    if (w.kind === "permission") {
      return box
        .child(div().text_color(t.blocked).child(`권한 요청: ${w.tool ?? ""}`))
        .when(Boolean(w.target), (el) => el.child(div().text_color(t.muted).truncate().child(w.target)))
        .when(can("permit"), (el) =>
          el.child(h_flex().gap(6)
            .child(this.button(t, `allow-${id}`, "허용", (cx) => this.send(cx, s, permitArgs(s, "allow"))))
            .child(this.button(t, `deny-${id}`, "거부", (cx) => this.send(cx, s, permitArgs(s, "deny"))))));
    }

    if (w.kind === "question") {
      const picks = this.picks[id] ?? {};
      return box
        .children((w.questions ?? []).map((q, qi) =>
          v_flex().gap(4)
            .child(div().text_color(t.foreground).child(q.question))
            .child(h_flex().flex_wrap().gap(6).children(q.options.map((o, oi) =>
              this.button(t, `q-${id}-${qi}-${oi}`, o.label, (cx) => this.pick(cx, s, qi, q, o.label), (picks[qi] ?? []).includes(o.label)))))))
        .when(can("answer"), (el) => el.child(h_flex().child(this.button(t, `reply-${id}`, "답 보내기", (cx) => {
          const args = replyArgs(s, picks);
          if (args) this.send(cx, s, args);
          else { this.notes[id] = "모든 질문에 답을 골라 주세요"; cx.notify(); }
        }))));
    }

    if (w.kind === "decision") {
      return box
        .child(div().text_color(t.foreground).child(w.ask ?? "결정 요청"))
        .when(can("decide"), (el) =>
          el.child(h_flex().flex_wrap().gap(6).children((w.options ?? []).map((o, oi) =>
            this.button(t, `d-${id}-${oi}`, o, (cx) => this.send(cx, s, decideArgs(s, o)))))));
    }

    return box.child(div().text_color(t.muted).child("입력 요청"));
  }

  card(t, s, depth) {
    const note = this.notes[s.session_id];
    return v_flex()
      .id(`card-${s.session_id}`)
      .ml(depth * 14)
      .p(8)
      .gap(2)
      .rounded(7)
      .border(1)
      .border_color(t.border)
      .bg(t.background)
      .child(h_flex()
        .id(`go-${s.session_id}`)
        .gap(6)
        .items_center()
        .cursor_pointer()
        .hover((el) => el.text_color(t.accent))
        .on_click((_e, cx) => this.go(cx, s))
        .child(div().flex_1().truncate().font_semibold().child(`${depth > 0 ? "↳ " : ""}${s.label ?? s.session_id}`))
        .child(div().flex_none().text_color(t.dim).child(s.age ?? "")))
      .when(Boolean(s.subtitle), (el) => el.child(div().truncate().text_color(t.muted).child(s.subtitle)))
      .when(Boolean(s.last_line), (el) => el.child(div().truncate().text_color(t.dim).child(s.last_line)))
      .when(!s.hc_terminal, (el) => el.child(div().text_size(11).text_color(t.dim).child("터미널 밖 세션")))
      .when(s.column === "needs_input" && Boolean(s.wait), (el) => el.child(this.waitBlock(t, s)))
      .when(Boolean(note), (el) => el.child(div().text_size(11).text_color(t.muted).child(note)));
  }

  group(t, g, depth) {
    return v_flex().gap(4)
      .child(this.card(t, g.session, depth))
      .children(g.children.map((c) => this.group(t, c, depth + 1)));
  }

  render(_cx) {
    const t = theme();
    const root = v_flex().id("atelier-board").size_full().bg(t.surface).p(10).gap(12).text_size(12.5).text_color(t.foreground).overflow_y_scrollbar();

    if (this.error && this.sessions === null) {
      return root
        .child(div().text_color(t.muted).child(this.error))
        .child(div().text_color(t.dim).child("/atelier:update 로 atelier CLI 를 설치하거나 갱신해요"));
    }
    if (this.sessions === null) return root.child(div().text_color(t.dim).child("불러오는 중…"));

    return root
      .when(Boolean(this.error), (el) => el.child(div().text_size(11).text_color(t.muted).child(this.error)))
      .children(layout(this.sessions, terminals()).map((col) =>
        v_flex().gap(6)
          .child(div().text_size(11.5).font_semibold().text_color(t.muted).child(`${col.heading} ${col.count}`))
          .children(col.groups.map((g) => this.group(t, g, 0)))));
  }
}
