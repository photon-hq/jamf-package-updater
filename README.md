# jamf-package-updater

Safely update Jamf Pro packages in place — same ID, new payload, zero broken policies.

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
- Verifies digest metadata changes after refresh and errors if Jamf still reports old values.
- Skips the update entirely when local file MD5 already matches Jamf package MD5.

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

### Homebrew 

```bash
brew install photon-hq/photon/jamf-package-updater
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

### Reusable GitHub Actions workflow

Other repositories can upload packages to Jamf Pro by calling this repo's reusable workflow. The calling job uploads the built `.pkg` or `.dmg` as a GitHub Actions artifact, then invokes the workflow:

```yaml
jobs:
  build:
    runs-on: macos-latest
    steps:
      - # ... your build steps ...
      - uses: actions/upload-artifact@v4
        with:
          name: myapp-installer
          path: MyApp-2.3.0.pkg

  upload-to-jamf:
    needs: build
    uses: photon-hq/jamf-package-updater/.github/workflows/jamf-upload.yml@main
    with:
      artifact_name: myapp-installer
      jamf_url: https://your-instance.jamfcloud.com
      # package_name: "MyApp"   # optional; defaults to the file stem
    secrets:
      JAMF_CLIENT_ID: ${{ secrets.JAMF_CLIENT_ID }}
      JAMF_CLIENT_SECRET: ${{ secrets.JAMF_CLIENT_SECRET }}
```

**Inputs**

| Input | Required | Description |
|---|---|---|
| `artifact_name` | yes | Name of the `upload-artifact` artifact containing the `.pkg`/`.dmg` |
| `jamf_url` | yes | Jamf Pro URL (e.g. `https://acme.jamfcloud.com`) |
| `package_name` | no | Jamf package record name; defaults to file stem |
| `tool_ref` | no | Git ref of this repo to build (default: `main`) |

**Secrets** — `JAMF_CLIENT_ID` and `JAMF_CLIENT_SECRET` must be set in the calling repository's secrets.

### Environment variables (direct invocation)

You can also skip keyring storage and provide credentials via environment variables:

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
