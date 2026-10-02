# skillmgr

A TUI to manage **Claude Code plugins** (collections of skills) — enable/disable
them without the CLI, with selectable colour schemes. Rust + ratatui, same spirit
as [podmangr](https://github.com/podmangr/podmangr).

## Model
- A **plugin** is a collection of skills (e.g. `rust`, `web-development`). Installing
  a plugin enables its whole skill collection.
- **Source of truth (no CLI needed):** `enabledPlugins` in `~/.claude/settings.json`.
  Plugin list/descriptions and each plugin's skills come from `~/skill-library`
  (`.claude-plugin/marketplace.json` and `plugins/<name>/skills/`).
- **Global skills** (`~/.claude/skills`) are **out of scope for release 1** — not touched.

## Layout
- **Top-left:** enabled plugins.
- **Top-right:** enabled skills — the skills contained in all installed plugins.
- **Middle:** every plugin; `space` installs/uninstalls. Enabled = bold/green `▣`.
- **Lower:** the selected plugin's skills (browse a collection's contents).

## Build & run
```bash
cargo run              # dev
cargo build --release  # ./target/release/skillmgr
```
Writes only the `enabledPlugins` key in `settings.json` (preserves order and every
other setting).

## Keys
| Key | Action |
|-----|--------|
| `j` / `k` (or ↓/↑) | move plugin selection |
| `space` / `Enter` | install / uninstall the selected plugin |
| `t` | cycle colour scheme (dark / light / solarized / gruvbox) |
| `r` | reload from settings.json |
| `q` | quit |

## Status (r1)
- [x] Read plugins + enabled state from `settings.json` / marketplace
- [x] Install / uninstall plugins (writes `enabledPlugins`, order preserved)
- [x] Top summary (enabled plugins | enabled skills) + per-plugin skill browser
- [x] Colour schemes (`t`)

## Roadmap
- [ ] Mouse / clickable buttons (r1 is keyboard-first, vim keys)
- [ ] Richer "button" rendering (per-item borders) matching the original sketch
- [ ] `/` filter/search across plugins & skills
- [ ] Per-skill enable/disable *within* a plugin — needs a mechanism, since Claude's
      `enabledPlugins` is plugin-level only (not natively per-skill)
- [ ] Separate view to display/manage the default (global) skills
