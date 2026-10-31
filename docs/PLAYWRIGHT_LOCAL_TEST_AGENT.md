# Playwright local test agent — Bridge/Camouhost (#802)

**Test-only operator mode**, never a second browser or profile writer. The agent
executes Python Playwright commands **on the same thread and BrowserContext**
that the shipping Camouhost already owns. Ordinary installed browser launches
are unchanged unless the operator explicitly opts in to the local agent.

## One-time local setup on the interactive Windows test host

**Normal Windows frontend Launch (recommended).** Opt in once, under the same
Windows account that owns the installed Bridge:

```powershell
$agentDir = Join-Path $env:LOCALAPPDATA 'ProfileBridgeAgentTests'
New-Item -ItemType Directory -Path $agentDir -Force | Out-Null
New-Item -ItemType File -Path (Join-Path $agentDir 'playwright-enabled') -Force | Out-Null
```

Then open the **ordinary authorized frontend** and press **Launch** for a
profile that is independently granted to this user. A `profilebridge://`
Windows protocol handler is a *new process*: it does not inherit environment
variables set in an unrelated PowerShell window. The explicit user-local marker
lets the installed Bridge find the test inbox anyway. No test URL/claim needs to
be copied into PowerShell, and no new server or browser writer is created.

The directory alone does **not** enable scripts (old diagnostic artifacts are
not opt-in). To turn off tests for subsequent launches without destroying
evidence, remove **only** the `playwright-enabled` file, after the current
test session has finished. Closing a Camoufox window does not magically disable
the mode for future launches while this file exists.

**Direct developer/CI launch (existing alternative).** An explicit per-process
environment setting remains supported and takes precedence over the marker:

```powershell
$agentDir = Join-Path $env:LOCALAPPDATA 'ProfileBridgeAgentTests'
New-Item -ItemType Directory -Path $agentDir -Force | Out-Null
$env:PROFILE_BRIDGE_AGENT_DIRECTORY = $agentDir
# Launch the installed Bridge from this same environment with a fresh,
# normally server-issued profilebridge:// URI when such a launch is allowed.
```

Both methods change **only** local Playwright test diagnostics. Neither creates
a server launch intent, grants profile access, bypasses Cloudflare Access or
Coordinator, changes a browser fingerprint, or disables the normal writer lock,
Save/Commit and recovery. If the execution environment prohibits operating a
browser, this opt-in does not override that policy: stop and record that
external blocker rather than creating or replaying a launch claim.

When Bridge has started Camouhost, inspect the available live sessions:

```powershell
python scripts/camouhost-agent.py --directory $agentDir --list
```

For multiple concurrent profiles select the intended Camouhost PID explicitly.

Create a normal Python script. It receives the live `context`, a snapshot
`pages`, the session ID `session_id`, a local `agent_artifacts_dir` and
ordinary Python imports. All Playwright functionality is available; no
profile-ID allowlist or data-redaction is applied to local tests:

```python
# example-playwright-action.py
from pathlib import Path
context.tracing.start(screenshots=True, snapshots=True, sources=True)
try:
    current = [
        {"url": page.url, "html": page.content()}
        for page in context.pages
    ]
    # In a deliberate browser test the agent may also use goto, locators,
    # fill/click, evaluate, network listeners, cookies, screenshots, storage...
    result = {"restored_pages": current, "cookies": context.cookies()}
finally:
    context.tracing.stop(path=str(agent_artifacts_dir / "trace.zip"))
```

Execute against the **existing** Camouhost process:

```powershell
python scripts/camouhost-agent.py --directory $agentDir --pid <CamouhostPID> --script .\example-playwright-action.py
```

Result/status and full stdout/stderr are retained under the run's
`results` directory. The original submitted source remains in
`requests/*.running`. Playwright traces, screenshots, network logs and other
artifacts created by the script remain on disk after Camouhost exits.
The explicitly opted-in Bridge also writes the **complete raw Camouhost stderr**
to a PID-scoped file under the agent directory; the small user-facing
diagnostic summary remains independent.

No automatic publication or upload to GitHub, Cloudflare or other endpoints
is performed. The agent has deliberately full local access to test profile
contents, including URLs, cookies and credentials: this is equivalent to
executing trusted local Python as the currently logged-in Windows user.

## Semantics and lifecycle

- Bridge remains the **only** owner of claim redemption, lease, exclusive
  generation writer, Close/Save/Commit and recovery. The agent does not invent
  an alternative authority or cloud API.
- In explicit local test mode, Camouhost polls its existing Bridge stdin pipe
  without a helper reader thread; **all Playwright and IPC dispatch run on the
  original Camouhost owner thread**. Normal sessions retain blocking stdin. There is no CDP port, second Playwright browser connection,
  sidecar daemon, headless replacement, or new server.
- The atomic request rename prevents execution of incomplete scripts; each
  request is claimed at most once. Raw stdout/stderr are flushed and fsynced
  before the result is published atomically. If storage fails or Camouhost dies,
  a claimed request can remain `.running` with **no confirmed result**; never
  assume exactly-once completed effects or blindly retry.
- The CLI checks the OS process is live before listing/choosing a session and
  while awaiting its result; stale PID descriptors after an abrupt process death
  are not treated as live. A timestamp-free reused PID is not independent
  authorization; the random per-run directory and existing Bridge ownership
  still determine which session may act.
- The agent can freely change browser state because all profiles here are
  the operator's test environment. The fact of an observed page or Close
  callback is **not** by itself proof that Save/Commit or crash recovery passed.
  Use the normal Bridge Close and verify the authoritative Coordinator result.
- A long/blocking synchronous agent script occupies the single Playwright
  thread. Python cannot safely preempt arbitrary synchronous Playwright calls;
  therefore a CLI timeout is **unconfirmed**, not cancellation or permission to
  retry. If the Camouhost descriptor disappears without a result, the CLI
  returns UNCONFIRMED; the original job/source remains on disk for examination.
  Use short scripts for interactive work. Bridge's existing bounded response
  timeouts and recovery remain authoritative; never force-kill the real profile
  merely to recover an agent script.
- Windows UI Automation is **not** an acceptance gate for web pages. Playwright
  can verify restored browser tabs, actual navigation/DOM/content/storage,
  but not every native Firefox toolbar/dialog element.

## First-operation read-only restoration evidence

Run `scripts/b7-playwright-observe.py` as the **first** agent script after a
normal authorized Open, before navigation, page creation, or modification.
It takes a read-only snapshot of the currently restored Playwright pages,
including their URL, title, DOM, origin storage (when accessible), screenshots,
full context cookies and a machine-readable local artifact. It does not create
tabs, change cookies, navigate, close, or invoke Save. If an observation fails,
its error is kept in the artifact and the script fails, rather than claiming
restoration passed. The snapshot is evidence **only**: compare URLs, DOM and
known test markers against the prior saved state and separately prove exact
Coordinator Save/Reopen and crash classification.

```powershell
python scripts/camouhost-agent.py --directory $agentDir --pid <CamouhostPID> --script scripts/b7-playwright-observe.py
```

## Required physical Windows acceptance

Using the packaged installed Bridge, verify restored pre-existing page URLs
and marker DOM **before any agent page creation**; navigation/interaction;
Close→Coordinator-confirmed Save→fresh Open; exact persistent test data on
three independent profiles; two concurrent profiles and one-writer conflict;
and separate verified crash vs controlled-close semantics on disposable
test data. An identical `D1 READY` projection is not a live Coordinator
authority witness. The hosted Windows acceptance also forces the exact disposable Camoufox
browser-main PID to exit abnormally and requires fail-closed classification.
After capturing the negative result, **the hosted disposable fixture only**
releases surviving Playwright/Firefox child processes through a native Windows
Job Object with kill-on-close (never through a hand-coded PID sweep and never by
deleting working profile files or locks),
while normal GUI Window Close remains a positive witness. This does not
replace the original-machine process-tree/Coordinator incident analysis.
In particular, investigate the Bridge534 forced-PID-close
observation [#802 comment 6515653455](https://github.com/iamaman11/part-crm-emai-profile/issues/802#issuecomment-6515653455)
before declaring crash/Save acceptance.

The source-side test `scripts/test-camouhost-agent.py` checks inbox claims,
full evidence, CLI and same context. The existing AR-10 Camoufox runtime
regression runs the agent against a **real browser**, performs Playwright
DOM actions and exports an actual screenshot/trace; hosted CI does not
replace physical installed Windows B7.
