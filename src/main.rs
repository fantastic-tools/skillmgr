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
            msg: "tab: focus · j/k: scroll · space: toggle · t: theme · r: reload · q: quit".into(),
            popup: None,
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
            _ => {}
        }
    }
    Ok(())
}
