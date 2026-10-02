# Releasing

New versions of COAT are built and published **automatically**. Merge to `master` and, a few
minutes later, a new release with the Mac and Windows apps appears on the
[Releases page](https://github.com/archways404/TVX-SOC-COAT/releases). This page explains what happens,
how version numbers are chosen, and the one-time setup.

- [What happens on each merge](#what-happens-on-each-merge)
- [Version numbers](#version-numbers)
- [One-time setup](#one-time-setup)
- [Day to day](#day-to-day)
- [Signing the apps (future)](#signing-the-apps-future)

## What happens on each merge

The workflow is `.github/workflows/build.yml`, run by GitHub Actions. On every push or merge to
`master` it does this:

```
version ─► macos ─────┐
         ─► windows ──┴─► release
```

1. **version** works out the next version number (see below). It stops the release if the
   simlog address secrets are missing, so a COAT that can't reach simlog is never published.
2. **macos** (on a GitHub Mac) stamps the version into the build, runs the tests, builds
   `COAT.app` for Apple Silicon and Intel, starts it the way a double-click does, checks that it
   answers, and quits it.
3. **windows** (on a GitHub Windows machine) does the same for `COAT.exe`.
4. **release** publishes GitHub release `vX.Y.Z` with `COAT-macOS.zip` and `COAT.exe`, a short
   "how to install" note, and a list of the changes since the previous release (generated from
   merged pull requests and commit messages).

The files always have the same names, so the download links in the README
(`…/releases/latest/download/COAT.exe`) always point to the newest version.

Also good to know:

- **Pull requests** run the same build and tests, so you see problems before merging, but they
  don't publish anything. The apps are kept as downloadable workflow *artifacts* for 14 days.
- **Changes that only touch documentation** (`*.md` files) don't trigger a release.
- **One release at a time**: if two merges happen close together, the second waits for the first,
  so they can't get the same version number.
- **Re-running** a workflow for a commit that's already released doesn't publish it twice.
- **Nothing is committed back** to `master`. The version number lives in the release's tag.

## Version numbers

Versions look like `MAJOR.MINOR.PATCH`, for example `1.4.2`. The workflow takes the newest release
tag and looks at the commit messages since then:

| If a commit message… | the version becomes | example |
|---|---|---|
| contains `#major` or `BREAKING CHANGE` | next major | 1.4.2 → 2.0.0 |
| starts with `feat`, or contains `#minor` | next minor | 1.4.2 → 1.5.0 |
| anything else | next patch | 1.4.2 → 1.4.3 |

With a squash merge, the pull request title becomes the commit message, so a PR titled
`feat: show voicemail steps` gives a minor release.

To jump to a specific version, for example `1.0.0` for a big launch, change `version` in
`Cargo.toml` to `1.0.0` and merge. `Cargo.toml` acts as a minimum: the workflow never picks
anything lower. Before there are any tags, the first release uses the `Cargo.toml` version as is.

To see which version the next merge would get, run `./packaging/next-version.sh`.

## One-time setup

### 1. Add the simlog addresses as secrets

The simlog addresses are internal and not in the code. The workflow reads them from two
repository **secrets** and compiles them into the apps.

1. On GitHub, open the repository's **Settings → Secrets and variables → Actions**.
2. Click **New repository secret** and add:

   | Name | Value |
   |---|---|
   | `COAT_SIMLOG_NORDIC` | the address of the Nordic simlog, e.g. `http://…` |
   | `COAT_SIMLOG_UAE` | the address of the UAE simlog |

Secrets are hidden in workflow logs and aren't given to pull requests from forks. Be aware that
the addresses do end up **inside the published apps**. If the repository and its releases are
public, anyone who downloads COAT could find them by inspecting the file. They only work from
inside the VPN.

### 2. Let the workflow publish releases

Open **Settings → Actions → General → Workflow permissions**. If it's set to *Read repository
contents*, the workflow can still publish, because it asks for write access only in its release
step. If your organisation forbids that, switch to **Read and write permissions**.

## Day to day

- **Ship a change**: merge it to `master`. That's all.
- **Watch it**: the **Actions** tab shows each run; the summary of the *version* step says which
  version it will publish.
- **A run failed**: open it and look at the red step. Test failures show which test and why.
  Fix it, merge again, and a new release with the next version is made.
- **Undo a release**: on the Releases page, delete the release and its tag. The next merge reuses
  the version number only if that tag is gone.

## Signing the apps (future)

Today the apps are not signed with a company certificate, so the first time someone opens COAT,
macOS and Windows ask whether to trust it (see the
[user guide](user-guide.md#the-first-time-you-open-it)). Signing would remove those warnings:

- **Mac**: an Apple Developer ID certificate, plus *notarization* (Apple scans the app). Both happen
  in the `macos` job with `codesign` and `xcrun notarytool`, using the certificate and an Apple
  account password stored as secrets.
- **Windows**: a code-signing certificate, applied in the `windows` job with `signtool`.
