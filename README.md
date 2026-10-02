# skillmgr

A fast, vim-keyed **TUI for managing Claude Code plugins and global skills** — no CLI required. Rust + [ratatui](https://ratatui.rs).

skillmgr reads and writes the real Claude Code configuration, so what you toggle takes effect immediately:

- **Install / uninstall plugins** (collections of skills) — flips `enabledPlugins` in `~/.claude/settings.json`.
- **Browse the skills** inside each plugin.
- **Archive / unarchive global skills** (directories under `~/.claude/skills`).
- **Colour themes**, remembered between runs.

## The model (worth knowing)

- A **plugin** is a collection of skills from a marketplace. Installing it enables its whole skill collection. (Plugins can also ship slash-commands, subagents, hooks and MCP servers; the `skill-library` plugins happen to be skill-only.)
- **Global skills** are directories under `~/.claude/skills`, active simply by being present.
- The only thing with a true on/off switch is a **plugin** (the `enabledPlugins` boolean). Skills ride along with their plugin, or exist by presence.

See `~/Documents/Claude/how-plugins-work.odt` for the full write-up.

## Layout

```
 Plugins | Global skills                         (Tab cycles panes)
┌ enabled plugins ─┐ ┌ enabled skills (12) ───────────────────────┐
│ rust             │ │ accessibility  hurl  react-2026  tailwind… │
│ web-development   │ └─────────────────────────────────────────────┘
└───────────────────┘
┌ plugins  (space = install/uninstall) ──────────────────────────┐
│ › ▣ rust              [21] Expert Rust patterns: ownership…     │
│   ☐ go                [12] Idiomatic Go: modules, errors…       │
└──────────────────────────────────────────────────────────────────┘
┌ skills in rust (21) ────────────────────────────────────────────┐
│ • egui                                                           │
│ • rust-async                                                     │
└──────────────────────────────────────────────────────────────────┘
```

Three panes, cycled with **Tab**: the **plugins** list, the selected plugin's **skills**, and the **global skills** view. The focused pane gets an accent border and a highlighted row; `j`/`k` scroll it.

## Build & run

Requires a stable Rust toolchain.

```bash
cargo run              # dev
cargo build --release  # ./target/release/skillmgr
```

Expects Claude Code's `~/.claude/settings.json`, and (for plugin skills) your marketplace at `~/skill-library`.

## Keys

| Key | Action |
|-----|--------|
| `Tab` | cycle focus: Plugins → that plugin's Skills → Global skills |
| `j` / `k` (or ↓/↑) | scroll the focused pane |
| `space` / `Enter` | toggle: install/uninstall a plugin, or archive/unarchive a global skill |
| `a` | (global-skills pane) archive / unarchive |
| `n` | (plugins pane) create a new plugin |
| `d` | (plugins pane) delete the selected plugin — asks **y / N** to confirm |
| `t` | cycle colour theme (persisted) |
| `r` | reload from disk |
| `q` | quit |

## Views

### Plugins
The middle pane lists every plugin from your marketplace with its enabled state (`▣` enabled/bold, `☐` disabled) and skill count. `space` installs/uninstalls it. The top strip summarises **enabled plugins** (left) and the **skills they provide** (right); the lower pane shows the **selected plugin's skills**.

### Creating & deleting plugins
From the Plugins pane:

- **`n`** opens a form (name + description) and scaffolds a new plugin under
  `~/skill-library/plugins/<name>/` (a `plugin.json` and an empty `skills/`), adds it
  to the marketplace, and lists it disabled — ready to populate and enable.
- **`d`** deletes the selected plugin after a **y / N** confirm. Deleting a plugin
  **only unregisters it** — it removes the plugin's marketplace entry and disables it.
  **It never deletes the skills that were registered with it.** Skills are shared
  across plugins (and with the global skills), so the skill files are always left on
  disk. Only plugins from your `skill-library` marketplace can be deleted.

### Global skills
Lists the skills under `~/.claude/skills`. `space`/`a` **archives** the selected skill — moving its directory to `~/.skillmgr/` (created on first use, adjacent to `~/.claude`) — or **unarchives** it by moving it back.

A skill that **belongs to any plugin** is considered *in use* and cannot be archived; skillmgr shows a popup: *"&lt;skill&gt; cannot be archived because it is in use."*

## What it touches (safety)

| Path | How |
|------|-----|
| `~/.claude/settings.json` | writes **only** the `enabledPlugins` key; key order and all other settings are preserved |
| `~/.claude/skills/` ⇄ `~/.skillmgr/` | archiving/unarchiving moves a skill **directory** between the two (reversible) |
| `~/.skillmgr/config.json` | stores your chosen theme |
| `~/skill-library/.claude-plugin/marketplace.json` | create adds an entry; delete removes it |
| `~/skill-library/plugins/<name>/` | create scaffolds it; **delete never removes skills** |

Note: `~/.claude/skills` is typically a git repo — archiving moves a tracked directory out of it, which git will show as a deletion. Commit that when and how you like.

## Themes

`dark`, `light`, `solarized`, `gruvbox` — cycle with `t`; your choice is saved to `~/.skillmgr/config.json` and restored next launch.

## Roadmap

- [ ] Mouse / clickable buttons
- [ ] `/` filter & search across plugins and skills
- [ ] Richer per-item "button" borders
- [ ] Project-level skills (`.claude/skills` in a repo)
- [ ] Per-skill enable/disable *within* a plugin (needs a mechanism — not native to Claude Code)

## License

See [LICENSE](LICENSE).
