use super::*;
use crate::render::renderable::Renderable;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::HashSet;

#[test]
fn curated_command_browser_matches_design_order() {
    let screen = SyndridScreen::command_browser(false);
    let commands = screen
        .commands
        .iter()
        .map(|command| command.command())
        .collect::<Vec<_>>();

    assert_eq!(
        commands,
        vec![
            "model",
            "effort",
            "plan",
            "permissions",
            "status",
            "usage",
            "goal",
            "review",
            "diff",
            "resume",
            "new",
            "compact",
            "mcp",
        ]
    );
}

#[test]
fn all_command_browser_has_no_duplicate_visible_commands() {
    let screen = SyndridScreen::command_browser(true);
    let commands = screen
        .commands
        .iter()
        .map(|command| command.command())
        .collect::<Vec<_>>();
    let unique = commands.iter().copied().collect::<HashSet<_>>();

    assert_eq!(commands.len(), unique.len());
}

#[test]
fn command_browser_filter_keeps_matching_rows() {
    let mut screen = SyndridScreen::command_browser(false);
    screen.filter = "perm".to_string();

    assert_eq!(
        screen
            .filtered_commands()
            .into_iter()
            .map(|command| command.command())
            .collect::<Vec<_>>(),
        vec!["permissions"]
    );
}

fn render_text(width: u16, height: u16, screen: &SyndridScreen) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    screen.render(area, &mut buffer);
    let mut rendered = String::new();
    for y in 0..height {
        for x in 0..width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    rendered
}

#[test]
fn command_browser_renders_selected_row_at_all_supported_heights() {
    for (width, height) in [(120, 30), (80, 12), (36, 6), (36, 3)] {
        let screen = SyndridScreen::command_browser(false);
        let rendered = render_text(width, height, &screen);
        assert!(
            rendered.contains("MODEL"),
            "missing row at {width}x{height}"
        );
    }
}

#[test]
fn command_browser_filter_and_backspace_restore_rows() {
    let mut screen = SyndridScreen::command_browser(false);
    screen.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    screen.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    screen.handle_key_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(screen.filtered_commands().len(), 1);
    screen.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    screen.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(screen.filtered_commands().len() > 1);
    screen.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(screen.filtered_commands().len(), 13);
}

#[test]
fn command_browser_tab_switches_and_escape_completes() {
    let mut screen = SyndridScreen::command_browser(false);
    screen.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(screen.title(), "ALL COMMANDS");
    screen.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(screen.completion(), Some(ViewCompletion::Cancelled));
}

#[test]
fn default_command_browser_matches_reference_geometry_at_supported_viewports() {
    for (width, height) in [(120, 30), (100, 28), (80, 24), (60, 20), (40, 18)] {
        let rendered = render_text(width, height, &SyndridScreen::command_browser(false));
        let rows = rendered.lines().collect::<Vec<_>>();
        let title = rows[0].find("SYNDRID COMMANDS").expect("title");
        let expected_title = (usize::from(width) - "SYNDRID COMMANDS".len()) / 2;
        assert!((title as isize - expected_title as isize).abs() <= 1);
        assert!(rows.last().is_some_and(|row| row.contains("ENTER TO OPEN")));
        assert!(rows.iter().any(|row| row.contains("# MODEL")));

        if width >= 50 {
            let separators = ["MODEL", "EFFORT", "PLAN", "PERMISSIONS"]
                .iter()
                .map(|name| {
                    rows.iter()
                        .find(|row| row.contains(name))
                        .and_then(|row| row.find('│'))
                        .expect("command separator")
                })
                .collect::<Vec<_>>();
            assert!(separators.windows(2).all(|pair| pair[0] == pair[1]));
        } else {
            assert!(rows.iter().any(|row| row.contains("CHOOSE THE ACTIVE")));
        }
    }

    let mut screen = SyndridScreen::command_browser(false);
    for _ in 0..12 {
        screen.handle_key_event(KeyEvent::from(KeyCode::Down));
    }
    let rendered = render_text(40, 18, &screen);
    assert!(rendered.contains("# MCP"), "{rendered}");
}

#[test]
fn all_command_browser_matches_reference_grid_at_supported_viewports() {
    for (width, height) in [(120, 30), (100, 28), (80, 24), (60, 20), (40, 18)] {
        let rendered = render_text(width, height, &SyndridScreen::command_browser(true));
        let rows = rendered.lines().collect::<Vec<_>>();
        assert!(rows[0].contains("ALL COMMANDS"));
        assert!(rows.iter().any(|row| row.contains("# MODEL")));
        assert!(
            rows.iter()
                .any(|row| row.contains("TAB FOR SYNDRID COMMANDS"))
        );
        assert!(
            !rows.iter().any(|row| row.contains("│")),
            "all-command design uses aligned cells, not row separators at {width}x{height}"
        );

        if width >= 120 {
            assert!(
                rows.iter()
                    .any(|row| row.contains("SYNDRID") && row.contains("SESSION"))
            );
            assert!(
                rows.iter()
                    .any(|row| row.contains("TOOLS") && row.contains("SETTINGS"))
            );
        } else if width >= 80 {
            assert!(
                rows.iter()
                    .any(|row| row.contains("SYNDRID") && row.contains("SESSION"))
            );
            assert!(
                rows.iter()
                    .any(|row| row.contains("WORKFLOW") && row.contains("TOOLS"))
            );
        } else {
            assert!(rows.iter().any(|row| row.contains("SYNDRID")));
            assert!(rows.iter().any(|row| row.contains("# MODEL")));
        }
    }

    let mut screen = SyndridScreen::command_browser(true);
    if screen
        .commands
        .iter()
        .any(|command| command.command() == "sandbox-add-read-dir")
    {
        let target = screen
            .commands
            .iter()
            .position(|command| command.command() == "sandbox-add-read-dir")
            .expect("sandbox command was detected");
        for _ in 0..target {
            screen.handle_key_event(KeyEvent::from(KeyCode::Down));
        }
        let rendered = render_text(120, 30, &screen);
        assert!(rendered.contains("SANDBOX READ DIR"));
    }
}

#[test]
fn all_command_browser_category_keys_move_between_groups() {
    let mut screen = SyndridScreen::command_browser(true);
    screen.handle_key_event(KeyEvent::from(KeyCode::Right));
    assert_eq!(screen.selected_command(), Some(SlashCommand::New));
    screen.handle_key_event(KeyEvent::from(KeyCode::Left));
    assert_eq!(screen.selected_command(), Some(SlashCommand::Model));
}

#[test]
fn informational_screens_render_visible_sections() {
    let status = render_text(80, 12, &SyndridScreen::status());
    assert!(status.contains("SESSION"));
    assert!(status.contains("EXECUTION"));

    let usage = render_text(80, 12, &SyndridScreen::usage());
    assert!(usage.contains("ACCOUNT"));
    assert!(usage.contains("TOKENS"));
}
