# z-ro cowork — session handoff

Snapshot of the project as of **2026-04-21** (updated after the
"coworker-grade Zee" sprint — Phases 1–4 of
`~/.claude/plans/plan-all-the-features-soft-quilt.md` are shipped). Use
this file (plus the original architectural plan at
`~/.claude/plans/context-i-m-building-the-quizzical-crayon.md`) to orient
a fresh `/clear`ed session without replaying the whole conversation.

## One-sentence goal

A macOS Tauri app where **Zee**, an NPC co-worker, watches the user's
screen, listens to their voice, and guides them through real-app tasks
(clone a repo, file a Jira ticket…) with live arrows + spoken direction,
like a senior colleague at their desk.

## Stack

- **Frontend:** Svelte 5 + Tauri 2 webview (`src/`).
- **Backend:** Rust + tokio (`src-tauri/src/`).
- **Capture:** Apple ScreenCaptureKit via `screencapturekit` crate — native
  `SCContentSharingPicker`, BGRA frames at 2 fps into a 5-frame ring.
- **Events:** Direct `CGEventTap` (not `rdev` — that crashed on macOS 15+
  via TSM main-thread assertion; see `src-tauri/src/npc/events.rs`).
- **LLM:** Cascaded — Claude (Haiku 4.5 default, Opus 4.7 / Sonnet 4.6
  selectable) via SSE streaming. OpenAI + Gemini available but not default.
- **Voice:** Groq Whisper STT → Claude LLM → OpenAI `onyx` TTS.
- **Platforms:** macOS 14+ only in v1.

## What's shipped

| Phase | Feature | Key files |
|---|---|---|
| 1 | Direct-help prompt + click-through overlay + 15 s safety timer | `npc/prompt_builder.rs`, `npc/overlay_driver.rs`, `overlay/mod.rs` |
| 2 | SCK native picker + FrameBuffer + coord translation (capture-local → monitor-local) | `capture/macos.rs`, `npc/frame_buffer.rs`, `commands/capture.rs`, `components/ScreenPicker.svelte` |
| 3a | Interrupt / barge-in (`CANCELLED_ERR` + `tokio::select!` in pipeline) + Stop button | `npc/interrupt.rs`, `npc/voice/pipeline.rs` |
| 3b | Global event bus (CGEventTap → `NpcEvent::{Click, KeyPress}` broadcast) | `npc/events.rs`, `commands/events.rs`, `components/EventsBadge.svelte` |
| 3c | `Step.milestones` + `Predicate` + `Verifier::first_unsatisfied` | `task_engine/types.rs`, `npc/verifier.rs`, `tasks/navigate_filesystem.json` |
| 3d | Reactive watcher: user click/key → run Verifier → emit `milestone-progress` → auto-advance step | `npc/coordinator.rs` (attach_event_bus / handle_user_event) |
| 4a-1/3 | Claude Haiku default + Opus 4.7 / Sonnet 4.6 in registry + persist model choice to `~/Library/Application Support/com.zro.cowork/npc_settings.json` | `npc/llm_providers/mod.rs`, `npc/settings_store.rs` |
| P1 | Screenshot-first accuracy: always-attach screenshot when task active + JPEG q=80 + dimensional metadata (capture px, monitor pt, scale) threaded into `ScreenContext` + **pixel coords** (was normalized) with `translate_overlay_coords_pure` pipeline pixel → capture-normalized → monitor-normalized | `npc/screen_reader.rs`, `npc/prompt_builder.rs`, `npc/coordinator.rs`, `npc/voice/pipeline.rs` |
| P2 | Continuous on-air voice mode: opt-in 📡 toggle, `src/lib/audio/vad.ts` AnalyserNode VAD (RELEASE 600 ms / MIN_UTT 200 ms), echo suppression while Zee speaks, `on_air_active: AtomicBool` on coordinator, `ConversationMemory` evicts short fillers first | `src/lib/audio/vad.ts`, `components/NpcPanel.svelte`, `npc/coordinator.rs`, `npc/conversation.rs` |
| P3 | Proactive speech on milestone progress: `pipeline.process_proactive_turn` (text→LLM→TTS), `coordinator.ask_proactive`, milestone-transition detection in `handle_user_event` (10 s cooldown + 2 s manual-ask blackout + same-milestone dedupe), `npc-proactive-turn` event + 🔔/🔕 mute toggle | `npc/coordinator.rs`, `npc/voice/pipeline.rs`, `components/NpcPanel.svelte` |
| P4 | WrongMove correction: `pipeline.judge_unexpected_action` (Haiku vision judge, strict-JSON response, nested-brace tolerant `extract_json_object`), debounce gate (≥3 no-progress clicks + >5 s since last progress + window stable) + shared proactive cooldown + mute + `[WRONG MOVE]` tagged ask_proactive with ⚠ prefix | `npc/voice/pipeline.rs`, `npc/coordinator.rs` |

Latest branch: `feature/gemini_live_api`. Latest commit on remote:
`68a199e — feat(npc): Phase 4 — WrongMove correction via LLM judge`.

Test count: **189** passing (`cargo test --lib`, no env needed — see
rpath fix in `1e27053`).

## Pending / explicitly deferred

| Item | Priority | Notes |
|---|---|---|
| **Real-world verification of P1–P4** | HIGHEST | Unit suite covers the deterministic gates + parsing, but live behavior (arrow accuracy on VS Code/Chrome, WrongMove false-positive rate, echo loop on laptops) needs a `npm run tauri dev` session on macOS with mic + screen-recording permissions. |
| **Claude Computer Use (former P5)** | DEFERRED | Only land if P1 measurement shows Haiku arrow accuracy is still unacceptable. ~1 week: SSE tool-use parser (`llm_providers/claude.rs:128–227`), `anthropic-beta: computer-use-2025-01-24` header, `tools: [{type:"computer_20250124",...}]`, translate Computer Use clicks → overlay arrows (visualize, don't execute). |
| **Verifier `Predicate::LlmJudge` arm** | LOW | The Phase 4 WrongMove flow uses the judge directly via `pipeline.judge_unexpected_action`; the `Predicate::LlmJudge(String)` milestone-level path still returns `Undecided`. Wiring it would let task authors use LLM judgements as milestones. |
| **Multi-display** | LOW | Overlay covers primary monitor only. Capture bounds translation assumes primary. On-air users on external displays will hit this. |
| **Windows / Linux** | DEFERRED | v1 is macOS-only. The original Plan file has the breakdown for UIA / AT-SPI later. |

## Known quirks

1. **Accessibility permission is mandatory.** Without it, `CGEventTap` is
   silent (no crash, no events). `EventsBadge` in the NpcPanel header
   shows amber "Grant Accessibility permission" until it's granted via
   System Settings → Privacy & Security → Accessibility → add z-ro.
   A restart is required after granting.
2. **Screen Recording permission** is also mandatory; it's granted by the
   user's first `SCContentSharingPicker` click (no separate prompt).
3. **Tests need no env var.** Rpath `/usr/lib/swift` in
   `.cargo/config.toml` resolves Swift runtime for both the Tauri main
   binary and `cargo test` binaries via the dyld shared cache. See
   `src-tauri/README-dev.md` for the history.
4. **Gemini free-tier 429** — don't use `gemini-2.0-flash` as a default;
   Claude Haiku is now the boot default if the Anthropic key is set.
5. **Opus 4.7 rejects `temperature`** — allow-list in `claude.rs` drops
   the field for that model only. Other Claude models keep `0.7`.
6. **Reactive watcher guards** — only fires when `NpcState::Idle` AND
   the active step has milestones. Clicks always clear a stale overlay
   (added 2026-04-21); keypresses don't (user likely typing what the
   arrow told them to).
7. **On-air mode uses a flag, not a new `NpcState`.** Phase 2 deliberately
   kept state `Idle` between utterances via `on_air_active: AtomicBool`
   so `handle_user_event`'s Idle gate (`coordinator.rs:659`) keeps the
   reactive loop firing. Promoting On-Air to a `NpcState` would break
   Phases 3/4 entirely.
8. **Proactive turns don't pollute history.** `ask_proactive` stores ONLY
   the assistant turn in `ConversationMemory` — the `[MILESTONE PROGRESS]`
   / `[WRONG MOVE]` tagged user prompt is a system construct and would
   corrupt future prompts if kept.
9. **Proactive cooldown is shared between milestone + WrongMove**
   (`last_proactive_at`, 10 s). A milestone speech and a WrongMove fire
   can't stack on each other.

## How to run

```
npm install   # first time only
npm run tauri dev
```

No env setup needed. The backend logs `[z-ro:*]` prefixed lines for each
subsystem; grep for `[z-ro:events]` to confirm the tap armed,
`[z-ro:capture]` for the SCK session, `[z-ro:npc]` for the coordinator.

## Key files to know

- `src-tauri/src/lib.rs` — boot wiring: env, task machine, coordinator,
  capture state, event bus, command registration (incl.
  `npc_set_on_air`, `npc_set_proactive_muted`).
- `src-tauri/src/npc/coordinator.rs` — central state machine: `ask_text`,
  `ask_voice`, `ask_proactive` (P3), `interrupt`, `attach_event_bus`,
  `handle_user_event`, `maybe_fire_proactive_for_milestone` (P3),
  `maybe_fire_wrong_move` (P4), `dispatch_overlay` +
  `translate_overlay_coords_pure`. Phase-3/4 state lives in the struct
  fields at the bottom of the definition.
- `src-tauri/src/npc/voice/pipeline.rs` — `process_text_turn` /
  `process_voice_turn` / `process_proactive_turn` (P3) /
  `judge_unexpected_action` (P4). Cancellation via `InterruptHandle`.
- `src-tauri/src/npc/prompt_builder.rs` — system prompt. Rule 7 now
  teaches PIXEL coords + uses the `Capture:` metadata line.
- `src-tauri/src/npc/screen_reader.rs` — `ScreenContext` now carries
  `capture_width_px`, `capture_height_px`, `monitor_*_pt`,
  `scale_factor`. Screenshot is always attached when `task_active` is
  set by the coordinator.
- `src-tauri/src/npc/verifier.rs` — `evaluate`, `first_unsatisfied`.
  `Predicate::LlmJudge` arm still returns `Undecided` (WrongMove flow
  uses the pipeline judge directly, not via a predicate).
- `src-tauri/src/npc/conversation.rs` — `ConversationMemory::push` now
  evicts oldest short-filler turn before falling back to FIFO.
- `src-tauri/src/npc/events.rs` — `EventBus`, `CGEventTap`.
- `src-tauri/src/capture/macos.rs` — `request_picker_and_start`,
  `CaptureSession`, BGRA → `FrameBuffer`.
- `src/lib/audio/vad.ts` — NEW. AnalyserNode + RMS energy VAD for P2.
- `src/lib/components/NpcPanel.svelte` — chat + PTT + 📡 on-air toggle
  + 🔔/🔕 proactive mute + `npc-proactive-turn` listener.
- `src/lib/npc/npcStore.svelte.ts` — `setOnAir`, `setProactiveMuted`.

## Architectural plan source

- `~/.claude/plans/plan-all-the-features-soft-quilt.md` — the current
  plan (Phases 1–4 shipped, Phase 5 Computer Use deferred). Drives
  all P1/P2/P3/P4 references above.
- `~/.claude/plans/context-i-m-building-the-quizzical-crayon.md` —
  the original 4-phase plan the user approved earlier. Still useful
  for `Phase 3b/3c/3d` context. Use this file ("HANDOFF.md") as the
  source of truth for "what's actually shipped" vs the plan files'
  "what we intended to ship".
