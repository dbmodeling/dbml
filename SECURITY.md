# Security & Code Signing Policy

## Code Signing

Free code signing for official Windows releases is provided by [SignPath.io](https://about.signpath.io/), using a certificate by the [SignPath Foundation](https://signpath.org/).

### Signing & Release Roles

- **Build origin:** All official release artifacts are built reproducibly via GitHub Actions from [`.github/workflows/release.yml`](.github/workflows/release.yml) on protected release tags (`v*`). No locally compiled binaries are uploaded as official releases.
- **Maintainers & Approvers:** Only repository maintainers of the `dbmodeling` GitHub organization have commit access to release branches/tags and permission to approve signing requests.

## Privacy Policy

DB Modeling runs completely locally/offline (Native Desktop app, local CLI Web Server, and VS Code Extension). The software does not collect telemetry, analytics, or personal user data, and does not send your `.dbml` schemas to external servers.

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately via [GitHub Security Advisories](https://github.com/dbmodeling/dbml/security/advisories/new) instead of opening a public issue.
