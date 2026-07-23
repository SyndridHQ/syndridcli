//! Best-effort startup sizing for the Syndrid terminal surface.

use codex_utils_cli::PublicBrand;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

const PREFERRED_COLUMNS: u16 = 130;
const PREFERRED_ROWS: u16 = 34;
const MINIMUM_COLUMNS: u16 = 120;
const MINIMUM_ROWS: u16 = 30;
static STARTUP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Viewport {
    columns: u16,
    rows: u16,
}

fn requested_viewport(current: Viewport, target: Viewport, supported: bool) -> Option<Viewport> {
    if !supported || (current.columns >= target.columns && current.rows >= target.rows) {
        return None;
    }
    Some(Viewport {
        columns: current.columns.max(target.columns),
        rows: current.rows.max(target.rows),
    })
}

fn startup_viewport_targets(current: Viewport) -> [Option<Viewport>; 2] {
    [
        requested_viewport(
            current,
            Viewport {
                columns: PREFERRED_COLUMNS,
                rows: PREFERRED_ROWS,
            },
            true,
        ),
        requested_viewport(
            current,
            Viewport {
                columns: MINIMUM_COLUMNS,
                rows: MINIMUM_ROWS,
            },
            true,
        ),
    ]
}

fn startup_request_allowed(public_brand: PublicBrand, already_requested: bool) -> bool {
    public_brand == PublicBrand::Syndrid && !already_requested
}

pub(crate) fn request_initial_viewport(public_brand: PublicBrand) {
    if !startup_request_allowed(public_brand, STARTUP_REQUESTED.swap(true, Ordering::AcqRel)) {
        return;
    }

    #[cfg(windows)]
    request_windows_viewport();
}

#[cfg(windows)]
fn request_windows_viewport() {
    if std::env::var_os("WT_SESSION").is_some() || std::env::var_os("ConEmuANSI").is_some() {
        return;
    }

    let Ok((columns, rows)) = crossterm::terminal::size() else {
        return;
    };
    let current = Viewport { columns, rows };
    let targets = startup_viewport_targets(current);

    use std::mem::zeroed;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::CONSOLE_SCREEN_BUFFER_INFO;
    use windows_sys::Win32::System::Console::COORD;
    use windows_sys::Win32::System::Console::GetConsoleScreenBufferInfo;
    use windows_sys::Win32::System::Console::GetStdHandle;
    use windows_sys::Win32::System::Console::SMALL_RECT;
    use windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE;
    use windows_sys::Win32::System::Console::SetConsoleScreenBufferSize;
    use windows_sys::Win32::System::Console::SetConsoleWindowInfo;

    // SAFETY: these calls use the process's standard output handle and only attempt
    // best-effort console geometry changes. Any unsupported-handle failure is ignored.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return;
        }
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            return;
        }
        for requested in targets.into_iter().flatten() {
            let buffer_size = COORD {
                X: info.dwSize.X.max(requested.columns as i16),
                Y: info.dwSize.Y.max(requested.rows as i16),
            };
            if SetConsoleScreenBufferSize(handle, buffer_size) == 0 {
                continue;
            }
            let window = SMALL_RECT {
                Left: 0,
                Top: 0,
                Right: requested.columns.saturating_sub(1) as i16,
                Bottom: requested.rows.saturating_sub(1) as i16,
            };
            if SetConsoleWindowInfo(handle, 1, &window) != 0 {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizing_never_shrinks_an_existing_viewport() {
        assert_eq!(
            requested_viewport(
                Viewport {
                    columns: 140,
                    rows: 40,
                },
                Viewport {
                    columns: MINIMUM_COLUMNS,
                    rows: MINIMUM_ROWS,
                },
                true,
            ),
            None
        );
    }

    #[test]
    fn sizing_requests_each_dimension_only_when_needed() {
        assert_eq!(
            requested_viewport(
                Viewport {
                    columns: 100,
                    rows: 40,
                },
                Viewport {
                    columns: MINIMUM_COLUMNS,
                    rows: MINIMUM_ROWS,
                },
                true,
            ),
            Some(Viewport {
                columns: MINIMUM_COLUMNS,
                rows: 40,
            })
        );
    }

    #[test]
    fn unsupported_hosts_fall_back_without_a_request() {
        assert_eq!(
            requested_viewport(
                Viewport {
                    columns: 80,
                    rows: 24,
                },
                Viewport {
                    columns: MINIMUM_COLUMNS,
                    rows: MINIMUM_ROWS,
                },
                false,
            ),
            None
        );
    }

    #[test]
    fn startup_request_is_syndrid_only_and_one_time() {
        assert!(startup_request_allowed(PublicBrand::Syndrid, false));
        assert!(!startup_request_allowed(PublicBrand::Syndrid, true));
        assert!(!startup_request_allowed(PublicBrand::Codex, false));
    }

    #[test]
    fn startup_prefers_the_larger_target_before_the_minimum() {
        assert_eq!(
            startup_viewport_targets(Viewport {
                columns: 100,
                rows: 24,
            })[0],
            Some(Viewport {
                columns: PREFERRED_COLUMNS,
                rows: PREFERRED_ROWS,
            })
        );
    }

    #[test]
    fn startup_minimum_is_not_requested_when_preferred_already_fits() {
        assert_eq!(
            startup_viewport_targets(Viewport {
                columns: 130,
                rows: 34,
            }),
            [None, None]
        );
    }
}
