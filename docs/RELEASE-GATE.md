# Validation and release gate

Written for a project
**without a CI server**: the maintainer's Git server is a plain Git server (SSH, bare repository),
and builds for each OS are run by the maintainer on separate machines.
Nothing is implemented yet (`not verified`).

## Environment

| Platform | Where it is built and validated | Who |
|---|---|---|
| macOS | Development Mac, macOS 27, arm64 | Agent and maintainer |
| Linux | Maintainer's Debian 13 (trixie) machine, amd64 | Maintainer |
| Windows | Maintainer's Windows 11 machine, x64 | Maintainer |

The agent cannot run anything on the Linux and Windows machines. It
provides scripts and instructions, and fixes the problems the maintainer
reports.

## Local gate (every commit and push)

`.githooks/pre-commit` scans staged changes and `.githooks/pre-push` scans
the full history with gitleaks; both fail closed. Enable once per clone:
`git config core.hooksPath .githooks`.

## Validation script (every milestone, on each OS)

`scripts/check.sh` (macOS, Linux; bash 3.2 compatible) and
`scripts/check.ps1` (Windows PowerShell 5.1 / PowerShell 7) run every check
that applies to the current state of the repository. A check whose tool or
inputs do not exist yet is reported as `SKIP (not verified)`, never as
`PASS`. They always validate the repository they belong to, whatever the
current directory.

```sh
scripts/check.sh            # macOS / Linux
scripts/check.sh --strict   # skipped checks count as failures
```

Speed settings (both default to the number of CPUs reported by `getconf`):

- `FERNY_CTEST_JOBS=<n>`: run CTest with `n` parallel jobs. Tests declare their
  order with fixtures; a test that shares a state path without a fixture must
  be fixed before raising this value. Measured on the development machine
  (12 CPUs): 167 of 167 passed in three runs, 13 s at 12 jobs, against 141 s
  in series.
- `FERNY_FUZZ_JOBS=<n>` and `FERNY_FUZZ_SECONDS=<s>`: fuzz concurrency and time
  per target (60 s by default). Measured: the 12 targets at 12 jobs took 63 s
  with all 12 finishing. Lowering the time reduces coverage; the long run stays a
  separate, scheduled job.
- A compiler cache (`ccache`) is used when it is installed. Its directory is
  `${XDG_CACHE_HOME:-$HOME/.cache}/ferny-check/ccache`. It only saves time when
  the same source path is built again (for example a rerun in the same
  worktree, or a fresh build tree in the same place). The cache is not a
  dependency of the product.
- Windows (`scripts/check.ps1`) is not changed by these settings: it runs
  CTest in one job, and fuzzing is skipped there (cargo-fuzz needs macOS or
  Linux), as before.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1 -Strict
```

(`-ExecutionPolicy Bypass` applies only to that process; it does not
change the machine policy.)

Exit codes: `0` pass, `1` a check failed (or something was skipped in
strict mode), `2` usage error. Strict mode becomes mandatory for milestone
closure once phase 0 ends (v0.1.0).

Checks:

| Check | Tool |
|---|---|
| Formatting | `cargo fmt --check`, `clang-format --dry-run` |
| Lints | `cargo clippy --all-targets -- -D warnings`, C++ warnings as errors |
| Tests | `cargo test --workspace`, CTest UI smoke tests |
| Licences, sources, advisories, bans | `cargo deny check` |
| Known vulnerabilities | `cargo audit` |
| Whitespace errors | `git diff --check HEAD` |
| Shell scripts | `shellcheck` on `*.sh` and `.githooks/*` (excluding `third_party/`) |
| Vendored code | Every file in `third_party/` against `third_party/*.sha256` |
| Secrets | `gitleaks git` over history, staged and unstaged changes (redacted) |
| C++ build | CMake configure + build with `FERNY_WARNINGS_AS_ERRORS=ON` (option defined in the top-level `CMakeLists.txt`; warnings in Ferny-owned UI sources are errors, generated CXX-Qt bridge includes are scoped as system includes) |
| SAST | Semgrep (registry rules `p/rust`, `p/c`; needs network to fetch rules; metrics off) |
| Fuzzing (short run, macOS/Linux) | `cargo +nightly fuzz run <target> -- -max_total_time=60` for every target in `fuzz/`, built once and run in parallel (`FERNY_FUZZ_JOBS`, default 4; time per target `FERNY_FUZZ_SECONDS`, default 60) |

Each script prints the tool versions and a summary block between
`BEGIN/END FERNY CHECK SUMMARY`. The maintainer pastes the summary of the
Linux and Windows runs into the milestone report; a milestone is not closed without a
green run on the three OS.

When changing `[workspace.package].version`, regenerate `Cargo.lock` with the
pinned Rust toolchain before running checks. The lockfile includes versions for
all local workspace members, including members without external dependencies;
`--locked` Clippy/test commands deliberately fail when any such entry is stale.

The CMake build and CTest checks are sequenced: configuration must pass before
the build; CTest runs only after a successful build. If configuration or build
fails, dependent stages are reported as `SKIP (not verified)` while the
original failure remains blocking. This avoids presenting tests against a
missing or stale executable as an independent test result.

The gate itself is tested: `bash test/scripts/check_sh_test.sh` (macOS,
Linux) and `test\scripts\check_ps1_test.ps1` (Windows) verify that
whitespace errors and staged/unstaged synthetic secrets fail the gate, that
secrets are redacted, and that strict mode rejects skipped checks.

The `ui_ipc_forward` smoke test chooses BSD/macOS or GNU `stat` syntax before
checking the private endpoint directory's permissions; both paths assert mode
700 and then exercise a two-process forwarding round trip.

## Rules that replace CI enforcement

- Without a server, the hooks and scripts are the only gate and can be
  bypassed. Never use `--no-verify`; record any skipped check as
  `not verified` in the milestone report.
- Toolchains are pinned (`rust-toolchain.toml`, Qt version, CMake minimum)
  so the three machines build the same thing.
- Dependencies are fetched only from crates.io and the pinned
  `third_party/` sources; `cargo deny` enforces it.
- The the maintainer's Git server Git server is the source of truth: SSH keys only, no shared
  accounts. [TO COMPLETE: who has push access.]

## Release gate

Releases are built by the maintainer, never by the agent, and only when
the maintainer asks:

1. Tag a reviewed commit with a signed tag (`git tag -s vX.Y.Z`).
2. On each OS, build from a **clean checkout of that tag** (fresh clone or
   `git clean -xfd`), with the validation script green.
3. Name artefacts `ferny-v{VERSION}-{platform}-{arch}` and store them in
   `bin/`.
4. `scripts/package-macos.sh --ref vX.Y.Z` writes the DMG, its SHA-256
   and a CycloneDX SBOM (`tools/ferny-sbom`, H4.3). With
   `FERNY_MINISIGN_KEY=<secret key file>` it also signs the checksum file
   with **minisign** (R4). The minisign secret key stays on the
   maintainer's machine, never in the repository or the agent's context;
   the public key is committed in `SECURITY.md` and published on the maintainer's Git server.
5. Platform signing: with `FERNY_SIGN_IDENTITY="Developer ID Application:
   …"` the script signs with the hardened runtime and a secure
   timestamp; with `FERNY_NOTARY_PROFILE=<notarytool keychain profile>`
   it notarises, staples and checks the DMG with Gatekeeper. Without them
   the bundle is signed ad-hoc and release notes say that Gatekeeper will
   warn. Windows Authenticode: later (R4).
6. Install/upgrade test on a clean VM or user account per OS.
7. Publish the artefacts, checksums, signature and public key on the maintainer's Git server
   (R4) [TO COMPLETE: download location and access method on the maintainer's Git server]. No
   in-app updater: users download each version.
8. Release report: tag, commit, toolchain versions, check
   results, checksums.

Blocking: confirmed critical/high vulnerability, leaked credential, failed
secret scan, a red validation run on any OS, or missing maintainer approval.

## Residual risk

No independent build server: provenance relies on the maintainer's
machines being clean. Mitigated by signed tags, clean checkouts, pinned
toolchains and recorded checksums. Reassess if a CI service is added later.
