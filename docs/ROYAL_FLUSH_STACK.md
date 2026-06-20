# Royal Flush Stack

Royal Flush is a built-in pay plan stack.

## Modules

1. Sponsor Allocation
2. Royal Flushline
3. Royal Matrix
4. Royal Pot Bonus
5. Royal Account Duplication

## Flushline

Tiers:

```text
Ten -> Jack -> Queen -> King -> Ace
```

Thresholds:

- Ten: 1
- Jack: 2
- Queen: 3
- King: 4
- Ace: 5

Graduation occurs after 15 total points.

## Matrix

7-slot matrix:

```text
        Owner
      /       \
    Slot2     Slot3
   /   \     /   \
 S4   S5   S6   S7
```

When full, it cycles.

## Pot Bonus

Weekly pool split:

- 75 percent equal qualified-user share
- 25 percent top cycler bonus

Qualification requires both:

- at least one Flushline graduation
- at least one Matrix cycle

## Duplication

Account duplicates only after both:

- Flushline graduated
- Matrix cycled

## Reset

After pot distribution, qualified graduated accounts reset to King, not Ten.
