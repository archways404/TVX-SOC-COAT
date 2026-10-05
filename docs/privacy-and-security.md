# Privacy and security

COAT shows details of real phone calls, so it's built to keep that data on your own computer.
This page explains what COAT handles, where it goes, and how it's protected.

## What data COAT handles

COAT reads a call's log lines from simlog. By default it asks for them **unscrubbed**, because
troubleshooting usually needs the real values. Unscrubbed log lines can contain:

- phone numbers of the caller and of everyone the call reached;
- names of users and agents;
- anything the caller typed in a menu or script, which can include an ID number;
- IP addresses of phones and servers, and technical details of the call.

Tick **scrub personal data** on the start page (or use `--scrub` in the terminal) to ask simlog to
replace these with placeholders instead.

## Where the data goes

**Nowhere except your own computer.**

- COAT fetches calls from simlog over the VPN. The only other place it contacts is GitHub, to check
  for updates (see below). It collects no usage statistics.
- **In the app**, traced calls are kept in memory while COAT runs, so that recent traces open
  instantly. They're gone when you click **Quit COAT**, or when COAT stops by itself after 12
  hours without use. Call data is never written to disk. COAT only saves one setting (whether to
  update automatically) in your user settings folder.
- **In the terminal**, `coat <link>` writes the report as an HTML file (by default in a `reports`
  folder in the current directory). Delete those files when you're done with them.
- Reports are single, self-contained files: opening one loads nothing from the internet.
- COAT keeps a small log file, `coat.log`, in your computer's temporary folder. It records when
  COAT started and stopped and any startup errors, not which calls were traced or what was in them.

## Updates

A few seconds after it starts, and every six hours after that, the app asks GitHub for the latest
release of COAT. That request contains nothing about calls: GitHub learns that a copy of COAT (and
which version) is asking, and, like any website, your network address. When there's a new version:

- it's downloaded over HTTPS from the repository's GitHub releases;
- its SHA-256 checksum must match the `SHA256SUMS.txt` published with the release, and the new
  program must report the expected version, or it's thrown away;
- it's kept next to the installed app until it's installed, then the old version is removed.

Only official builds (made by the release workflow) update themselves. A copy someone builds on
their own computer never replaces itself.

## How the app is protected

The app runs a small web server on your computer so that your browser can show it. That server:

- **only listens on `127.0.0.1`**, your own computer. Other machines on the network can't connect;
- **only answers requests addressed to `127.0.0.1` or `localhost`**. A website you visit can't use
  the trick known as *DNS rebinding* to reach it and read your reports;
- only stops (**Quit**) on a `POST` request, so a link on another website can't shut it down.

The **Open in COAT** bookmark only passes the session id from the simlog page you're on to COAT.
It doesn't send anything anywhere else.

## Sharing results safely

- Don't paste unscrubbed reports, screenshots of them, or report files into chats, emails or
  tickets.
- **Copy route as text** gives the short version: the numbers on the route and the reasons, with no
  names or typed digits.
- Need to share more? Trace the call again with **scrub personal data** ticked.

## What's in this repository, and what isn't

- **No internal addresses.** The addresses of the simlog servers are not in the code. Official
  builds get them from repository secrets when they're built (see [Releasing](releasing.md)).
  Developers set them locally (see [Development](development.md#pointing-coat-at-simlog)).
- **No real call data.** The test call in `tests/fixtures/` is made up. Its phone numbers come from
  ranges the Swedish telecom regulator (PTS) reserves for fiction, so they can't belong to anyone.
- **Safety nets.** Git ignores report files (`reports/`, `coat-*.html`) and saved calls
  (`*.coat.json`), so they aren't committed by accident. Keep saved calls outside the repository
  anyway.

## Reporting a security problem

If you find a security problem in COAT, please tell the maintainers privately, not in a public
issue, so it can be fixed before it's widely known.
