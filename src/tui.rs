use std::{
    io,
    sync::{
        atomic::Ordering,
        Arc,
    },
    time::{Duration, Instant},
};

pub use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::widgets::{List, ListItem, ListState};
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

pub(crate) fn run_app<B: Backend>(
    console_buffer: ShareableRecentLinesBuffer,
    terminal: &mut Terminal<B>,
    tick_rate: Duration,
    sample_data: Arc<crate::SampleData>,
    // use_scope: bool,
    info: String,
    // right: Arc<AtomicPtr<f32>>
) -> io::Result<()> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            ui(
                f,
                &sample_data,
                &info,
                &console_buffer,
            )
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Esc = key.code {
                    return Ok(());
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
        format!("press esc to exit tui"),
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
        format!("[0, 200]"),
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

    render_console(f, chunks[2], console_buffer);
}

fn render_console(f: &mut Frame<'_>, area: Rect, console_buffer: &ShareableRecentLinesBuffer) {
    let guard = console_buffer
        .0
        .lock()
        .expect("poisoned lock");

    let items = guard
        .read()
        .map(Line::raw)
        .map(ListItem::new)
        .collect::<Vec<_>>();

    let list = List::new(items).block(Block::default().title("console").borders(Borders::TOP));
    let mut state = ListState::default().with_selected(Some(list.len()));

    f.render_stateful_widget(list, area, &mut state);
}
