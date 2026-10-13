import json, random

items = []

def add(i, label, repo, a, b, why):
    items.append({"id": i, "label": label, "repo": repo, "task_a": a, "task_b": b, "why": why})

# ---------------- CONTRADICT ----------------
add("c01", "contradict",
    "A Python Flask monolith that serves a B2B invoicing API and logs via the standard logging module.",
    "Switch our application logging to structured JSON output. One JSON object per line, with fields like timestamp, level, request_id and message, so Datadog can parse them without grok rules.",
    "Heads up from ops: we grep the app logs on the boxes all the time during incidents (things like `grep 'ERROR invoice' app.log | tail`). Please don't change the log line layout. Keep the existing human-readable `[LEVEL] timestamp module: message` format, we have saved runbooks and alert scripts that depend on it.",
    "JSON-per-line logs replace the human-readable line format that the second task requires to stay unchanged.")

add("c02", "contradict",
    "A Node/Express REST API for a logistics platform with /v1 and /v2 route trees.",
    "Remove the /v1 API entirely. v2 has been live for a year and nobody should be on v1 anymore. Delete the v1 routers, controllers and tests, and drop the route mounting in app.js.",
    "Add per-API-key rate limiting (100 req/min) middleware to every v1 endpoint. We've got a couple of abusive partners hammering /v1/shipments and we need throttling there this week.",
    "Task A deletes all v1 endpoints while task B adds rate limiting to every v1 endpoint.")

add("c03", "contradict",
    "A TypeScript web app that uses the `zodish` schema validation library for request parsing.",
    "Pin zodish to the 2.x line in package.json (use ~2.8.0) and add a renovate rule so it never bumps the major. 3.x changed the error shape and breaks our API error formatter.",
    "Upgrade zodish to 3.x. We want the new `.transform()` pipeline API and the better tree-shaking for the frontend bundle. Update the schemas that need it.",
    "One task pins the dependency to 2.x and forbids major bumps, the other upgrades it to 3.x.")

add("c04", "contradict",
    "A Django REST service for a product catalog with a shared pagination settings module.",
    "Set the default page size for all list endpoints to 50 in settings/pagination.py.",
    "Set the default page size for all list endpoints to 20 in settings/pagination.py. 50 is too heavy for mobile clients.",
    "Both set the same global default page size to different values.")

add("c05", "contradict",
    "A Rails app for a membership platform with PostgreSQL migrations managed via ActiveRecord.",
    "Cleanup ticket: the `legacy_id` column on `users` is dead since the 2022 import. Write a migration to drop it and remove all references to it in models and serializers.",
    "Partners are still calling us with their old IDs. Add a unique index on users.legacy_id and build the new `GET /partners/lookup?legacy_id=` endpoint on top of that column.",
    "One task drops users.legacy_id, the other builds an index and an endpoint that depend on that column.")

add("c06", "contradict",
    "A Go microservice shipped as a Docker image, with a Dockerfile at the repo root and a CI pipeline that builds and pushes it.",
    "Our image is 900MB, which is silly. Move the runtime stage of the Dockerfile to alpine so we get the image under 100MB. Don't add anything fancy, just get the base image swapped.",
    "The service links against a vendor-supplied C library (libvendorsdk.so) that was compiled against glibc and crashes on musl. Make sure the runtime image stays on a glibc-based distro like debian-slim and add a CI smoke test that loads the library.",
    "Alpine uses musl while the second task requires a glibc-based runtime base for the vendor library.")

add("c07", "contradict",
    "A React SaaS dashboard with copy strings scattered across components and an i18n JSON file.",
    "Product wants us to rename 'Workspace' to 'Project' everywhere users can see it: nav, page titles, buttons, empty states, email templates. Make it consistent.",
    "Docs and legal are signed off on the term 'Workspace', and there is an in-app tour being finalised. Please make sure every user-facing label, empty state and toast uses the word 'Workspace' and fix any places that drifted to some other word.",
    "One task replaces the term Workspace with Project in user-facing text while the other mandates Workspace everywhere.")

add("c08", "contradict",
    "A fintech backend with an auth service (JWT + refresh tokens) used by a web app and a React Native mobile app.",
    "Compliance finding SEC-212: sessions must expire after 15 minutes of inactivity for all clients, mobile included, with no silent refresh past that window. Lower the refresh token idle TTL accordingly and force re-login.",
    "Users keep complaining that the mobile app logs them out. Change the auth config so that mobile sessions stay valid for 30 days of inactivity and refresh silently. No re-login prompts unless the user changes their password.",
    "Idle expiry of 15 minutes for all clients including mobile conflicts with 30-day silent sessions on mobile.")

add("c09", "contradict",
    "A Next.js storefront with an API layer that serves product and pricing data, deployed behind a CDN.",
    "Page speed is bad. Add Cache-Control: public, max-age=3600 on the /api/products and /api/products/:id responses so the CDN can serve them for an hour.",
    "Finance flagged that price changes made in the admin must show up on the storefront immediately. Make sure /api/products responses are never cached by the browser or the CDN (no-store), otherwise customers see stale prices and we get chargebacks.",
    "Caching product responses for an hour contradicts requiring that the same responses never be cached.")

add("c10", "contradict",
    "A Rust CLI tool `fleetctl` for managing cloud VMs, used both interactively and in CI pipelines.",
    "Make fleetctl fully non-interactive so it works in CI. Strip every prompt (y/N confirmations included) from all subcommands; the tool should just execute what it is told and never wait on stdin.",
    "Two people have wiped prod VMs by accident. Add a confirmation prompt to `fleetctl purge` where the user has to type the environment name. This must always happen, no --yes flag or env var to skip it.",
    "Removing every prompt from all subcommands conflicts with an unskippable confirmation prompt on purge.")

add("c11", "contradict",
    "An Airflow-based data pipeline repo that produces a nightly customer-activity export consumed by other teams.",
    "Change the nightly export task to write Parquet instead of CSV, partitioned by date. CSV is wasteful at our volume and we want to retire it.",
    "The finance team's Excel macros pick up `s3://exports/customer_activity/latest.csv` every morning. Do not touch that: the export has to keep producing that CSV file, same name, same columns, same location.",
    "Replacing CSV with Parquet output retires the CSV file that the second task says must keep being produced.")

add("c12", "contradict",
    "A Vue 3 e-commerce web app with an i18n plugin and a locale detection module.",
    "Locale detection: default the UI language to whatever the browser sends in Accept-Language / navigator.language, falling back to en only if we don't support it.",
    "Legal says that until the EU rollout is approved, every visitor must see the en-US storefront regardless of browser language. Hard-default the locale to en-US and don't auto-switch based on the browser.",
    "Defaulting to the browser language conflicts with forcing en-US regardless of browser settings.")

add("c13", "contradict",
    "A Python service configured through a config/settings.yaml file loaded at startup by config/loader.py.",
    "Twelve-factor cleanup: remove config/settings.yaml and the YAML loader. All configuration should come from environment variables only, nothing read from files.",
    "Add a new `retry.max_attempts` option (default 5) and a `retry.backoff_seconds` option to config/settings.yaml and make loader.py parse them into the Settings object.",
    "Removing the YAML config file and loader conflicts with adding new options to that file and loader.")

add("c14", "contradict",
    "A native iOS app (Swift, SwiftUI) used on consumer iPhones and on a fleet of old warehouse kiosk iPads.",
    "Raise the minimum deployment target to iOS 17 so we can delete all the availability checks and the UIKit fallbacks, and adopt the new Observation framework and SwiftData.",
    "The warehouse kiosks are iPad Air 2 devices stuck on iOS 15. Keep the app installable and fully working on iOS 15; don't use APIs newer than that without a fallback, and keep the deployment target at 15.",
    "Raising the minimum to iOS 17 makes the app uninstallable on iOS 15 devices that the second task requires to keep supporting.")

# ---------------- COMPATIBLE OVERLAP ----------------
add("o01", "compatible_overlap",
    "A Python e-commerce backend where order pricing lives in orders/pricing.py.",
    "Rename `calc_total` in orders/pricing.py to `compute_order_total` and update all callers. The new name matches the rest of the module.",
    "Add input validation to `calc_total` in orders/pricing.py: raise ValueError for negative quantities or negative unit prices. Add tests for those cases.",
    "A rename and an added validation on the same function can both be done; the validation just ends up in the renamed function.")

add("o02", "compatible_overlap",
    "A Rails app with a `users` table in PostgreSQL.",
    "Add a `last_login_at` timestamp column (nullable) to the users table and set it from the sessions controller on successful login.",
    "Login lookups by email are slow. Add a btree index on users.email (lowercased, via an expression index) in a new migration.",
    "Adding a column and indexing a different column on the same table are independent and fully compatible.")

add("o03", "compatible_overlap",
    "A React web app with a Settings page component at src/pages/Settings.tsx.",
    "Add a dark mode toggle to the Settings page, persisted in localStorage, and wire it to the theme provider.",
    "Add a 'Delete my account' section at the bottom of the Settings page with a confirm dialog that calls DELETE /api/me.",
    "Both add independent sections to the same page and can coexist.")

add("o04", "compatible_overlap",
    "A Node service with a single Dockerfile at the repo root.",
    "Convert the Dockerfile to a multi-stage build: one stage for npm ci + build, a slim final stage with only production dependencies and the dist folder.",
    "Add a HEALTHCHECK instruction to the Dockerfile that curls /healthz every 30s with a 3 retry limit.",
    "A multi-stage restructure and a HEALTHCHECK line in the final image can both be satisfied.")

add("o05", "compatible_overlap",
    "A web storefront with a checkout page whose copy lives in src/checkout/Checkout.vue and an i18n setup.",
    "Extract the hard-coded English strings in the checkout page into the i18n message files (en.json) using t() calls so translators can work on them.",
    "There is a typo on the checkout page: 'Confrim your order' should read 'Confirm your order'. Fix it, and also change the 'Pay now' button text to 'Place order'.",
    "Both touch the same strings, but the typo and copy fixes can be applied whether the strings are inline or extracted to the message file.")

add("o06", "compatible_overlap",
    "An Express auth service with a POST /login route handler in routes/login.js.",
    "Add rate limiting to POST /login: max 5 attempts per IP per 10 minutes, then respond with 429 and a Retry-After header.",
    "Add optional TOTP two-factor support to POST /login: if the account has 2FA enabled, require a `totp_code` field and verify it before issuing a session.",
    "Throttling attempts and adding a 2FA verification step in the same handler are independent behaviors.")

add("o07", "compatible_overlap",
    "A Go CLI called `shelf` with a `list` subcommand defined in cmd/list.go.",
    "Add a `--json` flag to `shelf list` that prints the results as a JSON array instead of the table.",
    "Add a `--limit N` flag to `shelf list` that caps how many items are returned (default: unlimited).",
    "Two new, independent flags on the same subcommand.")

add("o08", "compatible_overlap",
    "A dbt + Python ingestion pipeline with an ingest job at pipelines/ingest_events.py.",
    "Add a dedup step to ingest_events.py that drops rows with a duplicate event_id within the same batch before loading.",
    "Add logging of row counts to ingest_events.py: rows read, rows written, rows rejected, emitted as one summary line at the end of each run.",
    "A dedup step and row-count logging in the same job; counts can just reflect the dedup result.")

add("o09", "compatible_overlap",
    "An open source Python library with a single README.md at the repo root.",
    "Rewrite the Installation section of the README: add pipx and uv instructions and drop the outdated Python 3.7 notes.",
    "Add a Troubleshooting section to the README covering the three most common errors from GitHub issues (SSL errors, missing libffi, permission denied on Windows).",
    "Edits to different sections of the same document.")

add("o10", "compatible_overlap",
    "A Flutter mobile app with a news feed screen in lib/screens/feed_screen.dart.",
    "Add pull-to-refresh to the feed screen that reloads the first page of articles.",
    "Replace the spinner on the feed screen with skeleton loader cards while the first page is loading.",
    "Pull-to-refresh and skeleton loading placeholders are complementary changes to the same screen.")

add("o11", "compatible_overlap",
    "A TypeScript monorepo with a GitHub Actions workflow at .github/workflows/ci.yml.",
    "Add an ESLint job to ci.yml that runs on every PR and fails the build on warnings.",
    "Speed up ci.yml: cache the pnpm store and node_modules between runs using actions/cache keyed on the lockfile.",
    "A new lint job and added caching steps in the same workflow file can coexist.")

add("o12", "compatible_overlap",
    "A FastAPI backend with a GET /orders endpoint in app/routers/orders.py backed by SQLAlchemy.",
    "Add cursor-based pagination to GET /orders (limit + next_cursor in the response).",
    "Add an optional `status` query parameter to GET /orders to filter by order status (pending, shipped, cancelled).",
    "Pagination and filtering on the same endpoint are compatible and commonly combined.")

add("o13", "compatible_overlap",
    "A Java Spring service with a UserRepository class used across the codebase.",
    "Refactor UserRepository to return CompletableFuture-based async results instead of blocking calls, and update the service layer to match.",
    "Soft deletes: users now get a `deleted_at` timestamp. Make UserRepository's find methods exclude rows where deleted_at is not null.",
    "Making the repository async and filtering soft-deleted rows can be done together.")

add("o14", "compatible_overlap",
    "A Node backend with a getProduct(id) function in services/product.js that queries Postgres.",
    "Add a Redis read-through cache to getProduct(id) with a 60 second TTL, falling back to Postgres on a miss.",
    "Wrap getProduct(id) so that database errors are reported to Sentry with the product id as context, then rethrown.",
    "Caching and error reporting around the same function don't interfere.")

# ---------------- DISJOINT ----------------
add("d01", "disjoint",
    "A monorepo for a SaaS company: a Django backend, a React web client, and a Terraform infra directory.",
    "fix the 500 on /api/invoices when customer has no billing address. should return blank address block not crash",
    "Update the Terraform for the staging VPC to add a second NAT gateway in us-east-1b for redundancy.",
    "Backend bug fix and infrastructure change are unrelated.")

add("d02", "disjoint",
    "A mobile and web product: a React Native app, a Node API, and a docs site built with Docusaurus.",
    "Add haptic feedback on the 'Add to cart' button in the React Native app (light impact on iOS, short vibration on Android).",
    "Write a docs page explaining how to generate and rotate API keys in the developer portal, with screenshots placeholders.",
    "Mobile haptics and an API-key docs page touch unrelated areas.")

add("d03", "disjoint",
    "A data platform repo: Spark jobs, dbt models, and a small Flask admin tool.",
    "Add a dbt model `fct_daily_active_users` that aggregates events by day and platform, with unique + not_null tests on the date/platform key.",
    "The Flask admin tool's login page has a misaligned logo on Safari. Fix the CSS.",
    "A data model and a CSS fix are in different parts of the repo.")

add("d04", "disjoint",
    "A Python CLI toolkit `pdfkit-cli` for manipulating PDF files, with a GitHub Actions release pipeline.",
    "Add a `merge` subcommand that concatenates multiple PDFs into one, in the order given on the command line.",
    "Set up the release workflow to publish to PyPI via trusted publishing when a tag like v1.2.3 is pushed.",
    "A new CLI feature and release automation are separate concerns.")

add("d05", "disjoint",
    "A Kotlin Android app for a food delivery service with a shared backend in Spring Boot.",
    "Implement the order-tracking map screen on Android using Google Maps compose, showing the courier marker updating every 5s.",
    "Add a DB migration for a `promo_codes` table (code, discount_percent, expires_at, max_uses) in the Spring backend.",
    "An Android UI feature and an unrelated backend table migration.")

add("d06", "disjoint",
    "A company marketing site (Astro) plus an internal Go job runner.",
    "Replace the hero image on the homepage with the new 2026 brand photo (assets/brand-2026/hero.webp) and update the alt text.",
    "Make the Go job runner retry failed jobs up to 3 times with exponential backoff before marking them dead.",
    "A marketing site asset swap and job runner retry logic are unrelated.")

add("d07", "disjoint",
    "A security-sensitive internal platform: an auth gateway written in Go and a Python ML batch scoring service.",
    "Rotate the JWT signing key handling in the gateway so it supports two active keys (kid header lookup) to allow zero-downtime rotation.",
    "Batch scoring is eating 40GB of RAM. Stream the input parquet file in chunks of 100k rows instead of loading it all into memory.",
    "Auth key handling in the gateway and memory optimisation in the scoring service are different codebases and goals.")

add("d08", "disjoint",
    "A multi-language educational app with a Svelte frontend, a Postgres-backed API, and translation files for 9 locales.",
    "Add Brazilian Portuguese (pt-BR) translations for the existing UI strings in the lesson player.",
    "Add an index on lessons(course_id, position) to speed up the course outline query, via a new migration.",
    "Adding a locale and adding a DB index on the lessons table are independent.")

random.seed(20261012)
random.shuffle(items)
out = "/private/tmp/claude-502/-Users-anzalhussainabidi-personal/863c84af-d966-44ff-8c91-23b0fb772a6d/scratchpad/gt1test/intake/dataset.json"
with open(out, "w") as f:
    json.dump({"items": items}, f, indent=2, ensure_ascii=False)
