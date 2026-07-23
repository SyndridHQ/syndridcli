use super::*;
use crate::bottom_pane::SyndridContextUsage;
use crate::render::renderable::Renderable;
use crate::syndrid_live_state::ActivityEvent;
use crate::syndrid_live_state::ActivityStatus;
use crate::syndrid_live_state::ChangeEntry;
use crate::syndrid_live_state::DataQuality;
use crate::syndrid_live_state::LiveSessionState;
use crate::syndrid_live_state::LiveView;
use crate::syndrid_live_state::VerificationItem;
use crate::syndrid_live_state::VerificationStatus;
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
            "session",
            "activity",
            "changes",
            "verification",
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
            .map(super::super::slash_command::SlashCommand::command)
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
    assert_eq!(screen.filtered_commands().len(), 17);
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
    for _ in 0..16 {
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

#[test]
fn live_surfaces_render_observed_sections_and_quality_states() {
    let state = LiveSessionState {
        view: LiveView::Activity,
        session_id: Some("session-123456789".to_string()),
        task: Some("Inspect workspace".to_string()),
        activity: vec![ActivityEvent {
            event_type: "command".to_string(),
            summary: "cargo check completed".to_string(),
            status: ActivityStatus::Passed,
            ..Default::default()
        }],
        changes: crate::syndrid_live_state::ChangesProjection {
            files: vec![ChangeEntry {
                path: "src/main.rs".to_string(),
                change_type: Some("modified".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        },
        verifications: vec![VerificationItem {
            name: "cargo check".to_string(),
            status: VerificationStatus::Unavailable,
            evidence_quality: DataQuality::Unavailable,
            ..Default::default()
        }],
        ..Default::default()
    };

    assert!(render_text(120, 30, &SyndridScreen::live(state.clone())).contains("ACTIVITY"));
    assert!(
        render_text(
            80,
            24,
            &SyndridScreen::live(LiveSessionState {
                view: LiveView::Changes,
                ..state.clone()
            })
        )
        .contains("FILES")
    );
    let verification = render_text(
        40,
        18,
        &SyndridScreen::live(LiveSessionState {
            view: LiveView::Verification,
            ..state
        }),
    );
    assert!(verification.contains("Unavailable"));
    assert!(
        verification
            .lines()
            .all(|line| unicode_width::UnicodeWidthStr::width(line) <= 40)
    );
}

fn status_snapshot() -> SyndridStatusSnapshot {
    SyndridStatusSnapshot {
        identity: "SyndridCLI".to_string(),
        session_id: Some("019f6a12-3456-7890-abcd-ef0123456789".to_string()),
        workspace: Some(r"C:\SyndridCLI".to_string()),
        branch: Some("phase-3b/model-effort-controls".to_string()),
        state: Some("Working".to_string()),
        current_task: Some("Stabilize Phase 3B presentation".to_string()),
        model: "gpt-5.6-luna".to_string(),
        reasoning: Some("medium".to_string()),
        profile: Some("strict".to_string()),
        sandbox: "Workspace".to_string(),
        approval: "Ask".to_string(),
        plan_mode: false,
        context: Some(SyndridContextUsage {
            used_tokens: 42_100,
            context_window: 258_000,
        }),
        tokens_sparked: Some(968_239_501),
        running_subagents: 0,
        token_usage: Some(crate::token_usage::TokenUsage {
            input_tokens: 57_321,
            cached_input_tokens: 18_450,
            output_tokens: 8_623,
            reasoning_output_tokens: 0,
            total_tokens: 84_492,
        }),
    }
}

#[test]
fn status_dashboard_keeps_wide_matrix_centered_and_aligned() {
    let screen = SyndridScreen::status_with_snapshot(Some(status_snapshot()));
    let rendered = render_text(120, 30, &screen);
    let rows = rendered.lines().collect::<Vec<_>>();
    let heading = rows
        .iter()
        .find(|row| row.contains("SESSION") && row.contains("EXECUTION"))
        .expect("paired status headings");
    let left = heading.find("SESSION").expect("session heading");
    let right = heading.find("EXECUTION").expect("execution heading");
    assert_eq!(right - left, 57);
    assert_eq!(left, 27);
    assert_eq!(rows.iter().filter(|row| row.contains('│')).count(), 16);
    let divider_positions = rows
        .iter()
        .filter(|row| row.contains('│'))
        .map(|row| row.find('│'))
        .collect::<Vec<_>>();
    assert!(
        divider_positions
            .iter()
            .all(|position| matches!(position, Some(18) | Some(76)))
    );
    assert!(
        rows.iter()
            .all(|row| unicode_width::UnicodeWidthStr::width(*row) <= 120)
    );

    let lines = screen.status_dashboard_lines(120);
    assert_eq!(lines.iter().position(|line| line.width() > 0), Some(1));
    assert_eq!(
        lines
            .iter()
            .position(|line| line.to_string().contains("MODEL")),
        Some(11)
    );
    assert_eq!(
        lines
            .iter()
            .position(|line| line.to_string().contains("POLICY")),
        Some(19)
    );
    assert_eq!(lines.iter().rposition(|line| line.width() > 0), Some(23));
}

#[test]
fn status_session_id_is_always_single_line_and_truncates_safely() {
    let mut snapshot = status_snapshot();
    snapshot.session_id = Some("session-abcdefghijklmnopqrstuvwxyz-0123456789".to_string());
    let screen = SyndridScreen::status_with_snapshot(Some(snapshot));

    for (width, height) in [(120, 30), (112, 30), (100, 28)] {
        let rendered = render_text(width, height, &screen);
        let id_row = rendered
            .lines()
            .find(|row| row.contains("ID") && row.contains('│'))
            .expect("session id row");
        assert!(id_row.contains('…'));
        assert!(unicode_width::UnicodeWidthStr::width(id_row) <= usize::from(width));
        assert!(!id_row.contains("\n"));
    }
}

#[test]
fn status_session_id_uses_middle_truncation() {
    let id = "019f7f18-1234-5678-90ab-cdef01234567";
    assert_eq!(middle_truncate(id, 64), id);

    let truncated = middle_truncate(id, 15);
    assert_eq!(
        unicode_width::UnicodeWidthStr::width(truncated.as_str()),
        15
    );
    assert!(truncated.starts_with("019f7f"));
    assert!(truncated.ends_with("234567"));
    assert!(!truncated.starts_with('…'));

    let unicode = middle_truncate("界a界b界", 3);
    assert!(unicode_width::UnicodeWidthStr::width(unicode.as_str()) <= 3);
    assert_eq!(unicode.matches('…').count(), 1);
}

#[test]
fn status_dashboard_stacks_below_wide_layout_threshold() {
    let screen = SyndridScreen::status_with_snapshot(Some(status_snapshot()));
    let rendered = render_text(60, 80, &screen);
    let rows = rendered.lines().collect::<Vec<_>>();
    let headings = [
        "SESSION",
        "EXECUTION",
        "MODEL",
        "TOKENS",
        "POLICY",
        "HEALTH",
    ];
    let positions = headings
        .iter()
        .map(|heading| {
            rows.iter()
                .position(|row| row.contains(heading))
                .expect("stacked heading")
        })
        .collect::<Vec<_>>();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(rows.iter().all(|row| row.matches('│').count() <= 1));
}

#[test]
fn status_screen_escape_completes_as_cancelled() {
    let mut screen = SyndridScreen::status_with_snapshot(Some(status_snapshot()));
    screen.handle_key_event(KeyEvent::from(KeyCode::Esc));
    assert_eq!(screen.completion(), Some(ViewCompletion::Cancelled));
}

#[test]
fn usage_uses_exact_session_accounting_without_cached_double_counting() {
    let screen = SyndridScreen::usage_with_snapshot(Some(status_snapshot()));
    let lines = screen.usage_dashboard_lines(120);
    let text = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("84,492") || text.contains("84492"));
    assert!(text.contains("57,321") || text.contains("57321"));
    assert!(text.contains("18,450") || text.contains("18450"));
    assert!(text.contains("Exact"));
    assert!(text.contains("Derived"));
    assert!(!text.contains("102,942"));
}

#[test]
fn usage_quality_and_unavailable_fields_render_at_all_viewports() {
    let screen = SyndridScreen::usage_with_snapshot(Some(status_snapshot()));
    let source = screen
        .usage_dashboard_lines(120)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(source.contains("QUALITY"));
    assert!(source.contains("Unavailable"));
    for (width, height) in [(120, 30), (100, 28), (80, 24), (60, 20), (40, 18)] {
        let rendered = render_text(width, height, &screen);
        assert!(
            rendered.contains("SESSION"),
            "missing session at {width}x{height}"
        );
        assert!(
            rendered
                .lines()
                .all(|line| unicode_width::UnicodeWidthStr::width(line) <= usize::from(width))
        );
    }
}

#[test]
fn usage_context_percentage_handles_zero_denominator() {
    let mut snapshot = status_snapshot();
    snapshot.context = Some(SyndridContextUsage {
        used_tokens: 42,
        context_window: 0,
    });
    let screen = SyndridScreen::usage_with_snapshot(Some(snapshot));
    let text = screen
        .usage_dashboard_lines(120)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Context percentage") && text.contains("—"));
    assert!(!text.contains("inf"));
}
