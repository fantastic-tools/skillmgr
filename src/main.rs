//! skillmgr — a TUI to manage Claude Code plugins & global skills.
//! No CLI: plugins via enabledPlugins in ~/.claude/settings.json (+ ~/skill-library);
//! global skills are dirs under ~/.claude/skills, archived to ~/.skillmgr.
//! Tab cycles focus across the three panes; j/k scroll the focused pane.
//! Theme preference persists to ~/.skillmgr/config.json.

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use serde_json::Value;
use std::time::Duration;

struct Plug {
    key: String,
    name: String,
    desc: String,
    enabled: bool,
    skills: Vec<String>,
}
struct GSkill {
    name: String,
    archived: bool,
}

#[derive(Default)]
struct PlugForm {
    name: String,
    desc: String,
    focus: usize, // 0 = name, 1 = description
    err: String,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/root".into())
}
fn settings_path() -> String {
    format!("{}/.claude/settings.json", home())
}
fn skills_dir() -> String {
    format!("{}/.claude/skills", home())
}
fn skillmgr_dir() -> String {
    format!("{}/.skillmgr", home())
}
fn archive_dir() -> String {
    skillmgr_dir()
}
fn config_path() -> String {
    format!("{}/config.json", skillmgr_dir())
}

fn load_settings() -> Result<Value> {
    let p = settings_path();
    let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {p}"))?;
    Ok(serde_json::from_str(&txt)?)
}
fn save_settings(v: &Value) -> Result<()> {
    let p = settings_path();
    std::fs::write(&p, serde_json::to_string_pretty(v)? + "\n").with_context(|| format!("writing {p}"))?;
    Ok(())
}

fn load_theme_name() -> Option<String> {
    let txt = std::fs::read_to_string(config_path()).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    v.get("theme").and_then(|t| t.as_str()).map(|s| s.to_string())
}
fn save_theme_name(name: &str) {
    let _ = std::fs::create_dir_all(skillmgr_dir());
    let mut v: Value = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("theme".into(), Value::String(name.to_string()));
    }
    if let Ok(txt) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(config_path(), txt + "\n");
    }
}

fn plugin_skills(name: &str) -> Vec<String> {
    let dir = format!("{}/skill-library/plugins/{}/skills", home(), name);
    let mut v: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    if !n.starts_with('.') {
                        v.push(n.to_string());
                    }
                }
            }
        }
    }
    v.sort();
    v
}

fn load_plugins(settings: &Value) -> Vec<Plug> {
    let mut desc: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mkt = format!("{}/skill-library/.claude-plugin/marketplace.json", home());
    if let Ok(txt) = std::fs::read_to_string(&mkt) {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(arr) = v.get("plugins").and_then(|p| p.as_array()) {
                for p in arr {
                    if let Some(n) = p.get("name").and_then(|n| n.as_str()) {
                        desc.insert(
                            n.to_string(),
                            p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                        );
                    }
                }
            }
        }
    }
    let mut out: Vec<Plug> = Vec::new();
    if let Some(map) = settings.get("enabledPlugins").and_then(|e| e.as_object()) {
        for (key, val) in map {
            let name = key.split('@').next().unwrap_or(key).to_string();
            let skills = plugin_skills(&name);
            out.push(Plug {
                key: key.clone(),
                desc: desc.get(&name).cloned().unwrap_or_default(),
                enabled: val.as_bool().unwrap_or(false),
                name,
                skills,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn dirs_in(path: &str) -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    if !n.starts_with('.') && n != "synced" {
                        v.push(n.to_string());
                    }
                }
            }
        }
    }
    v
}
fn load_global_skills() -> Vec<GSkill> {
    let mut out: Vec<GSkill> = Vec::new();
    for n in dirs_in(&skills_dir()) {
        out.push(GSkill { name: n, archived: false });
    }
    for n in dirs_in(&archive_dir()) {
        out.push(GSkill { name: n, archived: true });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
fn set_archived(name: &str, archived: bool) -> Result<(), String> {
    let active = format!("{}/{}", skills_dir(), name);
    let arch = format!("{}/{}", archive_dir(), name);
    let (from, to) = if archived { (active, arch) } else { (arch, active) };
    if archived {
        std::fs::create_dir_all(archive_dir()).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&from, &to).map_err(|e| e.to_string())
}

fn marketplace_path() -> String {
    format!("{}/skill-library/.claude-plugin/marketplace.json", home())
}

fn valid_plugin_name(n: &str) -> bool {
    !n.is_empty()
        && n.chars().next().map(|c| c != '-').unwrap_or(false)
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn create_plugin_files(name: &str, desc: &str) -> Result<(), String> {
    if !valid_plugin_name(name) {
        return Err("name must be lowercase letters, digits, hyphens".into());
    }
    let base = format!("{}/skill-library/plugins/{}", home(), name);
    if std::path::Path::new(&base).exists() {
        return Err(format!("plugin '{name}' already exists"));
    }
    std::fs::create_dir_all(format!("{base}/.claude-plugin")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(format!("{base}/skills")).map_err(|e| e.to_string())?;
    let pj = serde_json::json!({
        "name": name, "version": "0.1.0", "description": desc,
        "author": { "name": "mrdoodles" }, "license": "MIT", "keywords": []
    });
    std::fs::write(
        format!("{base}/.claude-plugin/plugin.json"),
        serde_json::to_string_pretty(&pj).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    let mp = marketplace_path();
    let txt = std::fs::read_to_string(&mp).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let arr = v.get_mut("plugins").and_then(|p| p.as_array_mut()).ok_or("marketplace has no plugins array")?;
    if arr.iter().any(|p| p.get("name").and_then(|n| n.as_str()) == Some(name)) {
        return Err("already listed in marketplace".into());
    }
    arr.push(serde_json::json!({ "name": name, "source": format!("./plugins/{name}"), "description": desc }));
    std::fs::write(&mp, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())? + "\n").map_err(|e| e.to_string())?;
    Ok(())
}

fn delete_plugin_files(name: &str) -> Result<(), String> {
    if !valid_plugin_name(name) {
        return Err("invalid plugin name".into());
    }
    let base = format!("{}/skill-library/plugins/{}", home(), name);
    if std::path::Path::new(&base).exists() {
        std::fs::remove_dir_all(&base).map_err(|e| e.to_string())?;
    }
    let mp = marketplace_path();
    if let Ok(txt) = std::fs::read_to_string(&mp) {
        if let Ok(mut v) = serde_json::from_str::<Value>(&txt) {
            if let Some(arr) = v.get_mut("plugins").and_then(|p| p.as_array_mut()) {
                arr.retain(|p| p.get("name").and_then(|n| n.as_str()) != Some(name));
            }
            if let Ok(out) = serde_json::to_string_pretty(&v) {
                let _ = std::fs::write(&mp, out + "\n");
            }
        }
    }
    Ok(())
}

struct Theme {
    name: &'static str,
    fg: Color,
    bg: Color,
    accent: Color,
    on: Color,
    off: Color,
}
fn themes() -> Vec<Theme> {
    vec![
        Theme { name: "dark",      fg: Color::Gray,  bg: Color::Reset,                   accent: Color::Cyan,            on: Color::Green,          off: Color::DarkGray },
        Theme { name: "light",     fg: Color::Black, bg: Color::White,                   accent: Color::Blue,            on: Color::Rgb(0,135,0),   off: Color::Gray },
        Theme { name: "solarized", fg: Color::Rgb(131,148,150), bg: Color::Rgb(0,43,54), accent: Color::Rgb(38,139,210), on: Color::Rgb(133,153,0), off: Color::Rgb(88,110,117) },
        Theme { name: "gruvbox",   fg: Color::Rgb(235,219,178), bg: Color::Rgb(40,40,40), accent: Color::Rgb(250,189,47), on: Color::Rgb(184,187,38), off: Color::Rgb(146,131,116) },
    ]
}

#[derive(PartialEq, Clone, Copy)]
enum Focus {
    Plugins,
    PluginSkills,
    GlobalSkills,
}

struct App {
    settings: Value,
    plugins: Vec<Plug>,
    gskills: Vec<GSkill>,
    pstate: ListState,
    skstate: ListState, // skills of the selected plugin
    gstate: ListState,
    focus: Focus,
    theme: usize,
    msg: String,
    popup: Option<String>,
    new_form: Option<PlugForm>,
    confirm_delete: Option<(String, String)>, // (key, name)
    quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let settings = load_settings()?;
        let plugins = load_plugins(&settings);
        let gskills = load_global_skills();
        let mut pstate = ListState::default();
        pstate.select(if plugins.is_empty() { None } else { Some(0) });
        let mut skstate = ListState::default();
        skstate.select(Some(0));
        let mut gstate = ListState::default();
        gstate.select(if gskills.is_empty() { None } else { Some(0) });
        // restore saved theme
        let theme = load_theme_name()
            .and_then(|n| themes().iter().position(|t| t.name == n))
            .unwrap_or(0);
        Ok(Self {
            settings,
            plugins,
            gskills,
            pstate,
            skstate,
            gstate,
            focus: Focus::Plugins,
            theme,
            msg: "tab focus · j/k scroll · space toggle · n new · d delete · t theme · r reload · q quit".into(),
            popup: None,
            new_form: None,
            confirm_delete: None,
            quit: false,
        })
    }

    fn reload(&mut self) {
        if let Ok(s) = load_settings() {
            self.settings = s;
            self.plugins = load_plugins(&self.settings);
        }
        self.gskills = load_global_skills();
        self.msg = "reloaded".into();
    }

    fn sel_plugin_skills_len(&self) -> usize {
        self.pstate.selected().and_then(|i| self.plugins.get(i)).map(|p| p.skills.len()).unwrap_or(0)
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Plugins => Focus::PluginSkills,
            Focus::PluginSkills => Focus::GlobalSkills,
            Focus::GlobalSkills => Focus::Plugins,
        };
    }

    fn mv(&mut self, d: i64) {
        match self.focus {
            Focus::Plugins => {
                if self.plugins.is_empty() {
                    return;
                }
                let cur = self.pstate.selected().unwrap_or(0) as i64;
                self.pstate.select(Some((cur + d).clamp(0, self.plugins.len() as i64 - 1) as usize));
                self.skstate.select(Some(0)); // new plugin -> reset skills scroll
            }
            Focus::PluginSkills => {
                let len = self.sel_plugin_skills_len();
                if len == 0 {
                    return;
                }
                let cur = self.skstate.selected().unwrap_or(0) as i64;
                self.skstate.select(Some((cur + d).clamp(0, len as i64 - 1) as usize));
            }
            Focus::GlobalSkills => {
                if self.gskills.is_empty() {
                    return;
                }
                let cur = self.gstate.selected().unwrap_or(0) as i64;
                self.gstate.select(Some((cur + d).clamp(0, self.gskills.len() as i64 - 1) as usize));
            }
        }
    }

    fn toggle(&mut self) {
        match self.focus {
            Focus::Plugins => self.toggle_plugin(),
            Focus::GlobalSkills => self.toggle_skill(),
            Focus::PluginSkills => self.msg = "plugin skills are read-only (toggle the plugin)".into(),
        }
    }

    fn toggle_plugin(&mut self) {
        let Some(i) = self.pstate.selected() else { return };
        let Some(p) = self.plugins.get_mut(i) else { return };
        let newval = !p.enabled;
        if let Some(obj) = self.settings.get_mut("enabledPlugins").and_then(|e| e.as_object_mut()) {
            obj.insert(p.key.clone(), Value::Bool(newval));
        }
        match save_settings(&self.settings) {
            Ok(_) => {
                p.enabled = newval;
                self.msg = format!("{} {} ({} skills)", if newval { "installed" } else { "uninstalled" }, p.name, p.skills.len());
            }
            Err(e) => self.msg = format!("save failed: {e}"),
        }
    }

    fn toggle_skill(&mut self) {
        let Some(i) = self.gstate.selected() else { return };
        let (name, archived) = match self.gskills.get(i) {
            Some(g) => (g.name.clone(), g.archived),
            None => return,
        };
        let target = !archived;
        if target && self.skill_in_use(&name) {
            self.popup = Some(format!("{name} cannot be archived because it is in use."));
            return;
        }
        match set_archived(&name, target) {
            Ok(_) => {
                self.gskills = load_global_skills();
                self.msg = format!("{} {}", if target { "archived" } else { "unarchived" }, name);
            }
            Err(e) => self.msg = format!("{} failed: {e}", if target { "archive" } else { "unarchive" }),
        }
    }

    /// "In use" = the skill belongs to any plugin's skill collection (enabled or not).
    fn skill_in_use(&self, name: &str) -> bool {
        self.plugins.iter().any(|p| p.skills.iter().any(|s| s == name))
    }

    fn submit_new_plugin(&mut self) {
        let (name, desc) = match self.new_form.as_ref() {
            Some(f) => (f.name.trim().to_string(), f.desc.trim().to_string()),
            None => return,
        };
        if name.is_empty() {
            if let Some(f) = self.new_form.as_mut() {
                f.err = "name required".into();
            }
            return;
        }
        match create_plugin_files(&name, &desc) {
            Ok(_) => {
                if let Some(obj) = self.settings.get_mut("enabledPlugins").and_then(|e| e.as_object_mut()) {
                    obj.insert(format!("{name}@skill-library"), Value::Bool(false));
                }
                let _ = save_settings(&self.settings);
                self.new_form = None;
                self.reload();
                self.msg = format!("created plugin {name} (disabled)");
            }
            Err(e) => {
                if let Some(f) = self.new_form.as_mut() {
                    f.err = e;
                }
            }
        }
    }

    fn delete_confirmed(&mut self) {
        let Some((key, name)) = self.confirm_delete.take() else { return };
        match delete_plugin_files(&name) {
            Ok(_) => {
                if let Some(obj) = self.settings.get_mut("enabledPlugins").and_then(|e| e.as_object_mut()) {
                    obj.remove(&key);
                }
                let _ = save_settings(&self.settings);
                self.reload();
                self.msg = format!("deleted plugin {name}");
            }
            Err(e) => self.popup = Some(format!("delete failed: {e}")),
        }
    }

    fn enabled_skills(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .plugins
            .iter()
            .filter(|p| p.enabled)
            .flat_map(|p| p.skills.iter().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let th = &themes()[app.theme];
    let base = Style::new().fg(th.fg).bg(th.bg);
    f.render_widget(Block::default().style(base), f.area());

    let outer = Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).split(f.area());

    let in_plugins = matches!(app.focus, Focus::Plugins | Focus::PluginSkills);
    let (pt, st) = if in_plugins {
        (Style::new().fg(th.bg).bg(th.accent).bold(), Style::new().fg(th.off))
    } else {
        (Style::new().fg(th.off), Style::new().fg(th.bg).bg(th.accent).bold())
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Plugins ", pt),
            Span::raw("  "),
            Span::styled(" Global skills ", st),
            Span::styled("   (Tab cycles panes)", Style::new().fg(th.off)),
        ]))
        .style(base),
        outer[0],
    );

    if in_plugins {
        plugins_view(f, app, th, outer[1]);
    } else {
        skills_view(f, app, th, outer[1]);
    }

    let help = Line::from(vec![
        Span::styled(format!(" [{}] ", themes()[app.theme].name), Style::new().fg(th.accent).bold()),
        Span::styled(app.msg.clone(), Style::new().fg(th.fg)),
    ]);
    f.render_widget(Paragraph::new(help).style(base), outer[2]);

    if let Some(m) = &app.popup {
        let area = centered(f.area(), 64, 7);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled(format!("  {m}"), Style::new().fg(th.fg))),
                Line::raw(""),
                Line::from(Span::styled("  press any key to dismiss", Style::new().fg(th.off))),
            ])
            .block(Block::default().borders(Borders::ALL).title(" notice ").border_style(Style::new().fg(th.accent))),
            area,
        );
    }

    if let Some((_, name)) = &app.confirm_delete {
        let area = centered(f.area(), 68, 9);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(vec![
                    Span::raw("  Delete plugin "),
                    Span::styled(name.clone(), Style::new().fg(th.accent).bold()),
                    Span::raw(" ?"),
                ]),
                Line::from(Span::styled(
                    format!("  removes ~/skill-library/plugins/{name} and its skills"),
                    Style::new().fg(th.off),
                )),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("      "),
                    Span::styled("y", Style::new().fg(th.on).bold()),
                    Span::raw(" = yes    "),
                    Span::styled("n", Style::new().fg(th.off).bold()),
                    Span::raw("/Esc = no"),
                ]),
            ])
            .block(Block::default().borders(Borders::ALL).title(" confirm delete ").border_style(Style::new().fg(Color::Red))),
            area,
        );
    }

    if let Some(fm) = &app.new_form {
        let area = centered(f.area(), 68, 11);
        f.render_widget(Clear, area);
        let mut lines = vec![Line::raw("")];
        let fields = [("Name (lowercase, hyphens)", &fm.name), ("Description", &fm.desc)];
        for (i, (label, val)) in fields.iter().enumerate() {
            let m = if fm.focus == i { "› " } else { "  " };
            let vs = if fm.focus == i { Style::new().reversed() } else { Style::new() };
            lines.push(Line::from(vec![
                Span::raw(m),
                Span::styled(format!("{:<28}", label), Style::new().fg(th.accent)),
                Span::styled((*val).clone(), vs),
            ]));
        }
        lines.push(Line::raw(""));
        if !fm.err.is_empty() {
            lines.push(Line::from(Span::styled(format!("  {}", fm.err), Style::new().fg(Color::Red))));
        }
        lines.push(Line::from(Span::styled("  Enter create · Tab/↑↓ move · Esc cancel", Style::new().fg(th.off))));
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" new plugin ").border_style(Style::new().fg(th.accent))),
            area,
        );
    }
}

fn plugins_view(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(8), Constraint::Min(5), Constraint::Min(5)]).split(area);

    let top = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).split(rows[0]);
    let en_plugs: Vec<ListItem> = app.plugins.iter().filter(|p| p.enabled).map(|p| ListItem::new(p.name.clone()).style(Style::new().fg(th.on).bold())).collect();
    f.render_widget(List::new(en_plugs).block(panel(th, " enabled plugins ", th.accent)), top[0]);
    let es = app.enabled_skills();
    let en_skills: Vec<ListItem> = es.iter().map(|s| ListItem::new(s.clone()).style(Style::new().fg(th.on))).collect();
    f.render_widget(List::new(en_skills).block(panel(th, &format!(" enabled skills ({}) ", es.len()), th.accent)), top[1]);

    // plugins list
    let pfocus = app.focus == Focus::Plugins;
    let pitems: Vec<ListItem> = app.plugins.iter().map(|p| {
        let mark = if p.enabled { "▣ " } else { "☐ " };
        let style = if p.enabled { Style::new().fg(th.on).bold() } else { Style::new().fg(th.off) };
        ListItem::new(Line::from(vec![
            Span::styled(mark, style),
            Span::styled(format!("{:<20}", p.name), style),
            Span::styled(format!("[{:>2}] ", p.skills.len()), Style::new().fg(th.accent)),
            Span::styled(truncate(&p.desc, 42), Style::new().fg(th.off)),
        ]))
    }).collect();
    f.render_stateful_widget(
        List::new(pitems)
            .block(panel(th, " plugins  (space = install/uninstall) ", if pfocus { th.accent } else { th.off }))
            .highlight_style(if pfocus { Style::new().bg(th.accent).fg(th.bg) } else { Style::new() })
            .highlight_symbol(if pfocus { "› " } else { "  " }),
        rows[1],
        &mut app.pstate,
    );

    // skills-in-selected-plugin list (scrollable, stateful)
    let skfocus = app.focus == Focus::PluginSkills;
    let (title, items): (String, Vec<ListItem>) = match app.pstate.selected().and_then(|i| app.plugins.get(i)) {
        Some(p) => {
            let items = if p.skills.is_empty() {
                vec![ListItem::new("(no skills found for this plugin)").style(Style::new().fg(th.off))]
            } else {
                p.skills.iter().map(|s| {
                    ListItem::new(Line::from(vec![
                        Span::styled("• ", Style::new().fg(if p.enabled { th.on } else { th.off })),
                        Span::styled(s.clone(), Style::new().fg(if p.enabled { th.fg } else { th.off })),
                    ]))
                }).collect()
            };
            (format!(" skills in {} ({}) ", p.name, p.skills.len()), items)
        }
        None => (" skills ".to_string(), vec![]),
    };
    // keep skstate in range for the current plugin
    let len = app.sel_plugin_skills_len();
    if len == 0 {
        app.skstate.select(None);
    } else if app.skstate.selected().map(|i| i >= len).unwrap_or(true) {
        app.skstate.select(Some(len - 1));
    }
    f.render_stateful_widget(
        List::new(items)
            .block(panel(th, &title, if skfocus { th.accent } else { th.off }))
            .highlight_style(if skfocus { Style::new().bg(th.accent).fg(th.bg) } else { Style::new() })
            .highlight_symbol(if skfocus { "› " } else { "  " }),
        rows[2],
        &mut app.skstate,
    );
}

fn skills_view(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let active = app.gskills.iter().filter(|g| !g.archived).count();
    let archived = app.gskills.len() - active;
    let items: Vec<ListItem> = app.gskills.iter().map(|g| {
        let (tag, style) = if g.archived { ("[archived] ", Style::new().fg(th.off)) } else { ("[active]   ", Style::new().fg(th.on)) };
        ListItem::new(Line::from(vec![
            Span::styled(tag, style),
            Span::styled(g.name.clone(), Style::new().fg(if g.archived { th.off } else { th.fg })),
        ]))
    }).collect();
    f.render_stateful_widget(
        List::new(items)
            .block(panel(th, &format!(" global skills — {active} active · {archived} archived   (space = archive/unarchive) "), th.accent))
            .highlight_style(Style::new().bg(th.accent).fg(th.bg))
            .highlight_symbol("› "),
        area,
        &mut app.gstate,
    );
}

fn panel(th: &Theme, title: &str, border: Color) -> Block<'static> {
    Block::default().borders(Borders::ALL).title(title.to_string()).border_style(Style::new().fg(border)).title_style(Style::new().fg(th.accent).bold())
}
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>()) }
}
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn main() -> Result<()> {
    let mut app = App::new()?;
    let mut terminal = ratatui::init();
    let res = run(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let nthemes = themes().len();
    while !app.quit {
        terminal.draw(|f| ui(f, app))?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(k) = event::read()? else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        if app.popup.is_some() {
            app.popup = None;
            continue;
        }
        if app.confirm_delete.is_some() {
            match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => app.delete_confirmed(),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    app.confirm_delete = None;
                    app.msg = "cancelled".into();
                }
                _ => {}
            }
            continue;
        }
        if app.new_form.is_some() {
            match k.code {
                KeyCode::Esc => app.new_form = None,
                KeyCode::Enter => app.submit_new_plugin(),
                _ => {
                    if let Some(fm) = app.new_form.as_mut() {
                        match k.code {
                            KeyCode::Tab | KeyCode::Down | KeyCode::BackTab | KeyCode::Up => {
                                fm.focus = (fm.focus + 1) % 2
                            }
                            KeyCode::Backspace => {
                                if fm.focus == 0 { fm.name.pop(); } else { fm.desc.pop(); }
                            }
                            KeyCode::Char(c) => {
                                if fm.focus == 0 { fm.name.push(c); } else { fm.desc.push(c); }
                            }
                            _ => {}
                        }
                    }
                }
            }
            continue;
        }
        match k.code {
            KeyCode::Char('q') => app.quit = true,
            KeyCode::Tab => app.cycle_focus(),
            KeyCode::Char('j') | KeyCode::Down => app.mv(1),
            KeyCode::Char('k') | KeyCode::Up => app.mv(-1),
            KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('a') => app.toggle(),
            KeyCode::Char('t') => {
                app.theme = (app.theme + 1) % nthemes;
                save_theme_name(themes()[app.theme].name);
            }
            KeyCode::Char('r') => app.reload(),
            KeyCode::Char('n') => {
                if app.focus == Focus::Plugins {
                    app.new_form = Some(PlugForm::default());
                }
            }
            KeyCode::Char('d') => {
                if app.focus == Focus::Plugins {
                    let sel = app
                        .pstate
                        .selected()
                        .and_then(|i| app.plugins.get(i))
                        .map(|p| (p.key.clone(), p.name.clone()));
                    if let Some((key, name)) = sel {
                        if key.ends_with("@skill-library") {
                            app.confirm_delete = Some((key, name));
                        } else {
                            app.popup = Some("only skill-library plugins can be deleted here".into());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
