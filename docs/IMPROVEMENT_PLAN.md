# abs-opds — Repository Analysis & Improvement Plan

Audit date: 2026-08-01 · Commit analysed: `22ad5a6` · Toolchain: rustc 1.94.1

This document is written to be executed by an agentic coder. Milestones are ordered by
dependency; tasks inside a milestone are independent unless stated. Every task carries a
file reference, a rationale, and an acceptance criterion that can be checked mechanically.

---

## 1. Verification baseline

Everything below was measured on this commit, not inferred.

| Check | Command | Result |
|---|---|---|
| Library tests | `cargo test --lib` | **21 passed**, 0 failed |
| Full target build | `cargo test --all-targets` | **FAILS** — benches do not compile |
| Lints | `cargo clippy --lib` | **35 warnings** |
| Formatting | `cargo fmt --check` | **4082 diff lines** |
| CI coverage | `grep -r "cargo test" .github/` | **no workflow runs tests, clippy, fmt, or audit** |

The test suite is genuinely healthy — the problems are concentrated in things the tests
do not observe: response headers, URL construction, configuration edges, and CI.

Four behavioural hypotheses were confirmed with throwaway integration tests (since removed):

- **H1 — ETags never match.** Two identical feed builds 5 ms apart produced different bytes
  (`<updated>2026-08-01T20:46:45.711557833+00:00</updated>` vs `...716694167+00:00`).
- **H2 — category links are not URL-encoded.** A name containing `&` and `#` produced
  `href="/opds/libraries/lib1?name=Simon &amp; Schuster / Hörbuch #2&amp;type=authors"`.
- **H5 — `OPDS_PAGE_SIZE=0` passes validation, then panics.** `validate()` returned `Ok`,
  and the `total_pages` expression aborted with `panic_const_div_by_zero`.
- **H7 — the proxy traversal filter is bypassable.** `contains("..")` does not match
  `/%2e%2e/admin` or `/..%2fadmin`.

---

## 2. Findings by severity

### Critical

**C1 — The entire ETag / `304 Not Modified` layer is dead code.**
`src/xml.rs:54` writes `<updated>` from `chrono::Utc::now()` with nanosecond precision,
evaluated *inside* the builder rather than from the `updated_time` argument the callers
already thread through. Every response body is therefore unique, so every ETag is unique,
so `If-None-Match` can never match. Nine ETag/`304` blocks in `src/handlers.rs`
(lines 54, 93, 123, 164, 217, 268, 327, 399, 441) never fire. E-readers on slow links
re-download every feed in full.
Note the same bug is *absent* from the OPDS 2.0 navigation feeds — `Opds2Builder::build_root`
and `build_categories_root` ignore `_updated_time` entirely, so those ETags are already
stable. `build_publications` re-introduces it via `modified`.

**C2 — `OPDS_PAGE_SIZE=0` bricks the server.**
`AppConfig::validate` (`src/models.rs:210`) does not check `opds_page_size > 0`.
`(total_items + page_size - 1) / page_size` then panics in `src/handlers.rs:188`,
`src/handlers.rs:293`, and `src/service.rs:204`. There is no `CatchPanicLayer`, so the
panic kills the connection task on *every* library request. A one-character config typo
takes the service down with no useful error.

**C3 — A live Audiobookshelf JWT is committed.**
`docker-compose.yml:13` contains a real token whose payload decodes to
`{"userId":"a745f02f-…","username":"demo","iat":1732472292}`, pointed at the public
`https://audiobooks.dev`. It has been in history since commit `6df911f` ("PoC 1") in a
public AGPL-3.0 repository. Rotation is required — removing the line does not undo
disclosure.

### High

**H1 — Category browsing breaks on names containing `&`, `#`, `/`, or spaces.**
`src/xml.rs:218` and `src/opds2.rs:332` interpolate raw names into query strings; the
handlers do the same for pagination links (`src/handlers.rs:194-198`, `299-303`).
XML-escaping is applied, but `&amp;` decodes back to a literal `&`, so a reader parses
`?name=Simon ` plus a junk parameter, and `#2` starts a fragment. The link silently
resolves to the wrong result set. Percent-encoding is missing, not merely inconsistent.

**H2 — Every request deep-clones the entire library.**
`src/api.rs:166` returns `cached.response.clone()` — a full `Vec<AbsItemResult>` with all
its `String`s. `get_categories_data` then consumes it by value. For a 100k-item library
this is a multi-megabyte allocation *per request*, and it is invisible in
`performance_report.md` because the benchmarks measure filtering downstream of the clone.
Returning `Arc<AbsItemsResponse>` removes the copy outright.

**H3 — The whole library is fetched to serve 20 rows.**
`AbsClient::get_items` (`src/api.rs:160`) has no pagination, sort, or filter parameters,
so `GET /api/libraries/{id}/items` pulls everything and the process filters and paginates
in memory. Combined with an unbounded per-`(token, library)` items cache, memory scales
with users × library size. This is the ceiling on how large a library the server can serve.

**H4 — Arbitrary `?token=` values authenticate.**
`src/auth.rs:105-121`: with no `Authorization` header, any `?token=<anything>` yields
`Ok(AuthUser)` with that string as the API key, unvalidated. Downstream ABS calls will
reject a bad token, so library data is not exposed — but `proxy_handler` forwards to the
ABS host *without* attaching credentials, which turns the OPDS server into an
unauthenticated relay to whatever that host serves without auth. The token is also never
percent-decoded, so a token containing `+` or `/` cannot authenticate at all.

**H5 — Free-text search silently ignores narrators and series.**
`matches_search_abs` (`src/service.rs:376`) covers title, subtitle, description, publisher,
isbn, language, published_year, author, genres, tags — but not `narrator_name` or
`series_name`, both of which are first-class browse categories. Searching a narrator
returns nothing. `LibraryItem::matches_search` (`src/models.rs:51`) has the same gap and is
additionally dead code, superseded by the `AbsMetadata` variant.

**H6 — No CI gate.** `build.yaml` triggers only on the `release` branch and tags and only
builds a Docker image. Nothing runs tests, clippy, fmt, or a dependency audit on a pull
request. The broken benchmark (below) is exactly the kind of regression this would catch.

### Medium

- **M1 — `cargo test --all-targets` fails.** `benches/opds_benchmark.rs:387` and `:407` call
  `build_opds_skeleton` with 7 arguments; it has taken 8 since the acquisition/navigation
  split in commit `0633080`. The benchmark suite has been uncompilable since then.
- **M2 — Errors return HTTP 200.** Every failure path renders an Atom error feed with a
  success status (`src/handlers.rs:144`, `237`, `246`, `347`, `354`, `418`, `428`, `460`).
  Clients cannot distinguish an outage from an empty shelf, and caches store the error.
- **M3 — Non-standard elements in the Atom feed.** `<authentication>` (`src/xml.rs:46-52`)
  is not an Atom or OPDS 1.2 construct — OPDS 1.2 uses a separate
  `application/opds-authentication+json` document. `<subtitle>` inside `<entry>`
  (`src/xml.rs:270`) is feed-level-only in Atom. Strict validators reject both.
- **M4 — Broken and duplicate links.** `rel="alternate"` points at `/library/{id}`
  (`src/xml.rs:61`) — a path on the *OPDS* server, which does not exist; it should target
  the ABS web UI. Two `rel="search"` links (`src/xml.rs:62-63`) confuse some readers, and
  the generic `application/octet-stream` acquisition link is emitted *before* the typed one,
  so readers that take the first match download an untyped blob.
- **M5 — Lock poisoning panics.** `src/api.rs:81`, `101`, `163`, `185` `.unwrap()` on
  `RwLock` guards. One panic while a lock is held permanently bricks the cache.
- **M6 — Proxy hardening.** `contains("..")` (`src/handlers.rs:497`) misses percent-encoded
  traversal, and the proxy will relay *any* path on the ABS host. An allowlist of the
  paths OPDS actually needs (`/api/items/{id}/{cover,ebook,download}`) is the right shape.
- **M7 — Timing-unsafe password comparison.** `src/auth.rs:82` compares internal-user
  passwords with `==`. There is also no rate limiting: each failed attempt falls through to
  a live ABS login, making the OPDS server a brute-force amplifier.
- **M8 — Container hygiene.** No `.dockerignore`, so `COPY . .` ships `target/` into the
  build context. The image runs as root. `alpine:3.19` is past end-of-life.
  `COPY --from=builder /app/languages /languages` is dead weight — the JSON is embedded at
  compile time via `include_str!` (`src/i18n.rs:15-23`). `FROM … as builder` should be `AS`.
- **M9 — No health endpoint.** Nothing for `docker-compose` healthchecks or orchestrators.

### Low

- **L1 — `performance_tests` is compiled twice**, declared in both `src/lib.rs:19-21` and
  `src/service.rs:15-16`; `cargo test --lib` runs `test_performance_100000_items` twice.
- **L2 — Dead code.** `write_elem_ns` (`src/xml.rs:122`) is byte-identical to `write_elem`.
  `url_buf` in `src/opds2.rs:320-329` is built and discarded. The `PAGE_REGEX` page-stripping
  in `src/xml.rs:71` and `src/opds2.rs:256`/`395` can never fire, because no caller ever puts
  `page=` into `url_base`. `LibraryItem::matches_search` is unreferenced.
- **L3 — 35 clippy warnings**, dominated by 23 × `map_or` → `is_some_and` and
  3 × manual `div_ceil` (the same expression as C2 — `div_ceil` would have made the panic
  impossible to write).
- **L4 — `cargo fmt` never run**; 4082 lines of drift.
- **L5 — Stringly-typed categories.** `ItemType` exists in `src/models.rs:74` but
  `get_category` re-validates against a hardcoded `["authors", …]` array
  (`src/handlers.rs:370`) and passes `&str` down. Adding a category means editing three places.
- **L6 — Cargo.toml metadata.** No `license`, `description`, `repository`, or `rust-version`;
  no release profile (`lto`, `codegen-units`, `strip`) despite "single binary" being a
  headline feature.
- **L7 — Docs.** README documents neither OPDS 2.0 (implemented in `src/opds2.rs`),
  conditional requests, the three bundled languages, nor the AGPL-3.0 license.
  `performance_report.md` belongs under `docs/`.
- **L8 — `I18n::localize` allocates a `String` per lookup** on every category render, and
  language files are hardcoded in `src/i18n.rs` rather than discovered.

---

## 3. Milestone plan

Each task states its acceptance criterion. A milestone is done when every criterion in it
holds and `cargo test --all-targets` is green.

### M0 — Make the repo verifiable (no behaviour change)

Everything else depends on a working gate. Do this first and in this order.

| # | Task | Acceptance |
|---|---|---|
| 0.1 | Fix `benches/opds_benchmark.rs:387,407` — pass the missing `is_acquisition` bool (`true`, these build item entries) | `cargo bench --no-run` succeeds |
| 0.2 | Remove the duplicate `performance_tests` declaration in `src/service.rs:15-16`, keep the one in `src/lib.rs` | `cargo test --lib` lists `test_performance_100000_items` exactly once |
| 0.3 | Add `.github/workflows/ci.yaml` on `pull_request` + `push`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` | Workflow green on a PR |
| 0.4 | Add `cargo-audit` / `cargo-deny` as a separate non-blocking job | Advisory report appears in the run |
| 0.5 | Run `cargo clippy --fix` then `cargo fmt`, as a **commit of its own** with no other edits | `cargo clippy -- -D warnings` and `cargo fmt --check` both clean |
| 0.6 | Add `rustfmt.toml` and `clippy.toml` pinning the chosen style | Formatting is reproducible across machines |

> Ordering matters: land 0.5 alone. A formatting commit mixed with logic changes makes every
> later diff unreviewable.

### M1 — Stop the bleeding (correctness & security)

| # | Task | Acceptance |
|---|---|---|
| 1.1 | **C3**: rotate the ABS demo token, replace `docker-compose.yml:13` with a placeholder, add `.env.example`, document that history still contains it | No credential-shaped string in the working tree; rotation confirmed |
| 1.2 | **C2**: reject `opds_page_size == 0` in `AppConfig::validate`; switch the three sites to `usize::div_ceil` | Unit test: `validate()` errors on `0`; no bare `/ page_size` remains |
| 1.3 | **C1**: use the `updated_time` argument in `src/xml.rs:54` instead of `Utc::now()`; derive it from library/item mtime, or truncate to whole seconds if no better source exists | Integration test: two consecutive `GET /opds` return the same ETag, and the second with `If-None-Match` returns `304` |
| 1.4 | **H1**: percent-encode every interpolated query value (`src/xml.rs:218`, `src/opds2.rs:332`, `src/handlers.rs:194-198,299-303`). Add `form_urlencoded` or `percent-encoding` | Test: a name of `Simon & Schuster #2` round-trips through the link back to the same filter |
| 1.5 | **C2 follow-up**: add `tower_http::catch_panic::CatchPanicLayer` in `build_router` | A panicking handler returns 500 and the server survives |
| 1.6 | **H4**: require a `?token=` to match a configured internal user, or validate it against ABS before accepting; percent-decode it first | Test: an unknown token is rejected with 401; a token containing `+` authenticates |
| 1.7 | **M7**: constant-time password compare (`subtle`); add a failed-attempt backoff | No `==` on secrets; test covers repeated failures |
| 1.8 | **M6**: allowlist proxy paths and decode before the traversal check | Test: `/opds/proxy/%2e%2e/admin` and any non-allowlisted path return 400/403 |

### M2 — Protocol conformance

Best done against a real reader. `Thorium` and `Moon+ Reader` are the two the README claims
support for; add `KOReader`, since the README states that is the actual target.

| # | Task | Acceptance |
|---|---|---|
| 2.1 | **M2**: map errors to real status codes — 401 auth, 404 unknown library, 502 ABS unreachable — keeping the Atom error feed as the body | Test asserts status per failure mode |
| 2.2 | **M3**: remove `<authentication>` from the feed; if auth discovery is wanted, serve a proper `application/opds-authentication+json` document | Feed validates against an Atom/OPDS validator |
| 2.3 | **M3**: drop entry-level `<subtitle>`; fold it into `<title>` or `<summary>` | Validator clean |
| 2.4 | **M4**: fix `rel="alternate"` to point at the ABS web UI; keep one `rel="search"`; emit the typed acquisition link before the generic one | Manual check in one reader per task |
| 2.5 | Add a golden-file test: build one navigation and one acquisition feed, compare against a checked-in fixture | Fixture diffs surface in review |
| 2.6 | **L5**: parse the path segment into `ItemType` via the existing enum; delete the hardcoded array at `src/handlers.rs:370` | Adding a category requires one edit |

### M3 — Performance where it actually is

Re-run `benches/opds_benchmark.rs` before and after each task and record deltas. The current
`performance_report.md` numbers measure filtering only and will not move.

| # | Task | Acceptance |
|---|---|---|
| 3.1 | **H2**: return `Arc<AbsItemsResponse>` from `get_items`; make `get_categories_data` borrow instead of consume | Benchmark shows reduced allocation; no `.clone()` of the response remains |
| 3.2 | **H3**: pass `limit`/`page`/`sort` through to the ABS API; fall back to in-memory paging only when filtering demands the full set | A 20-item page issues a bounded upstream request |
| 3.3 | Bound the items cache — max entries or total size, LRU eviction — on top of the existing TTL sweep (`src/api.rs:46-58`) | Cache size stays bounded under a multi-user load test |
| 3.4 | **M5**: replace `RwLock().unwrap()` with poison-tolerant access or `parking_lot` | No `.unwrap()` on a lock guard |
| 3.5 | Precompute the category index (distinct authors/genres/series) once per cache fill instead of per request | Category feeds no longer scan all items |
| 3.6 | **L8**: return `&str`/`Cow` from `I18n::localize` | No per-lookup allocation |

### M4 — Operability

| # | Task | Acceptance |
|---|---|---|
| 4.1 | **M9**: add `GET /health` (liveness) and `GET /ready` (ABS reachable), both unauthenticated | Endpoints return 200/503 correctly |
| 4.2 | **M8**: add `.dockerignore`; run as a non-root `USER`; move to a supported Alpine; drop the dead `COPY languages`; `AS` not `as` | Image builds smaller and `docker run` shows a non-root uid |
| 4.3 | Add a `HEALTHCHECK` and wire `docker-compose.yml` to 4.1 | `docker compose ps` reports healthy |
| 4.4 | Replace the dummy-`main.rs` caching trick with `cargo-chef` | Dependency layer caches correctly |
| 4.5 | Add `RequestBodyLimitLayer` and a server-side timeout layer | Slow-loris and oversized requests are rejected |
| 4.6 | Redact `?token=` from `TraceLayer` spans | No token appears in logs at `debug` |
| 4.7 | **L6**: fill in Cargo.toml metadata; add `[profile.release]` with `lto = true`, `codegen-units = 1`, `strip = true` | Binary size recorded before/after |

### M5 — Features & polish

| # | Task | Acceptance |
|---|---|---|
| 5.1 | **H5**: add `narrator_name` and `series_name` to `matches_search_abs`; delete the dead `LibraryItem::matches_search` | Test: searching a narrator returns its books |
| 5.2 | **L2**: delete `write_elem_ns`, the dead `url_buf`, and the unreachable `PAGE_REGEX` blocks — or add pagination to `url_base` so the regex has a purpose. Decide, do not leave both | No unreachable branches remain |
| 5.3 | **L7**: document OPDS 2.0, conditional requests, the language files, and AGPL-3.0 in the README; move `performance_report.md` into `docs/` | README matches implemented behaviour |
| 5.4 | Extract i18n discovery from the hardcoded three in `src/i18n.rs:15-23` | Adding a language means adding a file |
| 5.5 | Add `CONTRIBUTING.md` and a `CHANGELOG.md`; adopt the release tags already wired in `release.yaml` | First tagged release from the new flow |
| 5.6 | Extend `release.yaml` beyond `linux-x86_64` — aarch64 at minimum, given the Kindle/KOReader target | Release carries multi-arch assets |

---

## 4. Suggested CI gate (target state)

```
on: [pull_request, push]
  fmt      → cargo fmt --check
  clippy   → cargo clippy --all-targets -- -D warnings
  test     → cargo test --all-targets
  audit    → cargo audit                      (non-blocking)
  docker   → docker build (no push)           (on PR)
```

`build.yaml` keeps its current `release`-branch and tag triggers for publishing.

---

## 5. Decisions needed before M2 and M3

These change the shape of the work and are the maintainer's call, not the implementer's:

1. **Where does `<updated>` come from?** Truncating `Utc::now()` to whole seconds makes ETags
   work within a one-second window — cheap, and enough for e-readers. Deriving it from ABS
   item mtimes is correct but needs a field that `AbsMetadata` does not currently deserialize.
   The plan assumes truncation; say so if you want the real thing.
2. **Is the `?token=` query parameter a supported entry point?** M1.6 tightens it, which will
   break any reader currently relying on an unvalidated token. If KOReader on the old Kindle
   depends on it, that task needs a compatibility path instead.
3. **How large a library must this serve?** M3.2 (upstream pagination) is a substantial change
   and only pays off above roughly 10k items. Below that, M3.1 and M3.3 alone are likely enough.

---

## 6. Suggested sequencing

M0 → M1 are prerequisites and should land as separate, reviewable PRs. M2, M3, and M4 are
independent of one another and can proceed in parallel once M1 is merged. M5 is
opportunistic — fold individual tasks into whichever PR touches the same file.

The highest-value single commit in the whole plan is **M1.3**: one line in `src/xml.rs`
activates nine already-written caching paths.
