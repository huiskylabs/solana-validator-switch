use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{AppState, ssh_async::AsyncSshPool};
use crate::solana_rpc::{fetch_vote_account_data, ValidatorVoteData};

/// Enhanced UI App state with async support
pub struct EnhancedStatusApp {
    pub app_state: Arc<AppState>,
    pub ssh_pool: Arc<AsyncSshPool>,
    pub ui_state: Arc<RwLock<UiState>>,
    pub log_sender: tokio::sync::broadcast::Sender<LogMessage>,
    pub should_quit: Arc<RwLock<bool>>,
}

/// UI State that can be shared across threads
pub struct UiState {
    // Vote data for each validator
    pub vote_data: Vec<Option<ValidatorVoteData>>,
    pub previous_last_slots: Vec<Option<u64>>,
    pub increment_times: Vec<Option<Instant>>,
    
    // Catchup status for each node
    pub catchup_data: Vec<NodePairStatus>,
    
    // SSH logs for each host
    pub host_logs: HashMap<String, Vec<String>>,
    pub selected_host: Option<String>,
    pub log_scroll_offset: usize,
    
    // Refresh state
    pub last_vote_refresh: Instant,
    pub last_catchup_refresh: Instant,
    pub is_refreshing: bool,
    
    // UI state
    pub focused_pane: FocusedPane,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FocusedPane {
    Summary,
    Logs,
}

#[derive(Clone)]
pub struct NodePairStatus {
    pub node_0: Option<CatchupStatus>,
    pub node_1: Option<CatchupStatus>,
}

#[derive(Clone)]
pub struct CatchupStatus {
    pub status: String,
    pub last_updated: Instant,
}

#[derive(Clone)]
pub struct LogMessage {
    pub host: String,
    pub message: String,
    pub timestamp: Instant,
    pub level: LogLevel,
}

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl EnhancedStatusApp {
    pub async fn new(app_state: Arc<AppState>) -> Result<Self> {
        let ssh_pool = Arc::new(AsyncSshPool::new());
        
        // Create broadcast channel for log messages
        let (log_sender, _) = tokio::sync::broadcast::channel(1000);
        
        // Initialize UI state
        let mut initial_vote_data = Vec::new();
        let mut initial_catchup_data = Vec::new();
        let mut host_logs = HashMap::new();
        
        for validator_status in &app_state.validator_statuses {
            initial_vote_data.push(None);
            
            let node_pair = NodePairStatus {
                node_0: None,
                node_1: None,
            };
            initial_catchup_data.push(node_pair);
            
            // Initialize logs for each host
            for node in &validator_status.nodes_with_status {
                host_logs.insert(node.node.host.clone(), Vec::new());
            }
        }
        
        let ui_state = Arc::new(RwLock::new(UiState {
            vote_data: initial_vote_data,
            previous_last_slots: Vec::new(),
            increment_times: Vec::new(),
            catchup_data: initial_catchup_data,
            host_logs,
            selected_host: None,
            log_scroll_offset: 0,
            last_vote_refresh: Instant::now(),
            last_catchup_refresh: Instant::now(),
            is_refreshing: false,
            focused_pane: FocusedPane::Summary,
        }));
        
        Ok(Self {
            app_state,
            ssh_pool,
            ui_state,
            log_sender,
            should_quit: Arc::new(RwLock::new(false)),
        })
    }
    
    /// Spawn background tasks for data fetching
    pub fn spawn_background_tasks(&self) {
        // Vote data refresh task
        let ui_state = Arc::clone(&self.ui_state);
        let app_state = Arc::clone(&self.app_state);
        let log_sender = self.log_sender.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                // Fetch vote data for all validators
                let mut new_vote_data = Vec::new();
                
                for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
                    let validator_pair = &validator_status.validator_pair;
                    
                    match fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey).await {
                        Ok(data) => {
                            let _ = log_sender.send(LogMessage {
                                host: format!("validator-{}", idx),
                                message: format!("Vote data fetched: last slot {}", 
                                    data.recent_votes.last().map(|v| v.slot).unwrap_or(0)),
                                timestamp: Instant::now(),
                                level: LogLevel::Info,
                            });
                            
                            new_vote_data.push(Some(data));
                        }
                        Err(e) => {
                            let _ = log_sender.send(LogMessage {
                                host: format!("validator-{}", idx),
                                message: format!("Failed to fetch vote data: {}", e),
                                timestamp: Instant::now(),
                                level: LogLevel::Error,
                            });
                            
                            new_vote_data.push(None);
                        }
                    }
                }
                
                // Update UI state
                let mut state = ui_state.write().await;
                
                // Calculate increments
                let mut new_increments = Vec::new();
                for (idx, new_data) in new_vote_data.iter().enumerate() {
                    if let (Some(new), Some(old)) = (new_data, state.vote_data.get(idx).and_then(|v| v.as_ref())) {
                        if let (Some(new_last), Some(old_last)) = (
                            new.recent_votes.last().map(|v| v.slot),
                            old.recent_votes.last().map(|v| v.slot)
                        ) {
                            if new_last > old_last {
                                new_increments.push(Some(Instant::now()));
                            } else {
                                // Keep existing increment if still valid
                                if let Some(existing) = state.increment_times.get(idx).and_then(|&v| v) {
                                    if existing.elapsed().as_secs() < 2 {
                                        new_increments.push(Some(existing));
                                    } else {
                                        new_increments.push(None);
                                    }
                                } else {
                                    new_increments.push(None);
                                }
                            }
                        } else {
                            new_increments.push(None);
                        }
                    } else {
                        new_increments.push(None);
                    }
                }
                
                // Update previous slots
                state.previous_last_slots = state.vote_data.iter()
                    .map(|v| v.as_ref().and_then(|d| d.recent_votes.last().map(|v| v.slot)))
                    .collect();
                
                state.vote_data = new_vote_data;
                state.increment_times = new_increments;
                state.last_vote_refresh = Instant::now();
            }
        });
        
        // Catchup status refresh task
        let ui_state = Arc::clone(&self.ui_state);
        let app_state = Arc::clone(&self.app_state);
        let ssh_pool = Arc::clone(&self.ssh_pool);
        let log_sender = self.log_sender.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Fetch catchup status for all nodes
                let mut new_catchup_data = Vec::new();
                
                for validator_status in &app_state.validator_statuses {
                    let mut node_pair = NodePairStatus {
                        node_0: None,
                        node_1: None,
                    };
                    
                    if validator_status.nodes_with_status.len() >= 2 {
                        // Fetch for node 0
                        let node_0 = &validator_status.nodes_with_status[0];
                        if let Some(ssh_key) = app_state.detected_ssh_keys.get(&node_0.node.host) {
                            node_pair.node_0 = fetch_catchup_for_node(
                                &ssh_pool,
                                &node_0,
                                ssh_key,
                                &log_sender,
                            ).await;
                        }
                        
                        // Fetch for node 1
                        let node_1 = &validator_status.nodes_with_status[1];
                        if let Some(ssh_key) = app_state.detected_ssh_keys.get(&node_1.node.host) {
                            node_pair.node_1 = fetch_catchup_for_node(
                                &ssh_pool,
                                &node_1,
                                ssh_key,
                                &log_sender,
                            ).await;
                        }
                    }
                    
                    new_catchup_data.push(node_pair);
                }
                
                // Update UI state
                let mut state = ui_state.write().await;
                state.catchup_data = new_catchup_data;
                state.last_catchup_refresh = Instant::now();
            }
        });
    }
}

async fn fetch_catchup_for_node(
    ssh_pool: &AsyncSshPool,
    node: &crate::types::NodeWithStatus,
    ssh_key: &str,
    log_sender: &tokio::sync::broadcast::Sender<LogMessage>,
) -> Option<CatchupStatus> {
    let solana_cli = node.solana_cli_executable.as_ref()
        .or(node.agave_validator_executable.as_ref()
            .map(|path| path.replace("agave-validator", "solana"))
            .as_ref())
        .cloned()?;
    
    let catchup_cmd = format!("{} catchup --our-localhost", solana_cli);
    
    match ssh_pool.execute_command_with_early_exit(
        &node.node,
        ssh_key,
        &catchup_cmd,
        |output| output.contains("0 slot(s)") || output.contains("has caught up")
    ).await {
        Ok(output) => {
            let status = if output.contains("0 slot(s)") || output.contains("has caught up") {
                "Caught up".to_string()
            } else if let Some(pos) = output.find(" slot(s) behind") {
                let start = output[..pos].rfind(' ').map(|i| i + 1).unwrap_or(0);
                let slots_str = &output[start..pos];
                if let Ok(slots) = slots_str.parse::<u64>() {
                    format!("{} slots behind", slots)
                } else {
                    "Checking...".to_string()
                }
            } else {
                "Unknown".to_string()
            };
            
            let _ = log_sender.send(LogMessage {
                host: node.node.host.clone(),
                message: format!("Catchup status: {}", status),
                timestamp: Instant::now(),
                level: LogLevel::Info,
            });
            
            Some(CatchupStatus {
                status,
                last_updated: Instant::now(),
            })
        }
        Err(e) => {
            let _ = log_sender.send(LogMessage {
                host: node.node.host.clone(),
                message: format!("Failed to get catchup status: {}", e),
                timestamp: Instant::now(),
                level: LogLevel::Error,
            });
            
            None
        }
    }
}

/// Run the enhanced UI
pub async fn run_enhanced_ui(app: &mut EnhancedStatusApp) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    
    // Spawn background tasks
    app.spawn_background_tasks();
    
    // Process log messages in background
    let ui_state = Arc::clone(&app.ui_state);
    let mut log_receiver = app.log_sender.subscribe();
    tokio::spawn(async move {
        while let Ok(log_msg) = log_receiver.recv().await {
            let mut state = ui_state.write().await;
            if let Some(logs) = state.host_logs.get_mut(&log_msg.host) {
                let formatted = format!(
                    "[{}] {}",
                    chrono::Local::now().format("%H:%M:%S"),
                    log_msg.message
                );
                logs.push(formatted);
                
                // Keep only last 1000 lines per host
                if logs.len() > 1000 {
                    logs.drain(0..logs.len() - 1000);
                }
            }
        }
    });
    
    // Main UI loop
    let mut ui_interval = interval(Duration::from_millis(100)); // 10 FPS
    
    loop {
        // Check for quit signal
        if *app.should_quit.read().await {
            break;
        }
        
        // Handle keyboard events
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(key, &app.ui_state, &app.should_quit).await?;
            }
        }
        
        // Draw UI
        let ui_state_read = app.ui_state.read().await;
        terminal.draw(|f| draw_ui(f, &ui_state_read, &app.app_state))?;
        drop(ui_state_read);
        
        // Wait for next frame
        ui_interval.tick().await;
    }
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    
    Ok(())
}

/// Handle keyboard events
async fn handle_key_event(
    key: KeyEvent,
    ui_state: &Arc<RwLock<UiState>>,
    should_quit: &Arc<RwLock<bool>>,
) -> Result<()> {
    let mut state = ui_state.write().await;
    
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            *should_quit.write().await = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *should_quit.write().await = true;
        }
        KeyCode::Tab => {
            // Switch focus between panes
            state.focused_pane = match state.focused_pane {
                FocusedPane::Summary => FocusedPane::Logs,
                FocusedPane::Logs => FocusedPane::Summary,
            };
        }
        KeyCode::Up => {
            if state.focused_pane == FocusedPane::Logs && state.log_scroll_offset > 0 {
                state.log_scroll_offset -= 1;
            }
        }
        KeyCode::Down => {
            if state.focused_pane == FocusedPane::Logs {
                state.log_scroll_offset += 1;
            }
        }
        KeyCode::PageUp => {
            if state.focused_pane == FocusedPane::Logs && state.log_scroll_offset > 10 {
                state.log_scroll_offset -= 10;
            } else {
                state.log_scroll_offset = 0;
            }
        }
        KeyCode::PageDown => {
            if state.focused_pane == FocusedPane::Logs {
                state.log_scroll_offset += 10;
            }
        }
        KeyCode::Home => {
            if state.focused_pane == FocusedPane::Logs {
                state.log_scroll_offset = 0;
            }
        }
        KeyCode::End => {
            if state.focused_pane == FocusedPane::Logs {
                if let Some(host) = &state.selected_host {
                    if let Some(logs) = state.host_logs.get(host) {
                        state.log_scroll_offset = logs.len().saturating_sub(1);
                    }
                }
            }
        }
        KeyCode::Char(c) if c >= '1' && c <= '9' => {
            // Select host by number
            if let Some(idx) = c.to_digit(10) {
                let idx = (idx as usize).saturating_sub(1);
                let hosts: Vec<_> = state.host_logs.keys().cloned().collect();
                if idx < hosts.len() {
                    state.selected_host = Some(hosts[idx].clone());
                    state.log_scroll_offset = 0;
                }
            }
        }
        _ => {}
    }
    
    Ok(())
}

/// Draw the main UI
fn draw_ui(f: &mut ratatui::Frame, ui_state: &UiState, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),    // Header
            Constraint::Percentage(60), // Summary tables
            Constraint::Min(10),      // Logs
            Constraint::Length(2),    // Footer
        ])
        .split(f.size());
    
    // Draw header
    draw_header(f, chunks[0], ui_state);
    
    // Draw validator summaries
    draw_validator_summaries(f, chunks[1], ui_state, app_state);
    
    // Draw logs
    draw_logs(f, chunks[2], ui_state);
    
    // Draw footer
    draw_footer(f, chunks[3], ui_state);
}

fn draw_header(f: &mut ratatui::Frame, area: Rect, ui_state: &UiState) {
    let current_time = chrono::Local::now().format("%H:%M:%S").to_string();
    
    let header_text = if ui_state.is_refreshing {
        format!("📋 Enhanced Validator Status - {} 🔄", current_time)
    } else {
        format!("📋 Enhanced Validator Status - {}", current_time)
    };
    
    let header = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            header_text,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from("─".repeat(area.width as usize)),
    ])
    .alignment(Alignment::Left);
    
    f.render_widget(header, area);
}

fn draw_validator_summaries(f: &mut ratatui::Frame, area: Rect, ui_state: &UiState, app_state: &AppState) {
    let validator_count = app_state.validator_statuses.len();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Percentage(100 / validator_count as u16); validator_count])
        .split(area);
    
    for (idx, (validator_status, chunk)) in app_state.validator_statuses.iter().zip(chunks.iter()).enumerate() {
        let vote_data = ui_state.vote_data.get(idx).and_then(|v| v.as_ref());
        let catchup_data = ui_state.catchup_data.get(idx);
        let prev_slot = ui_state.previous_last_slots.get(idx).and_then(|&v| v);
        let inc_time = ui_state.increment_times.get(idx).and_then(|&v| v);
        
        draw_validator_table(f, *chunk, validator_status, vote_data, catchup_data, prev_slot, inc_time);
    }
}

fn draw_validator_table(
    f: &mut ratatui::Frame,
    area: Rect,
    validator_status: &crate::ValidatorStatus,
    vote_data: Option<&ValidatorVoteData>,
    catchup_data: Option<&NodePairStatus>,
    previous_last_slot: Option<u64>,
    increment_time: Option<Instant>,
) {
    let validator_name = validator_status.metadata.as_ref()
        .and_then(|m| m.name.as_ref())
        .cloned()
        .unwrap_or_else(|| "Validator".to_string());
    
    let mut rows = vec![];
    
    // Node status row
    if validator_status.nodes_with_status.len() >= 2 {
        let node_0 = &validator_status.nodes_with_status[0];
        let node_1 = &validator_status.nodes_with_status[1];
        
        rows.push(Row::new(vec![
            Cell::from("Status"),
            Cell::from(format!("{} ({})", node_0.node.label, 
                match node_0.status {
                    crate::types::NodeStatus::Active => "ACTIVE",
                    crate::types::NodeStatus::Standby => "STANDBY",
                    crate::types::NodeStatus::Unknown => "UNKNOWN",
                }
            )).style(Style::default().fg(match node_0.status {
                crate::types::NodeStatus::Active => Color::Green,
                crate::types::NodeStatus::Standby => Color::Yellow,
                crate::types::NodeStatus::Unknown => Color::DarkGray,
            })),
            Cell::from(format!("{} ({})", node_1.node.label,
                match node_1.status {
                    crate::types::NodeStatus::Active => "ACTIVE",
                    crate::types::NodeStatus::Standby => "STANDBY",
                    crate::types::NodeStatus::Unknown => "UNKNOWN",
                }
            )).style(Style::default().fg(match node_1.status {
                crate::types::NodeStatus::Active => Color::Green,
                crate::types::NodeStatus::Standby => Color::Yellow,
                crate::types::NodeStatus::Unknown => Color::DarkGray,
            })),
        ]));
    }
    
    // Catchup status
    if let Some(catchup) = catchup_data {
        let node_0_status = catchup.node_0.as_ref()
            .map(|c| c.status.clone())
            .unwrap_or_else(|| "N/A".to_string());
        let node_1_status = catchup.node_1.as_ref()
            .map(|c| c.status.clone())
            .unwrap_or_else(|| "N/A".to_string());
        
        rows.push(Row::new(vec![
            Cell::from("Catchup"),
            Cell::from(node_0_status.clone()).style(
                if node_0_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                }
            ),
            Cell::from(node_1_status.clone()).style(
                if node_1_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                }
            ),
        ]));
    }
    
    // Vote status
    if let Some(vote_data) = vote_data {
        let vote_status = if vote_data.is_voting {
            "✅ Voting"
        } else {
            "⚠️ Not Voting"
        };
        
        let mut vote_display = String::new();
        if !vote_data.recent_votes.is_empty() {
            let first = vote_data.recent_votes.first().unwrap().slot;
            let last = vote_data.recent_votes.last().unwrap().slot;
            vote_display = if vote_data.recent_votes.len() > 1 {
                format!("{} ... {}", first, last)
            } else {
                format!("{}", first)
            };
            
            // Add increment if applicable
            if let Some(prev) = previous_last_slot {
                if last > prev {
                    let inc = format!(" (+{})", last - prev);
                    if increment_time.map(|t| t.elapsed().as_secs() < 2).unwrap_or(false) {
                        vote_display.push_str(&inc);
                    }
                }
            }
        }
        
        rows.push(Row::new(vec![
            Cell::from("Votes"),
            Cell::from(vote_status).style(
                if vote_data.is_voting {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                }
            ),
            Cell::from(vote_display),
        ]));
    }
    
    let table = Table::new(
        rows,
        vec![
            Constraint::Length(10),
            Constraint::Percentage(45),
            Constraint::Percentage(45),
        ]
    )
    .block(
        Block::default()
            .title(validator_name)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
    );
    
    f.render_widget(table, area);
}

fn draw_logs(f: &mut ratatui::Frame, area: Rect, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20), // Host list
            Constraint::Min(0),     // Log content
        ])
        .split(area);
    
    // Draw host list
    let hosts: Vec<_> = ui_state.host_logs.keys().cloned().collect();
    let host_items: Vec<_> = hosts.iter().enumerate()
        .map(|(idx, host)| {
            let selected = ui_state.selected_host.as_ref() == Some(host);
            let style = if selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{}: {}", idx + 1, host)).style(style)
        })
        .collect();
    
    let host_list = List::new(host_items)
        .block(
            Block::default()
                .title("Hosts")
                .borders(Borders::ALL)
                .border_style(if ui_state.focused_pane == FocusedPane::Logs {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                })
        );
    
    f.render_widget(host_list, chunks[0]);
    
    // Draw log content
    if let Some(selected_host) = &ui_state.selected_host {
        if let Some(logs) = ui_state.host_logs.get(selected_host) {
            let visible_height = (chunks[1].height.saturating_sub(2)) as usize;
            let start = ui_state.log_scroll_offset;
            let end = (start + visible_height).min(logs.len());
            
            let visible_logs: Vec<_> = logs[start..end].iter()
                .map(|log| ListItem::new(log.as_str()))
                .collect();
            
            let log_list = List::new(visible_logs)
                .block(
                    Block::default()
                        .title(format!("Logs: {} [{}/{}]", selected_host, start + 1, logs.len()))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                );
            
            f.render_widget(log_list, chunks[1]);
        }
    } else {
        let help = Paragraph::new("Select a host from the list (press 1-9)")
            .block(
                Block::default()
                    .title("Logs")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
            )
            .alignment(Alignment::Center);
        
        f.render_widget(help, chunks[1]);
    }
}

fn draw_footer(f: &mut ratatui::Frame, area: Rect, ui_state: &UiState) {
    let help_text = match ui_state.focused_pane {
        FocusedPane::Summary => {
            "Tab: Switch to logs | q/Esc: Quit"
        }
        FocusedPane::Logs => {
            "Tab: Switch to summary | ↑↓: Scroll | PgUp/PgDn: Page | 1-9: Select host | q/Esc: Quit"
        }
    };
    
    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    
    f.render_widget(footer, area);
}

/// Entry point for the enhanced UI
pub async fn show_enhanced_status_ui(app_state: &AppState) -> Result<()> {
    let app_state_arc = Arc::new(app_state.clone());
    let mut app = EnhancedStatusApp::new(app_state_arc).await?;
    run_enhanced_ui(&mut app).await
}