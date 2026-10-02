# How COAT works

This page explains how COAT gets from simlog's raw log lines to a route like
*"pressed 1 → forwarded → script → queue → agent answered"*. It's written for anyone curious:
support staff, and developers who want the big picture before reading the code. No programming
knowledge is needed. Technical words are explained in the [glossary](#glossary) at the end.

- [The big picture](#the-big-picture)
- [1. Fetching the call from simlog](#1-fetching-the-call-from-simlog)
- [2. Reading the log lines](#2-reading-the-log-lines)
- [3. Working out the route](#3-working-out-the-route)
- [4. The outcome, the queue and the problems](#4-the-outcome-the-queue-and-the-problems)
- [5. Showing it](#5-showing-it)
- [Limits](#limits)
- [Glossary](#glossary)

## The big picture

```
 simlog link ──► 1. fetch ──► 2. read lines ──► 3. work out ──► 4. outcome, ──► 5. show it
 (session id)    all pages     into entries      the route       queue,         (web page,
                 + linked                                         problems       terminal)
                 sessions
```

Everything runs on your own computer. COAT asks simlog for the call's log lines, analyses them in
memory, and shows the result in your browser. Nothing is stored or sent anywhere else.

## 1. Fetching the call from simlog

**Sessions.** Simlog groups log lines by *session*: one session per call leg, identified by a short
id such as `Ab3dE9x`. Many systems write into the same session: the SIP proxy that carries the
call, the script server that runs menus and scripts, the queue service, and others.

**Linked sessions.** Some things start a new session. When a queue rings an agent, for example, that
ringing is a separate call with its own id. Simlog records the link in plain sentences such as
*"Creating new call Xy7… => Call attempt from queue …"*, and the new session says *"Coming from …"*.
COAT follows these links in both directions, so you can paste the id of either side and still get
the whole call. It deliberately ignores other ids that merely appear in the lines, because a
queue's logs mention every caller waiting in it, and those are other people's calls.

**Pages.** A long call can have 60 000 log lines or more. Simlog hands them out in *pages* of about
20 000 lines, and building each page takes it several seconds. Fetching pages one after another
would take over half a minute for a long call. So when the first page is slow (a sign of a big
call), COAT asks for the next pages at the same time. Each page is requested so that it overlaps
the previous one by exactly one line, and COAT checks every overlap. If anything doesn't line up,
it quietly starts over and fetches the pages one by one, so this shortcut can make COAT faster but
never wrong.

**Scrubbing.** Simlog can replace personal data with placeholders (*scrubbing*). COAT asks for
unscrubbed data by default, because troubleshooting usually needs the real numbers. See
[Privacy and security](privacy-and-security.md).

## 2. Reading the log lines

Each log line is turned into a structured *entry*: the time (to the millisecond), the machine and
program that wrote it, the severity (INFO, WARN or ERROR), where in that program's code it came
from, and the message. Lines come in two shapes:

```
Oct  2 10:00:00 proxy1 tproxy: 2026-10-02 10:00:00,010 INFO PBX-thread-1 executePBX(PBX.java:108) [LID:Ab3dE9x] Executing PBX at …
└──────┬──────┘ └─┬──┘ └─┬──┘  └──────────┬──────────┘ └┬─┘ └────┬────┘ └───────────┬──────────┘ └─────┬─────┘ └──────┬──────┘
   received     machine program        time          level   thread         code location         session id      message

Oct  2 10:00:00 edge1 /usr/sbin/kamailio[42]: NOTICE: <script>: [LID:Ab3dE9x] Call to user …
```

Lines from different machines don't always arrive in order, so COAT sorts all entries by their own
timestamps before doing anything else.

## 3. Working out the route

**The dialplan.** The call flow is driven by a *dialplan*: a list of instructions for each number,
written as `context,number,step`. When a call reaches a number, the SIP proxy logs
*"Executing PBX at …,0846500401,1"* and then runs a small program (a *macro*) for that kind of
destination. The macro's name tells COAT what the number is:

| Macro | Shown as | What it is |
|---|---|---|
| `refer` | Switchboard / IVR menu | A menu with recordings and options |
| `anumber` | User | A person's number |
| `ivrscript` | IVR script | A custom automated script |
| `queue` | Queue | A call queue |

So **every time a macro starts on a new number, the call has arrived somewhere new**. That gives
COAT the stops on the route. When a queue's agent answers, the agent is added as the last stop.

**Why the call moved on.** Between two stops, COAT collects clues about *why* the call left the
previous one, and picks the strongest:

| Clue in the logs | Reason shown | Strength |
|---|---|---|
| A key was pressed, and the dialplan jumped to that key's option | keypress 1 | strong |
| A script logged *"Redirecting caller to …"* | script redirect | strong |
| The queue logged *"got answered after … ms by …"* | agent answered · *time* | strong |
| The redirect script logged *"REASON: unconditional"* (or busy, noanswer…) | unconditional redirect | medium |
| A transfer was logged | blind transfer by … | medium |
| The dialplan dialled another number | dial | weak |

The weaker clues aren't thrown away: they appear under **Arrived via** on the step's card, so you
can see the evidence for each hop.

**What happened at each stop.** Everything that happened between arriving at a stop and leaving it
belongs to that stop's card: recordings played, keys pressed, messages the scripts wrote, warnings,
holds, and settings that were changed (*variables*). Repeating messages are folded together (for
example a queue's "you are number 5 in line" announcement every 30 seconds becomes one line with a
count), and keys pressed in quick succession become one entry, like `pressed 1234#`.

## 4. The outcome, the queue and the problems

**Answered, and by whom.** The linked sessions say who answered: simlog writes sentences such as
*"0846500404 (Agent Smith) answers the call"*. That gives the answer time and the person's name.

**Who hung up.** COAT looks for the first *"… hung up"* sentence across all sessions. It also notes
the first BYE (the "goodbye" message in the call signalling), and uses that if there's no such
sentence.

**Failures.** Signalling replies in the 400–699 range to a call attempt (such as *486 Busy* or
*480 Unavailable*) are listed as failures.

**Queues.** For each queue COAT records when the caller entered and left, why they left (answered,
hung up, timed out), the announced place in line over time (the chart in the report), which agents
were rung, and how often no agent was free.

**Problems.** All WARN and ERROR lines are grouped by kind: lines that say the same thing apart from
numbers and ids count as one kind, with a count. Some kinds appear on nearly every healthy call
(for example *"Answer is a NoOp since it has already been called"*); COAT knows a list of those,
marks them as *usually noise*, and hides them behind a toggle so the real problems stand out.

## 5. Showing it

The same analysis is shown in three places:

- **The web page** (what you see after double-clicking COAT): a small web server built into COAT
  runs on your own computer and shows the report in your browser.
- **A report file**: `coat <link>` in a terminal writes the same report as a single HTML file. It
  needs no internet connection and no COAT to open.
- **The terminal**: a short summary with the route and the main problems.

## Limits

- COAT understands the log messages of today's systems. If a system changes the wording of a
  message COAT relies on, that part of the analysis stops working until COAT is updated. The tests
  catch this for the shapes COAT knows about.
- The route follows the caller. Calls that fork into several legs at once (ring groups) show the
  legs, but the main route is the one the caller was on.
- Simlog keeps calls for a limited time; older calls can't be traced.

## Glossary

| Term | Meaning |
|---|---|
| **simlog** | Telavox's internal tool that collects the log lines of all systems a call touches, grouped by call. |
| **session**, **session id** | One call leg in simlog, and its short id (e.g. `Ab3dE9x`). Also called *LID* in log lines: `[LID:Ab3dE9x]`. |
| **linked session** | Another session that belongs to the same call, such as a queue ringing an agent. |
| **scrubbing** | Replacing personal data (numbers, names, typed digits) with placeholders. |
| **SIP** | The signalling protocol phone calls use: messages such as INVITE (start a call), 200 OK (answered), BYE (hang up). |
| **SIP ladder** | A diagram of SIP messages between systems over time. |
| **re-INVITE** | An INVITE inside an existing call, used to change it, for example to put it on or off hold. |
| **dialplan** | The instructions that decide what happens to a call at each number. |
| **macro** | A reusable piece of dialplan for one kind of destination (menu, user, script, queue). |
| **IVR** | Interactive Voice Response: a menu or automated script the caller talks to with keypresses. |
| **AGI script** | A script the dialplan runs to make decisions, e.g. `Redirect.agi`. Its messages appear in the logs as *"From Redirect.agi: …"*. |
| **redirect** | Forwarding a call to another number: *unconditional* (always), *busy*, *noanswer*. |
| **DTMF** | The tones sent when a caller presses keys. |
| **queue agent** | A person who answers calls from a queue. |
| **WARN / ERROR** | The severity of a log line. Many WARN lines are routine. |
