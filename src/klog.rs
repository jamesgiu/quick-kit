use std::{fs, io};
use std::process::{Command};
use std::time::{Duration, Instant};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::{event, execute};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};
use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use crate::{FoundPod};

#[derive(Default)]
struct App {
    pub vertical_scroll_state: ScrollbarState,
    pub horizontal_scroll_state: ScrollbarState,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
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
    let app = App::default();
    let res = run_app(&mut terminal, app, tick_rate, &target);

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

fn delete_pod(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("delete")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
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
    tick_rate: Duration,
    target: &FoundPod
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut fetch_new_logs = false;
    let mut fetch_prev_container_logs = false;
    let mut delete_pod_next_tick = false;
    let mut reset_scroll = true;

    let mut logs = get_pod_logs(target, true, false).unwrap();

    loop {
        if reset_scroll {
            if logs.lines().count() > 0 {
                app.vertical_scroll = logs.lines().count() - 1;
            }
            reset_scroll = false;
        }

        if fetch_prev_container_logs {
            logs = get_pod_logs(target, true, true).unwrap();
            fetch_prev_container_logs = false;
            reset_scroll = true
        }

        if fetch_new_logs {
            logs = get_pod_logs(target, true, false).unwrap();
            fetch_new_logs = false;
            reset_scroll = true
        }

        if delete_pod_next_tick {
            logs = logs + "\nDeleted :(. Press 'q' to quit.";
            delete_pod(target).unwrap();
            delete_pod_next_tick = false;
            reset_scroll = true;
        }

        terminal.draw(|f| ui(f, &mut app, &target, &logs))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('f') => {
                        fetch_new_logs = true
                    },
                    KeyCode::Char('p') => {
                        delete_pod_next_tick = true
                    },
                    KeyCode::Char('d') => {
                        logs = describe_pod(target).unwrap();
                        app.vertical_scroll = 0;
                    },
                    KeyCode::Char('e') => {
                        terminal.clear().unwrap();
                        exec_into_pod(target).unwrap();
                        terminal.clear().unwrap();
                    },
                    KeyCode::Char('v') => {
                        terminal.clear().unwrap();
                        open_in_vim(target).unwrap();
                        terminal.clear().unwrap();
                    },
                    KeyCode::Char('l') => {
                        fetch_prev_container_logs = true;
                    },
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.vertical_scroll + 1 < logs.lines().count() {
                            app.vertical_scroll = app.vertical_scroll.saturating_add(1);
                            app.vertical_scroll_state =
                                app.vertical_scroll_state.position(app.vertical_scroll);
                        }
                    }
                    KeyCode::PageDown => {
                        if app.vertical_scroll + 20 < logs.lines().count() {
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
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &mut App, target: &FoundPod, logs: &str) {
    let size = f.size();
    let pod_name = &target.name;
    let pod_ns = &target.namespace;

    let details_content ="
    ▲ ▼ j k to scroll. \n
    pgUp pgDown to scroll furiously.\n
    Press 'q' to quit.\n
    Press 'f' to fetch new logs.\n
    Press 'l' to fetch the last container's logs.\n
    Press 'd' to fetch pod description.\n
    Press 'e' to exec into the pod.\n
    Press 'p' to delete the pod.\n
    Press 'v' to open the full logs in vim.\n";

    let chunks = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Percentage(20),
        Constraint::Percentage(80),
    ])
        .split(size);

    app.vertical_scroll_state = app.vertical_scroll_state.content_length(logs.len());
    app.horizontal_scroll_state = app.horizontal_scroll_state.content_length(logs.len());

    let details = Paragraph::new(details_content)
        .gray()
        .block(
            Block::bordered().gray().title("🎮 Controls").bold()
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });
    f.render_widget(details, chunks[1]);

    let paragraph = Paragraph::new(logs)
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
}