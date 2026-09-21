# lazypax — build log

Step-by-step history of the MVP build: what was built in what order, real
bugs found and fixed along the way, and test-harness quirks encountered
during verification. This is a historical record, not a live-updated
document — for the current-state checklist (what's implemented vs. missing
today), see `docs/status.md`.

## Build order

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
- [x] 8. Fetch + Open (suspend/resume bracket + `resolve_for_open`) — `f`/`o`
      on Library or Detail(Declared) spawn `JobKind::FetchPaper`/
      `ResolveForOpen`, both plain sync `pax_core` calls that shell out to
      `nix`, so they use the original `tokio::spawn` + `spawn_blocking`
      pattern (not `spawn_network` — they don't touch Crossref). A
      successful fetch patches the in-memory `Declared` detail's hash
      *only if its citation key still matches* (the user may have
      navigated to a different paper while the job was in flight — a real
      bug this test caught, not a hypothetical) and chains a `LoadLibrary`
      reload; a successful open does the same reload plus
      `Effect::LaunchViewer`. `TerminalGuard` gained `suspend()`/`resume()`;
      `main.rs`'s `handle_effect` now suspends, runs
      `Command::new(viewer).arg(path).status()` in the foreground, resumes,
      and feeds the result back through `update()` in the same tick via
      `Message::ViewerExited` — no channel round-trip needed for something
      that already happened synchronously.
      **Two real, unplanned findings from verification — one a genuine bug
      fixed, one a test-harness limit documented:**
      1. **Bug, fixed**: `resume()` originally called `Terminal::clear()`,
         which saves/restores the cursor position via a DSR query
         (`ESC[6n`) round-tripped through the terminal — and reproducibly
         failed here with `"the cursor position could not be read within a
         normal duration"`, killing the whole process (exit 1) right after
         a real, successful `nix build` and viewer launch. Fixed by using
         `Terminal::resize(current_size)` instead: confirmed in ratatui's
         own source that the fullscreen-viewport branch never reads cursor
         position at all, while still doing the same full clear + buffer
         reset. This isn't `script`-specific — a DSR query can stall under
         other pty/multiplexer setups too — so it's a real fix, not a
         workaround for the test environment.
      2. **Test-harness limit, not a code defect**: with that fix in place,
         a *single* `f` or a *single* `o` per `script` session works
         reliably (confirmed repeatedly, including a real `nix store
         prefetch-file` + `nix build` + stub-viewer invocation with a
         correct resolved path), but `f` immediately followed by `o` in the
         *same* session silently fails to launch the viewer — no crash, no
         printed error, `papers.nix` still gets the correct hash from the
         fetch. Isolated with the same standalone-diagnostic technique from
         step 6: driving `job::spawn`'s `FetchPaper` then `ResolveForOpen`
         directly (no pty at all) completes both correctly and quickly,
         confirming the job logic itself is sound. This is the same
         "two sequential subprocess/network jobs in one `script` session"
         pattern step 6 hit with search-then-add, just for a different job
         pair — a `script` limitation, not specific to Crossref/`!Send`
         this time, since `FetchPaper`/`ResolveForOpen` use plain
         `spawn_blocking`, not the dedicated-thread network pattern.
      Verified: clean build/clippy, 77 unit tests pass (up from 66,
      covering the guard, both success/failure paths for both jobs, the
      citation-key-mismatch protection, and `ViewerExited`'s two outcomes);
      real end-to-end runs against actual `research/` fixtures (built from
      `pax`'s own templates, git-tracked as `nix build` requires) with a
      real small HTTP URL — `nix store prefetch-file` and `nix build` both
      genuinely ran and produced correct, verifiable results on disk.
- [x] 9. Edit (tags/notes, then rename + identity corrections) — new
      `Screen::Edit`, reached via `e` from Library or Detail(Declared);
      `j`/`k` move focus across 7 fields (Tags/Notes/Citation
      key/Title/Authors/Year/DOI), `i`/`Enter` opens the focused field's
      buffer (reusing the existing Insert-mode machinery — one new
      `InsertTarget` variant per field, one shared `EditScreen.buffer`),
      `a`/`x` add/remove tags directly (no per-tag cursor — a deliberate
      MVP simplification: `x` always removes the *last* tag rather than a
      selected one), `w` saves via `JobKind::EditPaper`
      (`pax_core::edit_paper` — sync/local-only, no network, so the plain
      `tokio::spawn`+`spawn_blocking` pattern, not `spawn_network`), `Esc`
      discards and goes back without saving.
      **Real bug caught by my own tests, fixed before it shipped**: the
      first `build_edits` used "field is non-empty" to decide whether to
      include it in the saved `PaperEdits` — but `title`/`authors` are
      *always* non-empty on a real paper, so every save would have resent
      them regardless of whether the user touched them. Fixed by tracking
      each single-value field's original loaded value and only including
      it when the current value actually differs (and never as an empty
      string — `pax-core` has no way to clear a field back to `null`
      anyway, so an emptied buffer means "leave unchanged", not "blank
      it"). A successful save shows "Updated `<key>`" and returns to
      Library (rather than staying on a Detail/Edit session that might now
      reference a stale, renamed citation key) plus a `LoadLibrary` reload;
      a failure shows the `PaxError` and stays on the Edit screen so the
      user can retry.
      Verified: clean build/clippy, 103 unit tests pass (up from 93,
      including the caught-and-fixed dirty-tracking bug and a permanent
      regression test for it); ran real end-to-end saves against a live
      `research/papers.nix` — tag add, notes set, citation-key rename, and
      title correction — each confirming every *untouched* field (DOI,
      venue, hash, etc.) came back byte-for-byte identical, not just the
      touched ones changing correctly.
- [x] 10. Remove (via `ConfirmPrompt` overlay) — new `App.confirm:
      Option<ConfirmPrompt>` (`{ message, on_confirm: Action }`); `keymap`
      gained a `confirm_active` parameter checked *before* mode/screen
      dispatch at all, so every other key — including navigation — is
      swallowed while a prompt is up (`y`/`Enter` → `ConfirmYes`,
      `n`/`Esc` → `ConfirmNo`). `d` on Library/Detail(Declared) opens the
      prompt; `ConfirmYes` re-dispatches `on_confirm`
      (`Action::RemoveConfirmed`) through `apply()`, which re-resolves
      "the selected paper" at confirm time rather than freezing it in the
      prompt — safe *only* because `confirm_active` blocks navigation, a
      dependency made explicit by a test that calls `App::apply()`
      directly (bypassing `keymap`) to show the target *would* drift if
      that guard weren't there. `JobKind::RemovePaper` uses
      `pax_core::remove_paper` — sync/local-only like `EditPaper`, not
      `spawn_network`. Success returns to Library (same reasoning as
      Edit — avoid stale screen state) plus a `LoadLibrary` reload.
      Mechanical note: extending `map_key`'s signature with
      `confirm_active` touched all ~44 existing call sites in
      `keymap.rs`'s own tests — done via a scoped `sed` limited to the
      test module (verified the real signature/implementation were edited
      by hand, not swept up in the same substitution).
      Verified: clean build/clippy, 113 unit tests pass (up from 105); ran
      both paths end-to-end against a real two-paper `research/papers.nix`
      — `d` → `n` left both papers untouched, `d` → `y` removed exactly
      the selected one and left the other byte-for-byte intact.
- [x] 11. Sync/Check (shared `reports.rs` rendering) — new `Screen::SyncReport`/
      `CheckReport`, reached via `s`/`c` from Library only (library-wide,
      not selection-scoped like the other paper actions).
      `JobKind::Sync`/`Check` wrap `pax_core::sync_library`/`check_library`
      — sync/local-only (they shell out to `nix` per paper internally, but
      that's one job dispatch from `App`'s perspective, not several), so
      the plain `tokio::spawn`+`spawn_blocking` pattern again. One generic
      `draw_report<T>` in `ui/reports.rs` renders either report type via a
      per-type `to_line` closure (`sync_line`/`check_line`, each mirroring
      the `pax` CLI's own wording) — same "shared renderer, per-type
      formatting function" shape `ui/detail.rs` already used for
      Candidate/Declared. `j`/`k` scroll via the same selected-index +
      wraparound pattern as every other list screen (two small free
      functions, `move_index_down`/`up`, factor the now-four-times-repeated
      logic). A successful sync shows a count, pushes the report screen,
      and reloads Library (hashes may have changed); check does the same
      but never reloads, since it's read-only by design.
      Verified: clean build/clippy, 127 unit tests pass (up from 113); ran
      both against a real two-paper library (one fetchable, one without a
      `source_url`) — `sync` correctly fetched the one that could be and
      left the other's per-paper error isolated rather than failing the
      whole run; `check` came back with `papers.nix` byte-for-byte
      unchanged, confirming it's genuinely read-only; `j` scroll and
      `Esc`-back from the report screen both worked cleanly.
- [x] 12. Export (bibtex render + optional file write) — new
      `Screen::Export`, reached via `E` from Library only.
      `pax_core::bibtex::render` is pure (no I/O, just formats a `String`
      from data already in memory), so it's called directly when entering
      the screen — same treatment as `filter_papers`, no job. Always
      renders every declared paper via a new
      `LibraryScreen::declared_papers()`, deliberately bypassing the
      applied filter (`visible_papers()`) — export covers the whole
      library per the DoD, not whatever subset happens to be filtered in.
      `i`/`p` opens the one editable field (a file path, via
      `InsertTarget::ExportPath` — reusing the same generic Insert-mode
      machinery, one more field, mechanical); `w` in Normal mode spawns
      `JobKind::ExportToFile`, a plain `std::fs::write` — not a
      `pax_core` call at all, but still routed through `job.rs`, keeping
      the "every blocking I/O call lives in one auditable place" rule
      intact even for non-`pax_core` I/O. Guarded a real near-miss caught
      before it compiled: `w` was briefly wired into the *Insert*-mode key
      table (would have made typing a path containing the letter "w"
      impossible to type past), caught by re-reading the diff and moved to
      the Normal-mode table where `w` belongs — a permanent regression
      test locks in that a literal "w" while editing still types as a
      character, not save.
      Verified: clean build/clippy, 138 unit tests pass (up from 127); ran
      end-to-end against a real two-paper library — `E` → `i` → typed path
      → `Enter` → `w` produced a file with correct, valid BibTeX for both
      papers (including the with-DOI-and-tags and without-either cases).
- [x] 13. Init-on-launch — `i` on Library, only when
      `LibraryState::NotInitialized` (guarded in `App::trigger_init`, not
      just by the keybinding's screen scope — pressing `i` again once a
      library is loaded is correctly a no-op). `JobKind::Init` wraps
      `pax_core::init_library`, confirmed to return `std::io::Result<()>`
      (not `PaxError`, unlike every other job so far) — a real, previously-
      noted API-shape difference (see `docs/dod.md`'s own corrections
      section), handled with its own `JobOutcome::Initialized` variant
      rather than forcing it through the `PaxError` shape every other
      outcome uses. Success reloads via `LoadLibrary`, same pattern as
      every other library-mutating job.
      **This is the last item on the entire MVP build order** — all 13
      steps are now done.
      Verified: clean build/clippy, 143 unit tests pass (up from 139); ran
      against a genuinely empty directory (no `research/` at all — the one
      scenario every prior step deliberately avoided, since all of them
      developed against pre-created fixtures) and confirmed
      `research/flake.nix`/`papers.nix` were created byte-identical to
      `pax`'s own templates.
      **Capstone verification**: chained all nine real operations — init,
      search, add, fetch, edit, export, check, remove, and a final reload
      — through `job.rs` directly (the standalone-diagnostic technique from
      steps 6/8, since search-then-add and any two network/subprocess jobs
      in one interactive session still hit this sandbox's known `script`
      pty limitation). Every step used real backends: live provider search
      (resilient to 2 of 4 providers failing, matching `search_all`'s own
      per-provider isolation), a real `nix store prefetch-file` call that
      hit a genuine HTTP 405 from the source and correctly surfaced it as
      an error rather than crashing (not a scripted failure — an actual
      dead link found live), a real edit, a real BibTeX export reflecting
      those edits, a real `check` correctly reporting "not fetched"
      (consistent with the failed fetch), a real remove, and a final
      library confirmed empty. This is the strongest evidence gathered in
      this build that the full pipeline is correct end-to-end, not just
      each command in isolation.

## Standing lessons

Still worth carrying into any future work on this codebase:

- **The DSR cursor-query fix in `TerminalGuard::resume()`** (use `resize()`,
  not `clear()`) is a real, general-purpose fix, not a `script`-specific
  workaround — keep it in mind for any future place that might call
  `Terminal::clear()` directly.
- **This sandbox's `script`-based pty testing reliably fails on two
  sequential *subprocess/network-touching* jobs run in one session**
  (confirmed repeatedly: search-then-add in step 6, fetch-then-open in
  step 8, search-then-add again in step 13's golden-path attempt); sync/
  local-only jobs (`EditPaper`, `RemovePaper`, `LoadLibrary`, `Init`, a
  single `FetchPaper`) don't hit this. When a multi-job pty run seems to
  fail, cross-check with the standalone-diagnostic technique (driving
  `job::spawn` directly, no pty — used successfully four times now) before
  assuming a code defect.
- **Dirty-tracking, not emptiness, decides what a save includes** — any
  field prefilled from existing data needs "changed from original" to
  decide whether it's sent, not "is this non-empty" (caught in step 9, see
  its entry above for the concrete bug this avoided).
- **Confirmation overlays only work because `keymap`'s `confirm_active`
  check runs before everything else** — `App::apply()` itself has no such
  guard (see step 10's `keymap_not_app_is_what_keeps_the_confirmed_target_
  from_drifting` test); any new event-entry path into `App::update()` that
  bypassed `keymap` would reopen this.
- **Not every `pax_core` job returns `PaxError`** — `init_library` returns
  plain `std::io::Result<()>` (step 13); check a function's actual
  signature before assuming the common shape.
