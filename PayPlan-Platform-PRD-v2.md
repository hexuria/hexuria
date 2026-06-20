# PRD - Configurable MLM Pay Plan Platform v2

## 1. Product Summary

This product is a configurable MLM pay plan platform. It lets an operator create companies, define products and services, bundle them into packages, and attach compensation pay plan stacks to those packages.

Royal Flush Network is no longer treated as the whole app. It is one supported pay plan stack inside the platform. Binary is also part of the platform as a built-in pay plan family, not a future afterthought.

The platform must support:

- Companies
- Products and services
- One-time and recurring package billing
- Package purchases and subscriptions
- User enrollments into packages
- Configurable pay plan stacks
- Built-in Royal Flush modules
- Built-in Binary modules
- Reward ledger and audit events
- Leptos Spin SSR web app

## 2. Product Goal

The goal is to build a maintainable Rust platform where different companies can run different package and compensation structures without rewriting the app.

The system should answer:

- What company owns this package?
- What product or service is being sold?
- Is this purchase one-time or recurring?
- Which pay plan stack should run for this package?
- Which modules are active for this stack?
- What rewards were generated and why?
- What events happened for audit and debugging?

## 3. Non-Goals for the First Build

- No visual drag-and-drop pay plan builder.
- No arbitrary user-written formulas.
- No crypto or on-chain settlement.
- No multi-currency settlement engine yet.
- No external payment gateway lock-in in the domain layer.
- No direct payouts from pay plan modules.

Pay plan modules generate ledger entries. Payment and cashout can be added later as a separate workflow.

## 4. Core Product Model

### 4.1 Company

A company owns its catalog, packages, users, enrollments, and pay plan configurations.

Fields:

- `company_id`
- `name`
- `slug`
- `status`
- `settings`
- `created_at`

### 4.2 Catalog Item

A catalog item is either a product or a service.

Types:

- `Product`
- `Service`

Examples:

- Physical product
- Digital product
- Training service
- Membership access
- Software subscription
- Coaching package

Fields:

- `catalog_item_id`
- `company_id`
- `name`
- `description`
- `item_type`
- `sku`
- `status`
- `metadata`

### 4.3 Billing Plan

A catalog item can be sold one-time or recurring.

Billing types:

- `OneTime`
- `Recurring`

Recurring settings:

- interval: daily, weekly, monthly, quarterly, yearly
- interval count
- trial days
- cancel behavior
- grace period days

Fields:

- `billing_plan_id`
- `catalog_item_id`
- `billing_type`
- `price_amount`
- `currency`
- `recurrence_interval`
- `recurrence_count`
- `trial_days`
- `active`

### 4.4 Package

A package is what a user buys or subscribes to. A package can contain one or many catalog items.

A package can include:

- Products only
- Services only
- Products and services together
- One-time items
- Recurring items
- Commissionable and non-commissionable items

Fields:

- `package_id`
- `company_id`
- `name`
- `description`
- `status`
- `pay_plan_stack_id`
- `default_billing_plan_id`
- `created_at`

### 4.5 Package Item

A package item links a package to a catalog item and billing plan.

Fields:

- `package_item_id`
- `package_id`
- `catalog_item_id`
- `billing_plan_id`
- `quantity`
- `item_role`
- `is_commissionable`
- `commissionable_volume`
- `points_value`

Item roles:

- `Included`
- `Required`
- `OptionalAddon`
- `Upsell`

### 4.6 Purchase

A purchase records the buyer, selected package, price, and payment status.

Fields:

- `purchase_id`
- `company_id`
- `user_id`
- `package_id`
- `sponsor_user_id`
- `gross_amount`
- `net_amount`
- `currency`
- `status`
- `purchased_at`

Statuses:

- `Pending`
- `Paid`
- `Failed`
- `Refunded`
- `Cancelled`

### 4.7 Subscription

A subscription is created when a purchase includes recurring billing.

Fields:

- `subscription_id`
- `company_id`
- `user_id`
- `package_id`
- `billing_plan_id`
- `status`
- `current_period_start`
- `current_period_end`
- `cancelled_at`

Statuses:

- `Trialing`
- `Active`
- `PastDue`
- `Cancelled`
- `Expired`

### 4.8 Entitlement

An entitlement records what the user is allowed to access because of a purchase or subscription.

Examples:

- Membership access
- Course access
- Software access
- Product fulfillment permission

Fields:

- `entitlement_id`
- `company_id`
- `user_id`
- `package_id`
- `catalog_item_id`
- `source_purchase_id`
- `source_subscription_id`
- `status`
- `starts_at`
- `ends_at`

### 4.9 Enrollment

An enrollment is the user's entry into a package's pay plan stack.

Important: pay plans should usually run from enrollments, not directly from products.

Fields:

- `enrollment_id`
- `company_id`
- `user_id`
- `package_id`
- `purchase_id`
- `sponsor_user_id`
- `status`
- `joined_at`

Statuses:

- `Active`
- `Suspended`
- `Cancelled`
- `Expired`

### 4.10 Pay Plan Stack

A pay plan stack is an ordered set of modules attached to a package.

Examples:

Royal Flush Stack:

1. Sponsor Allocation
2. Royal Flushline
3. Royal Matrix
4. Royal Pot Bonus
5. Account Duplication

Binary Stack:

1. Sponsor Placement
2. Binary Tree
3. Binary Volume
4. Binary Pairing Bonus
5. Binary Rank or Cap Rules

Hybrid Stack:

1. Sponsor Allocation
2. Binary Tree
3. Royal Pot Bonus
4. Matching Bonus

Fields:

- `pay_plan_stack_id`
- `company_id`
- `name`
- `version`
- `status`
- `modules`

### 4.11 Pay Plan Module

A module is one compensation mechanic.

Built-in module families:

- Royal Flushline
- Royal Matrix
- Royal Pot Bonus
- Account Duplication
- Sponsor Allocation
- Binary Tree
- Binary Volume
- Binary Pairing Bonus
- Binary Rank and Cap Rules

Each module has:

- module key
- version
- config
- event subscriptions
- state storage
- emitted events
- ledger entries

## 5. Reward Ledger

Modules must not directly pay money. They create ledger entries.

The ledger is the neutral record of earned rewards.

Fields:

- `ledger_entry_id`
- `company_id`
- `user_id`
- `enrollment_id`
- `package_id`
- `source_module`
- `source_event_id`
- `amount`
- `points`
- `currency`
- `status`
- `reason`
- `created_at`

Statuses:

- `Pending`
- `Approved`
- `Paid`
- `Reversed`
- `Voided`

This allows Royal Flush, Binary, and future modules to all produce rewards through the same audit path.

## 6. Event Model

The system uses durable domain events. Events are stored in the database event log.

Core platform events:

- `CompanyCreated`
- `CatalogItemCreated`
- `BillingPlanCreated`
- `PackageCreated`
- `PackagePurchased`
- `SubscriptionCreated`
- `SubscriptionRenewed`
- `SubscriptionCancelled`
- `EntitlementGranted`
- `EnrollmentCreated`
- `EnrollmentSuspended`
- `RewardLedgerEntryCreated`

Royal Flush events:

- `RoyalFlushlineAccountCreated`
- `RoyalFlushlineGraduated`
- `RoyalMatrixCreated`
- `RoyalMatrixCycled`
- `RoyalPotBonusDistributed`
- `RoyalAccountDuplicated`
- `RoyalAccountResetToKing`

Binary events:

- `BinaryNodePlaced`
- `BinaryVolumeAdded`
- `BinaryPairMatched`
- `BinaryCommissionEarned`
- `BinaryCycleClosed`
- `BinaryCarryoverUpdated`

Events are append-only. Corrections should be represented by new reversal or adjustment events.

## 7. Built-In Royal Flush Stack

Royal Flush is implemented as a pay plan stack using built-in modules.

### 7.1 Royal Flushline

Accounts move through:

Ten -> Jack -> Queen -> King -> Ace

Canonical thresholds:

- Ten: 1
- Jack: 2
- Queen: 3
- King: 4
- Ace: 5

An account graduates after spending 15 points across all five tiers.

Key invariants:

- Only the top account in a cardline receives points.
- Graduated accounts are removed from active cardlines.
- An account in the graduated set must not appear in any queue.
- Weekly reset sends qualified graduated accounts back to King, not Ten.

### 7.2 Royal Matrix

Royal Matrix is a 7-slot binary-shaped matrix.

Slot 1 is owner. Slots 2 to 7 are fill slots.

Placement is sponsor-first when the sponsor is in the matrix, then sequential fallback.

When full, the matrix cycles and creates a new matrix for the owner.

### 7.3 Royal Pot Bonus

The pot bonus splits the pool:

- 75 percent equal profit sharing to qualified users
- 25 percent top cycler bonus

A user qualifies only if they have both:

- at least one Flushline graduation
- at least one Matrix cycle

Qualification is user-level, not account-level.

### 7.4 Royal Account Duplication

A Royal account duplicates only after both:

- Flushline graduated
- Matrix cycled

The duplication workflow creates a new account, assigns sponsor, creates matrix, and records events.

## 8. Built-In Binary Stack

Binary is a first-class pay plan family in the app.

Binary modules include:

- Binary Tree
- Binary Volume
- Binary Pairing Bonus
- Binary Carryover
- Binary Rank and Cap Rules

### 8.1 Binary Tree

Each enrollment is placed into a binary position with left and right legs.

Placement strategies:

- sponsor preference
- left/right manual preference
- auto-balance
- outside leg preference

### 8.2 Binary Volume

Purchases add commissionable volume into the binary tree.

Volume values can come from:

- package points value
- package commissionable volume
- product-specific volume
- service-specific recurring volume

### 8.3 Binary Pairing Bonus

The pairing module checks matched left/right volume and records commissions in the ledger.

Configurable settings:

- pairing ratio
- commission percentage
- flush period
- carryover enabled
- max payout cap
- rank requirement

### 8.4 Binary Carryover

Carryover stores unused volume per leg after a cycle closes.

This is built into the module family so binary can be used by any company/package stack.

## 9. Package Purchase Flow

When a user buys a package:

1. Create purchase record.
2. Validate package is active.
3. Validate package items and billing plans.
4. Process one-time or recurring billing intent.
5. Create subscription if recurring.
6. Grant entitlements for products and services.
7. Create enrollment into the package.
8. Emit `PackagePurchased`.
9. Emit `EnrollmentCreated`.
10. Load the package's pay plan stack.
11. Run modules that react to enrollment or purchase events.
12. Create ledger entries for rewards.
13. Save all state and events in one transaction.

## 10. Architecture

Workspace:

```text
payplan-platform/
├── crates/
│   ├── payplan_core/
│   ├── payplan_app/
│   ├── payplan_infra/
│   └── payplan_web/
├── docs/
└── migrations/
```

### payplan_core

Pure domain logic.

Contains:

- platform entities
- package/catalog/subscription model
- pay plan stack model
- event definitions
- ledger model
- Royal Flush modules
- Binary modules

No database, HTTP, Leptos, Spin, or email.

### payplan_app

Application workflows.

Contains:

- create company
- create catalog item
- create billing plan
- create package
- purchase package
- create enrollment
- run pay plan engine
- run Royal distribution
- close Binary cycle
- query dashboard summaries

### payplan_infra

External systems.

Contains:

- PostgreSQL repositories
- event store
- ledger store
- auth
- email
- scheduler
- payment adapter ports

### payplan_web

Leptos Spin SSR delivery layer.

Contains:

- SSR routes
- server functions
- forms
- session extraction
- admin pages
- company setup pages
- package builder
- pay plan stack config pages
- dashboards

This layer is not UI-only. It is the SSR web boundary, but it must call `payplan_app` for business workflows.

## 11. Persistence Model

Core tables:

- companies
- users
- catalog_items
- billing_plans
- packages
- package_items
- purchases
- subscriptions
- entitlements
- enrollments
- pay_plan_stacks
- pay_plan_stack_modules
- event_log
- reward_ledger

Royal module tables:

- royal_flushline_accounts
- royal_flushline_cardline_positions
- royal_matrices
- royal_matrix_slots
- royal_qualifications
- royal_pot_bonus_pool

Binary module tables:

- binary_nodes
- binary_volume_ledger
- binary_cycle_periods
- binary_pairing_results
- binary_carryover

## 12. Implementation Process

### Phase 1 - Platform domain

Build:

- Company
- Catalog Item
- Billing Plan
- Package
- Package Item
- Purchase
- Subscription
- Entitlement
- Enrollment
- Pay Plan Stack
- Reward Ledger
- Domain Events

### Phase 2 - Pay plan engine

Build:

- module contract
- module registry
- event router
- stack runner
- transaction result model
- ledger entry collector

### Phase 3 - Royal Flush modules

Build:

- Royal Flushline
- Royal Matrix
- Royal Pot Bonus
- Royal Account Duplication
- Royal Sponsor Allocation

### Phase 4 - Binary modules

Build:

- Binary Tree
- Binary Volume
- Binary Pairing Bonus
- Binary Carryover
- Binary Caps

### Phase 5 - Persistence

Build Postgres repositories and event log.

Everything important should save transactionally:

- purchase
- subscription
- enrollment
- module state changes
- ledger entries
- events

### Phase 6 - Leptos Spin SSR

Build:

- auth routes
- company admin
- catalog builder
- package builder
- pay plan stack builder
- purchase flow
- user dashboard
- admin event and ledger views

### Phase 7 - Operations

Build:

- recurring billing renewal workflow
- Royal weekly pot distribution job
- Binary cycle close job
- event retry workflow
- admin manual run buttons

## 13. Key Product Rules

- A package can contain products, services, or both.
- A package can be one-time, recurring, or mixed.
- Pay plan modules should run from purchases, renewals, and enrollments.
- Rewards are recorded in the ledger, not paid directly by modules.
- Royal Flush and Binary are built-in module families.
- Companies choose which stack their package uses.
- One package maps to one active pay plan stack version at a time.
- Stack versions are immutable once purchases exist.
- Changes to a package's compensation plan should create a new stack version.

## 14. Important Open Decisions

- Will recurring renewals generate new commissionable volume every billing period?
- Can a package include both one-time and recurring billing plans at the same time?
- Are products fulfilled internally or through an external provider?
- Should Binary placement be manual, auto-balanced, or sponsor-selected by default?
- Should package upgrades create new enrollments or update existing enrollments?
- Should failed recurring payments suspend pay plan eligibility immediately or after grace period?
