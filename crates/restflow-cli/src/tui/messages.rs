use crate::tui::app::{ChatMessage, ChatTui, MessageRole};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

pub fn build_message_items(app: &ChatTui) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();

    for msg in &app.messages {
        items.push(render_message(msg, app.show_tools));
    }

    if app.is_streaming && !app.streaming_text.is_empty() {
        let content = Line::from(vec![
            Span::raw("🤖 "),
            Span::styled(app.streaming_text.clone(), Style::default().fg(Color::Cyan)),
            Span::styled("▋", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]);
        items.push(ListItem::new(vec![content]));
    }

    items
}

fn render_message(message: &ChatMessage, show_tools: bool) -> ListItem<'static> {
    let (icon, style) = match message.role {
        MessageRole::User => ("🧑", Style::default().fg(Color::Green)),
        MessageRole::Assistant => ("🤖", Style::default().fg(Color::Cyan)),
        MessageRole::System => ("📌", Style::default().fg(Color::Yellow)),
        MessageRole::Error => ("❌", Style::default().fg(Color::Red)),
    };

    let mut lines = vec![Line::from(vec![
        Span::raw(format!("{} ", icon)),
        Span::styled(message.content.clone(), style),
    ])];

    if show_tools {
        for tool in &message.tool_calls {
            let status = if tool.success { "ok" } else { "error" };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("🔧 {} ({})", tool.name, status),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::raw(tool.input.clone()),
            ]));
            if tool.expanded && let Some(output) = &tool.output {
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::raw(output.clone()),
                ]));
            }
        }
    }

    ListItem::new(lines)
}
