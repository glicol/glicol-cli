use std::{
    io,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

pub use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{symbols::border, widgets::{Clear, List, ListItem, ListState}};
pub use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, GraphType},
    Frame, Terminal,
};

use crate::{recent_lines::ShareableRecentLinesBuffer, RB_SIZE};

pub enum ExitStatus {
    KeepAudio,
    ExitAll
}

pub(crate) fn run_app<B: Backend>(
    console_buffer: ShareableRecentLinesBuffer,
    terminal: &mut Terminal<B>,
    tick_rate: Duration,
    sample_data: Arc<crate::SampleData>,
    // use_scope: bool,
    info: String,
    // right: Arc<AtomicPtr<f32>>
) -> io::Result<ExitStatus> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &sample_data, &info, &console_buffer))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => return Ok(ExitStatus::KeepAudio),
                    KeyCode::Char('p' | ' ') => {
                        // this is only modified from this thread (just read from the other), so we
                        // don't have to worry about ordering or using like a swap/exchange loop
                        // when doing this here
                        let old = sample_data.paused.load(Ordering::Relaxed);
                        sample_data.paused.store(!old, Ordering::Relaxed);
                    },
                    KeyCode::Char('q') => return Ok(ExitStatus::ExitAll),
                    _ => ()
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // app.on_tick();
            last_tick = Instant::now();
        }
    }
}

fn ui(
    f: &mut Frame,
    sample_data: &Arc<crate::SampleData>,
    info: &str,
    console_buffer: &ShareableRecentLinesBuffer,
) {
    let mut data = [0.0; RB_SIZE];
    let mut data2 = [0.0; RB_SIZE];
    let ptr = sample_data.left_ptr.load(Ordering::Acquire);
    let ptr2 = sample_data.right_ptr.load(Ordering::Acquire);

    let mut idx = sample_data.index.load(Ordering::Acquire);

    for i in 0..RB_SIZE {
        data[RB_SIZE - 1 - i] = unsafe { ptr.add(idx).read() };
        data2[RB_SIZE - 1 - i] = unsafe { ptr2.add(idx).read() };
        if idx == 0 {
            idx = RB_SIZE - 1; // read from the tail
        } else {
            idx -= 1;
        }
    }

    let left: Vec<(f64, f64)> = data
        .into_iter()
        .enumerate()
        .map(|(x, y)| (x as f64, y as f64))
        .collect();
    let right: Vec<(f64, f64)> = data2
        .into_iter()
        .enumerate()
        .map(|(x, y)| (x as f64, y as f64))
        .collect();

    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Min(4),
            ]
            .as_ref(),
        )
        .split(size);

    let cap = sample_data.capacity.load(Ordering::Acquire);
    let portion = f32::from_bits(cap).clamp(0.0, 1.0);
    // print!(" cap {:?}, portion {:?}", cap, portion);

    // let label = Span::styled(
    //     format!("{:.2}%", portion * 100.0),
    //     Style::default()
    //         .fg(Color::White)
    //         .add_modifier(Modifier::ITALIC | Modifier::BOLD),
    // );

    let label = Span::styled(
        "press esc to exit tui, or q to exit program",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::ITALIC | Modifier::BOLD),
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" render capacity ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(portion as f64)
        .label(label)
        .use_unicode(true);
    f.render_widget(gauge, chunks[0]);

    let x_labels = vec![Span::styled(
        "[0, 200]",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let datasets = vec![
        Dataset::default()
            .name("left")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Blue))
            .data(&left),
        Dataset::default()
            .name("right")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Red))
            .data(&right),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::styled(
                    info.replace("SupportedStreamConfig", ""),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::NONE),
        )
        .x_axis(
            Axis::default()
                .title("X Axis")
                .style(Style::default().fg(Color::Gray))
                .labels(x_labels)
                .bounds([0., 200.]),
        )
        .y_axis(
            Axis::default()
                .title("Y Axis")
                .style(Style::default().fg(Color::Gray))
                .labels(vec![
                    Span::styled("-1", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("0"),
                    Span::styled("1", Style::default().add_modifier(Modifier::BOLD)),
                ])
                .bounds([-1., 1.]),
        );
    f.render_widget(chart, chunks[1]);

    if sample_data.paused.load(Ordering::Relaxed) {
        let frame_area = f.size();
        let width = 10;
        let height = 3;
        let block_rect = Rect {
            x: (frame_area.width - width) / 2,
            y: (frame_area.height - height) / 2,
            width,
            height
        };

        let block = Block::bordered()
            .border_set(border::DOUBLE)
            .border_style(Style::new().fg(Color::White));

        let mut label_rect = block.inner(block_rect);

        f.render_widget(Clear, block_rect);
        f.render_widget(block, block_rect);

        let label = Span::styled(
            "PAUSED",
            Style::new()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD)
        );

        label_rect.x += 1;

        f.render_widget(label, label_rect);
    }

    render_console(f, chunks[2], console_buffer);
}

fn render_console(f: &mut Frame<'_>, area: Rect, console_buffer: &ShareableRecentLinesBuffer) {
    let guard = console_buffer.0.lock().expect("poisoned lock");

    let items = guard
        .read()
        .map(Line::raw)
        .map(ListItem::new)
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::bordered()
            .title("console")
            .border_set(border::ROUNDED)
    );
    let mut state = ListState::default().with_selected(Some(list.len()));

    f.render_stateful_widget(list, area, &mut state);
}
