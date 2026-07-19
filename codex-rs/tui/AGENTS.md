# TUI instructions

## Compatibility

Codex rendering and behavior must remain unchanged. Gate Syndrid presentation with `PublicBrand::Syndrid`. A TUI task must not modify authentication, protocols, storage, sandboxing, approvals, model wire values, or update behavior. Reuse existing session and model-update mechanisms.

## Approved design references

The authoritative external banner directory is `C:\SyndridBanners`. Its approved surfaces are the home/landing page, model selector, effort selector, and terminal input box. Do not reinterpret or broadly redesign these surfaces without approval.

## Visual system

- Use a dark neutral canvas with warm firefly-gold accents.
- Use gold only for active selection, focus, active values, and mascot glow; keep borders and secondary text neutral.
- Use approved box-drawing characters. Never use literal tab characters for layout.
- Calculate widths responsively and account for Unicode display width.
- Handle wide, medium, narrow, zero-width, and one-width cases without panic or overflow.
- Avoid focus stealing and unrelated process launches.

## Verification

For visual changes:

1. Run focused rendering and interaction tests.
2. Run `cargo check -p codex-tui`.
3. Build the affected executable.
4. Launch the actual TUI manually.
5. Compare it with the approved banner and test narrow resizing.
6. Compare against Codex.

Do not claim visual success if an interactive TTY was unavailable.
