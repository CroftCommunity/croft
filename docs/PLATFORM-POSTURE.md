# Platform posture — what Croft promises, per platform

**Read this before writing a sentence of user-facing copy about background
behaviour, offline, or "works peer-to-peer."** The constraints below are platform
policy, not implementation gaps, and no amount of engineering removes them.

Sources: `discovery/alpha/thinking/ios-opportunistic-p2p.md`,
`thinking/app/platforms/*.md`, `thinking/app/estate-architecture-and-browser-constraints.md`,
`research/2026-07-27-social-tree-factcheck.md` §7, ROADMAP_TODO E71.

## The promise (owner decision, 2026-08-11)

> **Best effort, stated plainly — and not forever.**

Two halves, both binding:

- **Stated plainly.** We do not imply deterministic background delivery on a
  platform that cannot provide it. The limitation is named in the product, not
  buried.
- **Not forever.** This is a *dated posture*, not a permanent excuse. It is
  revisited when any of the triggers below fire.

**Revisit triggers** — any one reopens this document:
1. iOS gains a usable long-lived background socket or an equivalent primitive.
2. The meer's delivery reliability is measured well enough to promise something
   stronger than best-effort.
3. We decide to depend on push (APNs/FCM) as a *guaranteed* wake path rather than
   an opportunistic one — which is a governance decision, not a technical one
   (see "The push dependency" below).

Undated "best effort" is how a limitation becomes permanent by neglect. If this
file has not been reviewed in a year, that itself is the finding.

## iOS — the sharpest case

**You cannot hold a background socket.** Device-to-device P2P is therefore
**opportunistic, never deterministic**, and spontaneous off-grid meshing is
aspirational and unproven.

The structural consequence, and it is not a small one: **the meer is the
dependable backbone, not a bonus.** Any design that treats direct P2P as the
primary path and the meer as a fallback has the dependency backwards on iOS.

The working pattern (as shipped by Delta Chat and Berty):

```
OS event (push / BackgroundTasks) → native shell wakes
  → Rust core over UniFFI → ephemeral endpoint
  → one stateless write/read across the swarm
  → signal completion before the window closes
```

Other iOS facts that bite:

- **Every iOS browser is WebKit.** Browser-engine constraints are iOS constraints,
  not Safari quirks.
- **Cross-subdomain shared storage fails on WebKit** — partitioned per subdomain,
  confirmed on real shipping Safari. Topology is single-origin core + isolated
  subdomains.
- **WebKit evicts script-writable storage after 7 days**, exempted by
  `storage.persist()` **and** Home-Screen install. A web surface must request
  persistence and prefer install.
- **Not available on iOS Safari:** `share_target`, `protocol_handlers`,
  `file_handlers`, Background Sync, Periodic Background Sync,
  `navigator.connection`, File System Access pickers.

## Android — first target, and more forgiving

Foreground services + WorkManager give a workable background story — materially
better than iOS, still not guaranteed. Doze, app-standby and OEM battery killers
throttle background sync, with real per-OEM variance.

Keystore for secrets; FCM as the wake path.

## Web

OPFS is available broadly, **but OPFS is speed, not ownership** — same
eviction/quota regime as IndexedDB, not user-visible, gone on clear-site-data.
Never sell it as durability.

Folder-mirrored continuous export (`showDirectoryPicker`) is **desktop-Chromium
only** — not Firefox, not Safari, not Chrome on Android. So it is a desktop
differentiator, and **mobile needs a separate durability story**: second device,
peers who can re-admit you, explicit identity export.

## The push dependency (open, and it is a values question)

APNs and FCM are the only reliable wake primitives on iOS and Android. Depending
on them puts Apple and Google in the delivery path of a project whose stance is
*no central operator*.

This is **unresolved**, on both platforms, and it is the owner's call — not an
engineering trade-off to be optimised away. Options range from push-as-hint (the
payload carries nothing; it only wakes a device that then syncs from the meer) to
push-as-transport (rejected on its face). The hint shape preserves the property
that the wake channel learns nothing, and is the direction to explore first.

## App-store survivability

The gate is **governance posture, not decentralization**. A decentralized
transport does not exempt an app from review and can make it harder to clear: the
reviewer asks whether you can demonstrate control over abuse, discovery and
takedown. The recorded posture is Signal's *deny-the-surface* path — discovery off
by default, scale caps on broadcast, blind admission choke, block and report
handled blind — over Telegram's readable-moderation path.

Full case and reasoning: `discovery/beta/fenced/app-store-survivability-and-abuse-posture.md`.
