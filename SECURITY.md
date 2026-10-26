# Security policy — Ferny

Adapted for a desktop editor.
This guide complements, and does not replace, risk analysis and human review.

## Reporting a vulnerability

Report vulnerabilities privately to **ferny@ferarg.com**. Please do not open
public issues for vulnerabilities. Include affected version, platform, steps
to reproduce with synthetic data, and impact. Expect an acknowledgement
within [TO COMPLETE: response time].

Supported versions: none yet (pre-release).

## Principles

1. **Opening a file never runs code.** Running code from a project (build,
   language server, project plugins) requires the user to trust that folder.
2. **Least privilege**: plugins get only the permissions they declare and
   the user grants; child processes get a sanitised environment.
3. **Fail closed**: invalid input, failed verification or an exceeded limit
   stops the operation instead of degrading security.
4. **No network by default**: no telemetry, no crash upload, no automatic
   downloads. Any future network feature is opt-in and verified.
5. **Demonstrable security**: every control in `docs/THREAT-MODEL.md` has a
   test or recorded evidence; missing evidence is reported as
   `not verified`.
6. The agent proposes and executes bounded tasks; the maintainer decides on
   risk, architecture and releases.

## Process per change

1. **Classify** the change: routine (docs, styling), functional (new
   behaviour) or critical (files/save, process execution, IPC, plugins, LSP,
   downloads, dependencies, packaging, signing).
2. **Model**: for new features or boundaries update
   `docs/THREAT-MODEL.md` (skill `threat-modeling`); for critical changes
   record a dated review in the pull request.
3. **Implement** : no shell strings, confined paths,
   atomic writes, bounded resources, untrusted input everywhere.
4. **Verify**: tests including negative cases in `test/security/`, static
   analysis, dependency scans, and a gitleaks secret scan before every
   commit and push (enforced by `.githooks/`).
5. **Review** the full diff (a person other than the implementing agent for
   critical changes) and record residual risk.

Verification proportional to risk: routine → baseline checks; functional →
trust boundaries and negative tests; critical → abuse tests, audit and human
approval.

## Pull request gate

- [ ] Change classified; `docs/THREAT-MODEL.md` updated if boundaries changed.
- [ ] Technical documentation updated; evidence distinguished from goals.
- [ ] Untrusted inputs validated, bounded and confined (files, paths,
      project config, plugin calls, LSP messages, IPC).
- [ ] No new code path executes project-supplied content without workspace
      trust.
- [ ] No shell command strings; no new network access or telemetry.
- [ ] `unsafe`/FFI justified with `// SAFETY:` and limited to the bridge.
- [ ] No secrets or real data in the diff or history; gitleaks hooks ran
      (`pre-commit` on staged changes, `pre-push` on full history).
- [ ] Dependencies justified (maintenance, licence, origin, version,
      advisories), pinned, `cargo deny` and `cargo audit` clean.
- [ ] Negative/security tests added and passing.
- [ ] Logs free of file contents and secrets.
- [ ] Residual risk and unverified controls recorded; approval obtained for
      critical changes.

## Dependencies and supply chain

- `Cargo.lock` is committed. `deny.toml` allows only approved licences
  (GPL-2.0-or-later compatible) and the crates.io source.
- Vendored C/C++ (`third_party/`) is pinned with recorded version, origin and
  checksum; updates are reviewed as security changes.
- Every new tree-sitter grammar is reviewed for licence and origin before
  being compiled in.
- No indefinite exceptions: each one has an owner and an expiry.

## Validation and releases

There is no CI server (the maintainer's Git server is a plain Git server). See
`docs/RELEASE-GATE.md`: Git hooks and validation scripts on each OS;
releases built by the maintainer from clean checkouts of signed tags,
shipped with SHA-256 checksums signed with minisign and an SBOM, published
on the maintainer's Git server, and only with maintainer approval. Platform code signing (Apple
notarisation, Authenticode) comes before 1.0.

### Verifying a release

Every DMG comes with `<name>.dmg.sha256`, its minisign signature
`<name>.dmg.sha256.minisig` and an SBOM `<name>.cdx.json`. A local build reuses
a cache under `~/Library/Caches/ferny-package` (sources and build outputs only,
never secrets), but the DMG is still produced only from the committed ref:

```sh
minisign -Vm ferny-vX.Y.Z-macos-arm64.dmg.sha256 -P <public key>
shasum -a 256 -c ferny-vX.Y.Z-macos-arm64.dmg.sha256
```

Public key: [TO COMPLETE: the maintainer's minisign public key].

The Windows MSI (`scripts/package-msi.ps1`, v0.6.14) is not signed and has no
minisign signature or published SBOM yet. A local MSI is a build for testing
only: its SHA-256 is written next to it, but nothing in the repository proves
who built it. Authenticode signing and the minisign checksum are not verified
and come before any public Windows release.

## Reference frameworks

- OWASP ASVS (applicable chapters: V5 validation, V10 malicious code, V12
  files, V14 configuration).
- NIST SSDF for secure development practices and evidence.
- OWASP Top 10 for LLM Applications for AI-assisted development of this repo.
- [TO COMPLETE: record the versions of each standard adopted.]
