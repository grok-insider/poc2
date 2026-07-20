# Contributing to Path of Crafting 2

Thanks for hacking on poc2. This project is a PoE2 crafting advisor: a Rust
engine compiled to WebAssembly, driven by a Next.js web app and wrapped by an
Electron desktop shell. It optimizes for clean, patch-versioned data and a green
`master`.

## Dev setup

Prefer the Nix flake devshell (`nix develop`) — it provides the Rust toolchain
(+ `wasm32-unknown-unknown`), `wasm-bindgen`/`binaryen`, and `bun`/`nodejs_22`.
Windows devs without Nix use rustup (honors `rust-toolchain.toml`) + Bun; the
same `bun run` scripts work.

Common commands (full list in `AGENTS.md`):

- `cargo test --workspace` — Rust tests
- `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings`
- `bun run wasm` — build the WASM engine (re-run after touching `crates/poc2-wasm`)
- `bun run typecheck` · `bun run lint` · `bun run test:web` · `bun run build`
- `bun run desktop:typecheck` · `bun run test:desktop`

## Branch model

This repo mirrors [`grok-insider/open-media`](https://github.com/grok-insider/open-media):

```
feat/* · fix/* · ci/* · docs/* · release/*     ← work branches (typed prefixes)
            │  PR
            ▼
          dev                                    ← integration branch (always green)
            │  single "dev → master" PR
            ▼
         master                                  ← released branch (default)
            │  push → release-plz keeps a release PR open
            ▼
       merge release PR → tag vX.Y.Z + GitHub Release + desktop packages
```

- **`master`** is the released branch. **Never push to it directly.**
- **`dev`** is the integration branch. All feature/fix work merges here first.
- **Work branches** use typed prefixes: `feat/…`, `fix/…`, `ci/…`, `docs/…`,
  `release/…`, cut from `dev`.
- Flow: open your work branch off `dev` → PR into `dev` → when `dev` is ready, a
  **single `dev → master` PR** ships everything.
- `.github/workflows/guard-master.yml` enforces this: a PR into `master` fails
  its required check unless the head branch is `dev` or a release-bot head
  (`release-plz-*`, `release-plz-manual-*`, `release-please--*`,
  `release-please-manual-*`). Add that check to `master`'s branch protection
  to make it blocking.
- Automated data-refresh PRs (`.github/workflows/data-watch.yml`) target `dev`.

A PR should leave `dev`/`master` green: `fmt + clippy + test`, web typecheck +
lint + build, and the desktop typecheck + tests (CI runs all of these).

## Commit & PR style

This repo uses [Conventional Commits](https://www.conventionalcommits.org). The
commit history drives automated versioning and the changelog, so prefix every
commit subject with a type:

- `feat: …` — a user-visible feature → **minor** bump (`x.Y.0`).
- `fix: …` — a bug fix → **patch** bump (`x.y.Z`).
- `feat!: …` (or a `BREAKING CHANGE:` footer) — a breaking change → **major** bump.
- `docs:`, `refactor:`, `perf:`, `test:`, `chore:`, `ci:` — don't trigger a
  release on their own; they ride into the next release's changelog where
  relevant.

Keep subjects short and imperative; add a scope when it helps
(`feat(engine): …`, `fix(advisor): …`). Small, focused commits.

## Releases

Releasing is automated with [release-plz](https://release-plz.dev)
(`release-plz.toml` + `.github/workflows/release.yml`). You don't bump versions
or hand-write changelog entries:

1. Merge Conventional-Commit PRs into `dev`, then ship them with a single
   `dev → master` PR as usual.
2. On each push to `master`, release-plz keeps a **release PR** open
   (`chore: release v…`) that bumps the single `[workspace.package].version`
   (every crate inherits it via `version.workspace = true`), refreshes
   `Cargo.lock`, and regenerates `CHANGELOG.md` from the commits since the last
   tag. The [`grok-insider/release-changelog-action`](https://github.com/grok-insider/release-changelog-action)
   then rewrites that PR's notes into user-facing prose via OpenRouter. Polish
   them further if you like.
3. **Merge the release PR to ship.** It tags `vX.Y.Z` (anchored on the
   `poc2-engine` crate, which carries the shared workspace version + the root
   changelog), creates the GitHub Release, and attaches the Electron desktop
   packages — Windows NSIS `.exe` and Linux AppImage + `.deb`.

Admin-only major/minor bumps use **Manual Version Bump**
(`workflow_dispatch` on `manual-version-bump.yml`); they open a
`release-plz-manual-*` PR into `master` the same way as an automated release PR.

Nothing is published to crates.io (`git_only` in `release-plz.toml`). The web app
ships as the static export the desktop packages bundle.

> Do **not** hand-edit the `CHANGELOG.md` `[Unreleased]` block once release-plz
> owns the release PR — let the PR regenerate it.

### One-time GitHub setup

- **Secrets** (Settings → Secrets and variables → Actions):
  - `OPENROUTER_API_KEY` — for the AI changelog. Without it the action still
    runs and falls back to a plain commit-subject list (never blocks a release).
  - `RELEASE_PLZ_TOKEN` — a Personal Access Token. Required so the changelog
    commit pushed to the release PR **re-triggers** the required status checks
    (a commit pushed with the default `GITHUB_TOKEN` does not), and so release-plz
    can open PRs.
- **Settings → Actions → General → Workflow permissions:** enable *"Allow GitHub
  Actions to create and approve pull requests."*
- **Branch protection on `master`:** require a PR and the full CI matrix
  status checks (`only dev into master`, `Rust (fmt|clippy|test)`, `Flake check`,
  `Web (typecheck · lint · build)`, `Windows (cargo · wasm · web · desktop)`,
  `Desktop package (Linux)`), with `enforce_admins`. Protect `dev` with the same
  CI gates (without the guard). This is what makes the `guard master` gate
  enforceable.
- **Admin major/minor:** `workflow_dispatch` on `.github/workflows/manual-version-bump.yml`
  (repo admins only). Opens a `release-plz-manual-*` PR into `master` with AI
  changelog notes; merge tags via the normal Release workflow.
