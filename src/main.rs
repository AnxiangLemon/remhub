use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
};
use serde::{Deserialize, Serialize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const APP_NAME: &str = "remhub";
const APP_TITLE: &str = "RDP & SSH Launcher";
const DEFAULT_CONFIG_FILE: &str = "servers.toml";
const MAX_RECENT: usize = 5;
const UNGROUPED_LABEL: &str = "(default)";

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    servers: Vec<Server>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Defaults {
    #[serde(default = "default_rdp_command")]
    rdp_command: String,
    #[serde(default = "default_ssh_command")]
    ssh_command: String,
    #[serde(default)]
    rdp_extra_args: Vec<String>,
    #[serde(default)]
    ssh_extra_args: Vec<String>,
    /// When true, SSH sessions open in a new terminal window instead of taking over this one.
    #[serde(default = "default_ssh_new_window")]
    ssh_new_window: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Server {
    name: String,
    host: String,
    #[serde(default)]
    group: String,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    private_key_path: Option<PathBuf>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    rdp_file: Option<PathBuf>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Protocol {
    #[default]
    Rdp,
    Ssh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Browse,
    Search,
    Help,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentStore {
    #[serde(default)]
    by_config: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
struct App {
    config: Config,
    config_path: PathBuf,
    config_key: String,
    filtered: Vec<usize>,
    selected: usize,
    search: String,
    group_filter: Option<String>,
    groups: Vec<String>,
    recent_names: Vec<String>,
    recent_set: HashSet<String>,
    recent_store_path: PathBuf,
    today_iso: String,
    filtered_rdp: usize,
    filtered_ssh: usize,
    mode: Mode,
    message: String,
    ignore_enter_until: Instant,
    should_quit: bool,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            rdp_command: default_rdp_command(),
            ssh_command: default_ssh_command(),
            rdp_extra_args: Vec::new(),
            ssh_extra_args: Vec::new(),
            ssh_new_window: default_ssh_new_window(),
        }
    }
}

fn default_ssh_new_window() -> bool {
    cfg!(windows)
}

fn default_rdp_command() -> String {
    "mstsc".to_string()
}

fn default_ssh_command() -> String {
    "ssh".to_string()
}

fn main() -> Result<()> {
    let config_path = resolve_config_path();
    ensure_sample_config(&config_path)?;
    let config = load_config(&config_path)?;
    let mut app = App::new(config, config_path);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    drain_pending_events()?;

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| draw(frame, app))?;

        // Block until input arrives instead of redrawing in a busy loop.
        match event::read()? {
            Event::Key(key) => {
                handle_key(terminal, app, key)?;
                // When holding arrow keys, process queued repeats in one frame.
                if is_navigation_key(&key) {
                    while event::poll(Duration::from_millis(0))? {
                        if let Event::Key(next) = event::read()? {
                            handle_key(terminal, app, next)?;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn is_navigation_key(key: &KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    matches!(
        key.code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Char('j')
            | KeyCode::Char('k')
    )
}

impl App {
    fn new(config: Config, config_path: PathBuf) -> Self {
        let config_key = config_path.to_string_lossy().into_owned();
        let recent_store_path = recent_store_path();
        let recent_names =
            load_recent_names(&recent_store_path, &config_key).unwrap_or_default();
        let recent_set: HashSet<String> = recent_names.iter().cloned().collect();
        let groups = collect_groups(&config.servers);
        let today_iso = today_iso_date().unwrap_or_default();
        let mut app = Self {
            config,
            config_path,
            config_key,
            filtered: Vec::new(),
            selected: 0,
            search: String::new(),
            group_filter: None,
            groups,
            recent_names,
            recent_set,
            recent_store_path,
            today_iso,
            filtered_rdp: 0,
            filtered_ssh: 0,
            mode: Mode::Browse,
            message: String::new(),
            ignore_enter_until: Instant::now() + Duration::from_millis(700),
            should_quit: false,
        };
        app.refresh_filter();
        app.message = format!(
            "Loaded {} server(s). Press h for help.",
            app.config.servers.len()
        );
        app
    }

    fn rebuild_groups(&mut self) {
        self.groups = collect_groups(&self.config.servers);
        if let Some(filter) = &self.group_filter {
            if !self.groups.iter().any(|group| group == filter) {
                self.group_filter = None;
            }
        }
    }

    fn cycle_group_filter(&mut self) {
        self.group_filter = match &self.group_filter {
            None => self.groups.first().cloned(),
            Some(current) => {
                let pos = self.groups.iter().position(|group| group == current);
                match pos {
                    Some(index) if index + 1 < self.groups.len() => {
                        Some(self.groups[index + 1].clone())
                    }
                    _ => None,
                }
            }
        };
        self.refresh_filter();
        self.message = match &self.group_filter {
            Some(group) => format!("Group filter: {group}"),
            None => "Group filter: all".to_string(),
        };
    }

    fn group_filter_label(&self) -> String {
        match &self.group_filter {
            Some(group) => group.clone(),
            None => "all".to_string(),
        }
    }

    fn record_recent(&mut self, server_name: &str) {
        self.recent_names.retain(|name| name != server_name);
        self.recent_names.insert(0, server_name.to_string());
        self.recent_names.truncate(MAX_RECENT);
        self.recent_set = self.recent_names.iter().cloned().collect();
        if let Err(err) = save_recent_names(
            &self.recent_store_path,
            &self.config_key,
            &self.recent_names,
        ) {
            self.message = format!("Could not save recent list: {err:#}");
        }
        self.refresh_filter();
    }

    fn is_recent(&self, server: &Server) -> bool {
        self.recent_set.contains(&server.name)
    }

    fn refresh_filter(&mut self) {
        let needle = self.search.to_lowercase();
        let mut matching: Vec<usize> = self
            .config
            .servers
            .iter()
            .enumerate()
            .filter_map(|(idx, server)| {
                if !matches_group_filter(server, self.group_filter.as_deref()) {
                    return None;
                }
                let haystack = format!(
                    "{} {} {} {} {} {}",
                    server.name,
                    server.host,
                    server.group,
                    server.protocol.label(),
                    server_expires_at(server).unwrap_or_default(),
                    server.tags.join(" ")
                )
                .to_lowercase();
                (needle.is_empty() || haystack.contains(&needle)).then_some(idx)
            })
            .collect();

        let recent_rank: HashMap<&str, usize> = self
            .recent_names
            .iter()
            .enumerate()
            .map(|(rank, name)| (name.as_str(), rank))
            .collect();
        matching.sort_by(|left, right| {
            let left_name = self.config.servers[*left].name.as_str();
            let right_name = self.config.servers[*right].name.as_str();
            let left_rank = recent_rank.get(left_name).copied();
            let right_rank = recent_rank.get(right_name).copied();
            match (left_rank, right_rank) {
                (Some(l), Some(r)) => l.cmp(&r),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.cmp(right),
            }
        });

        self.filtered = matching;
        self.filtered_rdp = 0;
        self.filtered_ssh = 0;
        for idx in &self.filtered {
            match self.config.servers[*idx].protocol {
                Protocol::Rdp => self.filtered_rdp += 1,
                Protocol::Ssh => self.filtered_ssh += 1,
            }
        }

        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn selected_server(&self) -> Option<&Server> {
        self.filtered
            .get(self.selected)
            .and_then(|idx| self.config.servers.get(*idx))
    }

    fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn page_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 10).min(self.filtered.len() - 1);
        }
    }

    fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    fn reload(&mut self) {
        match load_config(&self.config_path) {
            Ok(config) => {
                self.config = config;
                self.rebuild_groups();
                self.refresh_filter();
                self.message = format!("Reloaded {}", self.config_path.display());
            }
            Err(err) => self.message = format!("Reload failed: {err:#}"),
        }
    }

    fn copy_selected_command(&mut self) {
        let Some(server) = self.selected_server().cloned() else {
            self.message = "No server selected.".to_string();
            return;
        };
        let command = connection_string(&server, &self.config.defaults);
        match copy_to_clipboard(&command) {
            Ok(()) => self.message = format!("Copied: {command}"),
            Err(err) => self.message = format!("Copy failed: {err:#}"),
        }
    }
}

impl Protocol {
    fn label(self) -> &'static str {
        match self {
            Protocol::Rdp => "RDP",
            Protocol::Ssh => "SSH",
        }
    }

    fn color(self) -> Color {
        match self {
            Protocol::Rdp => Color::Cyan,
            Protocol::Ssh => Color::Green,
        }
    }
}

fn handle_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<()> {
    if key.kind == KeyEventKind::Release {
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return Ok(());
    }

    match app.mode {
        Mode::Search => handle_search_key(app, key),
        Mode::Help => {
            app.mode = Mode::Browse;
            Ok(())
        }
        Mode::Browse => handle_browse_key(terminal, app, key),
    }
}

fn handle_browse_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('h') | KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.message = "Type to filter servers. Enter confirms, Esc clears focus.".to_string();
        }
        KeyCode::Char('r') => app.reload(),
        KeyCode::Char('g') => app.cycle_group_filter(),
        KeyCode::Char('c') => app.copy_selected_command(),
        KeyCode::Char(c @ '1'..='9') => {
            launch_at(terminal, app, (c as u8 - b'1') as usize)?
        }
        KeyCode::Enter if Instant::now() >= app.ignore_enter_until => {
            launch_selected(terminal, app)?
        }
        KeyCode::Enter => {
            app.message = "Ready. Press Enter again to connect.".to_string();
        }
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::PageUp => app.page_up(),
        KeyCode::Home => app.selected = 0,
        KeyCode::End => app.selected = app.filtered.len().saturating_sub(1),
        _ => {}
    }

    Ok(())
}

fn launch_at(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    visible_index: usize,
) -> Result<()> {
    if visible_index >= app.filtered.len() {
        app.message = format!("No server at shortcut {}.", visible_index + 1);
        return Ok(());
    }
    app.selected = visible_index;
    launch_selected(terminal, app)
}

fn launch_selected(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(server) = app.selected_server().cloned() else {
        app.message = "No server selected.".to_string();
        return Ok(());
    };

    match launch_server(terminal, &server, &app.config.defaults) {
        Ok(summary) => {
            app.record_recent(&server.name);
            app.message = summary;
        }
        Err(err) => app.message = format!("Launch failed: {err:#}"),
    }

    Ok(())
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Browse;
            app.message = "Search kept. Press / to edit or Backspace to clear text.".to_string();
        }
        KeyCode::Enter => app.mode = Mode::Browse,
        KeyCode::Backspace => {
            app.search.pop();
            app.refresh_filter();
        }
        KeyCode::Char(ch) => {
            app.search.push(ch);
            app.refresh_filter();
        }
        KeyCode::Down => app.move_down(),
        KeyCode::Up => app.move_up(),
        _ => {}
    }

    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, layout[0]);
    draw_body(frame, app, layout[1]);
    draw_footer(frame, app, layout[2]);

    match app.mode {
        Mode::Help => draw_help(frame, centered_rect(72, 68, area)),
        _ => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            format!(" {APP_NAME} "),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(format!(" {APP_TITLE}")),
    ]);
    let config_label = truncate_config_path(&app.config_path, 28);
    let right = format!(
        "{} shown · {} RDP · {} SSH · group:{}  |  {}",
        app.filtered.len(),
        app.filtered_rdp,
        app.filtered_ssh,
        app.group_filter_label(),
        config_label
    );

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(right.len() as u16 + 2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        ),
        chunks[1],
    );
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);

    draw_server_list(frame, app, chunks[0]);
    draw_side_panel(frame, app, chunks[1]);
}

fn draw_server_list(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);

    let header = Row::new(vec![
        Cell::from("#").style(column_header_style()),
        Cell::from("Type").style(column_header_style()),
        Cell::from("Name").style(column_header_style()),
        Cell::from("Address").style(column_header_style()),
        Cell::from("Expires").style(column_header_style()),
        Cell::from("Group").style(column_header_style()),
    ])
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .enumerate()
        .filter_map(|(visible_idx, idx)| {
            let server = app.config.servers.get(*idx)?;
            let endpoint = match server.port {
                Some(port) => format!("{}:{}", server.host, port),
                None => server.host.clone(),
            };
            let group = group_label(&server.group);
            let expires = server_expires_at(server).unwrap_or_else(|| "-".to_string());
            let shortcut = if visible_idx < 9 {
                format!("{}", visible_idx + 1)
            } else {
                if app.is_recent(server) {
                    "★".to_string()
                } else {
                    " ".to_string()
                }
            };
            let shortcut_style = if visible_idx < 9 {
                Style::default().fg(Color::Black).bg(Color::Magenta)
            } else if app.is_recent(server) {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Some(Row::new(vec![
                Cell::from(Span::styled(format!(" {shortcut} "), shortcut_style)),
                Cell::from(Span::styled(
                    format!(" {:<4} ", server.protocol.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(server.protocol.color()),
                )),
                Cell::from(Span::styled(
                    pad_visual(&server.name, 22),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    pad_visual(&endpoint, 24),
                    Style::default().fg(Color::Gray),
                )),
                Cell::from(Span::styled(
                    pad_visual(&expires, 12),
                    Style::default().fg(expiry_color(&expires, &app.today_iso)),
                )),
                Cell::from(Span::styled(
                    pad_visual(&group, 12),
                    Style::default().fg(Color::Yellow),
                )),
            ]))
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Length(24),
        Constraint::Min(18),
        Constraint::Length(12),
        Constraint::Length(14),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(
                    " Servers ({}/{}) ",
                    app.filtered.len(),
                    app.config.servers.len()
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = TableState::default();
    if !app.filtered.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);

    let content_height = app.filtered.len().saturating_sub(1);
    if content_height > 0 {
        let mut scrollbar = ScrollbarState::new(content_height).position(app.selected);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar,
        );
    }
}

fn draw_side_panel(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let selected = app.selected_server();
    let lines = if let Some(server) = selected {
        vec![
            Line::from(vec![
                Span::styled(
                    &server.name,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!(" {:<4} ", server.protocol.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(server.protocol.color()),
                ),
            ]),
            Line::raw(""),
            Line::raw(format!("Host     : {}", server.host)),
            Line::raw(format!(
                "Port     : {}",
                server
                    .port
                    .map(|port| port.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )),
            Line::raw(format!(
                "User     : {}",
                server.user.as_deref().unwrap_or("-")
            )),
            Line::raw(format!(
                "Expires  : {}",
                server_expires_at(server).unwrap_or_else(|| "-".to_string())
            )),
            Line::raw(format!(
                "Password : {}",
                if has_text(server.password.as_deref()) {
                    "saved"
                } else {
                    "-"
                }
            )),
            Line::raw(format!(
                "SSH key  : {}",
                if server.private_key_path.is_some() || has_text(server.private_key.as_deref()) {
                    "saved"
                } else {
                    "-"
                }
            )),
            Line::raw(format!("Group    : {}", group_label(&server.group))),
            Line::raw(format!(
                "Recent   : {}",
                if app.is_recent(server) { "yes" } else { "no" }
            )),
            Line::raw(format!(
                "Command  : {}",
                truncate_visual(&connection_string(server, &app.config.defaults), 42)
            )),
            Line::raw(format!(
                "Tags     : {}",
                if server.tags.is_empty() {
                    "-".to_string()
                } else {
                    server.tags.join(", ")
                }
            )),
            Line::raw(""),
            Line::raw(server.note.as_deref().unwrap_or("No note.")),
        ]
    } else {
        vec![Line::raw("No matching server.")]
    };

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let search_style = if app.mode == Mode::Search {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let search = if app.search.is_empty() {
        "/ filter".to_string()
    } else {
        format!("/ {}", app.search)
    };

    let group_style = if app.group_filter.is_some() {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let group = format!("g:{}", app.group_filter_label());

    let shortcuts = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default().fg(Color::Black).bg(Color::Green),
        ),
        Span::raw(" connect  "),
        Span::styled(" 1-9 ", Style::default().fg(Color::Black).bg(Color::Magenta)),
        Span::raw(" quick  "),
        Span::styled(" c ", Style::default().fg(Color::Black).bg(Color::Blue)),
        Span::raw(" copy  "),
        Span::styled(format!(" {group} "), group_style),
        Span::raw(" group  "),
        Span::styled(" h ", Style::default().fg(Color::Black).bg(Color::White)),
        Span::raw(" help  "),
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Red)),
        Span::raw(" quit"),
    ]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {search} "), search_style),
            Span::raw("  "),
            Span::styled(&app.message, message_style(&app.message)),
        ]))
        .block(Block::default().borders(Borders::TOP)),
        chunks[0],
    );
    frame.render_widget(Paragraph::new(shortcuts), chunks[1]);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
    let help = vec![
        Line::styled(
            format!("{APP_NAME} help"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Enter       Connect to selected server"),
        Line::raw("1-9         Quick connect to visible servers 1 through 9"),
        Line::raw("c           Copy connection command to clipboard"),
        Line::raw("g           Cycle group filter (all -> group1 -> ...)"),
        Line::raw("/           Search by name, host, group, protocol, or tags"),
        Line::raw("h           This help panel"),
        Line::raw("q / Esc     Quit"),
        Line::raw("Up/Down     Move selection"),
        Line::raw("j / k       Move selection (vim-style)"),
        Line::raw("PageUp/Down Jump 10 rows"),
        Line::raw("Home / End  Jump to first / last server"),
        Line::raw("r           Reload servers.toml"),
        Line::raw(""),
        Line::raw("Recent connections (last 5) are pinned to the top on startup."),
        Line::raw("RDP uses mstsc by default. SSH uses ssh by default."),
        Line::raw("On Windows, SSH opens in a new terminal window by default."),
        Line::raw("Set defaults.ssh_new_window = false to connect in this window."),
        Line::raw("Press any key to close this panel."),
    ];
    frame.render_widget(
        Paragraph::new(help).block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn launch_server(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    server: &Server,
    defaults: &Defaults,
) -> Result<String> {
    match server.protocol {
        Protocol::Rdp => launch_rdp(server, defaults),
        Protocol::Ssh if defaults.ssh_new_window => launch_ssh_new_window(server, defaults),
        Protocol::Ssh => launch_ssh_foreground(terminal, server, defaults),
    }
}

fn launch_rdp(server: &Server, defaults: &Defaults) -> Result<String> {
    if has_text(server.password.as_deref()) && has_text(server.user.as_deref()) {
        save_rdp_credential(server)?;
    }

    let mut command = Command::new(&defaults.rdp_command);
    if let Some(rdp_file) = &server.rdp_file {
        command.arg(rdp_file);
    } else {
        command.arg(format!(
            "/v:{}",
            endpoint_for_protocol(server, Protocol::Rdp)
        ));
        if has_admin_tag(server) {
            command.arg("/admin");
        }
    }
    command.args(&defaults.rdp_extra_args);
    command
        .spawn()
        .with_context(|| format!("could not start {}", defaults.rdp_command))?;

    let credential_note = if has_text(server.password.as_deref()) {
        " with saved credential"
    } else {
        ""
    };
    Ok(format!(
        "Started RDP session for {}{}",
        server.name, credential_note
    ))
}

fn build_ssh_command(server: &Server, defaults: &Defaults) -> Result<Command> {
    let mut command = Command::new(&defaults.ssh_command);
    if let Some(port) = server.port {
        command.args(["-p", &port.to_string()]);
    }
    let inline_key_path = materialize_inline_private_key(server)?;
    if let Some(private_key_path) = server
        .private_key_path
        .as_ref()
        .or(inline_key_path.as_ref())
    {
        command.args(["-i", private_key_path.to_string_lossy().as_ref()]);
    }
    command.args(&defaults.ssh_extra_args);
    command.arg(endpoint_for_protocol(server, Protocol::Ssh));
    Ok(command)
}

fn launch_ssh_new_window(server: &Server, defaults: &Defaults) -> Result<String> {
    let ssh = build_ssh_command(server, defaults)?;
    let program = ssh.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = ssh
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let window_title = format!("SSH - {}", server.name);

    let spawned = if windows_terminal_available() {
        let mut command = Command::new("wt");
        command.arg("new-tab").arg("--title").arg(&window_title).arg("--");
        command.arg(&program);
        command.args(&args);
        command
            .spawn()
            .with_context(|| format!("could not start {} in Windows Terminal", defaults.ssh_command))
    } else {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg(window_title).arg(program);
        command.args(args);
        command
            .spawn()
            .with_context(|| format!("could not start {} in a new window", defaults.ssh_command))
    };

    spawned?;

    Ok(format!(
        "Started SSH session for {} in a new window",
        server.name
    ))
}

fn windows_terminal_available() -> bool {
    Command::new("where")
        .arg("wt")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn launch_ssh_foreground(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    server: &Server,
    defaults: &Defaults,
) -> Result<String> {
    let mut command = build_ssh_command(server, defaults)?;
    suspend_terminal(terminal, || {
        command
            .status()
            .with_context(|| format!("could not start {}", defaults.ssh_command))
    })?;

    Ok(format!("SSH session ended for {}", server.name))
}

fn save_rdp_credential(server: &Server) -> Result<()> {
    let Some(user) = server
        .user
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };
    let Some(password) = server
        .password
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };

    let user = match server.domain.as_deref().filter(|value| !value.is_empty()) {
        Some(domain) => format!("{domain}\\{user}"),
        None => user.to_string(),
    };

    let host = server.host.trim();
    save_cmdkey_target(&format!("TERMSRV/{host}"), &user, password)?;
    if let Some(port) = server.port {
        save_cmdkey_target(&format!("TERMSRV/{host}:{port}"), &user, password)?;
    }

    Ok(())
}

fn save_cmdkey_target(target: &str, user: &str, password: &str) -> Result<()> {
    let status = Command::new("cmdkey")
        .arg(format!("/generic:{target}"))
        .arg(format!("/user:{user}"))
        .arg(format!("/pass:{password}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("could not run cmdkey for {target}"))?;

    if !status.success() {
        anyhow::bail!("cmdkey failed for {target}");
    }

    Ok(())
}

fn materialize_inline_private_key(server: &Server) -> Result<Option<PathBuf>> {
    let Some(private_key) = server
        .private_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };

    let dir = env::temp_dir().join(format!("{APP_NAME}-keys"));
    fs::create_dir_all(&dir)
        .with_context(|| format!("could not create key directory {}", dir.display()))?;
    let path = dir.join(format!("{}.key", sanitize_file_name(&server.name)));
    fs::write(&path, private_key.replace("\\n", "\n"))
        .with_context(|| format!("could not write SSH private key {}", path.display()))?;
    Ok(Some(path))
}

fn sanitize_file_name(value: &str) -> String {
    let name: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    if name.trim().is_empty() {
        "server".to_string()
    } else {
        name
    }
}

fn has_text(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn server_expires_at(server: &Server) -> Option<String> {
    server
        .expires_at
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .or_else(|| extract_date_from_note(server.note.as_deref()))
}

fn extract_date_from_note(note: Option<&str>) -> Option<String> {
    let note = note?;
    for (index, _) in note.char_indices() {
        let Some(candidate) = note.get(index..index.saturating_add(10)) else {
            continue;
        };
        let chars: Vec<char> = candidate.chars().collect();
        if chars.len() == 10
            && chars[0..4].iter().all(|ch| ch.is_ascii_digit())
            && chars[4] == '-'
            && chars[5..7].iter().all(|ch| ch.is_ascii_digit())
            && chars[7] == '-'
            && chars[8..10].iter().all(|ch| ch.is_ascii_digit())
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn expiry_color(expires: &str, today: &str) -> Color {
    if expires == "-" {
        return Color::DarkGray;
    }
    if today.is_empty() {
        return Color::LightMagenta;
    }
    if expires < today {
        Color::Red
    } else if days_between_iso(today, expires) <= 30 {
        Color::Yellow
    } else {
        Color::LightMagenta
    }
}

fn column_header_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
}

fn message_style(message: &str) -> Style {
    if message.starts_with("Started")
        || message.starts_with("Loaded")
        || message.starts_with("Copied")
        || message.contains("Reloaded")
        || message.starts_with("Ready.")
        || message.starts_with("Group filter:")
    {
        Style::default().fg(Color::Green)
    } else if message.starts_with("Launch failed")
        || message.starts_with("Reload failed")
        || message.starts_with("Copy failed")
    {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn group_label(group: &str) -> String {
    if group.is_empty() {
        UNGROUPED_LABEL.to_string()
    } else {
        group.to_string()
    }
}

fn has_admin_tag(server: &Server) -> bool {
    server
        .tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("admin"))
}

fn collect_groups(servers: &[Server]) -> Vec<String> {
    let mut groups: Vec<String> = servers
        .iter()
        .map(|server| group_label(&server.group))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    groups.sort_by(|left, right| {
        if left == UNGROUPED_LABEL {
            std::cmp::Ordering::Greater
        } else if right == UNGROUPED_LABEL {
            std::cmp::Ordering::Less
        } else {
            left.cmp(right)
        }
    });
    groups
}

fn matches_group_filter(server: &Server, group_filter: Option<&str>) -> bool {
    match group_filter {
        None => true,
        Some(filter) => group_label(&server.group) == filter,
    }
}

fn recent_store_path() -> PathBuf {
    if let Ok(local) = env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join(APP_NAME).join("recent.toml");
    }
    env::temp_dir().join(format!("{APP_NAME}-recent.toml"))
}

fn load_recent_names(store_path: &Path, config_key: &str) -> Result<Vec<String>> {
    if !store_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(store_path)
        .with_context(|| format!("could not read recent file {}", store_path.display()))?;
    let store: RecentStore = toml::from_str(&content)
        .with_context(|| format!("could not parse recent file {}", store_path.display()))?;
    Ok(store
        .by_config
        .get(config_key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_RECENT)
        .collect())
}

fn save_recent_names(store_path: &Path, config_key: &str, names: &[String]) -> Result<()> {
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create recent directory {}", parent.display()))?;
    }

    let mut store = if store_path.exists() {
        let content = fs::read_to_string(store_path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        RecentStore::default()
    };

    store.by_config.insert(
        config_key.to_string(),
        names.iter().take(MAX_RECENT).cloned().collect(),
    );
    let content = toml::to_string_pretty(&store)?;
    fs::write(store_path, content)
        .with_context(|| format!("could not write recent file {}", store_path.display()))?;
    Ok(())
}

fn connection_string(server: &Server, defaults: &Defaults) -> String {
    match server.protocol {
        Protocol::Rdp => {
            if let Some(rdp_file) = &server.rdp_file {
                format!("{} {}", defaults.rdp_command, rdp_file.display())
            } else {
                let mut parts = vec![
                    defaults.rdp_command.clone(),
                    format!("/v:{}", endpoint_for_protocol(server, Protocol::Rdp)),
                ];
                if has_admin_tag(server) {
                    parts.push("/admin".to_string());
                }
                parts.extend(defaults.rdp_extra_args.clone());
                parts.join(" ")
            }
        }
        Protocol::Ssh => {
            let mut parts = vec![defaults.ssh_command.clone()];
            if let Some(port) = server.port.filter(|port| *port != 22) {
                parts.push("-p".to_string());
                parts.push(port.to_string());
            }
            if let Some(private_key_path) = &server.private_key_path {
                parts.push("-i".to_string());
                parts.push(private_key_path.to_string_lossy().into_owned());
            }
            parts.extend(defaults.ssh_extra_args.clone());
            parts.push(endpoint_for_protocol(server, Protocol::Ssh));
            parts.join(" ")
        }
    }
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;

    #[cfg(windows)]
    {
        let mut child = Command::new("cmd")
            .args(["/C", "clip"])
            .stdin(Stdio::piped())
            .spawn()
            .context("could not run clip.exe")?;
        child
            .stdin
            .as_mut()
            .context("clip stdin unavailable")?
            .write_all(text.as_bytes())
            .context("could not write to clip.exe")?;
        child.wait().context("clip.exe did not exit cleanly")?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        for (program, args) in [
            ("pbcopy", Vec::<&str>::new()),
            ("xclip", vec!["-selection", "clipboard"]),
            ("wl-copy", Vec::<&str>::new()),
        ] {
            if Command::new("which")
                .arg(program)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            {
                let mut child = Command::new(program)
                    .args(&args)
                    .stdin(Stdio::piped())
                    .spawn()
                    .with_context(|| format!("could not run {program}"))?;
                child
                    .stdin
                    .as_mut()
                    .context("clipboard stdin unavailable")?
                    .write_all(text.as_bytes())?;
                child.wait()?;
                return Ok(());
            }
        }
        anyhow::bail!("no clipboard tool found (pbcopy, xclip, wl-copy)");
    }
}

fn truncate_config_path(path: &Path, max_width: usize) -> String {
    let full = path.display().to_string();
    if UnicodeWidthStr::width(full.as_str()) <= max_width {
        return full;
    }
    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
        let short = format!("…/{name}");
        if UnicodeWidthStr::width(short.as_str()) <= max_width {
            return short;
        }
    }
    truncate_visual(&full, max_width)
}

fn today_iso_date() -> Option<String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Date -Format yyyy-MM-dd",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (date.len() == 10).then_some(date)
}

fn days_between_iso(from: &str, to: &str) -> i64 {
    let Some((fy, fm, fd)) = parse_iso_date(from) else {
        return i64::MAX;
    };
    let Some((ty, tm, td)) = parse_iso_date(to) else {
        return i64::MAX;
    };
    let from_days = date_to_ordinal(fy, fm, fd);
    let to_days = date_to_ordinal(ty, tm, td);
    to_days - from_days
}

fn parse_iso_date(value: &str) -> Option<(i32, u32, u32)> {
    let value = value.trim();
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return None;
    }
    let year = value[0..4].parse().ok()?;
    let month = value[5..7].parse().ok()?;
    let day = value[8..10].parse().ok()?;
    (month >= 1 && month <= 12 && day >= 1 && day <= 31).then_some((year, month, day))
}

fn date_to_ordinal(year: i32, month: u32, day: u32) -> i64 {
    let mut y = year as i64;
    let m = month as i64;
    let d = day as i64;
    if m <= 2 {
        y -= 1;
    }
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn pad_visual(value: &str, width: usize) -> String {
    let clipped = truncate_visual(value, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(clipped.as_str()));
    format!("{clipped}{}", " ".repeat(padding))
}

fn truncate_visual(value: &str, max_width: usize) -> String {
    let mut output = String::new();
    let mut width = 0;
    for ch in value.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        output.push(ch);
        width += ch_width;
    }
    output
}

fn drain_pending_events() -> Result<()> {
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }
    Ok(())
}

fn suspend_terminal<T>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = action();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    result
}

fn endpoint_for_protocol(server: &Server, protocol: Protocol) -> String {
    match protocol {
        Protocol::Rdp => match server.port {
            Some(port) => format!("{}:{port}", server.host),
            None => server.host.clone(),
        },
        Protocol::Ssh => {
            let host = match &server.user {
                Some(user) if !user.is_empty() => format!("{user}@{}", server.host),
                _ => server.host.clone(),
            };
            host
        }
    }
}

fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn resolve_config_path() -> PathBuf {
    if let Some(path) = env::args().nth(1) {
        return PathBuf::from(path);
    }
    if let Ok(path) = env::var("REMHUB_CONFIG") {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::new();
    if let Some(dir) = exe_dir() {
        candidates.push(dir.join(DEFAULT_CONFIG_FILE));
    }
    if let Ok(cwd) = env::current_dir() {
        let in_cwd = cwd.join(DEFAULT_CONFIG_FILE);
        if !candidates.contains(&in_cwd) {
            candidates.push(in_cwd);
        }
    }

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    default_config_path()
}

fn default_config_path() -> PathBuf {
    if let Ok(cwd) = env::current_dir() {
        if cwd.join("Cargo.toml").exists() {
            return cwd.join(DEFAULT_CONFIG_FILE);
        }
    }
    exe_dir()
        .map(|dir| dir.join(DEFAULT_CONFIG_FILE))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
}

fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("could not read config {}", path.display()))?;
    let mut config: Config = toml::from_str(&content)
        .with_context(|| format!("could not parse config {}", path.display()))?;
    if config.servers.is_empty() {
        config.servers = sample_config().servers;
    }
    Ok(config)
}

fn ensure_sample_config(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create config directory {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(&sample_config())?;
    fs::write(path, content)
        .with_context(|| format!("could not create sample config {}", path.display()))?;
    Ok(())
}

fn sample_config() -> Config {
    Config {
        defaults: Defaults::default(),
        servers: vec![
            Server {
                name: "Windows Jumpbox".to_string(),
                host: "10.0.0.10".to_string(),
                group: "production".to_string(),
                protocol: Protocol::Rdp,
                port: Some(3389),
                user: Some("administrator".to_string()),
                password: None,
                private_key: None,
                private_key_path: None,
                domain: None,
                expires_at: Some("2028-09-02".to_string()),
                note: Some("Example RDP host. Replace it with your real server.".to_string()),
                rdp_file: None,
                tags: vec!["windows".to_string(), "rdp".to_string()],
            },
            Server {
                name: "Linux Bastion".to_string(),
                host: "10.0.0.20".to_string(),
                group: "production".to_string(),
                protocol: Protocol::Ssh,
                port: Some(22),
                user: Some("ops".to_string()),
                password: None,
                private_key: None,
                private_key_path: None,
                domain: None,
                expires_at: None,
                note: Some("Example SSH host.".to_string()),
                rdp_file: None,
                tags: vec!["linux".to_string(), "ssh".to_string()],
            },
        ],
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
