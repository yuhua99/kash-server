# Kash Web — Frontend Build Spec

This document is a **self-contained build spec**. An implementing agent must be able to
build the entire frontend from this file alone, **without reading any pre-existing frontend
code**. It lives in the `kash-server` mono-repo; the frontend is built under `web/`.

---

## 1. Stack & tooling

- **SvelteKit 2 + Svelte 5**, TypeScript (strict).
- **SPA only**: `@sveltejs/adapter-static`. No SSR, no Node server in production.
- **Bits UI** for interactive primitives (Dialog, Select, Tabs, DatePicker, Collapsible, Menubar).
- **`@internationalized/date`** for the date picker / calendar values.
- **Vitest** for unit tests.
- **No Tailwind.** Plain CSS only (see §6 Styling).
- **oxlint** for linting + **oxfmt** for formatting. **No Prettier, no ESLint, no Biome.**
  oxfmt formats `.ts`/`.svelte`/`.css`/`.json`; `.svelte` formatting is experimental and
  requires `svelte` (`^5`) installed as a peer and the option enabled in the oxfmt config
  (`.oxfmtrc.json`). `.svelte` type/a11y diagnostics are covered by `svelte-check` (the `check`
  script), not the linter. Pin exact oxlint/oxfmt versions (both pre-1.0) so an update can't
  silently reformat the tree.
- Package manager: **bun**.

### Scripts (`package.json`)

```
dev        vite dev
build      vite build
preview    vite preview
check      svelte-kit sync && svelte-check --tsconfig ./tsconfig.json
test       vitest run
lint       oxlint
fmt        oxfmt .
fmt:check  oxfmt --check .
gen:api    openapi-typescript ../openapi.json -o src/lib/api/schema.d.ts
```

### Quality gates (must pass before any commit)

```
bun run fmt
bun run lint
bun run check
bun run test
```

Zero oxlint warnings. Zero `svelte-check` errors. Zero failing tests.

---

## 2. Global architecture decisions (locked)

1. **SPA + static adapter.** Build output (`web/build`) is served by the Rust/Axum backend in
   production via a `ServeDir` fallback. In dev, `vite dev` proxies `/api` → the Rust server
   (default `http://localhost:3000`). The frontend talks to the API with `credentials: 'include'`
   (cookie auth).
2. **Client-side auth gating** in `routes/+layout.ts` (because there is no server layer). See §7.1.
3. **API types & client are GENERATED** from `../openapi.json` via `openapi-typescript`
   (`bun run gen:api` → `src/lib/api/schema.d.ts`, committed). **Never hand-write DTOs.**
   Feature `api.ts` files are thin wrappers over the typed client. See §5 / §7.2.
4. **One validation module** (`lib/validation.ts`) mirroring backend limits. See §8.
5. **Flat feature folders.** No `components/` subfolders, no `core/` wrapper. The only
   structural boundary is `lib/features/`. Inside a feature, `.ts` (logic) and `.svelte`
   (presentation) live side by side.
6. **Dependency rule (the one invariant):** `features/*` may import from `lib/` infra and from
   `features/money/` (a leaf domain). Features must **not** import each other otherwise (no
   cycles). `lib/` infra must **never** import from `features/`.
7. **Components are dumb.** All math / algorithms / state machines live in sibling `.ts`.
   A `.svelte` file is binding + markup only.
8. **File-size caps:** `.svelte` components ~300 LOC, logic `.ts` ~400 LOC. Split by ownership
   when approaching the cap. No junk drawers (`utils.ts`, `helpers.ts`, `misc.ts`, `shared.ts`).
9. **Styling:** see §6.
10. **Swipe navigation is intentionally NOT built.** (Removed from the old app.)

---

## 3. Directory layout

```
web/
  src/
    app.html
    app.css                      # tokens (:root) + reset + base element styles ONLY
    routes/                      # pages: data load + markup only
      +layout.svelte             # app shell: nav + <slot/> + ToastHost + <PendingInbox/> + SW reg
      +layout.ts                 # ssr=false + client auth guard
      login/+page.svelte
      register/+page.svelte
      home/+page.svelte
      records/+page.svelte
      categories/+page.svelte
      stats/+page.svelte
      settings/+page.svelte
      settings/friends/+page.svelte
      settings/friends/[friendId]/+page.svelte
    lib/
      api/
        schema.d.ts              # GENERATED from ../openapi.json (do not edit)
        client.ts                # typed fetch wrapper, credentials:'include'
        errors.ts                # ApiError type + handleApiError (401 -> /login)
      ui/                        # cross-feature dumb primitives (scoped <style> each)
        Button.svelte  ButtonRow.svelte  Block.svelte  Dialog.svelte
        ConfirmDialog.svelte  SelectField.svelte  ListRow.svelte
        ToastHost.svelte  toast.ts
      cache.ts                   # ONE createCache() factory
      config.ts                  # client API base url
      validation.ts              # all field rules (mirrors backend)
      date.ts                    # ISO <-> DateValue, todayIso, periodFromPreset
      features/
        auth/        api.ts  submit.ts  AuthForm.svelte
        categories/  api.ts  cache.ts  CategoryForm.svelte  CategoryList.svelte  CategoryEditDialog.svelte
        records/     api.ts  query.ts  view.ts  cache.ts
                     QuickAddForm.svelte  RecordList.svelte  RecordFilters.svelte
                     RecordEditDialog.svelte
        splits/      api.ts  allocation.ts  idempotency.ts  PendingSharesSection.svelte
        friends/     api.ts  cache.ts  sync.ts  FriendsList.svelte  FriendSearch.svelte
        money/       currency.ts  fx.ts  amount-display.ts  current-currency.ts
        periods/     presets.ts  PeriodControls.svelte
        stats/       query.ts  StatsBreakdown.svelte
        inbox/       data.ts  PendingInbox.svelte
        shell/       nav.ts  service-worker.ts
  static/                        # copy verbatim from reference: sw.js, manifest.webmanifest,
                                 # favicon.ico, apple-touch-icon*.png, robots.txt, icons/*.png
  package.json  svelte.config.js  vite.config.ts  tsconfig.json
```

**Tests** are co-located: `allocation.test.ts` next to `allocation.ts`. Required for:
`splits/allocation.ts`, `records/view.ts`, `money/fx.ts`, `money/currency.ts`,
`money/amount-display.ts`, `periods/presets.ts` (via `date.ts`), `validation.ts`, `date.ts`.
No tests for `.svelte`, generated API, or thin `api.ts` wrappers.

---

## 4. Backend API reference

Base URL = `/api` (browser, via proxy). All requests send cookies. JSON bodies.
Error responses carry `{ message?: string, error?: string }`. `204` = empty success.

> **Names below match `openapi.json` schema names** (what the generated `schema.d.ts` will
> expose). The old frontend used several stale paths/shapes — they are corrected here.
> Pagination/search params (`limit`,`offset`,`start_date`,`search`,`friend_id`, …) are
> **query params** and the spec declares them correctly as `in: query` (optional ones
> `required:false`). The `api.ts` wrappers send them on the query string.

| Method | Path                                 | Body / Query                                                  | Returns                                  |
| ------ | ------------------------------------ | ------------------------------------------------------------- | ---------------------------------------- |
| GET    | `/auth/me`                           | —                                                             | `User` (200) or `401`                    |
| POST   | `/auth/register`                     | `RegisterPayload {username,password}`                         | `User`                                   |
| POST   | `/auth/login`                        | `LoginPayload {username,password}`                            | `User`                                   |
| POST   | `/auth/logout`                       | —                                                             | `204`                                    |
| GET    | `/categories`                        | `?search&limit&offset`                                        | `GetCategoriesResponse`                  |
| POST   | `/categories`                        | `CreateCategoryPayload {name,is_income}`                      | `Category` (201)                         |
| PUT    | `/categories/:id`                    | `UpdateCategoryPayload {name}`                                | `Category`                               |
| DELETE | `/categories/:id`                    | —                                                             | `204`                                    |
| GET    | `/records`                           | `?start_date&end_date&limit&offset`                           | `GetRecordsResponse`                     |
| POST   | `/records`                           | `CreateRecordPayload {name,amount,currency,category_id,date}` | `Record` (201)                           |
| PUT    | `/records/:id`                       | `UpdateRecordPayload` (all optional/nullable)                 | `Record`                                 |
| DELETE | `/records/:id`                       | —                                                             | `204`                                    |
| GET    | `/fx/rates`                          | `?from&to&quotes` (quotes = CSV)                              | `GetFxRatesResponse`                     |
| GET    | `/settings`                          | —                                                             | `UserSettings`                           |
| PUT    | `/settings`                          | `UpdateUserSettingsPayload {main_currency}`                   | `UserSettings`                           |
| GET    | `/friends/search`                    | `?query&limit&offset`                                         | `PublicUser[]`                           |
| GET    | `/friends/list`                      | `?pending&limit&offset`                                       | `FriendListResponse`                     |
| POST   | `/friends/request`                   | `SendFriendRequestPayload {friend_username}`                  | `FriendshipRelation`                     |
| POST   | `/friends/accept`                    | `AcceptFriendPayload {friend_id}`                             | `FriendshipRelation`                     |
| POST   | `/friends/remove`                    | `RemoveFriendPayload {friend_id}`                             | `RemoveFriendResponse` (200, empty `{}`) |
| PATCH  | `/friends/nickname`                  | `UpdateNicknamePayload {friend_id,nickname:string\|null}`     | `FriendshipRelation`                     |
| POST   | `/splits`                            | `CreateSplitPayload`                                          | `SplitCreatedResponse` (201)             |
| POST   | `/splits/participants/:id/finalize`  | `FinalizeSharePayload {category_id}`                          | `Record`                                 |
| PUT    | `/splits/participants/:id/settle`    | —                                                             | `ShareStatusResponse`                    |
| GET    | `/splits/pending`                    | `?limit&offset`                                               | `PendingShareListResponse`               |
| GET    | `/splits/unsettled`                  | `?friend_id&limit&offset`                                     | `UnsettledShareListResponse`             |
| PUT    | `/splits/with/:friend_id/settle-all` | —                                                             | `SettleAllResponse`                      |

In the two `/splits/participants/:id/...` paths, **`:id` is a `participant_id`** (from a
`PendingShare`/`UnsettledShare`), not a record id.

Key response shapes (authoritative source is `schema.d.ts`; shown here for reference):

- `Record`: `{ id, name, amount:number, currency, category_id:string|null, date }`.
  **There is NO `pending` field on records.** Amount sign: **positive = income, negative =
  expense, never 0.**
- `Category`: `{ id, name, is_income:boolean }`.
- `FriendshipRelation`: `{ id, user_id, pending:boolean, nickname:string }` (nickname defaults to username).
- `PublicUser`: `{ id, username }`.
- `CreateSplitPayload`: `{ idempotency_key, total_amount, currency, description, date,
category_id, splits: SplitParticipant[] }`; `SplitParticipant = { user_id, amount }`.
- `SplitCreatedResponse`: `{ split_id, creditor_record_id, participants: ParticipantBrief[] }`;
  `ParticipantBrief = { id, debtor_user_id, amount }`.
- `PendingShare`: `{ participant_id, split_id, description, date, amount, currency,
creditor_user_id, creditor_name, settled }`. **(This is the "You owe" obligation — not a record.)**
- `UnsettledShare`: `{ participant_id, split_id, description, date, amount, currency,
counterparty_user_id, counterparty_name, direction:string, finalized, settled }`.
- `ShareStatusResponse`: `{ participant_id, finalized, settled }`.
- List responses wrap their array + pagination: `GetRecordsResponse {records,total_count}`,
  `GetCategoriesResponse {categories,total_count,limit,offset}`,
  `FriendListResponse {friends,...}`, `PendingShareListResponse {shares,...}`,
  `UnsettledShareListResponse {shares,...}`.
- `UserSettings`: `{ main_currency:string }`.
- `ExchangeRateRow`: `{ date, currency, rate:number }`; `GetFxRatesResponse`: `{ rates }`.

---

## 5. Infra files (`lib/`)

### `lib/config.ts`

- `getApiBaseUrl(): string` → `import.meta.env.VITE_API_BASE_URL || '/api'`.

### `lib/api/client.ts`

- A thin typed wrapper around `fetch` (may use `openapi-fetch` with `schema.d.ts`, or a small
  hand-written `request<T>()`). Requirements:
  - Always `credentials: 'include'`.
  - JSON-encode body; set `Content-Type: application/json` only when a body is present.
  - On non-2xx: parse `{message|error}`, throw an `ApiError` (Error with `.status:number`).
  - `204` → resolve `undefined`.
  - Helper to append query params, skipping `undefined`.
- Exposes typed methods per verb, or `client.request<T>(path, {method,body,headers})`.

### `lib/api/errors.ts`

- `export type ApiError = Error & { status?: number }`.
- `getErrorMessage(error: unknown, fallback: string): string` — returns `error.message` if a
  non-empty `Error`, else `fallback`.
- `async handleApiError(error: unknown, fallback: string): Promise<string>` — if `status===401`,
  `goto('/login')` and return `''`; otherwise return `getErrorMessage(error, fallback)`.
  (Callers `toast.error(...)` the returned message when non-empty.) This replaces the per-handler
  401 boilerplate; **use it in every feature action.**

### `lib/cache.ts` — `createCache<T>(fetcher: () => Promise<T>)`

One generic in-flight-dedup + versioned-invalidation cache. Replaces the 3 duplicated caches.

```ts
type Cache<T> = {
  get(): Promise<T>; // returns cached value, or the in-flight request, or fetches
  set(value: T): void; // seed the cache
  invalidate(): void; // bump version, clear value + in-flight request
};
```

Semantics: `get()` returns cached value if present; else returns the in-flight promise if one is
running; else starts `fetcher()`, stores the result **only if the version is unchanged** when it
resolves, and clears the in-flight handle in `finally`. `invalidate()` increments the version so a
late in-flight result is discarded.

### `lib/validation.ts`

Pure functions returning `string | null` (error message or null). See §8 for exact rules. Export:
`validateUsername`, `validatePassword`, `validateCategoryName`, `validateRecordName`,
`validateSearchTerm`, `validateDate`, `validateAmount`, `validateNickname`,
`validateFriendSearchQuery`, `validateSplitParticipantAmount`, `validateSplitTotals`.

### `lib/date.ts`

- `type PeriodPreset = 'month' | 'year' | 'custom'`.
- `todayIso(): string` — local `YYYY-MM-DD`.
- `isoToDateValue(value: string): DateValue | undefined` — `parseDate`, undefined on empty/invalid.
- `dateValueToIso(value: DateValue|null|undefined): string` — `value?.toString() ?? ''`.
- `periodFromPreset(preset, options?: {year?,month?,start?,end?}): {start,end}`:
  - `month`: 1st → last day of (year, month); default = current year/month.
  - `year`: Jan 1 → Dec 31 of year; default = current year.
  - `custom`: pass through `start`/`end`.
  - **Always cap `end` at today** (never return a future end date).

### `lib/ui/*` (dumb primitives, each with scoped `<style>`)

- `Button.svelte` — props `variant:'primary'|'secondary'`, `size?:'compact'`, `type`, `disabled`,
  `onclick`, `className?`; renders `<button class="btn ...">`+slot.
- `ButtonRow.svelte` — horizontal row container for buttons (slot).
- `Block.svelte` — titled `<section class="block">` with `title` prop + slot.
- `Dialog.svelte` — wrapper over `Bits UI Dialog` (Root/Portal/Overlay/Content/Title/Description);
  props `open`, `onOpenChange`, `title`, `description?` + slot.
- `ConfirmDialog.svelte` — props `open`, `onOpenChange`, `title`, `description`, `confirmLabel`,
  `confirmBusyLabel`, `busy`, `onConfirm`. Cancel + confirm buttons.
- `SelectField.svelte` — wrapper over `Bits UI Select`; props `id`, `value`, `label`,
  `items: Array<{value,label} | {kind:'separator'}>`, `disabled?`, `onValueChange(value:string)`.
- `ListRow.svelte` — generic row primitive (slot-based) for lists.
- `toast.ts` — toast store: `toast.success(msg)`, `toast.error(msg)`, `toast.info(msg)`;
  exposes a subscribable list of `{id,kind,message}` and auto-dismiss.
- `ToastHost.svelte` — renders the toast list (mounted once in the layout).

---

## 6. Styling & visual design

### 6.0 Mechanics (locked)

- **Remove Tailwind entirely.**
- **`app.css` (the only global stylesheet)** contains: ① design tokens in `:root`, ② CSS reset,
  ③ base element styles (`body`, `input`, `textarea`, `button`, `form`, `label`, `[role=alert]`,
  `[role=status]`, `.text-link`), ④ the optional texture layer (§6.5).
- **Component CSS is co-located** in each `.svelte` as a scoped `<style>` block. **No separate
  `.css` files. No inline `style=` for static styling.** To style Bits UI internals (rendered via
  the `class` prop you pass), use `:global(...)` inside that component's `<style>`.

### 6.1 Aesthetic — Tactical Telemetry (dark CRT terminal)

The app commits to **one** visual archetype: a dark, monospaced, military/aerospace terminal
look — rigid grids, razor-thin dividers, extreme type-scale contrast, utilitarian color, optional
analog degradation. **Do not** use the light "Swiss print" mode; do not mix light and dark.
**Forbidden everywhere:** `border-radius` (all corners 90°), gradients, soft/drop shadows, blur,
modern translucency/glassmorphism, decorative easing. Borders and grid gaps do the visual work.

**Project deviation from the source skill:** the skill mandates Aviation/Hazard Red as the sole
accent. **We override that** — the single accent is the existing warm amber `--accent`. Red is
used **only** as the semantic danger color, never as the brand accent.

### 6.2 Tokens (`app.css :root`)

```css
:root {
  /* substrate (deactivated CRT — never pure #000) */
  --bg: #0f0f0f;
  --surface: #161616;
  --panel: #1a1a1a;
  --panel-strong: #232323;
  --border: #282828;
  --border-strong: #3a3a3a;
  /* phosphor text */
  --text: #eaeaea;
  --text-muted: #a0a0a0;
  --text-dim: #8a8a8a;
  /* accent (KEPT — not hazard red) */
  --accent: #ffc799;
  --accent-strong: #ffd8b8;
  /* semantic only */
  --success: #99ffe4;
  --danger: #ff8080;
  --selection: #ffffff25;
  /* type */
  --font-mono: "IBM Plex Mono", "SF Mono", Menlo, Monaco, Consolas, monospace;
  --font-display: "Archivo", "Inter", system-ui, sans-serif; /* heavy grotesque for macro headers */
  /* spacing scale (use these, not magic numbers) */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-6: 24px;
  --space-8: 32px;
}
```

Load fonts in `app.html`: **IBM Plex Mono** (400/500/600) and **Archivo** (800/900). Mono is the
default `body` font; `tabular-nums` on; sharp corners; `main { width:min(100%,720px); margin:0
auto }`; app shell adds bottom padding for the nav bar; auth pages center a narrow bordered card.

### 6.3 Typography (extreme scale contrast = the core of the look)

- **Macro / display** (`--font-display`, heavy grotesque): page titles, the big amount on a record,
  hero numerals. Massive via `clamp()` (e.g. `clamp(2.5rem,8vw,5rem)`), **UPPERCASE**, tight
  tracking (`-0.03em`→`-0.05em`), compressed leading (`0.9`). Mobile-first: cap the clamp so it
  never overflows a phone.
- **Micro / telemetry** (`--font-mono`): everything else — labels, nav, metadata, table data,
  amounts in lists, IDs, dates. Small fixed sizes (`10`–`14px`), generous tracking
  (`0.05em`→`0.1em`), **UPPERCASE** for all labels/nav/metadata/units.
- Sentence-case is allowed only for free-text the user typed (record names, nicknames).

### 6.4 Layout & spatial engineering

- **Blueprint grid:** CSS Grid; elements anchor to tracks, they don't float.
- **Hairline dividers via grid:** prefer `display:grid; gap:1px; background:var(--border)` with
  child cells on `var(--surface)` to draw razor-thin 1px separators without per-side borders.
- **Visible compartmentalization:** delineate zones with `1px`/`2px solid var(--border)`; full-width
  `<hr>` to segregate units.
- **Bimodal density:** tightly-packed mono metadata clusters vs. calculated negative space framing
  macro type. Lists/records are dense; headers are spacious.

### 6.5 Components & symbology

- **ASCII framing** for section labels and tags: `[ RECORDS ]`, `< YOU OWE >`, directional
  `>>>` / `///`. Implement as CSS `::before/::after` content where possible, not literal text in
  the DOM that breaks i18n/reading order.
- **Industrial markers / crosshairs:** `+` at grid intersections, thin barcode-like rules, small
  technical strings (e.g. `REV 0.2`, `UNIT / KASH`) as decoration — decorative only,
  `aria-hidden`.
- **Semantic tags:** use `<data>`, `<output>`, `<samp>`, `<dl>/<dt>/<dd>` for telemetry-style data
  so the markup matches the aesthetic and stays accessible.
- Buttons/inputs: flat, bordered, square; focus = `border-color:var(--accent)` (no glow/shadow).

### 6.6 Textural / analog effects (OPTIONAL, tasteful, opt-in)

Global low-opacity grain and/or CRT scanlines may be applied at the app shell root to avoid a
purely-digital feel — e.g. a `repeating-linear-gradient` scanline overlay and a faint SVG noise
layer. **Constraints:** keep opacity very low (≤5%); must be `pointer-events:none`; must respect
`@media (prefers-reduced-motion: reduce)` (no animation) and stay cheap on mobile (no per-frame
filters). Never apply blur/heavy SVG filters that tank scroll performance. Halftone/1-bit dithering
is reserved for rare decorative imagery, not core UI. If an effect hurts readability or perf, omit
it — function first.

### 6.7 Token discipline

Component `<style>` blocks reference tokens (`var(--accent)`, `var(--space-3)`, `var(--font-mono)`)
for color/spacing/type. Avoid hardcoded hex colors and magic numbers; if a value recurs, add a
token to `app.css`. This keeps the whole look re-skinnable from `app.css` alone.

---

## 7. Feature specs

### 7.1 `routes/`

- **`+layout.ts`** — `export const ssr = false`. The client auth guard `load`:
  1. Call `getMe()` (auth/api). Put `{ user }` on the returned data.
  2. Apply redirects based on `url.pathname`:
     - `/` → `user ? '/home' : '/login'`.
     - Protected (`/home`,`/records`,`/categories`,`/stats`,`/settings` and subpaths) and `!user` → `/login`.
     - Auth routes (`/login`,`/register`) and `user` → `/home`.
- **`+layout.svelte`** — app shell. Renders: top nav (`Bits UI Menubar`) using `shell/nav.ts`
  items, highlighting the active route; `<ToastHost/>`; `<slot/>`; `<PendingInbox/>` (only when a
  user is present and not on an auth route). On mount, register the service worker via
  `shell/service-worker.ts`. Hide nav on auth routes. **No touch/swipe handlers.**
- Each `+page.svelte` loads its data **on the client** (in `onMount` or a client `+page.ts`),
  shows loading / empty / inline-error states, and delegates UI to feature components. Pages hold
  page-level state only; all reusable logic is imported from feature `.ts` modules.

Route responsibilities:

- `/login`, `/register` → `AuthForm` (mode differs). On success `goto('/home')`.
- `/home` → full-screen `QuickAddForm` only.
- `/records` → pending section (if any) + filters + record list + edit/delete dialogs. See 7.4.
- `/categories` → `CategoryForm` + `CategoryList` + `CategoryEditDialog`.
- `/stats` → `PeriodControls` + `StatsBreakdown`. See 7.9.
- `/settings` → logout button; main-currency setter; current-currency picker; amount-display
  toggle (cents/whole); link to friends.
- `/settings/friends` → `FriendSearch` + `FriendsList`.
- `/settings/friends/[friendId]` → that friend's detail: nickname edit, unsettled splits list,
  "settle all" action, remove friend.

### 7.2 `features/auth/`

- `api.ts`: `getMe(): Promise<User|null>` (200→User, 401→null, else throw),
  `register(username,password)`, `login(username,password)`, `logout()`.
- `submit.ts`: `handleAuthSubmit(opts)` — prevents default, trims+validates username/password via
  `lib/validation`, sets field errors, calls `onValidSubmit`, manages pending flag, maps thrown
  errors to a form-level error message. Signature mirrors: `{ event, username, password,
onValidSubmit, setUsernameError, setPasswordError, setFormError, setPending, fallbackErrorMessage }`.
- `AuthForm.svelte`: username/password inputs, inline errors, submit button. Prop `mode:'login'|'register'`.

### 7.3 `features/categories/`

- `api.ts`: `getCategories({search?,limit?,offset?})`, `createCategory({name,is_income})`,
  `updateCategory(id,{name})`, `deleteCategory(id)`.
- `cache.ts`: `const categoriesCache = createCache(() => getCategories({limit:1000,offset:0}).then(r=>r.categories))`.
  Export `getCategoriesCached()`, `invalidateCategoriesCache()`, `setCategoriesCache(list)`.
- `CategoryForm.svelte`: name input + income/expense toggle (Tabs) → `createCategory`,
  validate name (1–100), invalidate cache, toast.
- `CategoryList.svelte`: list with edit/delete actions; delete via `ConfirmDialog`.
- `CategoryEditDialog.svelte`: edit name (Dialog) → `updateCategory`.

### 7.4 `features/records/`

- `api.ts`: `getRecords({start_date?,end_date?,limit?,offset?})`, `createRecord(body)`,
  `updateRecord(id,partial)`, `deleteRecord(id)`. **No pending/settle here** — finalize and
  settle are share operations and live in `splits/api.ts`. `/records` has no `pending` filter.
- `query.ts`: `getAllRecordsByDateRange({startDate,endDate}): Promise<RecordItem[]>` — page through
  `/records` (limit 1000) until `records.length >= total_count` or a page is empty.
- `cache.ts`: `createCache(() => getRecords({limit:500,offset:0}).then(r=>r.records))` →
  `getRecentRecordsCached()`, `invalidateRecordsCache()`. Also export
  `filterRecordsByDateRange(records,start,end)`.
- `view.ts` (**pure, tested**):
  - `type SortMode = 'date_desc'|'date_asc'|'amount_desc'|'amount_asc'`.
  - `type CategoryFilterMode = 'all_expenses'|'all_incomes'|`category:${string}``.
(No `pending` filter — records carry no pending flag; "You owe" is a separate shares section.)
  - `matchesRecordFilters(record, {normalizedSearch, categoryFilter}): boolean` —
    search matches name (case-insensitive); `all_expenses`→amount<0; `all_incomes`→amount>0;
    `category:<id>`→`category_id===id`.
  - `compareRecords(a,b,mode,convertedById:Map<string,number>): number` — date modes sort by ISO
    string; amount modes sort by **converted** absolute amount (fallback to raw amount), tie-break
    by signed amount.
  - `groupRecordsByDate(records, mode, convertedSpendById, mainCurrency): DateGroup[]` where
    `DateGroup = {date, records, spendSummaries: {currency,amount}[]}`. Dates sorted per mode.
  - `summarizeDailySpend(records, convertedSpendById, mainCurrency)`: if a main currency is set,
    return one total in that currency (sum of converted **expense** amounts, only if >0); else
    return per-currency expense subtotals sorted by currency code.
- `QuickAddForm.svelte`: the add-record form. Fields: amount (number; step depends on
  amount-display mode), type tabs (expense/income), category select (filtered by type), date
  picker, name input with **suggestions** (recent records in the same category, ranked by amount
  closeness — dedup, max 5), and a **split** collapsible. Validation via `lib/validation`. On
  submit: if split disabled → `createRecord` with signed amount (`isIncome ? +a : -a`),
  `currency = $currentCurrency`; if split enabled → `createSplit` payload (see 7.5). Invalidate
  records cache; reset form; toast. Handle 401 (→login) and 409 (split: regen key + retry toast).
  **All amount math comes from `money/` and `splits/allocation.ts`; the component holds none.**
- `RecordList.svelte`: renders grouped or flat list (per sort mode), per-row action panel
  (edit/delete). `RecordFilters.svelte`: period controls (`PeriodControls`), search box (validated,
  ≤100), category filter select, sort select. `RecordEditDialog.svelte`: edit name/amount/
  category/date (Dialog) → `updateRecord`. The "You owe" block at the top of `/records` is
  rendered by `splits/PendingSharesSection.svelte` (see 7.5), fed by `/splits/pending` — these
  are obligations, not records.

### 7.5 `features/splits/`

- `api.ts`:
  - `createSplit(payload)` → `POST /splits` (returns `SplitCreatedResponse`).
  - `finalizeShare(participantId, categoryId)` → `POST /splits/participants/{id}/finalize`
    body `{category_id}` (returns `Record`).
  - `settleShare(participantId)` → `PUT /splits/participants/{id}/settle` (returns `ShareStatusResponse`).
  - `listPendingShares({limit?,offset?})` → `PendingShare[]` (the viewer's "you owe" obligations).
  - `listUnsettledShares(friendId,{limit?,offset?})` → `UnsettledShare[]`.
  - `settleAllWithFriend(friendId)` → `PUT /splits/with/{friend_id}/settle-all`.
- `PendingSharesSection.svelte`: "You owe" list of `PendingShare[]`, used by the `/records` page.
  (Lives in `splits/` because pending shares are a splits concern; routes may import it.)
- `idempotency.ts`: `generateIdempotencyKey(): string` — `crypto.randomUUID()` with a manual
  UUIDv4 fallback.
- `allocation.ts` (**pure, tested**) — the split math (see §9 for exact rules):
  - `computeAutoShares({selectedIds, total, lockedAmounts, touched, mode}): Record<string,string>`
    — equal split among **unlocked participants + the payer**; locked participants keep their
    amount; per-person share = floor to integer (whole mode) or to cents (cents mode); residual
    stays with payer.
  - `assignAllToFriends({selectedIds, total, mode}): Record<string,string>` — distribute the entire
    total across selected participants; remainder units handed out one-by-one to the first
    participants; all marked as touched (payer share = 0).
  - `buildParticipantSplits(ids, amountInputs): {user_id,amount}[]`.
  - Reuse `validateSplitParticipantAmount` and `validateSplitTotals` from `lib/validation.ts`
    (do not redefine them here).
  - Use `roundToCents` and amount formatting from `money/amount-display.ts`.

### 7.6 `features/friends/`

- `api.ts`: `searchUsers({query,limit?,offset?})`, `listFriends({pending?,limit?,offset?})`,
  `sendFriendRequest(friendUsername)`, `acceptFriend(friendUserId)`, `removeFriend(friendUserId)`,
  `updateNickname(friendUserId, nickname:string|null)`.
- `cache.ts`: `createCache(() => listFriends({pending:false,limit:1000,offset:0}).then(r=>r.friends))`
  → `getAcceptedFriendsCached()`, `invalidateFriendsCache()`.
- `sync.ts`: a small revision store so views refresh after friend mutations:
  `friendsSyncRevision` (subscribable) + `notifyFriendsSync()`.
- `FriendSearch.svelte`: search input (validate query 1–50) → `searchUsers` → send request.
- `FriendsList.svelte`: accepted friends list; row → friend detail route; nickname/remove actions
  may live in the detail route.

### 7.7 `features/money/` (leaf domain — may be imported by other features; imports nothing from features)

- `currency.ts` (**pure, tested**): `type SupportedCurrencyCode = 'TWD'|'USD'|'JPY'|'EUR'|'CNY'`;
  `SUPPORTED_CURRENCIES` with `fractionDigits` (TWD0, USD2, JPY0, EUR2, CNY2);
  `DEFAULT_CURRENCY_CODE='TWD'`; `isSupportedCurrencyCode`, `getCurrencyConfig`,
  `formatMoney(amount,code)`, `formatSignedMoney(amount,code)`.
- `fx.ts` (**pure, tested**): `buildRateLookup(rows): Map<string,number>` keyed `${date}:${currency}`;
  `convertAmountToMainCurrency(amount,from,to,date,rates)` — identity if `from===to`, else
  `amount * (toRate/fromRate)`, **throw** if either rate missing for that date;
  `buildCurrencySubtotals(items): {currency,total}[]` sorted by currency.
- `amount-display.ts` (**pure fns tested; also a store**): `type AmountDisplayMode='cents'|'whole'`;
  store `amountDisplayMode` (subscribable) backed by `localStorage` key `kash_amount_display_mode`;
  `setAmountDisplayMode(mode)`; `roundToCents(n)`; `normalizeAmountInputValue(value,mode)`
  (truncate to integer in whole mode); `formatAmount(value,mode,currency?)`
  (whole→`trunc`, cents→`toFixed(fractionDigits)`, JPY/TWD/etc 0 digits);
  `formatSignedAmount(value,mode,currency?)`.
- `current-currency.ts` (store): subscribable `currentCurrency` backed by `localStorage`
  `kash_current_currency` (default TWD); `setCurrentCurrency(code)`;
  `initializeCurrentCurrency(defaultFromSettings)` — use stored value if valid, else seed from the
  user's `main_currency`.

### 7.8 `features/periods/`

- `presets.ts`: thin re-use of `lib/date.periodFromPreset` plus the preset option list for the UI
  (month/year/custom). (If trivial, the component may call `lib/date` directly.)
- `PeriodControls.svelte`: preset tabs + (custom) start/end date pickers; emits/`onPeriodChange`
  with `{preset,start,end}`. Used by `/records` and `/stats` for consistent period behavior.

### 7.9 `features/stats/`

- `query.ts` (**pure, tested**): `calculateTotals(records): {netTotal,incomeTotal,expenseTotal}`;
  `buildBreakdown(records, categories): BreakdownItem[]` where `BreakdownItem =
{categoryId,name,isIncome,total,absoluteTotal,share}` (`share` = category abs total / total abs).
  Stats operate on **records already converted to the main currency**; conversion uses
  `money/fx.ts`. If conversion is incomplete, fall back to per-currency subtotals + a message.
- `StatsBreakdown.svelte`: renders net/income/expense totals and the category breakdown with
  share bars.

### 7.10 `features/inbox/`

- `data.ts`: load + actions for the pending queue. `loadPendingInbox()` →
  `Promise.allSettled([listFriends({pending:true,limit:1000}), listPendingShares({limit:1000})])`,
  building a queue: friend items first, then share items
  (`{kind:'friend',key,friend}` / `{kind:'share',key,share}`). Action helpers:
  `acceptPendingFriend`, `declinePendingFriend` (→ `acceptFriend`/`removeFriend`, then
  `invalidateFriendsCache()` + `notifyFriendsSync()`), `savePendingShare(participantId, categoryId)`
  (→ `finalizeShare(participant_id, category_id)` then `invalidateRecordsCache()`).
  Error mapping: 401 → `/login`; 404/409 → toast "already handled" + dismiss current item;
  else error toast.
- `PendingInbox.svelte`: shows **one dialog at a time** (queue head). Friend dialog =
  accept/decline. Share dialog = details (`creditor_name` / `description` / `date` / `amount` +
  currency) + category select (load categories, prefer expense categories) + save. Bootstrap
  **once per user** on app entry (guarded by a tracked user id), only when a user exists and not
  on an auth route. See §9.2.

### 7.11 `features/shell/`

- `nav.ts`: `navItems = [{href:'/home',label:'KASH!'},{'/records','Records'},
{'/categories','Categories'},{'/stats','Stats'},{'/settings','Settings'}]`;
  `isActive(pathname,href): boolean` (`===` or `startsWith(href + '/')`).
- `service-worker.ts`: `registerServiceWorker()` — skip in dev / when unsupported; register
  `/sw.js`; on a new worker reaching `installed` while a controller exists, post `SKIP_WAITING`
  and reload on `controllerchange` (only if there was a controller before, to avoid first-install
  reload). Returns a cleanup function for the layout's `onMount`.

---

## 8. Validation rules (`lib/validation.ts`) — must match backend exactly

| Function                         | Rule                                | Message                                                                                  |
| -------------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------- |
| `validateUsername`               | length 4–50 AND `^[A-Za-z0-9_-]+$`  | "Username must be 4-50 characters." / "Username allows letters, numbers, \_ and - only." |
| `validatePassword`               | length ≥ 6                          | "Password must be at least 6 characters."                                                |
| `validateCategoryName`           | length 1–100                        | "Category name must be 1-100 characters."                                                |
| `validateRecordName`             | length 1–255                        | "Record name must be 1-255 characters."                                                  |
| `validateSearchTerm`             | empty OK; else ≤ 100                | "Search term must be 1-100 characters."                                                  |
| `validateDate`                   | matches `^\d{4}-\d{2}-\d{2}$`       | "Date must use YYYY-MM-DD."                                                              |
| `validateAmount`                 | finite AND ≠ 0                      | "Amount must be a number." / "Amount cannot be 0."                                       |
| `validateNickname`               | ≤ 50                                | "Nickname must be 50 characters or fewer."                                               |
| `validateFriendSearchQuery`      | trimmed 1–50                        | "Search query is required." / "Search query must be 50 characters or fewer."             |
| `validateSplitParticipantAmount` | finite AND > 0                      | "Amount must be greater than 0."                                                         |
| `validateSplitTotals`            | `round(sum*100) ≤ round(total*100)` | "Participant shares cannot exceed total amount."                                         |

Amount form rule everywhere: the UI input is non-negative; **expense ⇒ store negative, income ⇒
store positive**. Reject negative typed input with "Amount cannot be negative." before the
`validateAmount` check.

---

## 9. Pinned behaviors (build exactly as specified)

### 9.1 Split auto-allocation (in `splits/allocation.ts`, consumed by `QuickAddForm`)

Context: `total` = entered amount (positive), payer = current user, participants = selected friends.

1. **Payer counts as one share.** Equal-split pool size = (number of selected friends) **+ 1**.
2. **Per-person share** = `floor(remaining / pool)`; whole mode truncates to integer, cents mode
   floors to cents (`floor(x*100)/100`).
3. **Locked participants:** if the user manually edits a friend's amount, that friend is "locked"
   and keeps their value; the remaining unlocked friends split `total − sum(locked)` equally.
4. **Residual stays with payer:** `yourShare = total − sum(participantAmounts)`, shown in the footer.
5. **Deselect:** keep that friend's last amount in state (restored if re-selected) but clear the
   locked flag.
6. **"Max" button:** assign the **entire `total`** across selected friends (payer share = 0);
   remainder units handed out one-by-one to the first friends; mark all as locked.
7. **Submit validation:** at least one friend selected; each participant amount > 0; participant
   sum ≤ total (compare in integer cents).
8. **Submit result:** success → reset form + regenerate idempotency key; HTTP 409 → regenerate
   idempotency key + toast "Duplicate key conflict — please try again."
   The split payload uses `currency = $currentCurrency`, `total_amount = total` (positive),
   `splits = [{user_id, amount}]` for each selected friend.

### 9.2 Pending inbox (auto-popup on app entry)

- Trigger: when a `user` is present, not on an auth route, and the inbox has not yet been
  bootstrapped for this user id. Run once per user id.
- Pending friends come from `GET /friends/list?pending=true` (`FriendshipRelation[]`); pending
  shares come from `GET /splits/pending` (`PendingShare[]`).
- Build the queue (friends then shares); display **one dialog at a time** (queue head).
- Friend dialog: Accept → `acceptFriend`; Decline → `removeFriend`. On success invalidate friends
  cache + `notifyFriendsSync()` + dismiss.
- Share dialog: show `creditor_name` / `description` / `date` / `amount`; require a category
  selection (load categories, prefer expense categories, allow retry if none); Save →
  `finalizeShare(participant_id, category_id)` → invalidate records cache + dismiss.
- Errors: 401 → `/login`; 404 or 409 → toast "already handled" + dismiss head; else error toast
  (keep item).

### 9.3 Records page FX conversion

- Conversion is only needed for: amount-sort modes (convert each record to main currency) and
  date-group daily spend (convert expenses to main currency).
- Fetch `getSettings()` for `main_currency`, then `getFxRates({from:startDate,to:endDate,
quotes:[...recordCurrencies, main_currency]})`, build a rate lookup, convert per record by its
  own date. Use a **sequence guard** (increment a counter per refresh; ignore stale results) so
  rapid period/sort changes don't apply out-of-order results. On any failure, clear converted maps
  and fall back to raw/per-currency display (no hard error).

### 9.4 Auth/session

- All mutations that can 401 must route through `handleApiError` (→ `/login`).
- After create/update/delete, invalidate the relevant cache and refetch authoritative data.

---

## 10. Acceptance checklist

- [ ] `bun run gen:api` produces `lib/api/schema.d.ts`; no hand-written DTOs anywhere.
- [ ] `bun run check` and `bun run test` pass; no `.svelte` over ~300 LOC, no `.ts` over ~400.
- [ ] No Tailwind; no separate component `.css` files; component styles are scoped `<style>`.
- [ ] All routes exist with loading/empty/inline-error states; auth gating + redirects work.
- [ ] Records: search/filter/sort/period/group/edit/delete; FX conversion with sequence guard.
- [ ] Categories: create/edit/delete with validation.
- [ ] Stats: net/income/expense + category breakdown in main currency, with subtotal fallback.
- [ ] Settings: logout, main currency, current currency picker, amount-display toggle.
- [ ] Friends: search/request/accept/remove/nickname; unsettled list + settle-all per friend.
- [ ] Splits: quick-add split with auto-allocation rules §9.1; idempotency + 409 handling.
- [ ] Pending inbox auto-popup queue §9.2.
- [ ] PWA: static assets copied; service worker auto-update works in production build.
- [ ] Unit tests cover allocation, records/view, money/{fx,currency,amount-display}, validation, date.

```

```
