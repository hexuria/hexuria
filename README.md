[hexuria.github.io/hexuria](https://hexuria.github.io/hexuria/)

**The Portable Agent Toolchain**

Agents shouldn't just use computers.
They should build their own.

Hexuria is building the Portable Agent Toolchain that lets AI workers acquire, verify, install, permission, execute, update and replace their own software without rebuilding their entire computer.

Infrastructure for persistent AI coworkers.

Hexuria is the infrastructure. The Portable Agent Toolchain is the category. Persistent AI coworkers are the outcome, not the category.

## The problem

Today a person builds a container, installs everything an agent might need, and hands it over. The next need — Postgres, ffmpeg, a compiler, an MCP server, an internal tool — means that person rebuilds the image.

That does not scale. A hundred specialized workers is already too many images to babysit. At ten thousand, rebuilds become the job. At a million, a human in the rebuild path is not an architecture.

A million agents cannot wait for a million Docker rebuilds. That is the scaling argument. Hexuria does not operate a million agents.

Don't rebuild the computer. Install the capability.

## The lifecycle

A capability is a shippable unit the worker can acquire, move, and run on its own.

Discover → Acquire → Verify → Authorize → Install → Execute → Observe → Update → Rollback / Revoke / Remove.

MCP tells an agent what tools it can call. The Portable Agent Toolchain tells it what software it can own. Calling a tool and owning a capability are different. Owning requires that lifecycle.

The model is replaceable. The worker remains. Each capability has identity: what it is, why it was installed, which version, where it came from, whether it was verified, which agent may use it, its permissions and credentials, which runtime executes it, what it may consume, how it is updated, rolled back, or revoked, and what happened when it ran. It survives a model turn, a model change, a closed window, and a worker restart where that is the right semantic.

This is not a second agent runtime. [OpenGrok](https://github.com/hexuria/opengrok-server) already owns the harness: one lifecycle for turns, tool calls, results, cancellation, retries, persistence, replay, permissions, failures, and state. The toolchain extends what that same worker can acquire and execute.

What is on main today is narrower than the picture. OpenGrok loads agent plugins as folders of skills and MCP servers. Unverified tools ask a person first. A pinned, per-account catalog install is not on main yet. The lifecycle is the design these repositories are growing into. It is not production-complete.

## Providers

The toolchain sits above the provider. The router picks one. They are not interchangeable. WASM does not replace Docker. A WASIX module does not run on Wasmtime.

| Provider | Status |
| --- | --- |
| [Box](https://github.com/hexuria/box) / OCI, a full Linux computer | Shipped |
| Enrolled local machines, through OpenGrok | Partial |
| Wasmtime / WASI | Planned |
| Wasmer / WASIX | Planned |
| Browser worker | Planned |
| Remote SSH | Planned |

## The stack

| | |
| --- | --- |
| [opengrok-server](https://github.com/hexuria/opengrok-server) | Shipped. Harness and control plane. One lifecycle. |
| [nativechat](https://github.com/hexuria/nativechat) | Shipped. The human window. It does not call models or hold API keys. |
| [open-ai-gateway](https://github.com/hexuria/open-ai-gateway) | Shipped. Model and provider routing. A piece, not the headline. |
| [box](https://github.com/hexuria/box) | Shipped. Container execution provider. |
| [impeccable-rust](https://github.com/hexuria/impeccable-rust) | Shipped. Verification practice for Rust an agent writes. Not a proof that every capability is verified. |
| [gpui-agent](https://github.com/hexuria/gpui-agent) | Experimental. In-process control plane for GPUI apps that embed it. |
| [reverse-web-mcp](https://github.com/hexuria/reverse-web-mcp) | Experimental. Web capability research: a plan and a receipt. Not the web provider. |

Wasmtime/WASI, Wasmer/WASIX, browser workers, and remote SSH are planned. No repository here claims them.

An install-time authority layer, beyond OpenGrok's consent gate (block refuses, ask raises a card, allow proceeds), is planned. It is not a product in this org.

Not in this picture: Allowly is a phone remote for macOS dialogs, not that authority layer. gol is a separate runtime, and this stack has one harness.

## The outcome

One worker. Its own tools. Its own computer. Its own permissions. Its own harness.

Models provide intelligence. The toolchain gives intelligence somewhere to work.

We're building the software infrastructure for software workers.

[Follow the build](https://github.com/hexuria)
