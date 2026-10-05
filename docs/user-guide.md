# User guide

This guide is for everyone who uses COAT. You don't need any technical background.

- [What COAT does](#what-coat-does)
- [Installing COAT](#installing-coat)
- [The first time you open it](#the-first-time-you-open-it)
- [Tracing a call](#tracing-a-call)
- [One click from simlog](#one-click-from-simlog)
- [Finding your way around](#finding-your-way-around)
- [Reading a report](#reading-a-report)
- [Personal data: scrubbed and unscrubbed](#personal-data-scrubbed-and-unscrubbed)
- [Updates](#updates)
- [Stopping COAT](#stopping-coat)
- [COAT Preview](#coat-preview)
- [Troubleshooting](#troubleshooting)

## What COAT does

When a customer asks "why did my call end up there?" or "why did nobody answer?", the answer is
in **simlog**, Telavox's call log tool. Simlog shows every log line that every system wrote about
the call, often tens of thousands of them. COAT reads those lines for you and shows:

- the **route**: each number the call reached, in order, and the reason it moved on;
- **where the time went**: how long the caller spent in each menu, script and queue;
- **the outcome**: who answered, after how long, how long they talked, and who hung up;
- **problems**: errors and warnings worth a look, with the routine noise filtered out.

## Installing COAT

You need to be connected to the Telavox **VPN** whenever you use COAT.

**Mac**

1. Download [COAT-macOS.zip](https://github.com/archways404/TVX-SOC-COAT/releases/latest/download/COAT-macOS.zip).
2. Double-click the zip file. A **COAT** app appears next to it.
3. Drag **COAT** into your **Applications** folder.

**Windows**

1. Download [COAT.exe](https://github.com/archways404/TVX-SOC-COAT/releases/latest/download/COAT.exe).
2. Move it somewhere you'll find it again, for example your desktop.

There's nothing else to install.

## The first time you open it

COAT isn't signed with a company certificate yet, so your computer asks whether to trust it.
You only do this once.

**Mac.** You'll see *"Apple could not verify COAT is free of malware"*.

1. Click **Done** (not "Move to Trash").
2. Open **System Settings → Privacy & Security**.
3. Scroll down to the message about COAT and click **Open Anyway**.
4. Confirm with your password or Touch ID.

**Windows.** You'll see *"Windows protected your PC"*.

1. Click **More info**.
2. Click **Run anyway**.

## Tracing a call

![COAT's start page: a box to paste a simlog link into, and the Open in COAT bookmark button](images/start-page.png)

1. **Double-click COAT.** Your web browser opens COAT's start page.
   There's no separate COAT window; COAT runs quietly in the background and lives in the browser.
   Double-clicking it again just opens another tab.
2. **Copy the link of the simlog page** for the call. It looks like
   `https://partner.telavox.se/partner2/simlog/index.jsp?sessionid=Ab3dE9x`.
   The session id alone (`Ab3dE9x`) works too.
3. **Paste it into the box** on COAT's start page. The trace starts straight away; you don't
   need to press anything. You can also drag a link onto the page.
4. **Wait a few seconds.** Small calls take a second or two. A very long call (for example half
   an hour in a queue) can take 15–20 seconds, because simlog needs time to hand over all the log
   lines. COAT shows its progress while it works.

The report opens when it's done. The calls you've traced are listed under **Recent** in the sidebar,
and opening one again within 15 minutes is instant. Tick **don't use a cached result** to fetch a fresh copy.

## One click from simlog

You can open any simlog page in COAT with one click, straight from simlog:

1. On COAT's start page, find the **Open in COAT** button under *One click from simlog*.
2. **Drag** that button onto your browser's bookmarks bar. (If you can't see the bookmarks bar:
   Chrome and Edge show it with *Ctrl+Shift+B*, or *⌘+Shift+B* on a Mac.)
3. From now on, when you're looking at a call in simlog, click **Open in COAT** in the bookmarks
   bar. The trace opens in a new tab.

COAT has to be running for this to work. If the bookmark opens a page that says it can't connect,
double-click COAT first and try again.

## Finding your way around

The **sidebar** on the left is how you get around:

- **Trace a call…** at the top: paste a link or session id and press Enter. Press `/` to jump to it.
- **This call** lists every part of the report you're reading. Click one to jump there; the part
  on screen is highlighted as you scroll. Under **Steps** there's one entry per stop of the route,
  in the same colour as on the route line. The numbers show how many steps and how many problems
  worth a look there are.
- **Recent** lists the calls you've traced since COAT started.
- At the bottom: whether COAT is up to date, **Quit COAT**, and the version.

To make more room, collapse the sidebar to a narrow strip of icons with the button at the top left,
or with **⌘B** (Mac) / **Ctrl+B** (Windows). COAT remembers your choice. In a narrow window the
sidebar hides completely and slides in when you click that button.

## Reading a report

![A COAT report](images/report.png)

*A made-up call; all numbers and names are fictional.*

From the top:

**The headline.** Who called which number, whether it was answered and by whom, and the key times:
how long the whole call lasted, how long the caller waited before someone answered, and how long
they talked.

**Route.** The whole journey on one line. Each dot is a number the call reached; the colour tells
you what kind of destination it is:

| Colour | Kind | What it means |
|---|---|---|
| purple | Switchboard / IVR menu | The caller hears a recording and can pick an option |
| blue | User | A person's number |
| orange | IVR script | An automated script, for example asking for an ID number |
| teal | Queue | Callers wait until an agent is free |
| green | Queue agent | The person in the queue who answered |

The text between two dots is the reason the call moved on:

| Reason | Meaning |
|---|---|
| keypress 1 | The caller pressed 1 in a menu |
| unconditional redirect | The number forwards all its calls |
| busy / noanswer redirect | The number forwards calls when busy / when nobody answers |
| script redirect | An automated script sent the caller on |
| agent answered · 21m 47s | An agent in the queue picked up, after this long in the queue |

**Copy route as text** gives you the same thing in words, ready to paste into a ticket:

```
0701740605 calls 0846500400
in 0846500400: keypress 1 → 0846500401
in 0846500401: unconditional redirect → 0846500402
```

**Where the time went.** One bar for the whole call, split by step. A long teal block means a long
wait in a queue.

**Steps.** One card per destination, in order. Each card shows how the call arrived there and what
happened while it was there: recordings played, keys pressed, messages from scripts, warnings, and
for queues a chart of the caller's place in line.

The sections further down (**Issues**, **Variables**, **SIP ladder**, **Log**) are mainly for
technical troubleshooting:

- **Issues** groups the warnings and errors. Kinds that appear on almost every normal call are
  hidden; tick the box to show them too.
- **Variables** lists the settings the call flow used.
- **SIP ladder** shows the signalling messages between systems. Click one to read it in full.
- **Log** lets you search all the log lines, filtered by step.

## Personal data: scrubbed and unscrubbed

By default COAT shows the call **unscrubbed**: real phone numbers, names, and anything the caller
typed (such as an ID number). That's usually what you need to troubleshoot, but treat it with care:

- Don't paste reports or screenshots into chats, emails or tickets. **Copy route as text** gives a
  short version with just the numbers on the route.
- Reports only live in your browser and on your own computer. COAT never sends them anywhere.
- Tick **scrub personal data** on the start page to get a version with numbers and names replaced
  by placeholders.

See [Privacy and security](privacy-and-security.md) for the details.

## Updates

COAT keeps itself up to date. A few seconds after it starts, and every few hours after that, it
checks whether there's a new version. What you see at the bottom of the sidebar:

| Sidebar says | What it means |
|---|---|
| **Up to date** | You have the newest version. **Check now** checks again. |
| **Downloading v…** | A new version is on its way; you can keep working. |
| **v… is ready**, with **Restart to update** | Click it to switch to the new version now. COAT restarts in a few seconds and opens the call you were looking at again. If you don't click, it installs by itself the next time COAT has been unused for half an hour. |
| **v… is available**, with **Download** | COAT can't replace itself where it is (see below). Download the new version and replace the old one by hand. |
| **Update failed** | Usually a network hiccup. **Try again**, or download the new version by hand. |

Untick **Update automatically** if you'd rather decide yourself: COAT then still tells you about new
versions, but only downloads one when you click **Download & install**.

COAT can't update itself when:

- **Mac**: it isn't in your **Applications** folder (for example, you opened it straight from
  Downloads). Move it to Applications and open it again.
- **Windows**: it's in a folder you're not allowed to change. Move `COAT.exe` to, for example, your
  desktop or Documents.

Every update is checked against a fingerprint (a SHA-256 checksum) published with the release before
it's installed, so a damaged or tampered download is never used.

## Stopping COAT

Click **Quit COAT** at the bottom of the sidebar. COAT also stops by itself after 12 hours without
use. Double-click COAT to start it again.

## COAT Preview

Besides COAT itself there's **COAT Preview**: the newest version, released before it reaches
everyone, so a few people can try it first. You'll recognise it by its **green sidebar**, the
**PREVIEW** label, and its green icon.

- It's a **separate app**: install it next to COAT, and use either. Both can run at the same time.
- Download it from the [Releases page](https://github.com/archways404/TVX-SOC-COAT/releases): pick
  the newest release marked **Pre-release** and download `COAT-PREVIEW-macOS.zip` (Mac) or
  `COAT-PREVIEW.exe` (Windows).
- It updates itself to newer previews, just like COAT updates itself to newer releases.
- Previews are less tested. If something looks wrong, check it in COAT, and tell the developer.

## Troubleshooting

| What you see | What to do |
|---|---|
| *"Can't reach simlog … Are you on the VPN?"* | Connect to the VPN and try again. |
| *"Session … not found"* | Check the link or id. Simlog only keeps calls for a limited time, so very old calls are gone. |
| *"Don't know how to read …"* | Paste the full simlog link, or just the session id. |
| *"This copy of COAT doesn't know the address of the … simlog"* | You have an unofficial build. Download COAT from the links above. |
| The **Open in COAT** bookmark says the page can't be reached | COAT isn't running. Double-click it, then click the bookmark again. |
| Nothing happens when you double-click COAT | Wait a few seconds, then look for a new browser tab. If there's none, open `http://127.0.0.1:7171` in your browser yourself. |
| The sidebar offers **Download** instead of **Restart to update** | COAT can't replace itself where it is: see [Updates](#updates). |
| The Mac or Windows warning keeps coming back | Follow [the first-time steps](#the-first-time-you-open-it) exactly once; download COAT again if the file was damaged. |

If something else goes wrong, COAT keeps a log file called `coat.log` in your computer's temporary
folder. Whoever helps you may ask for it.
