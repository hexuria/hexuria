# Pay Plan Module Contract

A module is one compensation mechanic.

Examples:

- Royal Flushline
- Royal Matrix
- Royal Pot Bonus
- Binary Tree
- Binary Volume
- Binary Pairing Bonus

## Module responsibilities

A module may:

- react to events
- load its own state
- validate configuration
- produce state changes
- emit new events
- create reward ledger entries

A module must not:

- call payment gateways directly
- send emails directly
- write SQL directly from core logic
- mutate another module's internal state directly
- perform cashout directly

## Module input

A module receives:

- company id
- package id
- enrollment id
- triggering event
- module config
- current module state

## Module output

A module returns:

- state changes
- emitted events
- reward ledger entries
- warnings or validation errors

## Built-in module families

Royal family:

- royal_flushline
- royal_matrix
- royal_pot_bonus
- royal_account_duplication
- sponsor_allocation

Binary family:

- binary_tree
- binary_volume
- binary_pairing_bonus
- binary_carryover
- binary_caps

## Module ordering

Stacks run modules in configured order. Order matters.

Example Royal Flush order:

1. Sponsor Allocation
2. Royal Flushline
3. Royal Matrix
4. Royal Pot Bonus
5. Royal Account Duplication

Example Binary order:

1. Sponsor Placement
2. Binary Tree
3. Binary Volume
4. Binary Pairing Bonus
5. Binary Carryover
6. Binary Caps
