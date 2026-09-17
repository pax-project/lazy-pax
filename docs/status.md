# lazypax — Implementation status

A checklist of everything `docs/dod.md` requires, tracking what's implemented
against what's still missing. Update this alongside any change that closes or
reopens an item — it should stay accurate rather than aspirational.

## Architecture

- [ ] `pax-core` added as a dependency (path or git)
- [ ] `lazypax` never shells out to the `pax` binary
- [ ] All `research/` reads/writes go through `pax_core` (`Library`,
      `add_candidate`, `edit_paper`, `remove_paper`, `fetch_paper`,
      `check_library`, `sync_library`, `resolve_artifact_path`,
      `bibtex::render`, ...) — no hand-parsing/writing of `papers.nix`
- [ ] A library produced/modified by `lazypax` stays fully usable from the
      plain `pax` CLI, and vice versa
- [ ] All provider network calls go through `pax_core`'s `Provider` impls —
      no direct provider API calls from `lazypax`
- [ ] Provider search / `nix` calls (`fetch`/`open`/`check`/`sync`) run off
      the UI thread/event loop

## Screens / features

### Library view

- [ ] Lists declared papers (citation key, title, authors, year)
- [ ] Fetched vs. not-fetched shown at a glance
- [ ] Incremental local filtering by author/year/tag
- [ ] Explicit empty-library state

### Search view

- [ ] Free-text search across all providers, provider-grouped results
- [ ] Per-result fields: title, authors, year, venue, DOI, PDF-availability,
      in-library indicator
- [ ] A single failing provider degrades only its section
- [ ] Author-only search mode reachable
- [ ] DOI-only search mode reachable

### Paper detail view

- [ ] Full detail for an unresolved search result (abstract, PDF sources,
      in-library status) before adding
- [ ] Full detail for a declared paper (identity, artifact status + hash,
      citation key, tags, notes)

### Add / declare

- [ ] Add a search result as a single confirmable action
- [ ] Success/failure shown in-place, no silent failure or crash
- [ ] UI states explicitly that add declares without materializing the PDF

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

## Deferred decisions (docs/dod.md §6)

- [ ] TUI framework chosen (`ratatui` + `crossterm` presumed default)
- [ ] Keybinding scheme decided
- [ ] `pax-core` dependency mode decided (path vs. pinned git/version)

## Critical path

Nothing implemented yet — `src/main.rs` is still the `cargo new` placeholder
and `Cargo.toml` has no dependencies. First steps: pick the `pax-core`
dependency mode, add `ratatui`/`crossterm` (or the chosen alternative), and
scaffold the library view against a real `research/` directory before
building out search/add/fetch.
