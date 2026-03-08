use std::{io};
use std::time::{Duration, Instant};
use crossterm::event::{DisableMouseCapture, Event, KeyCode};
use crossterm::{event, execute};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use rand::Rng;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::text::{Line, Span};
use ratatui::{Frame, Terminal};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use color_eyre::eyre::{Result};
use ratatui::widgets::{Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use tui_piechart::{PieChart, PieSlice};

use crate::kubectl::{self, FoundPod, KubectlRunnerAgent, get_pod_status};
use crate::cli::{self};

// FIXME support pods with no deployments??
// FIXME updater function

pub fn render_action_text<'a>(text: &'a str, action: InternalAction, last_action: &Option<InternalAction>) -> Span<'a> {
    if let Some(last_action) = last_action {
        if *last_action == action {
            return format!("{text}").blue();
        }
    }
    
    format!("{text}").white()
}

#[derive(PartialEq, Copy, Clone)]
pub enum InternalAction {
    FetchLogs,
    LastLogs,
    ViewDesc,
    Purge,
    World,
    Switch
}

#[derive(Default)]
struct App {
    pub vertical_scroll_state: ScrollbarState,
    pub horizontal_scroll_state: ScrollbarState,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
    pub is_loading: bool,
    pub emoji_frame: usize,
    pub show_pod_deleted_pop_up: bool,
    pub show_switch_error_text: bool,
    pub show_pie_chart_for_running_pods: bool,
    pub new_pod_search_pop_up: bool,
    pub input_text: String,
    pub target_pod: FoundPod,
    pub emoji: String,
    pub pod_status: String,
    pub last_action: Option<InternalAction>,
}


pub fn gui(target: FoundPod) -> Result<()> {

    // Find pod(s) based on supplied matcher in --all-namespaces
    // SSH can only be one, present list to user? or default to first
    // (Stretch) If multiple for log, then use tabs to open?
    // Build command, e.g. log --all-namespaces (no -n)
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let mut app = App::default();
    app.last_action = Some(InternalAction::FetchLogs);
    app.target_pod = target;
    let res = run_app(&mut terminal, app, tick_rate);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration
) -> Result<String> where <B as Backend>::Error: Send, <B as Backend>::Error: Sync, <B as Backend>::Error: 'static, <B as Backend>::Error: 'static {
    let mut last_tick = Instant::now();
    let mut fetch_new_logs = false;
    let mut fetch_prev_container_logs = false;
    let mut delete_pod_next_tick = false;
    let mut reset_scroll = true;
    let runner = KubectlRunnerAgent;
    let mut text = kubectl::get_pod_logs(&runner, &app.target_pod, true, false)?;
    let icons = ["🐝", "🦀", "🐋", "🐧", "🦕", "🦐", "🐬", "🦞", "🤖", "🐤", "🪿"];
    app.emoji_frame = rand::rng().random_range(0..icons.len());
    app.emoji = icons[app.emoji_frame].to_string();
    // Create a random number generator
    let mut rng = rand::rng();

    // Generate a random index within the array bounds
    let index = rng.random_range(0..icons.len());
    let emoji = icons[index];
    app.emoji = emoji.to_string();

    loop {
        app.pod_status = get_pod_status(&runner, &app.target_pod)?;

        if reset_scroll {
            if text.lines().count() > 0 {
                app.vertical_scroll = text.lines().count() - 1;
            }
            reset_scroll = false;
        }

        if fetch_prev_container_logs {
            text = kubectl::get_pod_logs(&runner, &app.target_pod, true, true)?;
            fetch_prev_container_logs = false;
            reset_scroll = true;
        }

        if fetch_new_logs {
            text = kubectl::get_pod_logs(&runner, &app.target_pod, true, false)?;
            fetch_new_logs = false;
            reset_scroll = true;
        }

        if delete_pod_next_tick {
            text = text + "\nDeleted :(. Press 'q' to quit.";
            app.show_pod_deleted_pop_up = true;
            kubectl::delete_pod(&runner, &app.target_pod).unwrap();
            delete_pod_next_tick = false;
            reset_scroll = true;
        }

        terminal.draw(|f| ui(f, &mut app, &text))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.new_pod_search_pop_up {
                    match key.code {
                        KeyCode::Char(to_insert) => {
                            app.input_text.insert(app.input_text.len(), to_insert);
                        },
                        KeyCode::Esc => {
                            app.new_pod_search_pop_up = false;
                            app.input_text.clear();
                        }
                        KeyCode::Enter => {
                            let matching_pod_result = kubectl::find_matching_pod(&runner, app.input_text.as_str());
                            match matching_pod_result {
                                Ok(matching_pod) => {
                                    app.target_pod = matching_pod;
                                    fetch_new_logs = true;
                                    app.is_loading = true;
                                    app.last_action = Some(InternalAction::FetchLogs);
                                    app.vertical_scroll = 0;
                                    app.input_text.clear();
                                    app.show_switch_error_text = false;
                                    app.new_pod_search_pop_up = false;
                                },
                                Err(_) => {
                                    app.input_text.clear(); 
                                    app.show_switch_error_text = true;
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.input_text.pop();
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => return Ok("quit".to_string()),
                        KeyCode::Char('s') => {
                            app.new_pod_search_pop_up = true;
                            app.last_action = Some(InternalAction::Switch);
                        }
                        KeyCode::Char('f') => {
                            fetch_new_logs = true;
                            
                            app.last_action = Some(InternalAction::FetchLogs);
                        },
                        KeyCode::Char('p') => {
                            delete_pod_next_tick = true;
                            app.last_action = Some(InternalAction::Purge);
                        },
                        KeyCode::Char('d') => {
                            text = kubectl::describe_pod(&runner, &app.target_pod).unwrap();
                            app.vertical_scroll = 0;
                            app.last_action = Some(InternalAction::ViewDesc);
                        },
                        KeyCode::Char('E') => {
                            terminal.clear().unwrap();
                            kubectl::edit_deployment(&runner, &app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('w') => {
                            text = kubectl::get_pods(&runner, &app.target_pod).unwrap();
                            app.vertical_scroll = 0;
                            app.show_pie_chart_for_running_pods = true;
                            app.last_action = Some(InternalAction::World);
                        },
                        KeyCode::Char('W') => {
                            text = kubectl::get_all(&runner, &app.target_pod).unwrap();
                            app.vertical_scroll = 0;
                            app.show_pie_chart_for_running_pods = false;
                            app.last_action = Some(InternalAction::World);
                        },
                        KeyCode::Char('e') => {
                            terminal.clear().unwrap();
                            kubectl::exec_into_pod(&runner, &app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('b') => {
                            terminal.clear().unwrap();
                            kubectl::debug_pod(&runner, &app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('v') => {
                            terminal.clear().unwrap();
                            cli::open_in_vim(&runner, &app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('l') => {
                            fetch_prev_container_logs = true;
                            app.last_action = Some(InternalAction::LastLogs);
                        },
                        KeyCode::Char('j') | KeyCode::Down => {
                            if app.vertical_scroll + 1 < text.lines().count() {
                                app.vertical_scroll = app.vertical_scroll.saturating_add(1);
                                app.vertical_scroll_state =
                                    app.vertical_scroll_state.position(app.vertical_scroll);
                            }
                        }
                        KeyCode::PageDown => {
                            if app.vertical_scroll + 20 < text.lines().count() {
                                app.vertical_scroll = app.vertical_scroll.saturating_add(20);
                                app.vertical_scroll_state =
                                    app.vertical_scroll_state.position(app.vertical_scroll);
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.vertical_scroll = app.vertical_scroll.saturating_sub(1);
                            app.vertical_scroll_state =
                                app.vertical_scroll_state.position(app.vertical_scroll);
                        }
                        KeyCode::PageUp => {
                            app.vertical_scroll = app.vertical_scroll.saturating_sub(20);
                            app.vertical_scroll_state =
                                app.vertical_scroll_state.position(app.vertical_scroll);
                        }
                        _ => {}
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &mut App, text: &str) {
    let size = f.area();
    let pod_name = &app.target_pod.name;
    let pod_deployment = &app.target_pod.deployment;
    let pod_ns = &app.target_pod.namespace;
    let last_action = &app.last_action;

    let details_content = vec![render_action_text("📜 [f]etch logs ", InternalAction::FetchLogs, last_action),
                                              render_action_text("📖 [l]ast logs ", InternalAction::LastLogs, last_action),
                                              Span::from("📝 [v]im logs")];

    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Percentage(100)
    ])
        .split(size);

    app.vertical_scroll_state = app.vertical_scroll_state.content_length(text.len());
    app.horizontal_scroll_state = app.horizontal_scroll_state.content_length(text.len());

    let paragraph = Paragraph::new(text)
        .gray()
        .block(
            Block::bordered().white()
            .title_top(Line::from(format!("{0} {pod_ns}/{pod_deployment}/{pod_name} ({1})", app.emoji, app.pod_status)).left_aligned().bold().white())
            .title_top(Line::from(vec![
                render_action_text("🔎 [d]esc ", InternalAction::ViewDesc, last_action),
                Span::from("💻 [e]xec "),
                Span::from("✏️ [E]dit "),
                Span::from("🐞 de[b]ug "),
                render_action_text("💀 [p]urge ", InternalAction::Purge, last_action),
                Span::from("[q]uit ✖️")]).right_aligned().white())
            .title_bottom(details_content).to_owned()
            .title_bottom(Line::from(vec![
                render_action_text("🗺️ [W/w]orld ", InternalAction::World, last_action),
                render_action_text("[s]witch ⚙️", InternalAction::Switch, last_action)]).white().right_aligned()))
        .style(Style::default().fg(Color::Rgb(186, 186, 186)))
        .scroll((app.vertical_scroll as u16, app.horizontal_scroll as u16))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, chunks[1]);

    if app.last_action == Some(InternalAction::World) && app.show_pie_chart_for_running_pods == true {
        let vertical_chunks = Layout::vertical([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
            .split(size);

        let horiz_chunks = Layout::horizontal([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
            .split(vertical_chunks[1]);
        
        // Do really bad way of counting by coutning substr matches
        let total_pods: f64 = text.lines().count() as f64;
        let running_pods: f64 = text.matches("🏃").count() as f64;
        let starting_pods: f64 = text.matches("✨️").count() as f64;
        // Create slices
        let slices = vec![
            PieSlice::new("Failed", (((&total_pods - (&starting_pods + &running_pods)) / &total_pods) * 100.0) + 0.0001, Color::Red),
            PieSlice::new("Starting", ((&starting_pods / &total_pods) * 100.0) + 0.001, Color::Blue),
            PieSlice::new("Running", ((&running_pods / &total_pods) * 100.0) + 0.001, Color::Green),
        ];

        let piechart = PieChart::new(slices)
        .show_legend(true)
        .show_percentages(true);

        f.render_widget(Block::new(),  vertical_chunks[0]);
        f.render_widget(piechart, horiz_chunks[1]);
    }

    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        chunks[1],
        &mut app.vertical_scroll_state,
    );

    if app.show_pod_deleted_pop_up {
        let block = Block::bordered().title("💬 Alert").on_blue();
        let message =  Paragraph::new("Pod deleted! Press 'q' to quit. :(".white()).wrap(Wrap { trim: true });
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(message.clone().block(block), area);
    }

    if app.new_pod_search_pop_up {
        let mut block = Block::bordered().title("🔎 Enter new pod matcher (ESC to close)").on_black();
        if app.show_switch_error_text {
            block = Block::bordered().title("❌ Pod not found! Please search again.").on_red();
        }
        let area = centered_rect(60, 20, f.area());

        let input = Paragraph::new(app.input_text.as_str().white())
            .style(
                Style::default().bg(Color::Black)
            );

        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(input.block(block), area);
    }
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
        .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
        .split(popup_layout[1])[1]
}
