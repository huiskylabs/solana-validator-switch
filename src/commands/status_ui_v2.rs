use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

use crate::alert::{AlertManager, AlertTracker};
use crate::solana_rpc::{fetch_vote_account_data, ValidatorVoteData};
use crate::{ssh::AsyncSshPool, AppState};

/// View states for the UI
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewState {
    Status,
    DryRunSwitch,
}

/// Enhanced UI App state with async support
pub struct EnhancedStatusApp {
    pub app_state: Arc<AppState>,
    pub ssh_pool: Arc<AsyncSshPool>,
    pub ui_state: Arc<RwLock<UiState>>,
    pub log_sender: tokio::sync::mpsc::UnboundedSender<LogMessage>,
    pub should_quit: Arc<RwLock<bool>>,
    pub view_state: Arc<RwLock<ViewState>>,
}

/// UI State that can be shared across threads
pub struct UiState {
    // Vote data for each validator
    pub vote_data: Vec<Option<ValidatorVoteData>>,
    pub previous_last_slots: Vec<Option<u64>>,
    pub increment_times: Vec<Option<Instant>>,

    // Track when each validator's last vote slot changed
    pub last_vote_slot_times: Vec<Option<(u64, Instant)>>, // (slot, time when slot last changed)

    // Catchup status for each node
    pub catchup_data: Vec<NodePairStatus>,

    // Refresh state
    pub last_vote_refresh: Instant,
    pub last_catchup_refresh: Instant,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub last_updated: Instant,
}

#[derive(Clone)]
#[allow(dead_code)]
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

        // Create unbounded channel for log messages
        let (log_sender, _log_receiver) = tokio::sync::mpsc::unbounded_channel();

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
            last_vote_slot_times: vec![None; app_state.validator_statuses.len()],
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
            view_state: Arc::new(RwLock::new(ViewState::Status)),
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

            // Initialize alert manager and tracker if alerts are configured
            let alert_manager = app_state
                .config
                .alert_config
                .as_ref()
                .filter(|config| config.enabled)
                .map(|config| AlertManager::new(config.clone()));

            let mut alert_tracker = AlertTracker::new(app_state.validator_statuses.len());

            loop {
                interval.tick().await;

                // Fetch vote data for all validators
                let mut new_vote_data = Vec::new();

                for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
                    let validator_pair = &validator_status.validator_pair;

                    match fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey)
                        .await
                    {
                        Ok(data) => {
                            let _ = log_sender.send(LogMessage {
                                host: format!("validator-{}", idx),
                                message: format!(
                                    "Vote data fetched: last slot {}",
                                    data.recent_votes.last().map(|v| v.slot).unwrap_or(0)
                                ),
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

                // Calculate increments and track slot changes
                let mut new_increments = Vec::new();
                let mut new_slot_times = Vec::new();

                for (idx, new_data) in new_vote_data.iter().enumerate() {
                    if let Some(new) = new_data {
                        let new_last_slot = new.recent_votes.last().map(|v| v.slot);

                        // Check if this is a new slot
                        if let Some(new_slot) = new_last_slot {
                            // Check against our tracked slot time
                            let should_update_slot_time = if let Some(tracked) =
                                state.last_vote_slot_times.get(idx).and_then(|&v| v)
                            {
                                tracked.0 != new_slot // Slot has changed
                            } else {
                                true // No previous tracking
                            };

                            if should_update_slot_time {
                                new_slot_times.push(Some((new_slot, Instant::now())));
                                // Reset alert tracker since slot is advancing
                                alert_tracker.reset(idx);
                            } else {
                                // Slot hasn't changed, keep existing time
                                new_slot_times
                                    .push(state.last_vote_slot_times.get(idx).and_then(|&v| v));

                                // Check for delinquency
                                if let (Some(alert_mgr), Some((_, last_change_time))) = (
                                    alert_manager.as_ref(),
                                    state.last_vote_slot_times.get(idx).and_then(|&v| v),
                                ) {
                                    let seconds_since_vote = last_change_time.elapsed().as_secs();
                                    let threshold = app_state
                                        .config
                                        .alert_config
                                        .as_ref()
                                        .map(|c| c.delinquency_threshold_seconds)
                                        .unwrap_or(30);

                                    if seconds_since_vote >= threshold
                                        && alert_tracker.should_send_alert(idx)
                                    {
                                        // Find which node is active
                                        let active_node = if let Some(node_with_status) = app_state
                                            .validator_statuses[idx]
                                            .nodes_with_status
                                            .iter()
                                            .find(|n| n.status == crate::types::NodeStatus::Active)
                                        {
                                            &node_with_status.node
                                        } else {
                                            &app_state.validator_statuses[idx].nodes_with_status[0]
                                                .node
                                        };

                                        let is_active = app_state.validator_statuses[idx]
                                            .nodes_with_status
                                            .iter()
                                            .any(|n| n.status == crate::types::NodeStatus::Active);

                                        // Send alert
                                        if let Err(e) = alert_mgr
                                            .send_delinquency_alert(
                                                &app_state.validator_statuses[idx]
                                                    .validator_pair
                                                    .identity_pubkey,
                                                &active_node.label,
                                                is_active,
                                                new_slot,
                                                seconds_since_vote,
                                            )
                                            .await
                                        {
                                            let _ = log_sender.send(LogMessage {
                                                host: format!("validator-{}", idx),
                                                message: format!(
                                                    "Failed to send delinquency alert: {}",
                                                    e
                                                ),
                                                timestamp: Instant::now(),
                                                level: LogLevel::Error,
                                            });
                                        } else {
                                            let _ = log_sender.send(LogMessage {
                                                host: format!("validator-{}", idx),
                                                message: format!("Delinquency alert sent: {} seconds without vote", seconds_since_vote),
                                                timestamp: Instant::now(),
                                                level: LogLevel::Warning,
                                            });
                                        }
                                    }
                                }
                            }

                            // Handle increment display (visual indicator)
                            if let Some(old) = state.vote_data.get(idx).and_then(|v| v.as_ref()) {
                                if let Some(old_last_slot) = old.recent_votes.last().map(|v| v.slot)
                                {
                                    if new_slot > old_last_slot {
                                        new_increments.push(Some(Instant::now()));
                                    } else {
                                        // Keep existing increment if still valid
                                        if let Some(existing) =
                                            state.increment_times.get(idx).and_then(|&v| v)
                                        {
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
                        } else {
                            new_increments.push(None);
                            new_slot_times.push(None);
                        }
                    } else {
                        new_increments.push(None);
                        new_slot_times.push(None);
                    }
                }

                // Update previous slots
                state.previous_last_slots = state
                    .vote_data
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .and_then(|d| d.recent_votes.last().map(|v| v.slot))
                    })
                    .collect();

                state.vote_data = new_vote_data;
                state.increment_times = new_increments;
                state.last_vote_slot_times = new_slot_times;
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
                            node_pair.node_0 =
                                fetch_catchup_for_node(&ssh_pool, &node_0, ssh_key, &log_sender)
                                    .await;
                        }

                        // Fetch for node 1
                        let node_1 = &validator_status.nodes_with_status[1];
                        if let Some(ssh_key) = app_state.detected_ssh_keys.get(&node_1.node.host) {
                            node_pair.node_1 =
                                fetch_catchup_for_node(&ssh_pool, &node_1, ssh_key, &log_sender)
                                    .await;
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

        // Telegram bot polling has been removed - bot only responds to messages now
    }
}

async fn fetch_catchup_for_node(
    ssh_pool: &AsyncSshPool,
    node: &crate::types::NodeWithStatus,
    ssh_key: &str,
    log_sender: &tokio::sync::mpsc::UnboundedSender<LogMessage>,
) -> Option<CatchupStatus> {
    // Log the executable paths for debugging
    let _ = log_sender.send(LogMessage {
        host: node.node.host.clone(),
        message: format!(
            "Executables - Solana CLI: {:?}, Agave: {:?}, Fdctl: {:?}",
            node.solana_cli_executable, node.agave_validator_executable, node.fdctl_executable
        ),
        timestamp: Instant::now(),
        level: LogLevel::Info,
    });

    let solana_cli = if let Some(cli) = node.solana_cli_executable.as_ref() {
        cli.clone()
    } else if let Some(validator) = node.agave_validator_executable.as_ref() {
        // Try to derive solana CLI path from agave-validator path
        let derived = validator.replace("agave-validator", "solana");
        let _ = log_sender.send(LogMessage {
            host: node.node.host.clone(),
            message: format!(
                "Deriving solana CLI from agave-validator: {} -> {}",
                validator, derived
            ),
            timestamp: Instant::now(),
            level: LogLevel::Info,
        });
        derived
    } else if node.validator_type == crate::types::ValidatorType::Firedancer {
        // For Firedancer, try to use fdctl to get status instead
        if let Some(fdctl) = node.fdctl_executable.as_ref() {
            // Use fdctl status instead of solana catchup for Firedancer
            let status_cmd = format!("{} status", fdctl);
            match ssh_pool
                .execute_command(&node.node, ssh_key, &status_cmd)
                .await
            {
                Ok(output) => {
                    let status = if output.contains("running") {
                        "Caught up".to_string()
                    } else {
                        "Unknown".to_string()
                    };
                    return Some(CatchupStatus {
                        status,
                        last_updated: Instant::now(),
                    });
                }
                Err(_) => return None,
            }
        }
        return None;
    } else {
        // Log that we couldn't find solana CLI
        let _ = log_sender.send(LogMessage {
            host: node.node.host.clone(),
            message: "Cannot find solana CLI executable".to_string(),
            timestamp: Instant::now(),
            level: LogLevel::Error,
        });
        return None;
    };

    // First check if the solana CLI exists
    let test_args = vec!["-f", &solana_cli];
    let file_exists = match ssh_pool
        .execute_command_with_args(&node.node, ssh_key, "test", &test_args)
        .await
    {
        Ok(_) => true,
        Err(_) => false,
    };

    if !file_exists {
        let _ = log_sender.send(LogMessage {
            host: node.node.host.clone(),
            message: format!("Solana CLI not found at: {}", solana_cli),
            timestamp: Instant::now(),
            level: LogLevel::Error,
        });
        return Some(CatchupStatus {
            status: "CLI not found".to_string(),
            last_updated: Instant::now(),
        });
    }

    // Test if we can run solana --version
    let version_args = vec!["--version"];
    match ssh_pool
        .execute_command_with_args(&node.node, ssh_key, &solana_cli, &version_args)
        .await
    {
        Ok(output) => {
            let _ = log_sender.send(LogMessage {
                host: node.node.host.clone(),
                message: format!("Solana CLI version output: {}", output.trim()),
                timestamp: Instant::now(),
                level: LogLevel::Info,
            });
        }
        Err(e) => {
            let _ = log_sender.send(LogMessage {
                host: node.node.host.clone(),
                message: format!("Failed to run solana --version: {}", e),
                timestamp: Instant::now(),
                level: LogLevel::Error,
            });
        }
    }

    // Use args approach for catchup command
    let args = vec!["catchup", "--our-localhost"];

    let _ = log_sender.send(LogMessage {
        host: node.node.host.clone(),
        message: format!(
            "Executing catchup command: {} {}",
            solana_cli,
            args.join(" ")
        ),
        timestamp: Instant::now(),
        level: LogLevel::Info,
    });

    // Try executing the command with args
    match ssh_pool
        .execute_command_with_args(&node.node, ssh_key, &solana_cli, &args)
        .await
    {
        Ok(output) => {
            // Log the raw output for debugging
            let _ = log_sender.send(LogMessage {
                host: node.node.host.clone(),
                message: format!(
                    "Catchup raw output: {}",
                    output.chars().take(200).collect::<String>()
                ),
                timestamp: Instant::now(),
                level: LogLevel::Info,
            });

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
                // Try a simple test command to verify SSH is working
                let echo_args = vec!["test"];
                if let Ok(test_output) = ssh_pool
                    .execute_command_with_args(&node.node, ssh_key, "echo", &echo_args)
                    .await
                {
                    if test_output.contains("test") {
                        "No catchup output".to_string()
                    } else {
                        "SSH issue".to_string()
                    }
                } else {
                    "SSH error".to_string()
                }
            } else {
                // For debugging: show first 50 chars of output
                let debug_msg = output.trim().chars().take(50).collect::<String>();
                format!("Unknown: {}", debug_msg)
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
    // Note: log messages are now consumed by the Telegram bot if enabled

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
                handle_key_event(
                    key,
                    &app.ui_state,
                    &app.should_quit,
                    &app.view_state,
                    &app.app_state,
                )
                .await?;
            }
        }

        // Draw UI based on current view
        let ui_state_read = app.ui_state.read().await;
        let view_state_read = app.view_state.read().await;

        terminal.draw(|f| match *view_state_read {
            ViewState::Status => draw_ui(f, &ui_state_read, &app.app_state),
            ViewState::DryRunSwitch => draw_dry_run_switch_ui(f, &app.app_state),
        })?;

        drop(ui_state_read);
        drop(view_state_read);

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
    view_state: &Arc<RwLock<ViewState>>,
    _app_state: &Arc<AppState>,
) -> Result<()> {
    let _state = ui_state.write().await;

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            *should_quit.write().await = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *should_quit.write().await = true;
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            // Show validator status view
            let mut view = view_state.write().await;
            *view = ViewState::Status;
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            // Show dry-run switch view (SD = Switch Dry-run, but just 'd' in CLI)
            let mut view = view_state.write().await;
            *view = ViewState::DryRunSwitch;
        }
        // Removed 's' key handling - real switch should only be done via CLI command or Telegram
        _ => {}
    }

    Ok(())
}

/// Draw the main UI
fn draw_ui(f: &mut ratatui::Frame, ui_state: &UiState, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Validator tables take all remaining space
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Draw validator summaries
    draw_validator_summaries(f, chunks[0], ui_state, app_state);

    // Draw footer
    draw_footer(f, chunks[1], ui_state);
}

#[allow(dead_code)]
fn draw_header(f: &mut ratatui::Frame, area: Rect, _ui_state: &UiState) {
    // Just leave empty - header will be in the table border
    let header = Paragraph::new("");
    f.render_widget(header, area);
}

fn draw_validator_summaries(
    f: &mut ratatui::Frame,
    area: Rect,
    ui_state: &UiState,
    app_state: &AppState,
) {
    let validator_count = app_state.validator_statuses.len();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Percentage(100 / validator_count as u16);
            validator_count
        ])
        .split(area);

    for (idx, (validator_status, chunk)) in app_state
        .validator_statuses
        .iter()
        .zip(chunks.iter())
        .enumerate()
    {
        let vote_data = ui_state.vote_data.get(idx).and_then(|v| v.as_ref());
        let catchup_data = ui_state.catchup_data.get(idx);
        let prev_slot = ui_state.previous_last_slots.get(idx).and_then(|&v| v);
        let inc_time = ui_state.increment_times.get(idx).and_then(|&v| v);

        draw_validator_table(
            f,
            *chunk,
            validator_status,
            vote_data,
            catchup_data,
            prev_slot,
            inc_time,
        );
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
    let vote_formatted = format!(
        "{}…{}",
        vote_key.chars().take(4).collect::<String>(),
        vote_key
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );

    let identity_key = &validator_status.validator_pair.identity_pubkey;
    let identity_formatted = format!(
        "{}…{}",
        identity_key.chars().take(4).collect::<String>(),
        identity_key
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );

    let _validator_name = validator_status
        .metadata
        .as_ref()
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
            Cell::from(format!(
                "{} ({})",
                match node_0.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "🔴 UNKNOWN",
                },
                node_0.node.label
            ))
            .style(Style::default().fg(match node_0.status {
                crate::types::NodeStatus::Active => Color::Green,
                crate::types::NodeStatus::Standby => Color::Yellow,
                crate::types::NodeStatus::Unknown => Color::Red,
            })),
            Cell::from(format!(
                "{} ({})",
                match node_1.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "🔴 UNKNOWN",
                },
                node_1.node.label
            ))
            .style(Style::default().fg(match node_1.status {
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
                format!(
                    "{} {}",
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
                format!(
                    "{} {}",
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
            format!(
                "{}…{}",
                id0.chars().take(4).collect::<String>(),
                id0.chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )
        } else {
            id0.to_string()
        };
        let id1_formatted = if id1 != "Unknown" && id1.len() > 8 {
            format!(
                "{}…{}",
                id1.chars().take(4).collect::<String>(),
                id1.chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
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
            Cell::from(if node_0.swap_ready.unwrap_or(false) {
                "✅ Ready"
            } else {
                "❌ Not Ready"
            })
            .style(Style::default().fg(if node_0.swap_ready.unwrap_or(false) {
                Color::Green
            } else {
                Color::Red
            })),
            Cell::from(if node_1.swap_ready.unwrap_or(false) {
                "✅ Ready"
            } else {
                "❌ Not Ready"
            })
            .style(Style::default().fg(if node_1.swap_ready.unwrap_or(false) {
                Color::Green
            } else {
                Color::Red
            })),
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
                Cell::from(
                    node_0
                        .ledger_path
                        .as_deref()
                        .unwrap_or("N/A")
                        .split('/')
                        .last()
                        .unwrap_or("N/A"),
                ),
                Cell::from(
                    node_1
                        .ledger_path
                        .as_deref()
                        .unwrap_or("N/A")
                        .split('/')
                        .last()
                        .unwrap_or("N/A"),
                ),
            ]));
        }

        // Executable paths - shortened to save space
        if node_0.solana_cli_executable.is_some() || node_1.solana_cli_executable.is_some() {
            rows.push(Row::new(vec![
                Cell::from("Solana CLI"),
                Cell::from(shorten_path(
                    node_0.solana_cli_executable.as_deref().unwrap_or("N/A"),
                    30,
                )),
                Cell::from(shorten_path(
                    node_1.solana_cli_executable.as_deref().unwrap_or("N/A"),
                    30,
                )),
            ]));
        }

        if node_0.fdctl_executable.is_some() || node_1.fdctl_executable.is_some() {
            rows.push(Row::new(vec![
                Cell::from("Fdctl Path"),
                Cell::from(shorten_path(
                    node_0.fdctl_executable.as_deref().unwrap_or("N/A"),
                    30,
                )),
                Cell::from(shorten_path(
                    node_1.fdctl_executable.as_deref().unwrap_or("N/A"),
                    30,
                )),
            ]));
        }

        if node_0.agave_validator_executable.is_some()
            || node_1.agave_validator_executable.is_some()
        {
            rows.push(Row::new(vec![
                Cell::from("Agave Path"),
                Cell::from(shorten_path(
                    node_0
                        .agave_validator_executable
                        .as_deref()
                        .unwrap_or("N/A"),
                    30,
                )),
                Cell::from(shorten_path(
                    node_1
                        .agave_validator_executable
                        .as_deref()
                        .unwrap_or("N/A"),
                    30,
                )),
            ]));
        }

        // Catchup status
        if let Some(catchup) = catchup_data {
            let node_0_status = catchup
                .node_0
                .as_ref()
                .map(|c| c.status.clone())
                .unwrap_or_else(|| "Checking...".to_string());
            let node_1_status = catchup
                .node_1
                .as_ref()
                .map(|c| c.status.clone())
                .unwrap_or_else(|| "Checking...".to_string());

            rows.push(Row::new(vec![
                Cell::from("Catchup"),
                Cell::from(node_0_status.clone()).style(if node_0_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else if node_0_status.contains("Error") {
                    Style::default().fg(Color::Red)
                } else if node_0_status.contains("Checking") {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Yellow)
                }),
                Cell::from(node_1_status.clone()).style(if node_1_status.contains("Caught up") {
                    Style::default().fg(Color::Green)
                } else if node_1_status.contains("Error") {
                    Style::default().fg(Color::Red)
                } else if node_1_status.contains("Checking") {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Yellow)
                }),
            ]));
        }

        // Vote status row and Last voted slot row - moved to bottom
        if let Some(vote_data) = vote_data {
            let vote_status = if vote_data.is_voting {
                "✅ Voting"
            } else {
                "⚠️ Not Voting"
            };

            rows.push(Row::new(vec![
                Cell::from("Vote Status"),
                Cell::from(if node_0.status == crate::types::NodeStatus::Active {
                    vote_status
                } else {
                    "-"
                })
                .style(Style::default().fg(
                    if node_0.status == crate::types::NodeStatus::Active && vote_data.is_voting {
                        Color::Green
                    } else {
                        Color::Yellow
                    },
                )),
                Cell::from(if node_1.status == crate::types::NodeStatus::Active {
                    vote_status
                } else {
                    "-"
                })
                .style(Style::default().fg(
                    if node_1.status == crate::types::NodeStatus::Active && vote_data.is_voting {
                        Color::Green
                    } else {
                        Color::Yellow
                    },
                )),
            ]));

            // Last voted slot row
            let last_slot_info = vote_data.recent_votes.last().map(|lv| lv.slot);

            if let Some(last_slot) = last_slot_info {
                let mut slot_display = format!("{}", last_slot);

                // Add increment if applicable
                if let Some(prev) = previous_last_slot {
                    if last_slot > prev {
                        let inc = format!(" (+{})", last_slot - prev);
                        if increment_time
                            .map(|t| t.elapsed().as_secs() < 3)
                            .unwrap_or(false)
                        {
                            slot_display.push_str(&inc);
                        }
                    }
                }

                rows.push(Row::new(vec![
                    Cell::from("Last Vote"),
                    Cell::from(if node_0.status == crate::types::NodeStatus::Active {
                        slot_display.clone()
                    } else {
                        "-".to_string()
                    })
                    .style(
                        if node_0.status == crate::types::NodeStatus::Active
                            && increment_time
                                .map(|t| t.elapsed().as_secs() < 3)
                                .unwrap_or(false)
                        {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                    Cell::from(if node_1.status == crate::types::NodeStatus::Active {
                        slot_display
                    } else {
                        "-".to_string()
                    })
                    .style(
                        if node_1.status == crate::types::NodeStatus::Active
                            && increment_time
                                .map(|t| t.elapsed().as_secs() < 3)
                                .unwrap_or(false)
                        {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                ]));
            } else {
                // Show loading or no data message
                rows.push(Row::new(vec![
                    Cell::from("Last Vote"),
                    Cell::from("No votes"),
                    Cell::from("No votes"),
                ]));
            }
        } else {
            rows.push(Row::new(vec![
                Cell::from("Vote Status"),
                Cell::from("Loading..."),
                Cell::from("Loading..."),
            ]));
            rows.push(Row::new(vec![
                Cell::from("Last Vote"),
                Cell::from("Loading..."),
                Cell::from("Loading..."),
            ]));
        }
    }

    let table = Table::new(
        rows,
        vec![
            Constraint::Length(15), // Wider label column
            Constraint::Percentage(42),
            Constraint::Percentage(43),
        ],
    )
    .block(
        Block::default()
            .title(format!(
                "Identity: {} | Vote: {} | Time: {}",
                identity_formatted,
                vote_formatted,
                chrono::Local::now().format("%H:%M:%S")
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(table, area);
}

// Removed draw_logs function as logs are no longer displayed

fn draw_footer(f: &mut ratatui::Frame, area: Rect, _ui_state: &UiState) {
    let help_text = "q/Esc: Quit | v: Validator view | d: Dry-run switch | Auto-refresh: 5s";

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    f.render_widget(footer, area);
}

/// Draw the dry-run switch UI
fn draw_dry_run_switch_ui(f: &mut ratatui::Frame, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new("🔄 DRY RUN SWITCH VIEW (Triggered from Telegram)")
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, chunks[0]);

    // Content area
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(10), // Status info
            Constraint::Length(10), // Actions
            Constraint::Min(0),     // Messages
        ])
        .split(chunks[1]);

    // Current status
    if !app_state.validator_statuses.is_empty() {
        let validator_status = &app_state.validator_statuses[0];

        let active_node = validator_status
            .nodes_with_status
            .iter()
            .find(|n| n.status == crate::types::NodeStatus::Active);
        let standby_node = validator_status
            .nodes_with_status
            .iter()
            .find(|n| n.status == crate::types::NodeStatus::Standby);

        let mut status_text = vec![];
        status_text.push(
            Line::from("Current State:").style(Style::default().add_modifier(Modifier::BOLD)),
        );

        if let (Some(active), Some(standby)) = (active_node, standby_node) {
            status_text.push(
                Line::from(format!("  {} → ACTIVE", active.node.label))
                    .style(Style::default().fg(Color::Green)),
            );
            status_text.push(
                Line::from(format!("  {} → STANDBY", standby.node.label))
                    .style(Style::default().fg(Color::Yellow)),
            );
            status_text.push(Line::from(""));
            status_text.push(
                Line::from("After Switch:").style(Style::default().add_modifier(Modifier::BOLD)),
            );
            status_text.push(
                Line::from(format!("  {} → STANDBY (was active)", active.node.label))
                    .style(Style::default().fg(Color::Yellow)),
            );
            status_text.push(
                Line::from(format!("  {} → ACTIVE (was standby)", standby.node.label))
                    .style(Style::default().fg(Color::Green)),
            );
        } else {
            status_text.push(
                Line::from("Unable to determine active/standby nodes")
                    .style(Style::default().fg(Color::Red)),
            );
        }

        let status_widget = Paragraph::new(status_text).block(
            Block::default()
                .title(" Status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        f.render_widget(status_widget, content_chunks[0]);

        // Actions that would be performed
        let actions_text = vec![
            Line::from("Actions to be performed:")
                .style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from("  1. Switch active node to unfunded identity"),
            Line::from("  2. Transfer tower file to standby node"),
            Line::from("  3. Switch standby node to funded identity"),
            Line::from(""),
            Line::from("⚠️  This is a DRY RUN - No actual changes will be made").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        let actions_widget = Paragraph::new(actions_text).block(
            Block::default()
                .title(" Dry Run Actions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
        f.render_widget(actions_widget, content_chunks[1]);
    }

    // Footer
    let footer =
        Paragraph::new("Press 'q' to quit | Auto-returning to status view in 10 seconds...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
    f.render_widget(footer, chunks[2]);
}

/// Helper function to shorten paths intelligently
fn shorten_path(path: &str, max_len: usize) -> String {
    if path == "N/A" || path.len() <= max_len {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();

    // Always try to keep the filename intact
    if let Some(filename) = parts.last() {
        if filename.len() >= max_len - 3 {
            // If filename alone is too long, just truncate it
            return format!(
                "...{}",
                &filename[filename.len().saturating_sub(max_len - 3)..]
            );
        }

        // We have room for some path + filename
        let available = max_len - filename.len() - 4; // 4 for ".../filename"

        // Try to fit as much of the beginning path as possible
        let mut result = String::new();
        let mut used = 0;

        for (i, part) in parts[..parts.len() - 1].iter().enumerate() {
            if i == 0 && part.is_empty() {
                // Handle absolute paths
                continue;
            }

            let part_len = if i == 0 { part.len() + 1 } else { part.len() }; // +1 for leading /

            if used + part_len <= available {
                if i == 0 {
                    result.push('/');
                }
                result.push_str(part);
                if i < parts.len() - 2 {
                    result.push('/');
                }
                used += part_len + 1;
            } else if used == 0 && !part.is_empty() {
                // If we haven't added anything yet, at least add a shortened version of the first part
                let shortened = if part.len() > 4 { &part[..3] } else { part };
                result.push('/');
                result.push_str(shortened);
                result.push_str("...");
                break;
            } else {
                result.push_str("...");
                break;
            }
        }

        if result.is_empty() {
            result = "...".to_string();
        } else if !result.ends_with("...") && !result.ends_with('/') {
            result.push('/');
        }

        result.push_str(filename);
        result
    } else {
        path.to_string()
    }
}

/// Entry point for the enhanced UI
pub async fn show_enhanced_status_ui(app_state: &AppState) -> Result<()> {
    // Clear any startup output before starting the TUI
    print!("\x1B[2J\x1B[1;1H"); // Clear screen and move cursor to top
    std::io::stdout().flush()?;

    // Small delay to ensure all startup output is complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    let app_state_arc = Arc::new(app_state.clone());
    let mut app = EnhancedStatusApp::new(app_state_arc).await?;
    run_enhanced_ui(&mut app).await
}
