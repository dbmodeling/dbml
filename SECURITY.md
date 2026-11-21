# Security & Code Signing Policy

## Code Signing

**Current status:** Official Windows, macOS, and Linux release binaries are currently **unsigned** and are not planned to be code-signed in the near term. Verify downloads using the published `SHA256SUMS.txt` checksums (see below).

The release pipeline ([`.github/workflows/release.yml`](.github/workflows/release.yml)) already includes optional signing hooks (SignPath for Windows and `codesign`/notarization for macOS) for future use when certificates are configured, and fails the job if signing is configured or `SIGNING_REQUIRED=true` is set and signing fails:

| Platform | Mechanism | Configuration (repository settings) |
| :--- | :--- | :--- |
| Windows installer | SignPath (`signpath/github-action-submit-signing-request`) | secret `SIGNPATH_API_TOKEN`; variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG` |
| macOS `.app`, `.pkg`, `.dmg` | `codesign` (Hardened Runtime), `pkgbuild --sign`, `notarytool` + `stapler` | secrets `MACOS_APP_CERT_P12`, `MACOS_INSTALLER_CERT_P12`, `MACOS_CERT_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`; variables `MACOS_CODESIGN_IDENTITY`, `MACOS_INSTALLER_IDENTITY` |
| Linux `.deb` / `.rpm` | Not signed; verify with the published checksums | n/a |

Set the repository variable `SIGNING_REQUIRED=true` for official releases: the Windows and macOS jobs then **fail** when the credentials above are missing instead of publishing unsigned installers. Without it, a missing credential only produces a log warning and the artifact is published unsigned, so treat a release built without `SIGNING_REQUIRED=true` as unsigned.

Every release also publishes `SHA256SUMS.txt`. `scripts/install.sh` downloads the package by resolved tag and aborts if its SHA-256 does not match. To check manually: `sha256sum -c SHA256SUMS.txt --ignore-missing`.

### Signing & Release Roles

- **Build origin:** All official release artifacts are built reproducibly via GitHub Actions from [`.github/workflows/release.yml`](.github/workflows/release.yml) on protected release tags (`v*`). No locally compiled binaries are uploaded as official releases.
- **Maintainers & Approvers:** Only repository maintainers of the `dbmodeling` GitHub organization have commit access to release branches/tags and permission to approve signing requests.

## Privacy Policy

DB Modeling runs completely locally/offline (Native Desktop app, local CLI Web Server, and VS Code Extension). The software does not collect telemetry, analytics, or personal user data, and does not send your `.dbml` schemas to external servers.

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately via [GitHub Security Advisories](https://github.com/dbmodeling/dbml/security/advisories/new) instead of opening a public issue.
