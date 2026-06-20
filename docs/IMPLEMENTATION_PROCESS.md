# Implementation Process

## Phase 1 - Platform domain first

Build pure domain models:

1. Company
2. Catalog Item
3. Billing Plan
4. Package
5. Package Item
6. Purchase
7. Subscription
8. Entitlement
9. Enrollment
10. Pay Plan Stack
11. Events
12. Reward Ledger

No database yet. Use in-memory tests.

## Phase 2 - Pay plan engine

Build the engine that runs modules.

Process:

1. Receive event
2. Identify company, package, enrollment
3. Load stack
4. Select modules that react to event
5. Run modules in order
6. Collect state changes
7. Collect emitted events
8. Collect ledger entries
9. Return transaction result

## Phase 3 - Royal Flush modules

Build and test:

1. Royal Flushline
2. Royal Matrix
3. Royal Pot Bonus
4. Account Duplication
5. Sponsor Allocation

Port the invariant tests from the RFN PRD.

## Phase 4 - Binary modules

Build and test:

1. Binary Tree
2. Binary Volume
3. Binary Pairing Bonus
4. Binary Carryover
5. Binary Caps

## Phase 5 - Persistence

Add Postgres repositories.

Save transactionally:

- purchase
- subscription
- entitlement
- enrollment
- module state
- emitted events
- ledger entries

## Phase 6 - Leptos Spin SSR

Build web in this order:

1. auth
2. company admin
3. catalog builder
4. billing plan builder
5. package builder
6. pay plan stack builder
7. purchase flow
8. user dashboard
9. admin event log
10. admin reward ledger

## Phase 7 - Scheduled workflows

Add after manual flows work:

- subscription renewal
- Royal weekly pot distribution
- Binary cycle close
- failed payment grace handling
- ledger approval and payout workflow

## Build discipline

For every feature, ask:

```text
Is this a rule? -> payplan_core
Is this a workflow? -> payplan_app
Is this DB, email, payment, auth, scheduler? -> payplan_infra
Is this route, form, SSR, page, session? -> payplan_web
```
