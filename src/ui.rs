use crate::app::App;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(f.area());

    // Left pane: process list
    let items: Vec<ListItem> = app
        .processes
        .iter()
        .map(|entry| {
            let checkbox = if entry.checked { "[x]" } else { "[ ]" };
            let running_indicator = if entry.handle.is_some() {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", checkbox), running_indicator),
                Span::raw(&entry.def.name),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Processes "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));
    f.render_stateful_widget(list, chunks[0], &mut list_state);

    // Right pane: output of selected process
    let (title, output_lines) = if let Some(key) = app.selected_key() {
        let name = app
            .processes
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.def.name.as_str())
            .unwrap_or(key);
        let lines = app
            .output_buffers
            .get(key)
            .map(|buf| {
                buf.iter()
                    .map(|l| Line::from(l.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        (format!(" Output ({}) ", name), lines)
    } else {
        (" Output ".to_string(), vec![])
    };

    // Auto-scroll: show last N lines that fit
    let output_height = chunks[1].height.saturating_sub(2) as usize; // minus borders
    let skip = output_lines.len().saturating_sub(output_height);
    let visible_lines: Vec<Line> = output_lines.into_iter().skip(skip).collect();

    let output = Paragraph::new(visible_lines)
        .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(output, chunks[1]);
}
