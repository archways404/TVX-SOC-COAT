# Releasing

New versions of COAT are built and published **automatically**. Merge to `master` and, a few
minutes later, a new release with the Mac and Windows apps appears on the
[Releases page](https://github.com/archways404/TVX-SOC-COAT/releases). This page explains what happens,
how version numbers are chosen, and the one-time setup.

- [Two channels: stable and preview](#two-channels-stable-and-preview)
- [What happens on each merge](#what-happens-on-each-merge)
- [Version numbers](#version-numbers)
- [One-time setup](#one-time-setup)
- [Day to day](#day-to-day)
- [Signing the apps (future)](#signing-the-apps-future)

## Two channels: stable and preview

COAT is released on two channels, each from its own branch:

| Branch | Channel | Release | Files | App |
|---|---|---|---|---|
| `master` | stable | `v1.5.0`, marked **Latest** | `COAT-macOS.zip`, `COAT.exe` | COAT (blue) |
| `preview` | preview | `v1.5.0-preview.3`, marked **Pre-release** | `COAT-PREVIEW-macOS.zip`, `COAT-PREVIEW.exe` | COAT Preview (green) |

The idea: merge work into `preview` first. Every merge there publishes a pre-release that a few
people try out. When it's good, merge `preview` into `master` to release it to everyone.

**COAT Preview is a separate app**, so it can be installed next to COAT without either replacing
the other. It has its own name and icon (green, with a PREVIEW band), a green sidebar with a
PREVIEW label, its own port (7191 instead of 7171) and its own settings. It updates only to newer
previews, and COAT only to newer stable releases.

To start using the channel, create the branch once: `git switch -c preview && git push -u origin preview`.

## What happens on each merge

The workflow is `.github/workflows/build.yml`, run by GitHub Actions. On every push or merge to
`master` or `preview` it does this:

```
version ─► macos ─────┐
         ─► windows ──┴─► release
```

1. **version** tests the version rules, then works out the next version number for the branch's
   channel (see below). It stops the release if the simlog address secrets are missing, so a COAT
   that can't reach simlog is never published.
2. **macos** (on a GitHub Mac) stamps the version into the build, runs the tests, runs the
   [update test](development.md#testing-automatic-updates) (an old COAT updates itself to a new
   one), builds `COAT.app` for Apple Silicon and Intel, starts it the way a double-click does,
   checks that it answers, and quits it.
3. **windows** (on a GitHub Windows machine) does the same for `COAT.exe`.
4. **release** creates the tag and publishes the GitHub release: `vX.Y.Z` (Latest) from `master`,
   or `vX.Y.Z-preview.N` (Pre-release) from `preview`. It includes the two apps,
   `SHA256SUMS.txt` (the checksums installed copies use to verify an update), a short "how to
   install" note, and a list of the changes since the previous release (generated from merged
   pull requests and commit messages).

Builds made by this workflow are **official**: they know which repository they came from
(`COAT_REPO`) and update themselves from its releases (`COAT_RELEASE_BUILD`). Within a few hours of
a release, everyone's running COAT shows "Update ready" in its sidebar.

The files always have the same names, so the download links in the README
(`…/releases/latest/download/COAT.exe`) always point to the newest stable version. "Latest" never
points at a pre-release.

Also good to know:

- **Pull requests** run the same build and tests, so you see problems before merging, but they
  don't publish anything. A pull request into `preview` builds COAT Preview. The apps are kept as
  downloadable workflow *artifacts* for 14 days.
- **Changes that only touch documentation** (`*.md` files) don't trigger a release.
- **One release at a time per branch**: if two merges to the same branch happen close together,
  the second waits for the first, so they can't get the same version number.
- **Re-running** a workflow for a commit that's already released doesn't publish it twice.
- **Nothing is committed back** to the branches. The version number lives in the release's tag;
  the workflow stamps it into `Cargo.toml` only for the build.

## Version numbers

Versions look like `MAJOR.MINOR.PATCH`, for example `1.4.2`. The workflow takes the newest
**stable** tag and looks at the commit and merge messages since then. Upper or lower case doesn't
matter (`#Minor`, `#MINOR` and `#minor` all work):

| If a commit or merge message… | the version becomes | example |
|---|---|---|
| contains `#major` or `BREAKING CHANGE` | next major | 1.4.2 → 2.0.0 |
| contains `#minor`, or starts with `feat` | next minor | 1.4.2 → 1.5.0 |
| contains `#patch`, or anything else | next patch | 1.4.2 → 1.4.3 |

The strongest one wins: one `#major` among twenty fixes still makes it a major release. With a
squash merge, the pull request title becomes the commit message, so a PR titled
`feat: show voicemail steps` gives a minor release.

**Preview versions** are the version `master` would release next, plus `-preview.N`. The first
preview of 1.5.0 is `1.5.0-preview.1`, the next `1.5.0-preview.2`, and so on. When `master`
releases 1.5.0, the next preview starts again at `1.5.1-preview.1` (or `1.6.0-preview.1` after a
`#minor`). A preview always counts as older than the release it previews.

To jump to a specific version, for example `1.0.0` for a big launch, change `version` in
`Cargo.toml` to `1.0.0` and merge. `Cargo.toml` acts as a minimum: the workflow never picks
anything lower. Before there are any tags, the first release uses the `Cargo.toml` version as is.

To see which version the next merge would get, run `./packaging/next-version.sh` (stable) or
`./packaging/next-version.sh --preview`. `./packaging/test-versions.sh` checks all these rules
(the workflow runs it on every build).

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
