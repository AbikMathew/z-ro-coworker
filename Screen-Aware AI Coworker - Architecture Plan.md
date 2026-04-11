# Screen-Aware AI Coworker: Architecture & Implementation Plan

## Executive Summary

You're building a screen-aware AI "coworker" NPC that observes a user's screen, understands what's happening, and provides real-time guidance through overlays, arrows, audio instructions, and even direct interaction (clicking buttons for the user). This document synthesizes research across Claude Computer Use, 8+ open-source screen agent projects, OS-level APIs, and cutting-edge research papers to give you a concrete architectural plan in Rust.

**The core finding: Your instinct is correct — periodic screenshotting alone won't scale.** The winning approach is a **hybrid architecture** combining accessibility APIs (structured data) + event-driven screenshots (visual data) + a tiered local/cloud AI model system.

---

## Part 1: Why Pure Screenshotting Falls Short

Your current approach (capture screenshot → send to AI → get response → show overlay) has several fundamental problems that every project in this space has encountered:

### 1.1 The Latency Problem
Even Claude's own Computer Use feature acknowledges this: the full round-trip (screenshot capture → image encoding → API transmission → vision analysis → decision making → action generation) takes **1-5+ seconds**. Research from the OSWorld-Human benchmark shows that **75-94% of total latency comes from the LLM planning/reflection phase**, not the screenshot capture itself.

During those seconds, the user may have closed a window, switched desktops, scrolled, or moved to a completely different context. Your overlay arrows would point at elements that no longer exist.

### 1.2 The Information Gap
A screenshot is just pixels. It tells you nothing about what each element *is*, what it *does*, or what *state* it's in. To understand a screenshot, the AI model has to do expensive visual reasoning that a structured data source could provide instantly. For example, a screenshot of a "Submit" button looks the same whether it's enabled or disabled — but the accessibility tree tells you its state directly.

### 1.3 The Bandwidth Problem
Sending full-resolution screenshots to a cloud API on every change is expensive (both in latency and cost). A 1024×768 screenshot costs ~1,600 tokens with Claude. In an active session with captures every 2-3 seconds, you're burning through tokens rapidly.

### 1.4 The Coordinate Mapping Problem
When the AI returns "click at position (450, 320)", that coordinate was calculated on a potentially downscaled version of the screenshot. You need to scale it back up, accounting for DPI differences, multi-monitor setups, and window repositioning that may have happened since capture.

---

## Part 2: How Existing Systems Solve This

### 2.1 Claude Computer Use
- Uses an **agent loop**: screenshot → model decides action → execute action → screenshot again → verify
- Screenshots are captured on-demand (not on an interval) inside a sandboxed virtual display
- Input simulation via xdotool (Linux), native accessibility permissions (macOS)
- Acknowledged limitation: **too slow for real-time interactive use**
- Newer models (Opus 4.6, Sonnet 4.6) added a `zoom` action to inspect specific screen regions at full resolution
- The model sometimes assumes actions worked without verifying — a known failure mode

### 2.2 UFO² (Microsoft) — Best-in-Class for Desktop
- **Dual-agent architecture**: HostAgent (picks the right app) + AppAgent (operates within the app)
- **Key differentiator**: Deeply integrated with **Windows UI Automation (UIA)** — doesn't rely on screenshots alone
- Uses a hybrid of GUI-based actions (clicking coordinates) and **native API calls** (direct programmatic control)
- **Speculative Multi-Action**: Batches multiple predicted actions to reduce LLM round-trips by ~51%
- Falls back between methods for robustness

### 2.3 OpenAdapt — Learning from Demonstrations
- Records time-aligned user actions, screenshots, and window state during demonstrations
- Dual processing: retrieval path (RAG) + training path (fine-tune vision models on recorded trajectories)
- Improved first-action accuracy from 46.7% to 100% by conditioning on demonstrations rather than zero-shot
- Separates **policy** (what to do) from **grounding** (where on screen to do it)

### 2.4 Self-Operating Computer (OthersideAI)
- Simpler architecture: screenshot → vision model → PyAutoGUI
- Multiple grounding modes: OCR mode (coordinate hash maps), Set-of-Mark prompting (numbered element overlays)
- Shows that the screenshot-only approach *works* for simple tasks but struggles with precision

### 2.5 Cradle (BAAI) — Game-Specific
- Six-module architecture: Information Gathering → Self-Reflection → Task Inference → Skill Curation → Action Planning → Memory
- Completed 40-minute missions in Red Dead Redemption 2
- Key insight: **skill curation** (reusable action patterns) dramatically reduces repeated LLM calls

---

## Part 3: The Recommended Architecture

### 3.1 High-Level Design

```
┌──────────────────────────────────────────────────────┐
│                    EVENT BUS (tokio channels)         │
│  OS events, accessibility changes, user triggers      │
└──────────┬───────────────────────────────┬───────────┘
           │                               │
    ┌──────▼──────┐                 ┌──────▼──────┐
    │  PERCEPTION │                 │   OVERLAY   │
    │   LAYER     │                 │   ENGINE    │
    │             │                 │  (egui +    │
    │ Accessibility│                │   winit)    │
    │ Tree + Events│                │             │
    │ Screen Capture│               │ Arrows,     │
    │ Process Info  │               │ circles,    │
    │ Clipboard     │               │ highlights  │
    └──────┬──────┘                 └──────▲──────┘
           │                               │
    ┌──────▼──────┐                        │
    │  LOCAL AI   │   (< 500ms)            │
    │  TIER 1     │────────────────────────┘
    │             │   Quick decisions:
    │ Qwen2.5-VL  │   - Screen changed?
    │ or Florence-2│   - Element locations
    │ (on-device)  │   - Simple guidance
    └──────┬──────┘
           │ Complex tasks only
    ┌──────▼──────┐
    │  CLOUD AI   │   (1-3 seconds)
    │  TIER 2     │
    │             │   Complex reasoning:
    │ Claude /    │   - Multi-step plans
    │ GPT-4o      │   - Debugging help
    │             │   - Strategy decisions
    └──────┬──────┘
           │
    ┌──────▼──────┐
    │  ACTION     │
    │  EXECUTOR   │   (< 100ms)
    │             │
    │ enigo crate │   Mouse clicks,
    │ + keyboard  │   key presses,
    │             │   text input
    └─────────────┘
```

### 3.2 The Four Layers Explained

#### Layer 1: Perception Layer (What's happening on screen?)

Instead of only taking screenshots, gather **multiple data streams**:

| Data Source | What It Provides | Latency | Rust Crate |
|------------|-----------------|---------|------------|
| Accessibility Tree | Element names, roles, states, positions, parent-child hierarchy | ~5ms | `uiautomation` (Windows), `accessibility_sys` (macOS), AT-SPI via D-Bus (Linux) |
| Accessibility Events | Real-time notifications of UI changes (focus, state, text changes) | 0ms (push) | Same as above, event listeners |
| Screenshot | Visual layout, colors, icons, spatial context | ~50ms capture | `xcap` or `scap` |
| Window Info | Active app, window title, position, size | ~1ms | `sysinfo` + OS APIs |
| Process List | What applications are running | ~10ms | `sysinfo` |
| Clipboard | Recently copied text | ~1ms | `clipboard-master` |
| File System | Detect downloads, file saves | Event-driven | `notify` crate |

**The key insight**: The accessibility tree gives you 80% of what you need to understand the screen — for free, instantly, and in structured form. Screenshots fill in the remaining 20% (visual layout, icons, colors).

**When to capture screenshots:**
- When an accessibility event indicates a significant UI change
- When the user explicitly asks for help
- When the local AI model needs visual context to disambiguate
- NOT on a fixed timer

#### Layer 2: AI Understanding (Tiered Model Architecture)

**Tier 1 — Local Model (on-device, < 500ms):**
- Use Qwen2.5-VL-3B or Florence-2 running locally
- Handles: screen change detection, element identification, simple guidance ("click the blue button")
- Input: accessibility tree + targeted screenshot region (not full screen)
- This handles ~70% of guidance scenarios without any cloud round-trip

**Tier 2 — Cloud Model (1-3 seconds):**
- Use Claude Sonnet or GPT-4o
- Handles: complex multi-step planning, debugging strategies, unfamiliar UI patterns
- Only invoked when Tier 1 can't handle the situation
- Input: structured context (accessibility tree + relevant screenshot region + task history)

**Context structure for the AI:**
```json
{
  "current_app": "VS Code",
  "window_title": "main.rs - my-project",
  "accessibility_tree": {
    "role": "editor",
    "children": [
      {"role": "tab", "name": "main.rs", "state": "active"},
      {"role": "textfield", "text": "fn main() { ... }", "cursor_line": 42}
    ]
  },
  "task_context": "User is trying to install the AWS SDK for Rust",
  "user_stuck_signal": "No meaningful input for 30 seconds",
  "screenshot_region": "<base64 of relevant 400x300 area>"
}
```

This is **dramatically cheaper and faster** than sending a full screenshot with "what's on the screen?"

#### Layer 3: Overlay Engine (Visual Guidance)

Use `egui_overlay` (built on winit + egui) for rendering guidance on top of all windows:

**Capabilities needed:**
- Transparent always-on-top window
- Click-through for non-interactive areas (mouse events pass to apps below)
- Draw arrows, circles, highlights, tooltips at specific screen coordinates
- Multi-monitor aware (winit handles this via `ScaleFactorChanged` events)
- DPI-aware coordinate mapping

**Handling screen changes and repositioning:**
- The overlay subscribes to the Event Bus
- When a window moves/resizes, the accessibility tree fires events with updated bounding rectangles
- The overlay immediately repositions its arrows/highlights using the new coordinates
- No need to re-screenshot — the accessibility tree gives you updated positions in real-time

**Implementation pattern:**
```
Accessibility Event: "Submit button moved to (450, 320)"
  → Overlay Engine: reposition arrow to (450, 320)
  → Latency: < 16ms (single frame)
```

Compare this to your current approach:
```
Timer fires → Screenshot → Send to AI → Wait 2-3 seconds → Parse coordinates → Draw arrow
  → Total latency: 2-5 seconds, element may have moved
```

#### Layer 4: Action Executor (Doing things for the user)

When the coworker needs to click a button or type for the user:
- Use the `enigo` crate for cross-platform mouse/keyboard simulation
- Coordinate with the accessibility tree to get exact element positions
- Implement a verification loop: execute action → check accessibility tree for expected state change → retry if needed

---

## Part 4: Solving Your Specific Concerns

### 4.1 "What about multiple desktops?"
The accessibility APIs report which desktop/workspace a window is on. You can detect desktop switches via window manager events and pause/resume guidance accordingly. On Windows, use Virtual Desktop APIs. On macOS, use `NSWorkspace` notifications. On Linux, use EWMH properties via X11 or the equivalent Wayland protocols.

### 4.2 "What about multiple monitors?"
`winit` natively handles multi-monitor setups. Each monitor has its own scale factor and coordinate space. The `xcap` crate can capture specific monitors. Your overlay window spans the correct monitor based on where the target element is.

### 4.3 "What if the user closes a window while we're processing?"
This is exactly why event-driven beats polling. With accessibility events, you get an immediate notification when a window closes. Your event bus propagates this to the AI layer, which can cancel its in-progress analysis and reset. With polling, you wouldn't know until the next screenshot cycle.

### 4.4 "How do overlays track moving/resizing elements?"
The accessibility tree provides bounding rectangles for every UI element, and fires events when they change. Your overlay engine listens for these events and repositions in real-time (sub-16ms). No AI call needed for repositioning.

### 4.5 "What about audio instructions?"
Add a Text-to-Speech module that takes the AI's guidance text and speaks it. On Windows, use SAPI. On macOS, use NSSpeechSynthesizer. On Linux, use espeak or speech-dispatcher. The `tts` Rust crate provides cross-platform TTS. Trigger audio when the guidance type is "explain" rather than "point at element."

---

## Part 5: Rust Crate Reference

### Core Dependencies

| Purpose | Crate | Platform |
|---------|-------|----------|
| Screen Capture | `xcap` | Windows, macOS, Linux (X11 + Wayland) |
| Overlay Rendering | `egui_overlay` + `winit` + `egui` | Cross-platform |
| Input Simulation | `enigo` | Windows, macOS, Linux |
| Accessibility (Windows) | `uiautomation` | Windows |
| Accessibility (macOS) | `accessibility_sys` or `macos-accessibility-client` | macOS |
| Accessibility (Cross-platform) | `accesskit` | Windows, macOS, Linux |
| Accessibility (Full stack) | `computeruse-rs` | macOS, Linux |
| Window Focus Monitoring | `focus_monitor` | Cross-platform |
| Clipboard Monitoring | `clipboard-master` | Cross-platform |
| Process Monitoring | `sysinfo` | Cross-platform |
| File System Watching | `notify` | Cross-platform |
| Async Runtime | `tokio` | Cross-platform |
| HTTP Client (for AI APIs) | `reqwest` | Cross-platform |
| Image Processing | `image` | Cross-platform |
| TTS | `tts` | Cross-platform |
| Local AI Inference | `candle` or `llama-cpp-rs` | Cross-platform |

---

## Part 6: Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
**Goal**: Replace interval-based screenshotting with event-driven perception.

1. Set up the Event Bus using tokio broadcast channels
2. Implement the Accessibility Tree reader for your primary OS (start with one)
3. Subscribe to accessibility events (focus changes, state changes, window events)
4. Implement targeted screenshot capture (only when events indicate visual change)
5. Build the basic overlay window with `egui_overlay` — draw a circle at given coordinates

**Deliverable**: A system that detects UI changes via events and can point at specific elements.

### Phase 2: AI Integration (Weeks 3-4)
**Goal**: Intelligent understanding of screen state.

1. Structure the multi-modal context (accessibility tree + screenshot region)
2. Integrate Claude/GPT-4o API for complex reasoning (Tier 2)
3. Implement the prompt template that combines structured data with visual
4. Build the guidance parser (AI response → overlay instructions)
5. Add verification loops (did the guidance help? did the user act on it?)

**Deliverable**: A system that understands what the user is stuck on and provides accurate guidance.

### Phase 3: Local AI + Speed (Weeks 5-6)
**Goal**: Reduce cloud dependency and latency.

1. Integrate a local vision model (Qwen2.5-VL-3B via `candle` or ONNX runtime)
2. Implement the tiered routing: local model handles simple cases, cloud handles complex
3. Add screen diffing to avoid redundant captures
4. Implement context pruning (keep history concise for faster LLM inference)
5. Add streaming responses (start showing guidance before AI finishes thinking)

**Deliverable**: Sub-second guidance for common scenarios, 2-3s for complex ones.

### Phase 4: Polish & Scale (Weeks 7-8)
**Goal**: Production-ready with full feature set.

1. Multi-monitor and DPI scaling support
2. Multiple desktop/workspace handling
3. Audio instruction generation (TTS)
4. Direct action execution (clicking/typing for the user)
5. Skill curation system (cache common guidance patterns, like Cradle's approach)
6. Cross-platform support (expand from primary OS to others)

**Deliverable**: A complete coworker NPC system.

---

## Part 7: Architecture Decision Summary

| Decision | Recommendation | Rationale |
|----------|---------------|-----------|
| Primary data source | Accessibility Tree + Events | Structured, instant, semantic; covers 80% of needs |
| Screenshot strategy | Event-driven, region-targeted | Only capture when needed, only the relevant area |
| AI architecture | Tiered (local + cloud) | Local for speed, cloud for complexity |
| Overlay framework | egui_overlay (Rust) | Native Rust, click-through, transparent, multi-monitor |
| Element tracking | Accessibility bounding rects + events | Real-time repositioning without re-screenshotting |
| Input simulation | enigo crate | Cross-platform, well-maintained |
| Inter-component communication | tokio broadcast channels | Async, non-blocking, Rust-native |
| Screen change detection | Accessibility events + dirty region tracking | Don't waste resources on unchanged screens |

---

## Part 8: Key Risks and Mitigations

### Risk 1: Accessibility tree not available for all apps
**Reality**: Most modern apps (browsers, IDEs, office suites, OS dialogs) expose good accessibility trees. Custom/game UIs may not.
**Mitigation**: Fall back to screenshot + vision model for apps without accessibility support. Detect this automatically.

### Risk 2: Local model accuracy insufficient
**Reality**: 3B parameter models are significantly less capable than cloud models.
**Mitigation**: Use local models only for well-defined tasks (element detection, change classification). Route ambiguous cases to cloud immediately.

### Risk 3: Cross-platform accessibility API differences
**Reality**: Windows UIA, macOS AX, and Linux AT-SPI have very different APIs and capabilities.
**Mitigation**: Define a common trait/interface in Rust, implement per-platform. Start with one platform, expand. The `accesskit` crate provides some abstraction.

### Risk 4: Overlay interfering with user interaction
**Reality**: An always-on-top overlay can block clicks or confuse users.
**Mitigation**: Use click-through mode (egui_overlay default). Make overlays dismissable. Fade out after a timeout. Never cover the element the user needs to click.

---

## Appendix: Key Research Sources

- **Claude Computer Use Docs**: [platform.claude.com/docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- **UFO² (Microsoft)**: Windows UI-focused agent with speculative multi-action
- **OpenAdapt**: Demonstration-conditioned automation — [github.com/OpenAdaptAI/OpenAdapt](https://github.com/OpenAdaptAI/OpenAdapt)
- **Self-Operating Computer**: [github.com/OthersideAI/self-operating-computer](https://github.com/OthersideAI/self-operating-computer)
- **Cradle**: General Computer Control for games — [baai-agents.github.io/Cradle](https://baai-agents.github.io/Cradle/)
- **OSWorld-Human Benchmark**: Quantifies agent latency breakdown (75-94% from LLM) — [arxiv.org/pdf/2506.16042](https://arxiv.org/pdf/2506.16042)
- **AccessKit (Rust)**: [github.com/AccessKit/accesskit](https://github.com/AccessKit/accesskit)
- **egui_overlay**: [crates.io/crates/egui_overlay](https://crates.io/crates/egui_overlay)
- **OS Agents Survey (ACL 2025)**: Comprehensive coverage of MLLM-based GUI agents
- **Qwen2.5-VL**: Production-grade local vision model — [huggingface.co/Qwen](https://huggingface.co/Qwen/Qwen2.5-VL-7B-Instruct)
