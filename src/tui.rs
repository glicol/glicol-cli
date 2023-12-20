use std::{
    io,
    sync::{
        atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
pub use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    symbols,
    text::Span,
    widgets::{Axis, Chart, Dataset, GraphType},
    widgets::{Block, Borders, Gauge, Sparkline},
    Frame, Terminal,
};

use crate::RB_SIZE;

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    tick_rate: Duration,
    samples_l_ptr: Arc<AtomicPtr<f32>>,
    samples_r_ptr: Arc<AtomicPtr<f32>>,
    sampels_index: Arc<AtomicUsize>,
    // use_scope: bool,
    info: String,
    capacity: Arc<AtomicU32>,
    // right: Arc<AtomicPtr<f32>>
) -> io::Result<()> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            ui(
                f,
                &samples_l_ptr,
                &samples_r_ptr,
                &sampels_index,
                &info,
                &capacity,
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

pub fn ui(
    f: &mut Frame,
    samples_l: &Arc<AtomicPtr<f32>>, // block step length
    samples_r: &Arc<AtomicPtr<f32>>,
    frame_index: &Arc<AtomicUsize>,
    info: &str,
    capacity: &Arc<AtomicU32>, // use_scope: bool
) {
    let mut data = [0.0; RB_SIZE];
    let mut data2 = [0.0; RB_SIZE];
    let ptr = samples_l.load(Ordering::Acquire);
    let ptr2 = samples_r.load(Ordering::Acquire);

    let mut idx = frame_index.load(Ordering::Acquire);

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
        .constraints([Constraint::Percentage(10), Constraint::Percentage(90)].as_ref())
        .split(size);

    let cap = capacity.load(Ordering::Acquire);
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
}
