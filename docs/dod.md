# lazypax — Definition of Done (MVP)

## 1. Purpose

`lazypax` is the interactive terminal interface for [`pax`](https://github.com/Santiago-Garrote/pax),
a Rust tool for discovering, declaring, managing, and reproducibly acquiring
academic papers with Nix as the artifact backend.

Per `pax`'s own design (`pax`'s `CLAUDE.md` / README): **`pax-core` is the library,
`pax` (the CLI) is one client of it, and `lazypax` is meant to be just another
client of the same library** — not a reimplementation, not a wrapper that shells
out to the `pax` binary, and not a fork of its logic.

The MVP is done when `lazypax` covers the same core workflow as the `pax` CLI —

```text
Search → Select → Declare → Fetch → Manage → Reproduce
```

— interactively, against a real `pax` research library, using `pax-core` directly.

---

## 2. Architectural constraints (non-negotiable for MVP)

- [ ] `lazypax` depends on `pax-core` as a library dependency (git or path
      dependency on `Santiago-Garrote/pax`, `cli` feature disabled — `lazypax`
      is its own binary, not a `clap` client). Shelling out to the `pax` binary
      or reimplementing provider/library logic is out of scope: any behavior
      that already exists in `pax-core` must be called, not rebuilt.
- [ ] `lazypax` operates on a `research/` library laid out exactly as `pax init`
      creates it (`research/flake.nix` + `research/papers.nix`), and reads/writes
      it only through `pax_core::Library` / the existing `pax_core` functions
      (`add_candidate`, `edit_paper`, `remove_paper`, `fetch_paper`,
      `check_library`, `sync_library`, `resolve_artifact_path`, `bibtex::render`,
      etc.) — never by hand-parsing or hand-writing `papers.nix`.
  - Note: `docs/status.md` in `pax` tracks which of these already exist; verify
    against it, don't assume.
- [ ] A `research/` library produced or modified through `lazypax` remains fully
      usable from the plain `pax` CLI and vice versa — no `lazypax`-only state,
      no divergent file format.
- [ ] Network calls to provider APIs happen only through `pax_core`'s existing
      `Provider` implementations (OpenAlex, Crossref, Semantic Scholar, arXiv) —
      `lazypax` does not talk to a provider API directly.
- [ ] All blocking work (provider search, `nix build`/`nix store prefetch-file`
      calls inside `fetch`/`open`/`check`/`sync`) runs off the UI thread/event
      loop so the interface never freezes mid-request.

---

## 3. Core MVP screens/features

Each maps to an existing (or planned, per `pax`'s `docs/status.md`) `pax-core`
capability — `lazypax` adds interactivity and visualization, not new domain
logic.

### 3.1 Library view (home screen)

- [ ] Lists all papers currently declared in `research/papers.nix`
      (citation key, title, authors, year at minimum).
- [ ] Distinguishes fetched vs. not-fetched papers at a glance.
- [ ] Supports incremental local filtering by author/year/tag (backed by
      `pax_core::filter_papers` / `search_local`), without leaving the screen.
- [ ] Empty-library state is handled explicitly (matches `pax list`'s "Library
      is empty" case) rather than showing a blank/broken screen.

### 3.2 Search view

- [ ] Free-text search across all providers (`pax_core::search_all`), rendered
      as a navigable, provider-grouped result list.
- [ ] Per-result display includes title, authors, year, venue, DOI,
      PDF-availability, and an "already in library" indicator — same fields
      `pax search`/`pax show` expose today.
- [ ] A provider failing (e.g. a Crossref deserialize error, a rate limit)
      degrades that provider's section only; it never blocks or crashes the
      rest of the search, mirroring `pax_core::search_all`'s
      per-provider `Result`.
- [ ] Author-only and DOI-only search modes are reachable (not necessarily as
      separate screens — a mode toggle/flag in the same view is fine).

### 3.3 Paper detail view

- [ ] Selecting a search result shows the full `show`-equivalent detail
      (abstract when available, PDF sources, in-library status) before
      committing to add it.
- [ ] Selecting a declared paper shows its full record: identity, artifact
      status (fetched/not fetched + hash), citation key, tags, notes.

### 3.4 Add / declare

- [ ] Adding a search result (`pax_core::add_candidate`) is a single
      confirmable action from the detail view; success and failure are both
      shown in-place (no silent failure, no crash on a provider/network error).
- [ ] Reflects `pax`'s lazy semantics explicitly: adding a paper declares it
      without materializing the PDF, and the UI says so.

### 3.5 Fetch / open

- [ ] A declared paper can be fetched (`pax_core::fetch_paper`) with visible
      progress/pending state (this can take several seconds — the UI must not
      look frozen or unresponsive during it).
- [ ] Opening a paper (`pax_core::resolve_artifact_path` +
      `$PAX_PDF_VIEWER`/`xdg-open`) launches the external viewer the same way
      `pax open` does, fetching automatically first if not yet materialized.
- [ ] Errors surfaced by `pax-core` (no source URL recorded, viewer launch
      failure, etc.) are shown to the user, not swallowed.

### 3.6 Edit

- [ ] Tags and notes on a declared paper can be viewed and edited in place via
      `pax_core::edit_paper`.
- [ ] Citation-key rename and identity corrections (title/author/year/DOI) are
      included **only if** already implemented in the `pax-core` version
      `lazypax` depends on — do not reimplement citation-key validation or
      collision-checking inside `lazypax`; call into `pax-core`. If not yet
      available upstream, tag/notes editing alone satisfies this section for
      MVP.

### 3.7 Remove

- [ ] A declared paper can be removed from the library
      (`pax_core::remove_paper`) with an explicit confirmation step (removal is
      destructive to the declaration, even though Nix-store GC is out of
      `lazypax`'s hands).

### 3.8 Sync / check

- [ ] `pax_core::sync_library` (fetch everything not yet materialized) and
      `pax_core::check_library` (verify recorded hashes) are reachable as
      library-wide actions, with a summary view equivalent to `pax sync`/
      `pax check`'s per-paper report.

### 3.9 Export

- [ ] The library can be exported as BibTeX (`pax_core::bibtex::render`),
      viewable in-app and writable to a file the user chooses.

### 3.10 Init

- [ ] If launched outside an initialized library, `lazypax` offers to run
      `pax_core::init_library` rather than erroring out with no path forward.

---

## 4. Non-goals for MVP (explicitly out of scope)

- Any provider beyond what `pax-core` already implements (no DBLP work inside
  `lazypax` — that's `pax-core`'s call to make, not this project's).
- PDF preview/rendering inside the terminal.
- PDF annotation, full-text indexing, citation graphs, related-paper
  recommendations, AI summaries.
- Any local cache/database of its own — `research/papers.nix` (via
  `pax-core`) remains the single source of truth; no shadow state that can
  drift from it.
- Config/theming system beyond what's needed to point at a `PAX_PDF_VIEWER`
  and a library path.
- Multi-library / workspace switching in one running instance.
- Vim-style modal editing, scripting, or a plugin system.
- Packaging/distribution polish (installers, prebuilt binaries) — a
  `cargo run` / `cargo build --release` workflow is sufficient for MVP.
- Any change to `pax-core`'s public API driven by `lazypax`'s convenience
  alone — if a gap is found, it's raised against `pax`, not patched around
  locally.

---

## 5. MVP success criteria (the actual "done" bar)

`lazypax` is done for MVP when, against a real `research/` library, a user can
complete this entire loop **without touching the `pax` CLI or hand-editing any
file**:

```text
launch lazypax on an uninitialized directory
  → initialize the library
  → search "actor model"
  → inspect a result
  → add it
  → see it appear in the library view
  → fetch it
  → open it in the configured PDF viewer
  → edit its tags/notes
  → export the library as BibTeX
  → remove it
```

And afterward:

- [ ] The resulting `research/flake.nix` + `research/papers.nix` are byte-for-byte
      what `pax` itself would have produced for the same sequence of actions —
      confirmed by running the equivalent `pax` CLI commands against a fresh
      copy of the same starting state and diffing the two libraries.
- [ ] The library remains independently reproducible per `pax`'s own bar: a
      `git clone` + `nix build` on a separate checkout reconstructs the same
      artifact, with zero `lazypax` (or `pax`) code involved.
- [ ] No panic, unhandled `Result`, or stuck/unresponsive screen is reachable
      through any of the above steps, including once each error case: a
      provider network failure during search, adding a paper with no PDF
      source, opening/fetching without network access, and quitting mid-action.

---

## 6. Resolved decisions

- **TUI framework:** `ratatui` + `crossterm`.
- **Keybinding scheme:** vim-style (hjkl navigation, modal input where a mode
  distinction is needed, `/`-style filtering) — consistent with `lazypax`
  sitting in the same "lazy" family as `lazygit`/`lazydocker`.
- **`pax-core` dependency:** `git` dependency on `Santiago-Garrote/pax`
  pinned to the `1.0.0` release tag, `default-features = false` (the `cli`
  feature — and its `clap`/`dotenvy`/`tokio` deps — stays off; `lazypax`
  brings its own `tokio` for its own event loop and to drive `pax-core`'s
  async functions). Confirmed building cleanly against that tag. Bumping the
  pin to a later `pax` release is a deliberate, explicit step, not automatic.
