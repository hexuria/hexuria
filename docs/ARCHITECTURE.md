# Architecture

## Architecture goal

Build a maintainable Rust pay plan platform, not a hardcoded MLM app.

The platform must support multiple companies, package catalogs, one-time and recurring billing, and stackable pay plan modules.

## Layers

### payplan_core

Pure domain layer.

Owns:

- Company identity model
- Catalog item model
- Product/service distinction
- Billing plan model
- Package model
- Purchase model
- Subscription model
- Entitlement model
- Enrollment model
- Pay plan stack model
- Event model
- Reward ledger model
- Royal Flush modules
- Binary modules

Does not own:

- SQL
- HTTP
- Leptos
- Spin
- payment gateway SDKs
- email
- scheduled jobs

### payplan_app

Workflow layer.

Owns:

- create company
- create product or service
- create billing plan
- create package
- purchase package
- create enrollment
- run pay plan engine
- run module stack
- collect events
- collect ledger entries
- close compensation periods

### payplan_infra

Infrastructure layer.

Owns:

- Postgres repositories
- event store
- ledger store
- auth
- email
- payment gateway adapters
- schedulers
- transaction manager

### payplan_web

Leptos Spin SSR delivery layer.

Owns:

- routes
- server functions
- forms
- session extraction
- redirects
- admin pages
- dashboard pages
- company setup
- package builder
- pay plan stack builder

`payplan_web` is not UI-only. It is the SSR web boundary. It should still call `payplan_app` instead of directly mutating domain state.

## Dependency direction

```text
payplan_web -> payplan_app -> payplan_core
payplan_infra -> payplan_app ports
```

Core remains pure. App coordinates. Infra persists. Web delivers.

## Module execution

A package purchase produces events. The pay plan engine loads the package stack and runs modules interested in those events.

```text
PackagePurchased
EnrollmentCreated
  -> PayPlanEngine
  -> active modules
  -> state changes
  -> emitted events
  -> ledger entries
  -> transaction commit
```

## Stack versioning

Pay plan stack versions should be immutable after purchases exist.

When a company changes compensation rules, create a new stack version and assign future purchases to it.
