@echo off
setlocal

cd /d C:\SyndridCLI\codex-rs

set CARGO_BUILD_JOBS=1
set CARGO_PROFILE_DEV_DEBUG=0
set CARGO_PROFILE_TEST_DEBUG=0

echo Building Syndrid with reduced system impact...
start "" /belowNormal /wait cargo build -p codex-cli --bin syndrid --target-dir C:\SyndridCLI\target-syndrid-presentation-audit

endlocal