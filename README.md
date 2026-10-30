# lazypax

**lazypax** is an interactive terminal interface (TUI) for [`pax`](https://github.com/pax-project/pax-core) — a Rust tool for discovering, declaring, managing, and reproducibly acquiring academic papers using Nix as the artifact backend.

It's a client of `pax-core`, the same way the `pax` CLI is: no shelling out to `pax`, no reimplemented provider/library logic, no shadow state. Anything `lazypax` writes to a `research/` library stays fully usable from the plain `pax` CLI, and vice versa.

## Installation

Build from source with the Nix devshell (`direnv allow`, or `nix develop` — same flake `pax-core` uses), then:

```Bash
cargo build --release
./target/release/lazy-pax
```

`lazypax` also needs a `nix` binary on `PATH` at runtime — fetching and opening papers shells out to it, the same way `pax` itself does.

## Usage

Launch `lazypax` inside (or above) a `research/` library directory. If none exists yet, it offers to initialize one on first launch.

```Bash
cd my-papers/
lazy-pax
```

### Keybindings (vim-style)

| Key | Action |
| --- | --- |
| `j`/`k`, `↓`/`↑` | Move selection |
| `gg` / `G` | Jump to top / bottom |
| `/` | Filter the library, or edit the search query |
| `S` | Open search (from the library view) |
| `Tab` | Cycle the search mode: free text, author, DOI (while editing the query) |
| `F` | Edit the search filters: `year:YYYY[-YYYY] type:TYPE lang:CODE` |
| `m` / `M` | Load more search results (double each provider's limit) / load all of them |
| `Enter` / `l` | Open detail view |
| `a` | Add the selected search result / add a tag (in Edit) |
| `f` | Fetch (materialize) the selected paper |
| `o` | Open the selected paper in `$PAX_PDF_VIEWER` |
| `e` | Edit tags, notes, or identity fields |
| `d` | Remove (asks for confirmation) |
| `s` / `c` | Sync / check the whole library |
| `E` | Export the library as BibTeX |
| `i` | Initialize a library (if none exists) / edit a field (in Edit) |
| `w` | Save (in Edit or Export) |
| `q` | Quit |
| `Esc` | Back / cancel |

### Search

Search shows the same information `pax search` prints:

- **Totals:** each provider's line says how many results it retrieved of how many match (`retrieved 50 of 1200 — more available`), when it hit its per-query ceiling, and how it applied each filter (on the server, after retrieval, or not at all).
- **One row per paper:** results from different providers are merged, by DOI, arXiv id, or title and year. Each row names the providers it came from, and the list title shows the merge counts (records before → papers after). Opening a row shows the version `a` adds.
- **Flags:** `[in library]` and `[excluded]` (from `research/excluded.nix`) match a paper by any of its versions; `[language unknown]` marks a record the language filter couldn't check.
- **Filters:** `F` takes year, document type and language in the syntax of `pax search --year/--type/--lang`, and re-runs the current search.
- **Load more:** pax-core's search has no offset, so `m` re-runs the search with each provider's limit doubled, and `M` asks for every result (slow for large result sets).
- **Search log:** every free-text and author search is recorded in `research/search-log.jsonl`, the same log the CLI writes, so `pax stats` counts it. A DOI lookup isn't logged, as in the CLI.

## Configuration

- `PAX_PDF_VIEWER` — command used to open a fetched PDF (default: `xdg-open`).
- `SEMANTIC_SCHOLAR_API_KEY`, `ARXIV_CONTACT` — optional provider credentials, passed through to `pax-core`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
