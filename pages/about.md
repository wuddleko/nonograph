---
title: About Nostorgraph
date: 2026-02-11
author: nostorgraph
generator: nostorgraph v0.2.8
---

**A Minimalist Publishing Platform**

Nostorgraph is a clean, simple publishing tool built in Rust with a focus on simplicity, speed, and elegance.

**Philosophy**

- Simplicity over complexity
- Content over flashy features
- Function over form
- Quality over quantity

**Features**

- **No accounts** - Just write and publish
- **Instant publishing** - Posts go live immediately
- **Clean typography** - Beautiful serif fonts for reading
- **Minimal markup** - Just the essentials for formatting
- **Sealed notes** - The host does not keep a copy of what you publish
- **Anonymous by design** - Privacy-first approach

**Links**

Store offers three addresses. A reader only has the one they were sent. The key is the part after `#`. This site never sees it.

- **Long.** Most private. This site stores nothing. The event and the relays are in the address. The key is the part after #. `https://<site>/<locator>/<event-id>?r=<relay>#key`
- **Short.** This site stores the event id and the relays. It does not store the writing or the key. `https://<site>/<locator>#key`
- **Title.** Least private. This site stores the title, the event id, and the relays. The title is in the address. It does not store the writing or the key. `https://<site>/<title>#key`

**Technical Details**

Built with modern Rust using:
- Rocket web framework for reliability
- Custom template engine for speed
- Ammonia for security and XSS prevention
- Unicode-aware markdown parser
- Sealed notes published to relays; this host does not store them

**Design Principles**

Every design decision prioritizes:
1. **Clarity** - Clean, readable interface
2. **Performance** - Fast loading and publishing
3. **Security** - Safe handling of user content
4. **Longevity** - Built to last with minimal dependencies

**Open Source**

Nostorgraph is free and open source software. The complete source code is available on GitHub for transparency, contributions, and self-hosting.

**Inspiration**

Built with modern web standards and a focus on sustainable, anonymous, user-respecting design.

*Simple tools for thoughtful publishing.*
