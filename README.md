# Z-RO Cowork

> A gamified desktop "coworker" that sees your screen, talks with you,
> and guides you through real company tasks (git, Jira, PRs, Slack,
> debugging, and more).

Aimed at students / new-to-industry devs, Z-RO turns the first weeks on
the job into a series of tiny, XP-rewarding tasks. The coworker — **Zee** —
reads whatever app is on screen, chats by voice or text, and literally
points at buttons with an on-screen arrow when you ask "where do I click?"

---

## Stack at a glance

| Layer        | Tech                                                 |
|--------------|------------------------------------------------------|
| Desktop      | Tauri 2 (Rust)                                       |
| UI           | SvelteKit + Svelte 5 runes + Tailwind CSS v4         |
| Fonts        | Inter Variable + Plus Jakarta Sans                   |
| Vision/LLM   | OpenAI · Google Gemini · Anthropic Claude (swappable) |
| Voice STT    | Groq Whisper                                         |
| Voice TTS    | OpenAI TTS (onyx)                                    |
| Screen read  | Native active-window info + full-screen JPEG capture |

---

## What's already built

### Phase 1 — text-only NPC + task engine
- `TaskMachine` loads JSON task definitions from `tasks/` and tracks the
  user's progress through a linear step list (XP per step, XP per task).
- `NpcCoordinator` wraps an LLM behind a text-only "ask" API, reads the
  focused window (name + title + optional AX tree) and attaches a
  screenshot to each turn.
- LLM responses can emit fenced ` ```overlay ... ``` ` JSON blocks to
  draw on-screen guidance.

### Phase 2 — voice pipeline
- Full STT → LLM → TTS round-trip behind `npc_ask_voice`.
- Groq Whisper for transcription, OpenAI TTS for speech, all streamed
  through one Tauri command so the UI just hands it a `webm` blob.
- Hold-to-talk mic in `NpcPanel`.

### Phase 3 — swappable brains + pointing arrows + UI redesign
- **Multi-provider LLM**: `OpenAiLlm`, `GeminiLlm`, `ClaudeLlm` all
  implement the same `LlmProvider` trait with SSE streaming. The
  `VoicePipeline` holds the active provider behind `RwLock`, so the user
  can hot-swap the brain from Settings without restarting.
  - OpenAI: `gpt-4o-mini`, `gpt-4o`
  - Gemini: `gemini-2.0-flash`, `gemini-2.5-pro`
  - Claude: `claude-haiku-4-5`, `claude-sonnet-4-5`, `claude-opus-4-5`
- **Overlay driver**: normalized `(x, y)` coords (0–1 fraction of screen)
  parsed from LLM responses → converted to pixels → pushed to a borderless
  transparent overlay window that draws arrows, boxes, and labels with
  fade-in animation and auto-hide after 15 s.
- **Dashboard** (`/`): grid of task cards grouped by category, each with
  emoji icon, category-gradient thumbnail, difficulty pill, time and XP
  chips. Hero header with greeting + level + XP bar.
- **Active task view** (`/task`): 3-column layout — step timeline on the
  left (pulsing current step, progress ring, checked-off steps), Zee
  avatar + chat in the center, "what Zee sees" + task progress sidebar
  on the right. Zee avatar has idle / listening / thinking / speaking
  CSS animations.
- **Settings** (`/settings`): radio-card model picker grouped by provider,
  cost-tier badges, vision chips, `localStorage`-persisted selection.
  Plus overlay on/off toggles.
- **Task content**: 9 JSON tasks covering git (clone, commit, PR), Jira
  (file ticket), Slack, calendar, coding (first function, debugging),
  and filesystem navigation.

---

## Project layout

```
z-ro cowork/
├─ src/                         # SvelteKit frontend
│  ├─ routes/
│  │  ├─ +layout.svelte         # top nav + chrome
│  │  ├─ +page.svelte           # dashboard (task grid)
│  │  ├─ task/+page.svelte      # active task view
│  │  ├─ settings/+page.svelte  # model picker + toggles
│  │  └─ overlay/+page.svelte   # transparent arrow/box renderer
│  ├─ lib/
│  │  ├─ components/
│  │  │  ├─ dashboard/          # HeroHeader, TaskCard
│  │  │  ├─ task/               # StepTimeline, ActiveTaskLayout
│  │  │  ├─ coworker/ZeeAvatar  # animated avatar
│  │  │  ├─ settings/ModelPicker
│  │  │  └─ NpcPanel            # chat + mic
│  │  ├─ npc/npcStore.svelte.ts # reactive Svelte-5 store
│  │  └─ stores/taskStore.ts    # active task state
│  └─ app.css                   # design tokens (fonts, colors, gradients)
│
├─ src-tauri/                   # Rust backend
│  └─ src/
│     ├─ lib.rs                 # Tauri builder, env, coordinator wiring
│     ├─ commands/              # Tauri invoke handlers
│     │  ├─ task.rs, npc.rs, overlay.rs, context.rs
│     ├─ task_engine/           # TaskMachine, Task/Step types
│     └─ npc/
│        ├─ coordinator.rs      # orchestrates turns + overlay dispatch
│        ├─ overlay_driver.rs   # normalized → pixel, auto-hide
│        ├─ prompt_builder.rs   # system prompt + overlay rules
│        ├─ screen_reader.rs    # active window + screenshot
│        ├─ voice/pipeline.rs   # STT→LLM→TTS, RwLock<LlmProvider>
│        ├─ llm_providers/      # openai.rs, gemini.rs, claude.rs
│        ├─ stt_providers/groq_whisper.rs
│        └─ tts_providers/openai_tts.rs
│
└─ tasks/                       # Task definitions (JSON)
```

---

## Running it

```bash
# Install deps
npm install

# Set up API keys (at least one LLM, STT for voice, TTS for speech)
cat > .env <<'EOF'
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GEMINI_API_KEY=AI...
GROQ_API_KEY=gsk_...
EOF

# Dev — launches the Tauri shell + Vite dev server
npm run tauri dev
```

Optional env overrides:
- `NPC_PROVIDER=anthropic` / `NPC_MODEL=claude-haiku-4-5` — force a
  default provider instead of the built-in fallback order
  (Gemini → OpenAI).

Tests:

```bash
cd src-tauri && cargo test --lib     # 129 tests, ~6s
cd .. && npm run check               # svelte-check: 0 errors / 0 warnings
```

---

## Known rough edges

- **macOS Accessibility tree is empty** (`ax_tree=0B`): the vision path
  still works via screenshot — AX scraping needs the accessibility
  permission to be granted to Terminal/VS Code/whatever spawns `tauri dev`.
- **Multi-monitor**: only the primary monitor is captured / overlaid.
- **Gemini free-tier quota is small**: if `gemini-2.0-flash` starts
  returning 429, use Settings → Brain to flip to Claude Haiku or
  GPT-4o mini.
- **`/coworker` route**: legacy playground page, still wired but not
  linked from the new nav. Delete in a later cleanup pass.
