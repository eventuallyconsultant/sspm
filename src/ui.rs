use ansi_to_tui::IntoText as _;
use crate::app::{App, ProcessStatus};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

pub fn draw(f: &mut Frame, app: &App) {
  let chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
    .split(f.area());

  // Left pane: process list (multi-line items)
  let items: Vec<ListItem> = app
    .processes
    .iter()
    .map(|entry| {
      let (checkbox, style) = match entry.status {
        ProcessStatus::Running => ("[x]", Style::default().fg(Color::Green)),
        ProcessStatus::Failed => ("[-]", Style::default().fg(Color::Red)),
        ProcessStatus::Stopped => ("[ ]", Style::default().fg(Color::DarkGray)),
      };

      let mut lines = vec![
        // Line 1: checkbox + name
        Line::from(vec![Span::styled(format!(" {} ", checkbox), style), Span::raw(entry.def.display_name(&entry.key))]),
        // Line 2: command (dimmed)
        Line::from(Span::styled(format!("     {}", &entry.def.command), Style::default().fg(Color::DarkGray))),
      ];

      // Line 3: last exit code (red, only when non-zero)
      if let Some(code) = entry.last_exit_code {
        lines.push(Line::from(Span::styled(format!("     last exit code: {}", code), Style::default().fg(Color::Red))));
      }

      ListItem::new(lines)
    })
    .collect();

  let list = List::new(items)
    .block(Block::default().borders(Borders::ALL).title(" Processes "))
    .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

  let mut list_state = ListState::default();
  list_state.select(Some(app.selected));
  f.render_stateful_widget(list, chunks[0], &mut list_state);

  // Scrollbar for process list
  if app.processes.len() > 1 {
    let mut scrollbar_state = ScrollbarState::new(app.processes.len().saturating_sub(1)).position(app.selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    f.render_stateful_widget(scrollbar, chunks[0], &mut scrollbar_state);
  }

  // Right pane: output of selected process
  let (title, output_lines) = if let Some(key) = app.selected_key() {
    let name = app.processes.iter().find(|e| e.key == key).map(|e| e.def.display_name(&e.key)).unwrap_or(key);
    let lines = app
      .output_buffers
      .get(key)
      .map(|buf| {
        let raw = buf.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n");
        raw.into_text().map(|t| t.lines).unwrap_or_default()
      })
      .unwrap_or_default();
    (format!(" Output ({}) — Shift+drag to select ", name), lines)
  } else {
    (" Output ".to_string(), vec![])
  };

  // Scrollable log: log_scroll=0 means tail (auto-follow)
  let output_height = chunks[1].height.saturating_sub(2) as usize; // minus borders
  let total = output_lines.len();
  let max_scroll = total.saturating_sub(output_height);
  let clamped_scroll = app.log_scroll.min(max_scroll);
  let scroll_y = max_scroll.saturating_sub(clamped_scroll);

  let output = Paragraph::new(output_lines)
    .block(Block::default().borders(Borders::ALL).title(title))
    .scroll((scroll_y as u16, 0));

  f.render_widget(output, chunks[1]);

  // Scrollbar for log pane
  if total > output_height {
    let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_y);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    f.render_stateful_widget(scrollbar, chunks[1], &mut scrollbar_state);
  }
}
