# z-ro cowork — session handoff

Snapshot of the project as of **2026-04-21**. Use this file (plus the full
plan at `~/.claude/plans/context-i-m-building-the-quizzical-crayon.md`) to
orient a fresh `/clear`ed session without replaying the whole conversation.

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

Latest branch: `feature/gemini_live_api`. Latest commit on remote:
`2af5f13 — fix(events): replace rdev with direct CGEventTap`.

Test count: 171 passing (`cargo test --lib`, no env needed after the
`/usr/lib/swift` rpath fix in `1e27053`).

## Pending / explicitly deferred

| Item | Priority | Notes |
|---|---|---|
| **On-call voice mode** | HIGHEST | User wants continuous listening (VAD) + proactive speech on milestone events. Push-to-talk is the current UX gap. See design sketch in the latest chat. |
| **Phase 4a-4 — Claude Computer Use tool** | HIGH | Directly addresses the "arrow sometimes off" complaint. Needs SSE parsing for `tool_use` blocks, pixel-coord contract, beta header. |
| **Coord accuracy polish** | MED | The coord-translation geometry is right (see Phase 4a-5, tested with 6 unit tests) but the LLM's normalized prediction itself is imprecise without Computer Use. |
| **Verifier `LlmJudge` wiring** | LOW | `Predicate::LlmJudge(String)` currently returns `Undecided`. Needs a cheap Haiku call with before/after screenshots when a non-LLM predicate can't decide. |
| **Multi-display** | LOW | Overlay covers primary monitor only. Capture bounds translation assumes primary. |
| **Windows / Linux** | DEFERRED | v1 is macOS-only. The Plan file has the breakdown for UIA / AT-SPI later. |

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
  capture state, event bus, command registration.
- `src-tauri/src/npc/coordinator.rs` — central state machine: `ask_text`,
  `ask_voice`, `interrupt`, `attach_event_bus`, `handle_user_event`,
  `dispatch_overlay` + `translate_overlay_coords_pure`.
- `src-tauri/src/npc/voice/pipeline.rs` — `process_text_turn` /
  `process_voice_turn` with cancellation via `InterruptHandle`.
- `src-tauri/src/npc/prompt_builder.rs` — system prompt. Rule 1 is the
  "be directly helpful" instruction.
- `src-tauri/src/npc/verifier.rs` — `evaluate`, `first_unsatisfied`.
- `src-tauri/src/npc/events.rs` — `EventBus`, `CGEventTap` via
  `core_graphics::event::CGEventTap::new`.
- `src-tauri/src/capture/macos.rs` — `request_picker_and_start`,
  `CaptureSession`, BGRA → `FrameBuffer`.
- `src/lib/components/NpcPanel.svelte` — chat + hold-to-talk + Stop
  button + `EventsBadge` + milestone-progress pills.

## Architectural plan source

`~/.claude/plans/context-i-m-building-the-quizzical-crayon.md` — the
original 4-phase plan the user approved before any code changes. Still
mostly accurate, but some items have moved (e.g. 4a-5 coord translation
shipped before 4a-4 Computer Use tool). Use this file as the source of
truth for "what's actually shipped" vs the plan file's "what we intended
to ship".
