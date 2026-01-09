# AI Development Quick Start

This is a quick reference for AI agents working on this codebase.

## First Steps

1. **READ `.ai/AI_CONTRACT.md`** - This is mandatory
2. Read `CLAUDE.md` for development guidance
3. Understand the monorepo structure

## Architecture at a Glance

```
ddns-core (authoritative DDNS logic)
    ↓ traits: IpSource, DnsProvider, StateStore
    ↓ engine: DdnsEngine (orchestration)
    ↓ registry: ProviderRegistry (plugin system)

ddnsd (thin integration layer)
    ↓ reads env vars
    ↓ assembles ddns-core components
    ↓ starts engine

Providers (separate crates)
    ↓ ddns-provider-cloudflare
    ↓ ddns-ip-netlink
    ↓ implements traits, registers with registry
```

## Red Flags 🚩

Stop and review AI_CONTRACT.md if you're about to:

- Move DDNS logic into `ddnsd`
- Use `match provider_type { ... }` for provider selection
- Add polling as the primary IP monitoring mechanism
- Add config files (TOML/YAML/JSON)
- Merge responsibilities (e.g., IpSource calling DnsProvider)
- Add Web UI, control plane, or DNS server features
- Add heavy dependencies without justification

## Green Flags ✅

You're on the right track if you're:

- Adding new providers as separate crates
- Implementing core traits in provider crates
- Using ProviderRegistry for dynamic provider selection
- Using async streams for event-driven monitoring
- Keeping `ddnsd` as a thin integration layer
- Reading config from environment variables only

## Checklist Before Committing

- [ ] Does this change violate any AI_CONTRACT.md constraint?
- [ ] Is business logic in `ddns-core`, not `ddnsd`?
- [ ] Are providers using the registry pattern?
- [ ] Is IP monitoring event-driven (not polling-first)?
- [ ] Did I update documentation if architecture changed?
- [ ] Does `cargo check` pass?

## Need Help?

- Architecture questions: Re-read AI_CONTRACT.md
- Implementation questions: Check CLAUDE.md
- Code structure: Explore existing implementations
