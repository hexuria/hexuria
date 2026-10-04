[hexuria.github.io/hexuria](https://hexuria.github.io/hexuria/)

**The Portable Agent Toolchain**

Agents shouldn't just use computers.
They should build their own.

Hexuria is building the Portable Agent Toolchain: AI workers acquire, verify, install, permission, and replace their own software. Install the capability. Do not rebuild the machine.

## 1. What we are building

A capability is a shippable unit the worker can acquire, move, and run. It has identity: what, why, version, provenance, who may use it, permissions, runtime, limits, update, rollback, revoke, trace. It survives a model turn, a model change, a closed window, and a worker restart where that is the right semantic.

The model is replaceable. The worker remains.

Discover → Acquire → Verify → Authorize → Install → Execute → Observe → Update → Rollback / Revoke / Remove.

MCP tells an agent what tools it can call. The Portable Agent Toolchain tells it what software it can own.

The router sits above the provider. One provider per capability. They are not interchangeable. WASM does not replace Docker. WASIX does not run on Wasmtime.

| Provider | |
| --- | --- |
| Box / OCI, a full Linux computer | Shipped |
| Enrolled local machines | Partial |
| Wasmtime / WASI | Planned |
| Wasmer / WASIX | Planned |
| Browser worker | Planned |
| Remote SSH | Planned |

One harness, not a second runtime. The toolchain extends what that worker can acquire and run.

## 2. What we have built so far

| | |
| --- | --- |
| [opengrok-server](https://github.com/hexuria/opengrok-server) | Shipped. The harness. One lifecycle for turns, tools, computers, and the consent gate. |
| [nativechat](https://github.com/hexuria/nativechat) | Shipped. The human window. It does not call models or hold API keys. |
| [open-ai-gateway](https://github.com/hexuria/open-ai-gateway) | Shipped. Model routing. A piece, not the headline. |
| [box](https://github.com/hexuria/box) | Shipped. The full-computer provider. A Linux guest, not the control plane. |
| [impeccable-rust](https://github.com/hexuria/impeccable-rust) | Shipped. A verification skill for Rust an agent writes. Not a proof of every capability. |
| [gpui-agent](https://github.com/hexuria/gpui-agent) | Experimental. In-process control for GPUI apps that embed it. Not a computer. |
| [reverse-web-mcp](https://github.com/hexuria/reverse-web-mcp) | Experimental. Web-action research: a plan and a receipt. Not the web provider. |

**Not built yet:** Wasmtime/WASI, Wasmer/WASIX, browser workers, remote SSH, an install-time authority layer beyond the consent gate, and a pinned catalog install. Plugins today are folders of skills and MCP servers. Unverified tools ask a person first.

Not production-complete.

## 3. Ambition and goal

Agents that build and maintain their own computers. Capabilities portable across workers.

A hundred specialized workers is already too many images to babysit. At ten thousand, rebuilds become the job. At a million, a human in the rebuild path is not an architecture. That is the scaling argument. Hexuria does not run a million agents.

A million agents cannot wait for a million Docker rebuilds.

We're building the software infrastructure for software workers.

[Follow the build](https://github.com/hexuria)
