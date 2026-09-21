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

## Configuration

- `PAX_PDF_VIEWER` — command used to open a fetched PDF (default: `xdg-open`).
- `SEMANTIC_SCHOLAR_API_KEY`, `ARXIV_CONTACT` — optional provider credentials, passed through to `pax-core`.
