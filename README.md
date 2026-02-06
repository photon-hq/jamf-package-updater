# jamf-package-updater

Small, focused, and built for one job:
update a Jamf Pro package safely, without breaking the policies that depend on it.

Maintained by Photon.

## Why it exists

Replacing package files in Jamf Pro is easy to do manually and easy to get wrong.
This tool keeps the same package record, uploads a new payload, refreshes inventory,
and confirms which policies are affected.

## What it does

- Authenticates with Jamf Pro using OAuth client credentials.
- Finds a package by name.
- Scans policies to detect references to that package name or file name.
- Updates package metadata in place (same package ID).
- Uploads a new `.pkg` or `.dmg` file with retry support.
- Triggers JCDS inventory refresh so checksums are recalculated.

## Requirements

- Rust toolchain (edition 2024 project)
- Access to a Jamf Pro instance
- Jamf API client credentials (`client_id`, `client_secret`)

Required Jamf API privileges:
- Update Packages
- Read Jamf Cloud Distribution Service Files
- Create Packages
- Read Packages
- Delete Packages
- Update Policies
- Read Policies

## Install

```bash
git clone <your-repo-url>
cd jamf-package-updater
cargo build --release
```

Binary path:

```bash
./target/release/jamf-package-updater
```

## Quick start

### 1) Save credentials (local machine keychain/keyring)

```bash
jamf-package-updater auth \
  --client-id "<jamf-client-id>" \
  --client-secret "<jamf-client-secret>" \
  --url "https://your-instance.jamfcloud.com"
```

### 2) Update a package

Use the file stem as package name:

```bash
jamf-package-updater update /path/to/App-2.3.0.pkg
```

Or pass an explicit Jamf package name:

```bash
jamf-package-updater update /path/to/App-2.3.0.pkg --name "App Installer"
```

## CI / automation

You can skip keyring storage and provide credentials via environment variables:

```bash
export JAMF_CLIENT_ID="..."
export JAMF_CLIENT_SECRET="..."
export JAMF_URL="https://your-instance.jamfcloud.com"

jamf-package-updater update /path/to/App-2.3.0.pkg --name "App Installer"
```

Environment variables take precedence over keyring values.

## Command reference

```bash
jamf-package-updater auth --client-id <id> --client-secret <secret> --url <jamf-url>
jamf-package-updater update <path-to-pkg-or-dmg> [--name <package-name>]
```

## Behavior notes

- Supported upload formats: `.pkg`, `.dmg`
- Update flow is in-place: existing package ID is preserved
- Upload retries up to 3 times for server-side failures
- Policy references are discovered by scanning policy XML package configuration

## Troubleshooting

- `No credentials found`:
  run `auth` first or set `JAMF_CLIENT_ID`, `JAMF_CLIENT_SECRET`, `JAMF_URL`.
- `Package '<name>' not found`:
  verify the package name in Jamf Pro or pass `--name`.
- Upload/auth failures:
  confirm Jamf URL, credentials, and API role permissions.

## Development

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Maintainer

Photon
