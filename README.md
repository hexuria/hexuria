# The Portable Agent Toolchain

I've been working on a problem with OpenGrok that I think will become more common as AI agents get their own computers.

An agent can install software. But what happens to that software when we move the agent to another computer?

Say an agent installs Rust, Node.js, GitHub CLI, and a database client. It configures everything and starts working.

Now we want to replace its container or run that same agent somewhere else.

We can rebuild the Docker image, use mounted volumes, or write setup scripts. Those are valid solutions, and we already use persistent mounts in Box.

But I want something different.

**I want the agent to own its software environment independently of the computer running it.**

That's the idea behind the Portable Agent Toolchain.

## What we're building

The Portable Agent Toolchain is a way for AI workers to acquire, install, manage, and reuse their own software.

Instead of maintaining a custom Docker image for every kind of agent, we want to let agents build up their own toolchains and attach them to compatible execution environments.

For example, if a coding agent installs a particular version of Rust and a few CLI utilities, that setup should be reusable when we replace its computer.

It shouldn't have to rediscover and reinstall everything.

But copying binaries around isn't enough. We need to account for dependencies, operating systems, CPU architectures, permissions, and updates.

Each installed capability should eventually have a record of:

- What was installed, its version, and where it came from
- Which runtimes and systems it supports
- Which agents may use it and with what permissions
- How it was verified and what was actually checked
- How to update, roll back, revoke, or remove it
- What happened when it was executed

The agent could request new software, but it wouldn't get unrestricted installation rights. The existing harness and human approval system would still control what it can do.

We want that software inventory to persist independently of the model, conversation, and execution environment.

Changing the model shouldn't erase the worker's tools.

### How this relates to MCP

We're not replacing MCP.

MCP lets applications expose tools, resources, and prompts to agents.

The Portable Agent Toolchain addresses a different problem: the software installed in the agent's execution environment.

A GitHub MCP server and the GitHub CLI are not the same thing.

The CLI is a real executable with dependencies, versions, installation requirements, and access permissions.

We want agents to manage software like that without requiring a new machine image every time.

## Where it runs

We're not building another agent harness. OpenGrok already has one.

The Portable Agent Toolchain will extend that harness with software lifecycle management.

The existing and proposed execution environments are:

| Environment | Status |
|---|---|
| Local Docker / OCI | Implemented |
| Box Linux guest | Implemented separately |
| Enrolled local computers | Partial |
| Wasmtime / WASI | Planned |
| Wasmer / WASIX | Planned |
| Dedicated browser worker | Planned |
| Remote SSH | Planned |

These environments have different capabilities.

A Linux executable cannot automatically run on macOS. WASI and WASIX have different compatibility requirements. Some tools need an entire operating system; others could run as WebAssembly components.

The toolchain needs to understand those differences rather than pretend every environment can run the same software.

OpenGrok Server already abstracts computer operations behind a `Computer` interface, with local Docker and box.ascii.dev adapters. Our separate Box project implements a Linux guest with execution, filesystem, browser, and computer-use interfaces.

The longer-term plan is to route installed capabilities to compatible environments through the existing harness.

## What we already have

This isn't starting from an empty repository.

We've built several pieces of the infrastructure:

| Project | What it does |
|---|---|
| [opengrok](https://github.com/hexuria/opengrok) | Native Rust/GPUI desktop interface for OpenGrok |
| [opengrok-server](https://github.com/hexuria/opengrok-server) | Durable agent harness, tools, policies, scheduling, approvals, and computer management |
| [open-ai-gateway](https://github.com/hexuria/open-ai-gateway) | Model routing and provider access |
| [box](https://github.com/hexuria/box) | Linux agent computer with shell execution, files, Chromium, CDP, and computer-use APIs |
| [impeccable-skills](https://github.com/hexuria/impeccable-skills) | Skill router for Rust and Python code verification workflows |
| [gpui-agent](https://github.com/hexuria/gpui-agent) | Experimental semantic automation for GPUI applications |
| [plugin-marketplace](https://github.com/hexuria/plugin-marketplace) | Catalog of plugins, including sources pinned to Git commits |

The existing system already has useful pieces of capability management.

OpenGrok Server has a policy and consent system. Plugins can bundle skills and MCP servers. The marketplace supports pinned source revisions. Box supports persistent workspace and browser-profile directories.

However, none of that is the complete Portable Agent Toolchain.

We still need general-purpose software packaging, verification, installation policies, dependency handling, compatibility checks, and a way to attach a managed toolchain to another computer.

We also need reliable updates, rollback, revocation, and execution records.

Those are the parts we're working toward.

## Why I'm building this

Today, we can give an agent a Linux computer and let it install whatever software its permissions allow.

But the more specialized an agent becomes, the more software it depends on.

One agent needs a Rust toolchain. Another needs Python and database utilities. Another needs accounting software and browser automation.

I don't want those requirements permanently tied to the machine image each agent started with.

Docker remains useful. So do volumes, containers, remote machines, and WASM runtimes.

The point isn't to replace them.

**The point is to stop treating the computer and the agent's installed software as the same thing.**

This becomes especially important when you have many specialized agents, each maintaining a different set of tools.

I want to be able to replace a computer, move an agent, or reuse its verified toolchain without rebuilding its entire working environment by hand.

We're still early. The Portable Agent Toolchain isn't production-complete, and several execution providers are still only planned.

But we've already built enough of the surrounding infrastructure to start tackling it.

That's what we're building at Hexuria.

[Follow the build](https://x.com/codeitlikemiley) · A [Goldcoders Corp](https://goldcoders.dev) project
