use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};
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

use crate::{ssh::AsyncSshPool, AppState};
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
    
    // Refresh state
    pub last_vote_refresh: Instant,
    pub last_catchup_refresh: Instant,
    pub is_refreshing: bool,
    
}

// Removed FocusedPane enum as logs are no longer displayed

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
        let ssh_pool = Arc::clone(&app_state.ssh_pool);
        
        // Create broadcast channel for log messages
        let (log_sender, _) = tokio::sync::broadcast::channel(1000);
        
        // Initialize UI state
        let mut initial_vote_data = Vec::new();
        let mut initial_catchup_data = Vec::new();
        
        for _validator_status in &app_state.validator_statuses {
            initial_vote_data.push(None);
            
            let node_pair = NodePairStatus {
                node_0: None,
                node_1: None,
            };
            initial_catchup_data.push(node_pair);
        }
        
        let ui_state = Arc::new(RwLock::new(UiState {
            vote_data: initial_vote_data,
            previous_last_slots: Vec::new(),
            increment_times: Vec::new(),
            catchup_data: initial_catchup_data,
            last_vote_refresh: Instant::now(),
            last_catchup_refresh: Instant::now(),
            is_refreshing: false,
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
            let mut interval = interval(Duration::from_secs(5));
            
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
    
    let catchup_cmd = format!("{} catchup localhost", solana_cli);
    
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
            } else if output.contains("Error") || output.contains("error") {
                // If there's an error, show a cleaner message
                "Error".to_string()
            } else if output.trim().is_empty() {
                "Checking...".to_string()
            } else {
                // Show "Checking..." instead of raw output
                "Checking...".to_string()
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
    
    // Process log messages in background (keeping for internal use but not displaying)
    let mut log_receiver = app.log_sender.subscribe();
    tokio::spawn(async move {
        while let Ok(_log_msg) = log_receiver.recv().await {
            // Messages are received but not stored for UI display
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
    let _state = ui_state.write().await;
    
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            *should_quit.write().await = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *should_quit.write().await = true;
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
            Constraint::Min(0),       // Validator tables take all remaining space
            Constraint::Length(2),    // Footer
        ])
        .split(f.size());
    
    // Draw header
    draw_header(f, chunks[0], ui_state);
    
    // Draw validator summaries
    draw_validator_summaries(f, chunks[1], ui_state, app_state);
    
    // Draw footer
    draw_footer(f, chunks[2], ui_state);
}

fn draw_header(f: &mut ratatui::Frame, area: Rect, _ui_state: &UiState) {
    // Just leave empty - header will be in the table border
    let header = Paragraph::new("");
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
    let vote_key = &validator_status.validator_pair.vote_pubkey;
    let vote_formatted = format!("{}…{}", 
        vote_key.chars().take(4).collect::<String>(), 
        vote_key.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>()
    );
    
    let _validator_name = validator_status.metadata.as_ref()
        .and_then(|m| m.name.as_ref())
        .cloned()
        .unwrap_or_else(|| vote_formatted.clone());
    
    let mut rows = vec![];
    
    // Node status row with host and status
    if validator_status.nodes_with_status.len() >= 2 {
        let node_0 = &validator_status.nodes_with_status[0];
        let node_1 = &validator_status.nodes_with_status[1];
        
        // Status row
        rows.push(Row::new(vec![
            Cell::from("Status"),
            Cell::from(format!("{} ({})", 
                match node_0.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "🔴 UNKNOWN",
                },
                node_0.node.label
            )).style(Style::default().fg(match node_0.status {
                crate::types::NodeStatus::Active => Color::Green,
                crate::types::NodeStatus::Standby => Color::Yellow,
                crate::types::NodeStatus::Unknown => Color::Red,
            })),
            Cell::from(format!("{} ({})",
                match node_1.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "🔴 UNKNOWN",
                },
                node_1.node.label
            )).style(Style::default().fg(match node_1.status {
                crate::types::NodeStatus::Active => Color::Green,
                crate::types::NodeStatus::Standby => Color::Yellow,
                crate::types::NodeStatus::Unknown => Color::Red,
            })),
        ]));
        
        // Host info row
        rows.push(Row::new(vec![
            Cell::from("Host"),
            Cell::from(node_0.node.host.as_str()),
            Cell::from(node_1.node.host.as_str()),
        ]));
        
        // Validator type and version row
        rows.push(Row::new(vec![
            Cell::from("Type/Version"),
            Cell::from({
                let version = node_0.version.as_deref().unwrap_or("");
                let cleaned_version = version
                    .replace("Firedancer ", "")
                    .replace("Agave ", "")
                    .replace("Jito ", "");
                format!("{} {}", 
                    match node_0.validator_type {
                        crate::types::ValidatorType::Firedancer => "Firedancer",
                        crate::types::ValidatorType::Agave => "Agave",
                        crate::types::ValidatorType::Jito => "Jito",
                        crate::types::ValidatorType::Unknown => "Unknown",
                    },
                    cleaned_version
                )
            }),
            Cell::from({
                let version = node_1.version.as_deref().unwrap_or("");
                let cleaned_version = version
                    .replace("Firedancer ", "")
                    .replace("Agave ", "")
                    .replace("Jito ", "");
                format!("{} {}",
                    match node_1.validator_type {
                        crate::types::ValidatorType::Firedancer => "Firedancer",
                        crate::types::ValidatorType::Agave => "Agave",
                        crate::types::ValidatorType::Jito => "Jito",
                        crate::types::ValidatorType::Unknown => "Unknown",
                    },
                    cleaned_version
                )
            }),
        ]));
        
        // Identity row - format as ascd...edsas
        let id0 = node_0.current_identity.as_deref().unwrap_or("Unknown");
        let id1 = node_1.current_identity.as_deref().unwrap_or("Unknown");
        let id0_formatted = if id0 != "Unknown" && id0.len() > 8 {
            format!("{}…{}", 
                id0.chars().take(4).collect::<String>(),
                id0.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>()
            )
        } else {
            id0.to_string()
        };
        let id1_formatted = if id1 != "Unknown" && id1.len() > 8 {
            format!("{}…{}", 
                id1.chars().take(4).collect::<String>(),
                id1.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>()
            )
        } else {
            id1.to_string()
        };
        
        rows.push(Row::new(vec![
            Cell::from("Identity"),
            Cell::from(id0_formatted),
            Cell::from(id1_formatted),
        ]));
        
        // Swap readiness row
        rows.push(Row::new(vec![
            Cell::from("Swap Ready"),
            Cell::from(if node_0.swap_ready.unwrap_or(false) { "✅ Ready" } else { "❌ Not Ready" })
                .style(Style::default().fg(if node_0.swap_ready.unwrap_or(false) { Color::Green } else { Color::Red })),
            Cell::from(if node_1.swap_ready.unwrap_or(false) { "✅ Ready" } else { "❌ Not Ready" })
                .style(Style::default().fg(if node_1.swap_ready.unwrap_or(false) { Color::Green } else { Color::Red })),
        ]));
        
        // Sync status row if available
        if node_0.sync_status.is_some() || node_1.sync_status.is_some() {
            rows.push(Row::new(vec![
                Cell::from("Sync Status"),
                Cell::from(node_0.sync_status.as_deref().unwrap_or("N/A")),
                Cell::from(node_1.sync_status.as_deref().unwrap_or("N/A")),
            ]));
        }
        
        // Ledger path row if available
        if node_0.ledger_path.is_some() || node_1.ledger_path.is_some() {
            rows.push(Row::new(vec![
                Cell::from("Ledger Path"),
                Cell::from(node_0.ledger_path.as_deref().unwrap_or("N/A")
                    .split('/').last().unwrap_or("N/A")),
                Cell::from(node_1.ledger_path.as_deref().unwrap_or("N/A")
                    .split('/').last().unwrap_or("N/A")),
            ]));
        }
    }
    
    // Catchup status
    if let Some(catchup) = catchup_data {
        let node_0_status = catchup.node_0.as_ref()
            .map(|c| c.status.clone())
            .unwrap_or_else(|| "Checking...".to_string());
        let node_1_status = catchup.node_1.as_ref()
            .map(|c| c.status.clone())
            .unwrap_or_else(|| "Checking...".to_string());
        
        rows.push(Row::new(vec![
            Cell::from("Catchup"),
            Cell::from(node_0_status.clone()).style(
                if node_0_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else if node_0_status.contains("Error") {
                    Style::default().fg(Color::Red)
                } else if node_0_status.contains("Checking") {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Yellow)
                }
            ),
            Cell::from(node_1_status.clone()).style(
                if node_1_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else if node_1_status.contains("Error") {
                    Style::default().fg(Color::Red)
                } else if node_1_status.contains("Checking") {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Yellow)
                }
            ),
        ]));
    }
    
    // Vote account info - determine which node is active
    if let Some(vote_data) = vote_data {
        if validator_status.nodes_with_status.len() >= 2 {
            let node_0 = &validator_status.nodes_with_status[0];
            let node_1 = &validator_status.nodes_with_status[1];
            
            // Vote status row - show which node is voting
            let vote_status = if vote_data.is_voting {
                "✅ Voting"
            } else {
                "⚠️ Not Voting"
            };
            
            rows.push(Row::new(vec![
                Cell::from("Vote Status"),
                Cell::from(if node_0.status == crate::types::NodeStatus::Active { vote_status } else { "-" })
                    .style(Style::default().fg(if node_0.status == crate::types::NodeStatus::Active && vote_data.is_voting { Color::Green } else { Color::Yellow })),
                Cell::from(if node_1.status == crate::types::NodeStatus::Active { vote_status } else { "-" })
                    .style(Style::default().fg(if node_1.status == crate::types::NodeStatus::Active && vote_data.is_voting { Color::Green } else { Color::Yellow })),
            ]));
        }
        
        // Last voted slot row - show for active validator
        if let Some(last_vote) = vote_data.recent_votes.last() {
            if validator_status.nodes_with_status.len() >= 2 {
                let node_0 = &validator_status.nodes_with_status[0];
                let node_1 = &validator_status.nodes_with_status[1];
                
                let last_slot = last_vote.slot;
                let mut slot_display = format!("{}", last_slot);
                
                // Add increment if applicable
                if let Some(prev) = previous_last_slot {
                    if last_slot > prev {
                        let inc = format!(" (+{})", last_slot - prev);
                        if increment_time.map(|t| t.elapsed().as_secs() < 3).unwrap_or(false) {
                            slot_display.push_str(&inc);
                        }
                    }
                }
                
                let slot_cell = Cell::from(slot_display.clone()).style(
                    if increment_time.map(|t| t.elapsed().as_secs() < 3).unwrap_or(false) {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    }
                );
                
                rows.push(Row::new(vec![
                    Cell::from("Last Vote"),
                    Cell::from(if node_0.status == crate::types::NodeStatus::Active { slot_display.clone() } else { "-".to_string() }),
                    Cell::from(if node_1.status == crate::types::NodeStatus::Active { slot_display } else { "-".to_string() }),
                ]));
            }
        }
        
    }
    
    let table = Table::new(
        rows,
        vec![
            Constraint::Length(15),  // Wider label column
            Constraint::Percentage(42),
            Constraint::Percentage(43),
        ]
    )
    .block(
        Block::default()
            .title(format!("Vote: {} | Time: {}", 
                vote_formatted,
                chrono::Local::now().format("%H:%M:%S")
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
    );
    
    f.render_widget(table, area);
}

// Removed draw_logs function as logs are no longer displayed

fn draw_footer(f: &mut ratatui::Frame, area: Rect, _ui_state: &UiState) {
    let help_text = "q/Esc: Quit | Ctrl+C: Exit | Auto-refresh: 5s";
    
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