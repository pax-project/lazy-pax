# lazypax — Implementation status

A checklist of everything `docs/dod.md` requires, tracking what's implemented
against what's still missing. Update this alongside any change that closes or
reopens an item — it should stay accurate rather than aspirational.

## Architecture

- [x] `pax-core` added as a dependency (path or git)
- [x] `lazypax` never shells out to the `pax` binary
- [x] All `research/` reads/writes go through `pax_core` (`Library`,
      `add_candidate`, `edit_paper`, `remove_paper`, `fetch_paper`,
      `check_library`, `sync_library`, `resolve_artifact_path`,
      `bibtex::render`, ...) — no hand-parsing/writing of `papers.nix`
      (so far: `Library::load` only, via `job::load_library`)
- [ ] A library produced/modified by `lazypax` stays fully usable from the
      plain `pax` CLI, and vice versa (nothing writes yet — first checkable
      once add/edit/remove/fetch land)
- [x] All provider network calls go through `pax_core`'s `Provider` impls —
      no direct provider API calls from `lazypax` (`search_all`/
      `search_by_author`/`search_by_doi`, via `job::spawn_search`)
- [x] Provider search / `nix` calls (`fetch`/`open`/`check`/`sync`) run off
      the UI thread/event loop (`job::spawn` wraps blocking calls in
      `tokio::task::spawn_blocking`; established with `LoadLibrary`, applies
      to every future `JobKind`)

## Screens / features

### Library view

- [x] Lists declared papers (citation key, title, authors, year)
- [x] Fetched vs. not-fetched shown at a glance (✓/✗ column from
      `artifact.hash.is_some()`)
- [x] Incremental local filtering by author/year/tag (`/` opens a vim-style
      insert-mode buffer over the citation key; `Enter` applies it via
      `pax_core::filter_papers` — three single-field calls, author/tag/year,
      unioned by citation key rather than ANDed against one string; `Esc`
      cancels an in-progress edit or clears an applied filter)
- [x] Explicit empty-library state (and a distinct
      not-yet-initialized-here state, ahead of the DoD's own wording, since
      `PaxError::Io(NotFound)` needed handling to avoid a raw error on first
      launch)

### Search view

- [x] Free-text search across all providers, provider-grouped results
      (fixed provider order — OpenAlex, Crossref, Semantic Scholar, arXiv —
      matching the `pax` CLI's own iteration order)
- [x] Per-result fields: title, authors, year, venue, DOI, PDF-availability,
      in-library indicator (`known_dois` + `normalize_doi`, same as the CLI)
- [x] A single failing provider degrades only its section (rendered as a
      one-line "provider: failed — ..." note above the results list, rather
      than blocking or omitting the other providers' results)
- [x] Author-only search mode reachable (`Tab` cycles Free → Author → DOI)
- [x] DOI-only search mode reachable

### Paper detail view

- [x] Full detail for an unresolved search result (abstract, PDF sources,
      in-library status) before adding — mirrors the `pax` CLI's own `show`
      field set/wording so a paper reads the same in both places
- [x] Full detail for a declared paper (identity, artifact status + hash,
      citation key, tags, notes) — mirrors `pax show <key>`'s own field
      set/wording, same as the candidate detail does for `pax show
      provider:id`

### Add / declare

- [x] Add a search result as a single confirmable action (`a` on the
      candidate Detail screen, no y/n prompt — resolved as non-destructive/
      reversible via remove)
- [x] Success/failure shown in-place, no silent failure or crash (status
      bar, green for success / red for error — new `StatusKind`); a
      successful add auto-triggers a `LoadLibrary` reload so the Library
      view is current when the user navigates back to it
- [x] UI states explicitly that add declares without materializing the PDF
      ("Added `<key>` — PDF not fetched yet")

### Fetch / open

- [ ] Fetch a declared paper with visible pending/progress state
- [ ] Open launches configured viewer, auto-fetching first if needed
- [ ] `pax-core` errors (no source URL, viewer launch failure) surfaced to
      the user

### Edit

- [ ] Tags editable in place
- [ ] Notes editable in place
- [ ] Citation-key rename (only if present in the `pax-core` version depended
      on)
- [ ] Identity corrections — title/author/year/DOI (only if present in the
      `pax-core` version depended on)

### Remove

- [ ] Remove a declared paper, with explicit confirmation step

### Sync / check

- [ ] `sync` reachable as a library-wide action with per-paper summary
- [ ] `check` reachable as a library-wide action with per-paper summary

### Export

- [ ] Export library as BibTeX, viewable in-app
- [ ] Export writable to a file the user chooses

### Init

- [ ] Launching outside an initialized library offers to run `init_library`
      instead of erroring out

## Non-goals — explicitly not required for MVP

New providers beyond `pax-core`, in-terminal PDF preview/rendering, PDF
annotation, full-text indexing, citation graphs, related-paper
recommendations, AI summaries, any local cache/database of its own,
config/theming beyond viewer + library path, multi-library/workspace
switching, modal editing/scripting/plugins, packaging/distribution polish,
any `pax-core` API change driven by `lazypax`'s convenience alone.

## MVP success criteria (docs/dod.md §5 — the actual "done" bar)

- [ ] Full loop runnable through `lazypax` alone, no `pax` CLI, no hand-editing
      any file: init → search → inspect → add → appears in library view →
      fetch → open → edit tags/notes → export BibTeX → remove
- [ ] Resulting `research/flake.nix` + `research/papers.nix` byte-for-byte
      identical to what the equivalent `pax` CLI sequence produces (diffed
      against a fresh copy of the same starting state)
- [ ] Library remains independently reproducible: `git clone` + `nix build`
      on a separate checkout, zero `lazypax`/`pax` code involved
- [ ] No panic/unhandled `Result`/stuck screen through the happy path or these
      error cases: provider network failure during search, adding a paper
      with no PDF source, opening/fetching without network access, quitting
      mid-action

## Decisions (docs/dod.md §6)

- [x] TUI framework chosen — `ratatui` + `crossterm`
- [x] Keybinding scheme decided — vim-style (hjkl, modal where needed)
- [x] `pax-core` dependency mode decided — `git` dependency pinned to the
      `1.0.0` tag, `default-features = false`; confirmed building

## Build order (see the MVP implementation plan for full architecture)

- [x] 1. Event-loop skeleton — `TerminalGuard`, panic hook, input thread,
      tick + `mpsc` select, `q` to quit. No `pax_core` calls yet. Verified:
      clean build/clippy, keymap unit tests pass, ran in a pty and confirmed
      alt-screen/raw-mode enter and exit are correctly paired on quit.
- [x] 2. Library view, read-only, against a fixture `research/papers.nix` —
      `job.rs`/`pax_ctx.rs` added, `LoadLibrary` job dispatched on startup.
      Distinguishes not-initialized (`PaxError::Io(NotFound)`), empty, and
      loaded (table with citation key/title/authors/year/fetched-glyph)
      states; `j`/`k`/`gg`/`G` selection navigation. Verified: clean
      build/clippy, 14 unit tests (keymap, `App::update` job-outcome
      handling, `LibraryScreen` selection wrap-around) all pass; ran against
      hand-written fixtures for all three states in a pty with clean exit,
      no panics. Note: this sandbox's pty doesn't report a real terminal
      size, so the actual rendered layout couldn't be visually confirmed
      here — worth a quick `cargo run` in a real terminal to eyeball it.
- [x] 3. In-memory `/`-filter via `filter_papers` — introduced `Mode`
      (Normal/Insert) and `InsertTarget` to `app.rs`, and the generic
      Insert-mode key routing (`KeyChar`/`Backspace`/`Enter`/`Esc` ->
      `InputChar`/`InputBackspace`/`SubmitInput`/`CancelInput`) that every
      future text field reuses. `/` opens the buffer prefilled with the
      currently-applied query; `Enter` commits, `Esc` discards the buffer
      (or clears an applied filter from Normal mode). A no-match state
      ("No papers match the current filter") is distinct from the
      genuinely-empty-library state. Verified: clean build/clippy, 27 unit
      tests pass (up from 14 — keymap's Insert-mode routing,
      `filter_by_query`'s per-field union semantics, `App`'s
      enter/type/submit/cancel/clear flows); ran end-to-end in a pty with
      realistic inter-keystroke delays (immediate, unspaced piped input hits
      a well-known terminal ambiguity — a bare `Esc` immediately followed by
      another byte can parse as an Alt+key chord instead of two separate
      keys — which isn't specific to this app and doesn't occur with real
      human typing speed).
- [x] 4. Search view + provider-grouped rendering — `Screen::Search` added,
      reached via `S` from Library (auto-enters query-edit Insert mode);
      `Tab` cycles Free/Author/DOI, `Enter` dispatches the matching
      `JobKind`, `Esc` goes back to Library. Results render as a flat list
      in fixed provider order with a global selection cursor; a failing
      provider's error is a one-line note above the list, not a blocker.
      **Real, unplanned architectural fix**: `pax_core`'s search functions
      turned out to be `!Send` (the `crossref` crate's HTTP client holds an
      `Rc` internally), so they can't run on `tokio::spawn`'s default
      multi-threaded executor as the plan assumed — discovered as a genuine
      compiler error, not a judgment call. Fixed by giving each search job
      its own OS thread with a small current-thread Tokio runtime
      (`job::spawn_search`), still fully off the main event loop either way.
      `known_dois` is now called directly inside that same thread rather
      than via a separate `spawn_blocking` (already off the main runtime).
      Verified: clean build/clippy, 42 unit tests pass (up from 27); ran all
      three search modes end-to-end against the real provider APIs (network
      available in this sandbox) with clean exits, no panics — free-text
      "actor model", author "Hewitt", and DOI
      10.1112/plms/s2-42.1.230 — plus Esc-back-to-Library after a real
      search.
- [x] 5. Detail view for a `CandidateWork` — new `Screen::Detail`, reached
      via `Enter`/`l` on a Search result (no new job: the candidate is
      already in memory from the search). Introduced `screen_stack` +
      `push_screen`/`go_back` for general back-navigation, since `Esc` from
      Detail needs to return to Search specifically, not always Library —
      Library → Search → Detail → back → back now unwinds correctly. Field
      set/wording mirrors `pax show`'s own candidate display. Verified:
      clean build/clippy, 52 unit tests pass (up from 42); ran the full
      Library → Search (live "actor model" query) → Detail → back → back →
      quit path end-to-end with a clean exit, no panics.
- [x] 6. Add flow (`JobKind::AddCandidate`) — `a` on the candidate Detail
      screen spawns `add_candidate` via the same dedicated-thread pattern as
      search (also touches Crossref, so also `!Send`). Added a minimal
      `App::job_running` guard (`start_job()`): a second job-triggering
      action pressed while one is in flight is rejected with a status
      message instead of racing — concretely, double-pressing `a` before
      the first `add_candidate` returns could otherwise declare the same
      paper twice under two different citation keys, since each call
      independently loads the library, generates a fresh key, and saves.
      A successful add shows "Added `<key>` — PDF not fetched yet" (green)
      and chains a `LoadLibrary` reload; a failure shows the `PaxError` in
      red. Verified: clean build/clippy, 58 unit tests pass (up from 52,
      covering the guard, both outcomes, and the reload chaining).
      **Real, unplanned finding — a test-harness artifact, not a code
      defect**: driving the full flow through `script`-wrapped ptys (this
      sandbox's only available pty tool) reliably hangs on the *second*
      sequential network job in one session (confirmed via tracing:
      `add_candidate`'s dedicated thread starts and builds its runtime, but
      `block_on` never returns). Isolated with a standalone diagnostic
      binary built from the exact same `job.rs` — it reproduces the hang
      under `script`, and reliably completes both a search *and* an add
      (writing correctly to `papers.nix`) when run directly with no pty
      wrapper, twice in a row. No other pty tool (`unbuffer`/`socat`/
      `expect`/`python3`) is available here to double-check interactively.
      Confidence instead rests on: the diagnostic proving the real `job.rs`
      code path end-to-end outside `script`, and the 58 unit tests covering
      `App`'s reaction to every outcome. Worth a real-terminal `cargo run`
      sanity check on your end; flagging rather than papering over it.
- [x] 7. Detail view for a declared `Paper` — reused `Screen::Detail`/
      `OpenDetail` from step 5 rather than adding new machinery:
      `DetailSubject` gained a `Declared(Paper)` variant, and `Enter`/`l`
      now opens detail from Library too (previously Search-only), via a new
      `LibraryScreen::selected_paper()` mirroring `selected_candidate()`.
      No new job — the paper is already in memory from `LoadLibrary`. Field
      set/wording mirrors `pax show <key>`. Verified: clean build/clippy,
      66 unit tests pass (up from 58); ran Library → Detail (via both
      `Enter` and `l`) → back → quit end-to-end against a two-paper fixture
      with a clean exit and no panics (pure local state, so unaffected by
      step 6's `script`-pty network-job caveat).
- [ ] 8. Fetch + Open (suspend/resume bracket + `resolve_for_open`)
- [ ] 9. Edit (tags/notes, then rename + identity corrections)
- [ ] 10. Remove (via `ConfirmPrompt` overlay)
- [ ] 11. Sync/Check (shared `reports.rs` rendering)
- [ ] 12. Export (bibtex render + optional file write)
- [ ] 13. Init-on-launch

## Critical path

Steps 1–7 of the build order are done (see above) — `lazypax` can search,
inspect (both unresolved candidates and declared papers), and declare
papers end-to-end. `Screen::Detail` now serves two subjects
(`Candidate`/`Declared`) through one screen, reached from either Search or
Library — worth keeping in mind for step 8 (Fetch/Open), which is also
triggered from a paper's detail view and needs to know which subject it's
looking at (only `Declared` papers are fetchable/openable). Reminders
carried over: `job::spawn_network`'s dedicated-thread pattern is required
for any job awaiting a `pax_core` async fn touching Crossref
(`show_reference`, when a future step needs it); `App::job_running`'s guard
is general-purpose — route any new job-triggering action through
`start_job()`. Next: step 8, Fetch + Open — the first step that shells out
to `nix` itself, and the first to need the terminal suspend/resume bracket
around the external PDF viewer.
