use std::{fs, io};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::{event, execute};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use crate::{find_matching_pod, FoundPod};

#[derive(Default)]
struct App {
    pub vertical_scroll_state: ScrollbarState,
    pub horizontal_scroll_state: ScrollbarState,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
    pub show_pod_deleted_pop_up: bool,
    pub new_pod_search_pop_up: bool,
    pub input_text: String,
    pub target_pod: FoundPod,
}

pub(crate) fn klog(target: FoundPod) -> anyhow::Result<()> {

    // Find pod(s) based on supplied matcher in --all-namespaces
    // SSH can only be one, present list to user? or default to first
    // (Stretch) If multiple for log, then use tabs to open?
    // Build command, e.g. log --all-namespaces (no -n)
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let mut app = App::default();
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

fn get_pod_logs(pod: &FoundPod, lite: bool, last_container: bool) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("logs")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg(if lite {"--tail=500"} else {"--tail=-1"})
            .arg(if last_container {"--previous=true"} else {"--previous=false"})
            .output()
            .expect("failed to execute process")
    };

    let logs = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(logs)
}

fn describe_pod(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("describe")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .output()
            .expect("failed to execute process")
    };

    let describe = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(describe)
}

fn get_pods(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("get")
            .arg("pods")
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--sort-by=.status.startTime")
            .arg("--no-headers")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    };

    let tac = {
        Command::new("tac")
            .stdin(Stdio::from(output.stdout.unwrap()))
            .output()
            .expect("failed to execute process")
    };

    let pods = String::from_utf8(tac.stdout).unwrap().
        replace("Running", "✔️ Running").
        replace("Terminating", "💀️ Terminating").
        replace("CrashLoopBackOff", "🔥 CrashLoopBackOff").
        replace("ImagePullBackOff", "👻 ImagePullBackOff").
        replace("ContainerCreating", "✨️ ContainerCreating")
            .to_string();

    Ok(pods)
}

fn delete_pod(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("delete")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--wait=false")
            .output()
            .expect("failed to execute process")
    };

    let delete = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(delete)
}

fn exec_into_pod(pod: &FoundPod) -> anyhow::Result<()> {
    let _output = {
        Command::new("kubectl")
            .arg("exec")
            .arg("--stdin")
            .arg("--tty")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--")
            .arg("/bin/sh")
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process")
    };

    Ok(())
}

fn open_in_vim(pod: &FoundPod) -> anyhow::Result<()> {
    let logs = get_pod_logs(pod, false, false).unwrap();
    let name = &pod.name;
    let fname = format!("/tmp/klog_{name}");
    fs::write(&fname, logs).expect("Unable to write file");
    let _output = {
        Command::new("vim")
            .arg(&fname)
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process")
    };

    Ok(())
}
fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut fetch_new_logs = false;
    let mut fetch_prev_container_logs = false;
    let mut delete_pod_next_tick = false;
    let mut reset_scroll = true;
    let mut text = get_pod_logs(&app.target_pod, true, false).unwrap();

    loop {
        if reset_scroll {
            if text.lines().count() > 0 {
                app.vertical_scroll = text.lines().count() - 1;
            }
            reset_scroll = false;
        }

        if fetch_prev_container_logs {
            text = get_pod_logs(&app.target_pod, true, true).unwrap();
            fetch_prev_container_logs = false;
            reset_scroll = true;
        }

        if fetch_new_logs {
            text = get_pod_logs(&app.target_pod, true, false).unwrap();
            fetch_new_logs = false;
            reset_scroll = true;
        }

        if delete_pod_next_tick {
            text = text + "\nDeleted :(. Press 'q' to quit.";
            app.show_pod_deleted_pop_up = true;
            delete_pod(&app.target_pod).unwrap();
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
                            app.new_pod_search_pop_up = false;
                            app.target_pod = find_matching_pod(app.input_text.as_str()).unwrap();
                            fetch_new_logs = true;
                            app.vertical_scroll = 0;
                            app.input_text.clear();
                        }
                        KeyCode::Backspace => {
                            app.input_text.pop();
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('s') => {
                            app.new_pod_search_pop_up = true
                        }
                        KeyCode::Char('f') => {
                            fetch_new_logs = true
                        },
                        KeyCode::Char('p') => {
                            delete_pod_next_tick = true;
                        },
                        KeyCode::Char('d') => {
                            text = describe_pod(&app.target_pod).unwrap();
                            app.vertical_scroll = 0;
                        },
                        KeyCode::Char('w') => {
                            text = get_pods(&app.target_pod).unwrap();
                            app.vertical_scroll = 0;
                        },
                        KeyCode::Char('e') => {
                            terminal.clear().unwrap();
                            exec_into_pod(&app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('v') => {
                            terminal.clear().unwrap();
                            open_in_vim(&app.target_pod).unwrap();
                            terminal.clear().unwrap();
                        },
                        KeyCode::Char('l') => {
                            fetch_prev_container_logs = true;
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
    let size = f.size();
    let pod_name = &app.target_pod.name;
    let pod_ns = &app.target_pod.namespace;

    let details_content ="
    <▲ ▼ j k>\n<pgUp pgDown> - scroll \n\n <q> - quit\n
    <f> - new logs\n  <l> - last logs\n  <v> - open in vim\n <d> - description\n\n
    <e> - exec \n <p> - delete \n <w> - get pods \n <s> - switch pod";

    let chunks = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Percentage(20),
        Constraint::Percentage(80),
    ])
        .split(size);

    app.vertical_scroll_state = app.vertical_scroll_state.content_length(text.len());
    app.horizontal_scroll_state = app.horizontal_scroll_state.content_length(text.len());

    let details = Paragraph::new(details_content)
        .gray()
        .block(
            Block::bordered().gray().title("🎮 Controls").bold()
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });
    f.render_widget(details, chunks[1]);

    let paragraph = Paragraph::new(text)
        .gray()
        .block(
            Block::bordered().gray().title(format!("🤖 {pod_ns}/{pod_name}").to_owned().bold()
            ))
        .style(Style::default().fg(Color::Rgb(186, 186, 186)))
        .scroll((app.vertical_scroll as u16, app.horizontal_scroll as u16))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunks[2]);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        chunks[2],
        &mut app.vertical_scroll_state,
    );

    if app.show_pod_deleted_pop_up {
        let block = Block::bordered().title("💬 Alert").on_blue();
        let message =  Paragraph::new("Pod deleted! Press 'q' to quit. :(".white()).wrap(Wrap { trim: true });
        let area = centered_rect(60, 20, f.size());
        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(message.clone().block(block), area);
    }

    if app.new_pod_search_pop_up {
        let block = Block::bordered().title("🔎 Enter new pod matcher (ESC to close)").on_yellow();
        let area = centered_rect(60, 20, f.size());

        let input = Paragraph::new(app.input_text.as_str().white())
            .style(
                Style::default().bg(Color::Yellow)
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
