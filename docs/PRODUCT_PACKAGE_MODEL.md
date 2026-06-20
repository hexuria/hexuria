# Product and Package Model

## Catalog Item

A catalog item is what the company sells.

Types:

- Product
- Service

Products can be physical or digital. Services can be access, coaching, training, software, membership, or support.

## Billing Plan

A billing plan defines how a catalog item is charged.

Supported types:

- One-time
- Recurring

Recurring fields:

- recurrence interval
- recurrence count
- trial days
- grace period days
- cancellation behavior

## Package

A package bundles catalog items and chooses a pay plan stack.

A package can include:

- products only
- services only
- products and services
- one-time billing only
- recurring billing only
- mixed one-time and recurring items

## Package Item

Package items define what is inside a package and how each item contributes to compensation.

Fields to track:

- quantity
- billing plan
- commissionable flag
- commissionable volume
- points value
- item role

Item roles:

- Included
- Required
- OptionalAddon
- Upsell

## Purchase

A purchase creates the commercial record.

The purchase should not directly mutate pay plan state. It should emit events.

## Subscription

A subscription exists when at least one package item is recurring.

Subscription renewals may emit new commissionable volume depending on company/package configuration.

## Entitlement

An entitlement records access granted by purchase or subscription.

Examples:

- course access
- membership access
- digital product access
- service access
- software access

## Enrollment

Enrollment is the pay plan entry point.

A user buys a package. The purchase creates an enrollment. The enrollment enters the selected pay plan stack.
