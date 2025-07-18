use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};
use std::io;
use std::time::Duration;
use tokio::time::interval;

use crate::solana_rpc::{fetch_vote_account_data, ValidatorVoteData};
use crate::AppState;

pub async fn show_auto_refresh_status_ui(app_state: &AppState) -> Result<()> {
    // Setup terminal with proper configuration
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // Configure backend for better performance
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Clear the terminal and hide cursor for cleaner UI
    terminal.clear()?;
    terminal.hide_cursor()?;

    // Create app state
    let mut app = StatusApp {
        app_state,
        vote_data: Vec::new(),
        catchup_data: Vec::new(),
        should_quit: false,
        is_refreshing: false,
        last_refresh: std::time::Instant::now(),
        last_catchup_refresh: std::time::Instant::now(),
        refresh_count: 0,
        previous_last_slots: Vec::new(),
        increment_times: Vec::new(),
    };

    // Initialize catchup data from startup sync status
    app.initialize_catchup_from_sync_status();

    // Don't wait for initial data fetch - let UI show immediately
    // Vote data will be fetched in the first refresh cycle

    // Create refresh interval
    let mut refresh_interval = interval(Duration::from_secs(5));

    // Run the app
    let res = run_app(&mut terminal, &mut app, &mut refresh_interval).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

struct StatusApp<'a> {
    app_state: &'a AppState,
    vote_data: Vec<Option<ValidatorVoteData>>,
    catchup_data: Vec<(Option<CatchupStatus>, Option<CatchupStatus>)>, // (node0, node1)
    #[allow(dead_code)]
    should_quit: bool,
    is_refreshing: bool,
    last_refresh: std::time::Instant,
    last_catchup_refresh: std::time::Instant,
    refresh_count: u32,
    previous_last_slots: Vec<Option<u64>>, // Track previous last slot for each validator
    increment_times: Vec<Option<std::time::Instant>>, // Track when increment was last updated
}

#[derive(Clone)]
struct CatchupStatus {
    status: String, // "Caught up", "X slots behind", or "ERROR"
}

impl<'a> StatusApp<'a> {
    fn initialize_catchup_from_sync_status(&mut self) {
        self.catchup_data.clear();

        for validator_status in self.app_state.validator_statuses.iter() {
            let mut catchup_statuses = (None, None);

            if validator_status.nodes_with_status.len() >= 2 {
                // Use existing sync status from startup
                if let Some(sync) = &validator_status.nodes_with_status[0].sync_status {
                    catchup_statuses.0 = Some(CatchupStatus {
                        status: if sync.contains("Caught up") || sync.contains("slot:") {
                            "Caught up".to_string()
                        } else if sync == "Unknown" {
                            "Checking...".to_string()
                        } else {
                            sync.clone()
                        },
                    });
                }
                if let Some(sync) = &validator_status.nodes_with_status[1].sync_status {
                    catchup_statuses.1 = Some(CatchupStatus {
                        status: if sync.contains("Caught up") || sync.contains("slot:") {
                            "Caught up".to_string()
                        } else if sync == "Unknown" {
                            "Checking...".to_string()
                        } else {
                            sync.clone()
                        },
                    });
                }
            }

            self.catchup_data.push(catchup_statuses);
        }
    }

    async fn fetch_vote_data_only(&mut self) {
        // Save previous last slots before clearing
        self.previous_last_slots.clear();
        let mut old_slots = Vec::new();
        for vote_data in &self.vote_data {
            if let Some(data) = vote_data {
                if let Some(last_vote) = data.recent_votes.last() {
                    self.previous_last_slots.push(Some(last_vote.slot));
                    old_slots.push(Some(last_vote.slot));
                } else {
                    self.previous_last_slots.push(None);
                    old_slots.push(None);
                }
            } else {
                self.previous_last_slots.push(None);
                old_slots.push(None);
            }
        }

        self.vote_data.clear();

        // Update increment times based on new data
        // Keep existing increment times that are still valid (within 2 seconds)
        let mut new_increment_times = Vec::new();

        for (index, validator_status) in self.app_state.validator_statuses.iter().enumerate() {
            let validator_pair = &validator_status.validator_pair;

            let vote_data =
                fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey)
                    .await
                    .ok();

            // Check if there's a new increment
            if let (Some(ref data), Some(old_slot)) =
                (&vote_data, old_slots.get(index).and_then(|&x| x))
            {
                if let Some(last_vote) = data.recent_votes.last() {
                    if last_vote.slot > old_slot {
                        new_increment_times.push(Some(std::time::Instant::now()));
                    } else {
                        // Keep existing increment time if it exists and is still within 2 seconds
                        let existing = if index < self.increment_times.len() {
                            self.increment_times.get(index).and_then(|&x| x)
                        } else {
                            None
                        };

                        if let Some(time) = existing {
                            if time.elapsed().as_secs() < 2 {
                                new_increment_times.push(Some(time));
                            } else {
                                new_increment_times.push(None);
                            }
                        } else {
                            new_increment_times.push(None);
                        }
                    }
                } else {
                    new_increment_times.push(None);
                }
            } else {
                new_increment_times.push(None);
            }

            self.vote_data.push(vote_data);
        }

        // Replace the increment times with the new ones
        self.increment_times = new_increment_times;
    }

    async fn fetch_catchup_data_only(&mut self) {
        self.catchup_data.clear();

        for validator_status in self.app_state.validator_statuses.iter() {
            let mut catchup_statuses = (None, None);

            if validator_status.nodes_with_status.len() >= 2 {
                // Node 0 catchup
                catchup_statuses.0 = self
                    .fetch_catchup_for_node(&validator_status.nodes_with_status[0])
                    .await;
                // Node 1 catchup
                catchup_statuses.1 = self
                    .fetch_catchup_for_node(&validator_status.nodes_with_status[1])
                    .await;
            }

            self.catchup_data.push(catchup_statuses);
        }
    }

    #[allow(dead_code)]
    async fn fetch_vote_data(&mut self) {
        self.vote_data.clear();
        self.catchup_data.clear();

        for validator_status in self.app_state.validator_statuses.iter() {
            let validator_pair = &validator_status.validator_pair;

            let vote_data =
                fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey)
                    .await
                    .ok();

            self.vote_data.push(vote_data);

            // Fetch catchup status for both nodes
            let mut catchup_statuses = (None, None);

            if validator_status.nodes_with_status.len() >= 2 {
                // Node 0 catchup
                catchup_statuses.0 = self
                    .fetch_catchup_for_node(&validator_status.nodes_with_status[0])
                    .await;
                // Node 1 catchup
                catchup_statuses.1 = self
                    .fetch_catchup_for_node(&validator_status.nodes_with_status[1])
                    .await;
            }

            self.catchup_data.push(catchup_statuses);
        }
    }

    async fn fetch_catchup_for_node(
        &self,
        node: &crate::types::NodeWithStatus,
    ) -> Option<CatchupStatus> {
        // Use detected solana CLI or derive from agave path
        let solana_cli = node
            .solana_cli_executable
            .as_ref()
            .or(node
                .agave_validator_executable
                .as_ref()
                .map(|path| path.replace("agave-validator", "solana"))
                .as_ref())
            .cloned();

        let solana_cli = match solana_cli {
            Some(cli) => cli,
            None => {
                // If no solana CLI available, just use the sync status from startup
                return node.sync_status.as_ref().map(|sync| CatchupStatus {
                    status: if sync.contains("Caught up") || sync.contains("slot:") {
                        "Caught up".to_string()
                    } else if sync == "Unknown" {
                        "N/A".to_string()
                    } else {
                        sync.clone()
                    },
                });
            }
        };

        let catchup_cmd = format!("{} catchup --our-localhost", solana_cli);

        let ssh_key = match self.app_state.detected_ssh_keys.get(&node.node.host) {
            Some(key) => key,
            None => {
                // No SSH key, return sync status from startup
                return node.sync_status.as_ref().map(|sync| CatchupStatus {
                    status: sync.clone(),
                });
            }
        };

        let pool = self.app_state.ssh_pool.clone();

        match pool
            .execute_command_with_early_exit(&node.node, ssh_key, &catchup_cmd, |output| {
                output.contains("0 slot(s)") || output.contains("has caught up")
            })
            .await
        {
            Ok(output) => {
                if output.contains("0 slot(s)") || output.contains("has caught up") {
                    Some(CatchupStatus {
                        status: "Caught up".to_string(),
                    })
                } else if let Some(pos) = output.find(" slot(s) behind") {
                    // Extract number of slots behind
                    let start = output[..pos].rfind(' ').map(|i| i + 1).unwrap_or(0);
                    let slots_str = &output[start..pos];
                    if let Ok(slots) = slots_str.parse::<u64>() {
                        Some(CatchupStatus {
                            status: format!("{} slots behind", slots),
                        })
                    } else {
                        Some(CatchupStatus {
                            status: "Checking...".to_string(),
                        })
                    }
                } else {
                    // Couldn't parse output, use sync status from startup
                    node.sync_status.as_ref().map(|sync| CatchupStatus {
                        status: sync.clone(),
                    })
                }
            }
            Err(_) => {
                // If error, use sync status from startup instead of showing ERROR
                node.sync_status.as_ref().map(|sync| CatchupStatus {
                    status: sync.clone(),
                })
            }
        }
    }
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut StatusApp<'_>,
    refresh_interval: &mut tokio::time::Interval,
) -> Result<()> {
    // Initial draw - show UI immediately
    terminal.draw(|f| ui(f, app))?;

    // Create a separate interval for UI updates (1 second)
    let mut ui_interval = interval(Duration::from_secs(1));
    ui_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Start initial data fetch immediately in background
    app.is_refreshing = true;
    terminal.draw(|f| ui(f, app))?;

    // Fetch initial data
    app.fetch_vote_data_only().await;
    app.last_refresh = std::time::Instant::now();
    app.is_refreshing = false;
    terminal.draw(|f| ui(f, app))?;

    loop {
        // Check for keyboard events synchronously
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        tokio::select! {
            biased;

            // UI update every second (for clock)
            _ = ui_interval.tick() => {
                terminal.draw(|f| ui(f, app))?;
            }

            // Data refresh every 5 seconds
            _ = refresh_interval.tick() => {
                app.is_refreshing = true;
                app.refresh_count += 1;

                // Redraw to show refresh indicator
                terminal.draw(|f| ui(f, app))?;

                // Check if we need to update catchup status
                let should_update_catchup = app.last_catchup_refresh.elapsed().as_secs() >= 30;

                // Fetch data
                app.fetch_vote_data_only().await;

                if should_update_catchup {
                    app.fetch_catchup_data_only().await;
                    app.last_catchup_refresh = std::time::Instant::now();
                }

                app.last_refresh = std::time::Instant::now();
                app.is_refreshing = false;

                // Redraw with new data
                terminal.draw(|f| ui(f, app))?;
            }
        }
    }
}

fn ui(f: &mut ratatui::Frame, app: &StatusApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header with current time
    let current_time = chrono::Local::now().format("%H:%M:%S").to_string();

    let header_text = if app.is_refreshing {
        format!("📋 Validator Status (5s refresh) - {} 🔄", current_time)
    } else {
        format!("📋 Validator Status (5s refresh) - {}", current_time)
    };

    let header = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            header_text,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("─".repeat(chunks[0].width as usize)),
    ])
    .alignment(Alignment::Left);
    f.render_widget(header, chunks[0]);

    // Content area for validators
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            app.app_state
                .validator_statuses
                .iter()
                .map(|_| Constraint::Length(20)) // Each validator table height (increased for catchup row)
                .collect::<Vec<_>>(),
        )
        .split(chunks[1]);

    // Render each validator
    for (index, ((((validator_status, vote_data), catchup_data), prev_slot), inc_time)) in app
        .app_state
        .validator_statuses
        .iter()
        .zip(app.vote_data.iter())
        .zip(app.catchup_data.iter())
        .zip(
            app.previous_last_slots
                .iter()
                .chain(std::iter::repeat(&None)),
        )
        .zip(app.increment_times.iter().chain(std::iter::repeat(&None)))
        .enumerate()
    {
        if index < content_chunks.len() {
            render_validator(
                f,
                content_chunks[index],
                validator_status,
                vote_data.as_ref(),
                catchup_data,
                index,
                *prev_slot,
                *inc_time,
            );
        }
    }

    // Footer with exit instructions
    let footer = Paragraph::new(vec![Line::from(vec![
        Span::raw("Press "),
        Span::styled(
            "q",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(", "),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(", or "),
        Span::styled(
            "Ctrl+C",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" to exit"),
    ])])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}

#[allow(clippy::too_many_arguments)]
fn render_validator(
    f: &mut ratatui::Frame,
    area: Rect,
    validator_status: &crate::ValidatorStatus,
    vote_data: Option<&ValidatorVoteData>,
    catchup_data: &(Option<CatchupStatus>, Option<CatchupStatus>),
    index: usize,
    previous_last_slot: Option<u64>,
    increment_time: Option<std::time::Instant>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Validator info
            Constraint::Min(0),    // Table
        ])
        .split(area);

    // Validator info with better styling
    let validator_pair = &validator_status.validator_pair;
    let mut info_lines = vec![];

    if let Some(ref metadata) = validator_status.metadata {
        if let Some(ref name) = metadata.name {
            info_lines.push(Line::from(vec![
                Span::raw("━━━ "),
                Span::styled("⚡", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(
                    name.to_uppercase(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled("⚡", Style::default().fg(Color::Yellow)),
                Span::raw(" ━━━"),
            ]));
            info_lines.push(Line::from(vec![Span::raw("")]));
            info_lines.push(Line::from(vec![
                Span::raw("  🗳️  Vote: "),
                Span::styled(
                    truncate_string(&validator_pair.vote_pubkey, 44),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
            info_lines.push(Line::from(vec![
                Span::raw("  🆔 Identity: "),
                Span::styled(
                    truncate_string(&validator_pair.identity_pubkey, 44),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }
    } else {
        info_lines.push(Line::from(vec![
            Span::raw("━━━ "),
            Span::styled("⚡", Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(
                format!("VALIDATOR {}", index + 1),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("⚡", Style::default().fg(Color::Yellow)),
            Span::raw(" ━━━"),
        ]));
        info_lines.push(Line::from(vec![Span::raw("")]));
        info_lines.push(Line::from(vec![
            Span::raw("  🗳️  Vote: "),
            Span::styled(
                truncate_string(&validator_pair.vote_pubkey, 44),
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }

    let info = Paragraph::new(info_lines);
    f.render_widget(info, chunks[0]);

    // Table
    if validator_status.nodes_with_status.len() >= 2 {
        let node_0 = &validator_status.nodes_with_status[0];
        let node_1 = &validator_status.nodes_with_status[1];

        render_status_table(
            f,
            chunks[1],
            node_0,
            node_1,
            validator_status,
            vote_data,
            catchup_data,
            previous_last_slot,
            increment_time,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_status_table(
    f: &mut ratatui::Frame,
    area: Rect,
    node_0: &crate::types::NodeWithStatus,
    node_1: &crate::types::NodeWithStatus,
    _validator_status: &crate::ValidatorStatus,
    vote_data: Option<&ValidatorVoteData>,
    catchup_data: &(Option<CatchupStatus>, Option<CatchupStatus>),
    previous_last_slot: Option<u64>,
    increment_time: Option<std::time::Instant>,
) {
    let node_0_label = match node_0.status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };
    let node_1_label = match node_1.status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };

    let node_0_color = match node_0.status {
        crate::types::NodeStatus::Active => Color::Green,
        crate::types::NodeStatus::Standby => Color::Yellow,
        crate::types::NodeStatus::Unknown => Color::DarkGray,
    };
    let node_1_color = match node_1.status {
        crate::types::NodeStatus::Active => Color::Green,
        crate::types::NodeStatus::Standby => Color::Yellow,
        crate::types::NodeStatus::Unknown => Color::DarkGray,
    };

    // Build rows
    let mut rows = vec![];

    // Header row with node info
    rows.push(
        Row::new(vec![
            Cell::from(""),
            Cell::from(vec![
                Line::from(vec![
                    Span::raw("🖥️  "),
                    Span::styled(
                        node_0.node.label.to_string(),
                        Style::default()
                            .fg(if node_0.status == crate::types::NodeStatus::Active {
                                Color::Green
                            } else {
                                Color::Yellow
                            })
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        node_0.node.host.to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
            ]),
            Cell::from(vec![
                Line::from(vec![
                    Span::raw("🖥️  "),
                    Span::styled(
                        node_1.node.label.to_string(),
                        Style::default()
                            .fg(if node_1.status == crate::types::NodeStatus::Active {
                                Color::Green
                            } else {
                                Color::Yellow
                            })
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        node_1.node.host.to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
            ]),
        ])
        .height(2),
    );

    // Empty row for spacing
    rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]).height(1));

    // Status row with better icons
    let node_0_status_icon = match node_0.status {
        crate::types::NodeStatus::Active => "◉", // Large circle with dot
        crate::types::NodeStatus::Standby => "○", // Large circle
        crate::types::NodeStatus::Unknown => "⚠", // Warning
    };
    let node_1_status_icon = match node_1.status {
        crate::types::NodeStatus::Active => "◉",
        crate::types::NodeStatus::Standby => "○",
        crate::types::NodeStatus::Unknown => "⚠",
    };
    rows.push(Row::new(vec![
        Cell::from(" → Status").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(" {} {}", node_0_status_icon, node_0_label)).style(
            Style::default()
                .fg(node_0_color)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from(format!(" {} {}", node_1_status_icon, node_1_label)).style(
            Style::default()
                .fg(node_1_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Version row with icon
    let node_0_version = node_0
        .version
        .as_ref()
        .cloned()
        .unwrap_or("N/A".to_string());
    let node_1_version = node_1
        .version
        .as_ref()
        .cloned()
        .unwrap_or("N/A".to_string());
    rows.push(Row::new(vec![
        Cell::from(" → Version").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(" 📦 {}", node_0_version)).style(Style::default().fg(Color::Cyan)),
        Cell::from(format!(" 📦 {}", node_1_version)).style(Style::default().fg(Color::Cyan)),
    ]));

    // Sync status row with icons
    let node_0_sync = node_0
        .sync_status
        .as_ref()
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_sync = node_1
        .sync_status
        .as_ref()
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_0_sync_icon = if node_0_sync.contains("Caught up") {
        "🔄"
    } else if node_0_sync == "Unknown" {
        "⚠️"
    } else {
        "⏳"
    };
    let node_1_sync_icon = if node_1_sync.contains("Caught up") {
        "🔄"
    } else if node_1_sync == "Unknown" {
        "⚠️"
    } else {
        "⏳"
    };
    let node_0_sync_color = if node_0_sync.contains("Caught up") {
        Color::Green
    } else if node_0_sync == "Unknown" {
        Color::Red
    } else {
        Color::Yellow
    };
    let node_1_sync_color = if node_1_sync.contains("Caught up") {
        Color::Green
    } else if node_1_sync == "Unknown" {
        Color::Red
    } else {
        Color::Yellow
    };
    rows.push(Row::new(vec![
        Cell::from(" → Sync").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(
            " {} {}",
            node_0_sync_icon,
            truncate_sync_status(&node_0_sync, 35)
        ))
        .style(Style::default().fg(node_0_sync_color)),
        Cell::from(format!(
            " {} {}",
            node_1_sync_icon,
            truncate_sync_status(&node_1_sync, 35)
        ))
        .style(Style::default().fg(node_1_sync_color)),
    ]));

    // Identity row with key icon
    let node_0_identity = node_0
        .current_identity
        .as_ref()
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_identity = node_1
        .current_identity
        .as_ref()
        .cloned()
        .unwrap_or("Unknown".to_string());
    rows.push(Row::new(vec![
        Cell::from(" → Identity").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(" 🔑 {}", truncate_string(&node_0_identity, 25)))
            .style(Style::default().fg(Color::White)),
        Cell::from(format!(" 🔑 {}", truncate_string(&node_1_identity, 25)))
            .style(Style::default().fg(Color::White)),
    ]));

    // Ledger path row with folder icon
    let node_0_ledger = node_0
        .ledger_path
        .as_ref()
        .map(|p| truncate_path(p, 35))
        .unwrap_or("N/A".to_string());
    let node_1_ledger = node_1
        .ledger_path
        .as_ref()
        .map(|p| truncate_path(p, 35))
        .unwrap_or("N/A".to_string());
    rows.push(Row::new(vec![
        Cell::from(" → Ledger").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(" 📁 {}", node_0_ledger)).style(Style::default().fg(Color::DarkGray)),
        Cell::from(format!(" 📁 {}", node_1_ledger)).style(Style::default().fg(Color::DarkGray)),
    ]));

    // Swap ready row
    let (node_0_swap, node_0_swap_color) = match node_0.swap_ready {
        Some(true) => (" ✅ Ready", Color::Green),
        Some(false) => (" ❌ Not Ready", Color::Red),
        None => (" ❓ Unknown", Color::Yellow),
    };
    let (node_1_swap, node_1_swap_color) = match node_1.swap_ready {
        Some(true) => (" ✅ Ready", Color::Green),
        Some(false) => (" ❌ Not Ready", Color::Red),
        None => (" ❓ Unknown", Color::Yellow),
    };
    rows.push(Row::new(vec![
        Cell::from(" → Swap Ready").style(Style::default().fg(Color::Gray)),
        Cell::from(node_0_swap).style(Style::default().fg(node_0_swap_color)),
        Cell::from(node_1_swap).style(Style::default().fg(node_1_swap_color)),
    ]));

    // Empty row for visual separation
    rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]).height(1));

    // Section header for dynamic data
    rows.push(Row::new(vec![
        Cell::from(" 🔄 LIVE DATA").style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from(""),
        Cell::from(""),
    ]));

    // Catchup status row
    let node_0_catchup = catchup_data
        .0
        .as_ref()
        .map(|c| c.status.clone())
        .unwrap_or("N/A".to_string());
    let node_1_catchup = catchup_data
        .1
        .as_ref()
        .map(|c| c.status.clone())
        .unwrap_or("N/A".to_string());

    let node_0_catchup_color = if node_0_catchup.contains("Caught up") {
        Color::Green
    } else if node_0_catchup == "ERROR" || node_0_catchup == "N/A" {
        Color::Red
    } else {
        Color::Yellow
    };
    let node_1_catchup_color = if node_1_catchup.contains("Caught up") {
        Color::Green
    } else if node_1_catchup == "ERROR" || node_1_catchup == "N/A" {
        Color::Red
    } else {
        Color::Yellow
    };

    let node_0_catchup_icon = if node_0_catchup.contains("Caught up") {
        "✓"
    } else if node_0_catchup == "N/A" {
        "✗"
    } else {
        "⌛"
    };
    let node_1_catchup_icon = if node_1_catchup.contains("Caught up") {
        "✓"
    } else if node_1_catchup == "N/A" {
        "✗"
    } else {
        "⌛"
    };

    rows.push(Row::new(vec![
        Cell::from(" → Catchup").style(Style::default().fg(Color::Gray)),
        Cell::from(format!(
            " {} {}",
            node_0_catchup_icon,
            truncate_sync_status(&node_0_catchup, 35)
        ))
        .style(Style::default().fg(node_0_catchup_color)),
        Cell::from(format!(
            " {} {}",
            node_1_catchup_icon,
            truncate_sync_status(&node_1_catchup, 35)
        ))
        .style(Style::default().fg(node_1_catchup_color)),
    ]));

    // Vote status row (dynamic)
    if let Some(vote_data) = vote_data {
        let voting_status = if vote_data.is_voting {
            "✅ Voting"
        } else {
            "⚠️ Not Voting"
        };

        let voting_color = if vote_data.is_voting {
            Color::Green
        } else {
            Color::Yellow
        };

        // Show votes with ellipsis format and increment
        let votes_cell = if !vote_data.recent_votes.is_empty() {
            let total_votes = vote_data.recent_votes.len();
            let last_slot = vote_data.recent_votes[total_votes - 1].slot;

            // Calculate increment from previous refresh
            let (increment, should_highlight) = if let Some(prev_slot) = previous_last_slot {
                if last_slot > prev_slot {
                    let inc_str = format!(" (+{})", last_slot - prev_slot);
                    // Check if we should highlight (within 2 seconds of increment)
                    let highlight = increment_time
                        .map(|t| t.elapsed().as_secs() < 2)
                        .unwrap_or(false);
                    (inc_str, highlight)
                } else {
                    (String::new(), false)
                }
            } else {
                (String::new(), false)
            };

            let vote_list = if total_votes == 1 {
                // Show single vote
                format!("{}", vote_data.recent_votes[0].slot)
            } else if total_votes > 0 {
                // Show first and last with ... in between
                let first = vote_data.recent_votes[0].slot.to_string();
                let last = vote_data.recent_votes[total_votes - 1].slot.to_string();
                format!("{} ... {}", first, last)
            } else {
                "No votes".to_string()
            };

            // Create the votes cell with optional highlighted increment
            if !increment.is_empty() {
                let mut spans = vec![Span::raw(" "), Span::raw(vote_list)];
                if should_highlight {
                    spans.push(Span::styled(
                        increment,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::raw(increment));
                }
                Cell::from(Line::from(spans))
            } else {
                Cell::from(format!(" {}", vote_list))
            }
        } else {
            Cell::from("No recent votes")
        };

        rows.push(Row::new(vec![
            Cell::from(" → Vote Slots").style(Style::default().fg(Color::Gray)),
            Cell::from(format!(" {}", voting_status)).style(
                Style::default()
                    .fg(voting_color)
                    .add_modifier(Modifier::BOLD),
            ),
            votes_cell,
        ]));
    } else {
        // Show vote status row even when no vote data
        rows.push(Row::new(vec![
            Cell::from(" → Vote Slots").style(Style::default().fg(Color::Gray)),
            Cell::from(" ⚠️ No Data").style(Style::default().fg(Color::Red)),
            Cell::from(" Unable to fetch vote data").style(Style::default().fg(Color::DarkGray)),
        ]));
    }

    let widths = vec![
        Constraint::Length(18),
        Constraint::Percentage(40),
        Constraint::Percentage(40),
    ];

    let table = Table::new(rows, widths).style(Style::default()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .border_type(ratatui::widgets::BorderType::Rounded),
    );

    f.render_widget(table, area);
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...{}", &s[..8], &s[s.len() - 8..])
    }
}

fn truncate_sync_status(status: &str, max_len: usize) -> String {
    if status.len() <= max_len {
        status.to_string()
    } else {
        // For sync status, preserve the important parts
        if status.contains("slot:") {
            // Extract just the slot number
            if let Some(start) = status.find("slot: ") {
                let slot_part = &status[start + 6..];
                if let Some(end) = slot_part.find(')') {
                    return format!("Caught up (slot: {})", &slot_part[..end]);
                }
            }
        }
        status.chars().take(max_len - 3).collect::<String>() + "..."
    }
}

fn truncate_path(path: &str, max_length: usize) -> String {
    if path.len() <= max_length {
        path.to_string()
    } else {
        let start = if path.len() > max_length - 3 {
            path.len() - (max_length - 3)
        } else {
            0
        };
        format!("...{}", &path[start..])
    }
}
