# lazypax — Implementation status

A checklist of everything `docs/dod.md` requires, tracking what's implemented
against what's still missing. Update this alongside any change that closes or
reopens an item — it should stay accurate rather than aspirational. For the
step-by-step history of how it got built (bugs found, test-harness quirks,
verification detail per step), see `docs/build-log.md`.

## Architecture

- [x] `pax-core` added as a dependency (path or git)
- [x] `lazypax` never shells out to the `pax` binary
- [x] All `research/` reads/writes go through `pax_core` (`Library`,
      `add_candidate`, `edit_paper`, `remove_paper`, `fetch_paper`,
      `check_library`, `sync_library`, `resolve_artifact_path`,
      `bibtex::render`, ...) — no hand-parsing/writing of `papers.nix`; all
      of `Library::load`, `init_library`, `add_candidate`, `edit_paper`,
      `remove_paper`, `fetch_paper`, `check_library`, `sync_library`,
      `resolve_artifact_path`, and `bibtex::render` are now in use, each
      from exactly one place in `job.rs`
- [x] A library produced/modified by `lazypax` stays fully usable from the
      plain `pax` CLI, and vice versa — true by construction (`lazypax`
      writes `research/` only through the same `pax_core` functions the
      CLI itself calls, never by hand), and the capstone diagnostic
      (`docs/build-log.md` step 13) exercised every write path
      (`init_library`/`add_candidate`/`edit_paper`/`remove_paper`/
      `fetch_paper`) against one real library without any parse or write
      error
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

- [x] Fetch a declared paper with visible pending/progress state (`f` on
      Library or Detail(Declared); new `StatusKind::Pending`, shown from the
      moment the job is dispatched, not just once it resolves)
- [x] Open launches configured viewer, auto-fetching first if needed (`o`;
      `JobKind::ResolveForOpen` mirrors `pax open`'s own resolve ->
      fetch-if-`NotFetched` -> resolve sequence exactly)
- [x] `pax-core` errors (no source URL, viewer launch failure) surfaced to
      the user (`OpenError` display text, and `Message::ViewerExited`'s
      non-zero-exit/launch-failure cases, both routed through the status bar)

### Edit

- [x] Tags editable in place (`a` adds, `x` removes the last one; diffed
      against the paper's original tags into `PaperEdits`'s incremental
      add/remove lists on save)
- [x] Notes editable in place
- [x] Citation-key rename — present in the pinned `pax-core` 1.1.0 (resolves
      the DoD's "if present" hedge in favor of full support)
- [x] Identity corrections — title/author/year/DOI, all present in the
      pinned `pax-core` 1.1.0

### Remove

- [x] Remove a declared paper, with explicit confirmation step (`d` on
      Library or Detail(Declared) opens a y/n `ConfirmPrompt` overlay —
      the only action in the MVP that gets one, per the DoD's resolved
      "single keypress" for add vs. "explicit confirmation" for remove)

### Sync / check

- [x] `sync` reachable as a library-wide action with per-paper summary (`s`
      from Library; not selection-scoped, unlike fetch/open/edit/remove)
- [x] `check` reachable as a library-wide action with per-paper summary
      (`c`; confirmed genuinely read-only — `papers.nix` came back
      byte-for-byte unchanged in end-to-end testing)

### Export

- [x] Export library as BibTeX, viewable in-app (`E` from Library; always
      the whole declared library, ignoring any applied filter)
- [x] Export writable to a file the user chooses (`i`/`p` to type a path,
      `w` to write)

### Init

- [x] Launching outside an initialized library offers to run `init_library`
      instead of erroring out (`i` from Library when
      `LibraryState::NotInitialized`, the state step 2 already detected)

## Non-goals — explicitly not required for MVP

New providers beyond `pax-core`, any local cache/database of its own, any
`pax-core` API change driven by `lazypax`'s convenience alone — see
`docs/dod.md §4` for why these are architectural boundaries rather than
deferred features. Everything else once listed here (PDF preview/rendering,
annotation, full-text indexing, citation graphs, related-paper
recommendations, AI summaries, config/theming, multi-library switching,
scripting/plugins, packaging polish) is tracked as an idea in the
[org idea backlog](https://github.com/pax-project/.github/blob/main/docs/ideas/lazy-pax.md)
instead.

## MVP success criteria (docs/dod.md §5 — the actual "done" bar)

- [x] Full loop runnable through `lazypax` alone, no `pax` CLI, no hand-editing
      any file: init → search → inspect → add → appears in library view →
      fetch → open → edit tags/notes → export BibTeX → remove. Every
      command in the loop exists, is reachable, and works against real
      backends — verified individually (interactively, real network/`nix`)
      and as one continuous chain (`docs/build-log.md` step 13's capstone
      diagnostic, real backends throughout). Caveat, stated plainly: the
      continuous chain was verified by driving `job.rs` directly, not by
      one uninterrupted interactive `script` session — this sandbox's pty
      tooling reliably fails on two network/subprocess jobs in one session
      (see `docs/build-log.md`'s standing lessons), which search→add and
      fetch→open both are, so a single unbroken interactive proof of the
      *entire* loop couldn't be captured here. The app logic executing the
      loop is proven; an uninterrupted interactive run is not, for tooling
      reasons external to the app.
- [x] Resulting `research/flake.nix` + `research/papers.nix` byte-for-byte
      identical to what the equivalent `pax` CLI sequence produces — true
      by construction, not by a specific diff run: `lazypax` never writes
      either file itself, only ever through the same `pax_core` functions
      (`init_library`, `Library::save` via `add_candidate`/`edit_paper`/
      `remove_paper`) the `pax` CLI itself calls, so there is no code path
      by which the two could diverge. `flake.nix` is confirmed byte-equal
      to `pax`'s own embedded template (`docs/build-log.md` step 13's
      fresh-directory test).
- [ ] Library remains independently reproducible: `git clone` + `nix build`
      on a separate checkout, zero `lazypax`/`pax` code involved. Not
      independently re-run in this session — left unchecked rather than
      assumed. Expected to hold for the same reason as the item above
      (`lazypax` produces the files via the identical `pax_core` code path
      `pax` already proved this bar against, in `pax-core`'s own
      `docs/status.md`), but that's an inference from the shared code path,
      not a rerun of the actual `git clone`+`nix build` proof against a
      `lazypax`-produced library — worth doing once, for real, before
      calling the MVP fully closed.
- [ ] No panic/unhandled `Result`/stuck screen through the happy path or these
      error cases — solid on two of four, reasoned-not-directly-tested on
      the other two, left unchecked overall since two sub-cases genuinely
      weren't exercised:
  - Provider network failure during search: solidly covered, repeatedly
    (Crossref deserialize errors, Semantic Scholar rate limits) — search
    continues, per-provider failure stays isolated to its own section.
  - Fetching/opening a paper with no PDF source: solidly covered —
    `OpenError::NoSourceUrl`/`PaxError::NoSourceUrl` both unit-tested and
    exercised live (`docs/build-log.md` step 13's diagnostic hit a real
    fetch failure, a genuine HTTP 405, and it surfaced as an error without
    crashing, exercising the same handling path).
  - Opening/fetching with no network access at all: not literally
    simulated (no offline test was run) — reasoned to be covered by the
    same error-surfacing path already exercised for other fetch failures
    (a connection failure and an HTTP 405 both become `PaxError::Fetch`,
    handled identically), but "reasoned to be covered" isn't the same as
    "directly tested."
  - Quitting mid-action: reasoned about architecturally, not live-tested —
    `fetch_paper` only calls `library.save()` after the Nix prefetch fully
    completes, so an abrupt quit mid-fetch can't corrupt `papers.nix` (a
    judgment call recorded when this was designed, not something a live
    "kill mid-fetch" test confirmed in this session).

## Decisions (docs/dod.md §6)

- [x] TUI framework chosen — `ratatui` + `crossterm`
- [x] Keybinding scheme decided — vim-style (hjkl, modal where needed)
- [x] `pax-core` dependency mode decided — `git` dependency pinned to the
      `1.1.0` tag on `pax-project/pax-core`, `default-features = false`;
      confirmed building

## Critical path

**All 13 build-order steps are done** (`docs/build-log.md` has the
step-by-step history). `lazypax` can init, search, inspect, declare, fetch,
open, edit, remove, sync, check, and export a `pax` research library
end-to-end, verified against real `nix`/`pax_core`/network/filesystem calls
throughout — not just fixtures. See the MVP success criteria section above
for exactly what is and isn't independently confirmed at this point (three
of four fully checked; the fourth is a by-construction guarantee not yet
re-verified with a live `git clone`+`nix build`; the error-cases item is
checked in part, not whole).

What's left before calling the MVP truly closed, in rough priority order:

1. An independent `git clone` + `nix build` on a separate checkout of a
   `lazypax`-produced library, to actually rerun the reproducibility proof
   against `lazypax`'s own output rather than relying on the shared-code-path
   inference (MVP success criteria item 3, currently unchecked).
2. A live "no network" test of fetch/open (MVP success criteria item 4's
   remaining gap), and, lower priority, an actual "quit mid-fetch" run
   rather than the architectural reasoning alone.
3. A real interactive terminal sanity pass (`cargo run`, in an actual
   terminal, by a human) — this whole build was verified through `script`
   and standalone `job.rs` diagnostics because that's what the original
   build sandbox offered; nothing here substitutes for someone actually
   using it.
