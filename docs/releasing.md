# Releasing

New versions of COAT are built and published **automatically**. Merge to `preview` for a
pre-release, or to `master` for a release, and a few minutes later the Mac and Windows apps appear
on the [Releases page](https://github.com/archways404/TVX-SOC-COAT/releases). This page explains how
to ship a change, how version numbers are chosen, what the workflow does, and the one-time setup.

- [Two channels: stable and preview](#two-channels-stable-and-preview)
- [How to ship a change](#how-to-ship-a-change)
- [Version numbers](#version-numbers)
- [Release files](#release-files)
- [What happens on each merge](#what-happens-on-each-merge)
- [One-time setup](#one-time-setup)
- [Day to day](#day-to-day)
- [Signing the apps (future)](#signing-the-apps-future)

## Two channels: stable and preview

COAT is released on two channels, each from its own branch:

| Branch | Channel | Release | Files | App |
|---|---|---|---|---|
| `master` | stable | `v1.5.0`, marked **Latest** | `COAT_v1.5.0.zip`, `COAT_v1.5.0.exe` | COAT (blue) |
| `preview` | preview | `v1.5.0-preview.3`, marked **Pre-release** | `COAT_v1.5.0-preview.3_PREVIEW.zip`, `…_PREVIEW.exe` | COAT Preview (green) |

The idea: merge work into `preview` first. Every merge there publishes a pre-release that a few
people try out. When it's good, merge `preview` into `master` to release it to everyone.

**COAT Preview is a separate app**, so it can be installed next to COAT without either replacing
the other. It has its own name and icon (green, with a PREVIEW band), a green sidebar with a
PREVIEW label, its own port (7191 instead of 7171) and its own settings. It updates only to newer
previews, and COAT only to newer stable releases.

To start using the channel, create the branch once: `git switch -c preview && git push -u origin preview`.

## How to ship a change

```
 feature branch ──PR──►  preview  ──PR──►  master
                          │                  │
                          ▼                  ▼
                 v1.5.0-preview.1      v1.5.0 (Latest)
                 v1.5.0-preview.2
```

### A normal change

1. Branch off `preview`: `git switch preview && git pull && git switch -c fix-queue-chart`.
2. Commit, push, and open a pull request **into `preview`**. The pull request builds and tests both
   apps (as COAT Preview), so problems show up before merging.
3. Merge it. A pre-release, for example `v1.5.0-preview.1`, appears a few minutes later. Everyone
   running COAT Preview gets it automatically.
4. Repeat for more changes; each merge makes the next preview (`-preview.2`, `-preview.3`, …).
5. When `preview` is good, open a pull request **from `preview` into `master`** and merge it.
   That publishes the release, for example `v1.5.0`, to everyone.

Use a **merge commit** (not "squash") when merging `preview` into `master`, so the individual
changes, and any `#minor` or `#major` in them, stay visible in the history and in the release notes.

### What kind of version: patch, minor or major

The version is decided by the messages of the commits and merges since the last release; see
[Version numbers](#version-numbers). In short:

| You want | Put this in a commit message or the pull request title |
|---|---|
| a patch release (bug fixes), `1.4.2 → 1.4.3` | nothing; that's the default (or `#patch`) |
| a minor release (new features), `1.4.2 → 1.5.0` | `#minor`, or start the message with `feat` |
| a major release (big or breaking changes), `1.4.2 → 2.0.0` | `#major` or `BREAKING CHANGE` |

### Releasing a new major version

1. Do the work on `preview` as usual. As soon as one commit or merge into `preview` contains
   `#major`, the previews become `2.0.0-preview.1`, `2.0.0-preview.2`, …, so testers can see a major
   version is coming.
2. When it's ready, merge `preview` into `master` with a merge commit. The `#major` in the history
   makes it `2.0.0`. To be explicit, also write it in the merge message:
   `Release 2.0 #major`.
3. Want an exact number instead, say `3.0.0`? Change `version` in `Cargo.toml` to `3.0.0` in that
   pull request. The workflow never picks a version lower than `Cargo.toml` says.

### A hotfix for the stable release

Something is broken in `v1.5.0` and can't wait for the next preview round:

1. Branch off `master`, fix it, and open a pull request **into `master`**. Merging releases `v1.5.1`.
2. The workflow then merges `master` back into `preview` by itself (see below), so the fix is in
   the previews too and the two branches don't drift apart.

### Keeping `preview` up to date

After every release from `master`, the workflow merges `master` back into `preview` by itself
(the **sync-preview** step). Then `preview` always contains everything that's released, and the
next preview starts from the new version (after `v1.5.0`, the next preview is `v1.5.1-preview.1`,
or `v1.6.0-preview.1` after a `#minor`).

- Right after `preview` was merged into `master` this is just a fast-forward: no new commit.
- After a hotfix on `master` it makes a merge commit on `preview`, "Merge master (v1.5.1) into preview".
- It doesn't publish a preview by itself. The next change merged into `preview` does.
- If `master` and `preview` changed the same lines, it can't merge them. The run then shows an
  error on **sync-preview**, and `preview` is left as it was. Open a pull request from `master`
  into `preview`, fix the conflict there, and merge it.
- If `preview` is protected so that only pull requests may change it, allow GitHub Actions to push
  to it, or the step fails the same way.

### Undoing a release

On the Releases page, delete the release **and its tag**. Installed copies that already updated
keep that version until a newer one is published. Fix the problem and merge again: the workflow
reuses the version number only if its tag is gone.

## What happens on each merge

The workflow is `.github/workflows/build.yml`, run by GitHub Actions. On every push or merge to
`master` or `preview` it does this:

```
version ─► macos ─────┐
         ─► windows ──┴─► release ─► sync-preview (master only)
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
5. **sync-preview** (only on `master`) merges `master` back into `preview`; see
   [Keeping `preview` up to date](#keeping-preview-up-to-date).

Builds made by this workflow are **official**: they know which repository they came from
(`COAT_REPO`) and update themselves from its releases (`COAT_RELEASE_BUILD`). Within a few hours of
a release, everyone's running COAT shows "Update ready" in its sidebar.

The files are named after their version (see [Release files](#release-files)), so the README
links to the [latest release page](https://github.com/archways404/TVX-SOC-COAT/releases/latest)
rather than to a file. "Latest" never points at a pre-release.

Also good to know:

- **Pull requests** run the same build and tests, so you see problems before merging, but they
  don't publish anything. A pull request into `preview` builds COAT Preview. The apps are kept as
  downloadable workflow *artifacts* for 14 days.
- **Changes that only touch documentation** (`*.md` files) don't trigger a release.
- **One release at a time per branch**: if two merges to the same branch happen close together,
  the second waits for the first, so they can't get the same version number.
- **Re-running** a workflow for a commit that's already released doesn't publish it twice.
- **No version commits.** The version number lives in the release's tag; the workflow stamps it
  into `Cargo.toml` only for the build. The only thing the workflow pushes is the merge of
  `master` into `preview`.

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

## Release files

Every release has the version in its file names:

| | Mac | Windows |
|---|---|---|
| COAT | `COAT_v1.5.0.zip` (contains `COAT.app`) | `COAT_v1.5.0.exe` |
| COAT Preview | `COAT_v1.5.0-preview.3_PREVIEW.zip` (contains `COAT Preview.app`) | `COAT_v1.5.0-preview.3_PREVIEW.exe` |

Plus `SHA256SUMS.txt`, the checksums that installed copies use to verify an update.

**Old names, for old copies.** Releases before 0.5 used fixed names (`COAT-macOS.zip`, `COAT.exe`,
`COAT-PREVIEW-macOS.zip`, `COAT-PREVIEW.exe`), and copies of COAT 0.4 and older look for those
names when they update themselves. So each release also includes its files under the old names.
Once nobody runs 0.4 or older any more, set `LEGACY_FILE_NAMES: "false"` in the release job of
`.github/workflows/build.yml` to stop publishing them.

**Homebrew.** Each release also updates the Homebrew tap (once it's [set up](#3-homebrew-optional)),
so `brew install --cask archways404/tap/coat` (or `coat@preview`) always installs the newest one.

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

### 3. Homebrew (optional)

To offer `brew install --cask archways404/tap/coat`:

1. Create a **public** repository called **`homebrew-tap`** under the same owner
   (`archways404/homebrew-tap`), with just a README. Homebrew finds it by that name.
2. Create a token that may push to it: GitHub **Settings → Developer settings → Personal access
   tokens → Fine-grained tokens → Generate new token**. Repository access: only `homebrew-tap`.
   Permissions: **Contents: Read and write**.
3. In **this** repository, add it as the secret **`HOMEBREW_TAP_TOKEN`** (Settings → Secrets and
   variables → Actions → New repository secret).

From the next release on, the workflow writes `Casks/coat.rb` (and `Casks/coat@preview.rb` for
previews) into the tap. Without the secret it skips this step and says so in the run.

Each channel writes only its own cask: `coat` appears with the next **stable** release (a merge to
`master`), and `coat@preview` with the next preview. Until then, `brew install` says
"Cask 'coat' is unavailable".

Casks are generated by `packaging/homebrew/write-cask.sh`. They mark COAT as updating itself
(`auto_updates true`), so `brew upgrade` leaves it alone. Homebrew can't skip the
[first-time question](user-guide.md#the-first-time-you-open-it) for an app that isn't signed with a
company certificate.

## Day to day

- **Ship a change**: merge it to `preview` for a pre-release, then `preview` to `master` for a
  release. See [How to ship a change](#how-to-ship-a-change).
- **Watch it**: the **Actions** tab shows each run; the summary of the *version* step says which
  version it will publish.
- **A run failed**: open it and look at the red step. Test failures show which test and why.
  Fix it, merge again, and a new release with the next version is made.
- **Undo a release**: see [Undoing a release](#undoing-a-release).

## Signing the apps (future)

Today the apps are not signed with a company certificate, so the first time someone opens COAT,
macOS and Windows ask whether to trust it (see the
[user guide](user-guide.md#the-first-time-you-open-it)). Signing would remove those warnings:

- **Mac**: an Apple Developer ID certificate, plus *notarization* (Apple scans the app). Both happen
  in the `macos` job with `codesign` and `xcrun notarytool`, using the certificate and an Apple
  account password stored as secrets.
- **Windows**: a code-signing certificate, applied in the `windows` job with `signtool`.
