# API Resource Coverage Census — OpenProject v3 vs. nexgen-rs Seed

_Generated for Sprint C0 (W1-X1). Date: 2026-06-01._

This census measures how much of OpenProject's HTTP API v3 surface the Rust seed
(`openproject-nexgen-rs`) currently reimplements. The v3 resource set is enumerated
from the directories under `/home/user/openproject/lib/api/v3/` (each subdirectory is
an API resource; loose `.rb` files such as `root.rb`, `cors.rb`, `parser.rb`,
`root_representer.rb` are not resources and are excluded). Seed coverage is measured
across four surfaces: HTTP **handlers** (`crates/op-api/src/handlers/`), domain
**models** (`crates/op-models/src/`), DB **repos** (`crates/op-db/src/`), and HAL
**representers** (`crates/op-api/src/representers/`). A resource is counted **covered**
only when both a handler and a model are present; **partial** when some surfaces
(typically handler + repo) exist but a first-class model is missing; **absent** when no
surface implements it. This turns the project's informal "6–18%" coverage estimate into
a measured figure.

## Coverage Table

| v3 resource | seed handler? | seed model? | seed repo? | representer? | status |
|---|---|---|---|---|---|
| actions | – | – | – | – | absent |
| activities | yes (+journals) | – | yes (+journals) | – | partial |
| attachments | yes | – | yes | – | partial |
| backups | – | – | – | – | absent |
| capabilities | – | – | – | – | absent |
| categories | yes | – | yes | – | partial |
| configuration | – | – | – | – | absent |
| custom_actions | – | – | – | – | absent |
| custom_fields | – | – | – | – | absent |
| custom_options | – | – | – | – | absent |
| days | – | – | – | – | absent |
| emoji_reactions | – | – | – | – | absent |
| errors | – | – | – | – | absent (utility) |
| favorites | – | – | – | – | absent |
| formatter | – | – | – | – | absent (utility) |
| groups | – | – | – | – | absent |
| help_texts | – | – | – | – | absent |
| memberships | yes | yes (member) | yes (members) | – | covered |
| news | – (in progress) | yes (news.rs, not yet exported) | yes (news.rs) | – | partial (in progress) |
| notifications | – | – | – | – | absent |
| oauth | – | – | – | – | absent (utility) |
| placeholder_users | – | – | – | – | absent |
| portfolios | – | – | – | – | absent |
| posts | – | – | – | – | absent |
| principals | – | – | – | – | absent |
| priorities | yes | yes | yes | – | covered |
| programs | – | – | – | – | absent |
| project_phase_definitions | – | – | – | – | absent |
| project_phases | – | – | – | – | absent |
| projects | yes | yes (project/) | yes | yes | covered |
| queries | yes | – | yes | yes | partial |
| relations | yes | – | yes | – | partial |
| reminders | – | – | – | – | absent |
| render | – | – | – | – | absent (utility) |
| repositories | – | – | – | – | absent |
| roles | yes | yes | yes | – | covered |
| schemas | – | – | – | – | absent (utility) |
| shares | – | – | – | – | absent |
| statuses | yes | yes | yes | – | covered |
| string_objects | – | – | – | – | absent (utility) |
| types | yes | yes (type_def) | yes | – | covered |
| user_non_working_times | – | – | – | – | absent |
| user_preferences | – | – | – | – | absent |
| user_working_hours | – | – | – | – | absent |
| users | yes | yes (user/) | yes | yes | covered |
| utilities | – | – | – | – | absent (utility) |
| values | – | – | – | – | absent (utility) |
| versions | yes | yes | yes | – | covered |
| views | – | – | – | – | absent |
| watchers | yes | – | yes | – | partial |
| wiki_pages | – | – | – | – | absent |
| work_packages | yes | yes (work_package/) | yes | yes | covered |
| workspaces | – | – | – | – | absent |

## Summary Statistics

```
Total v3 resource directories            : 53
  of which utility/shared (non-domain)    : 9   (errors, formatter, oauth, render,
                                                 schemas, string_objects, utilities,
                                                 values, configuration)
  domain resources (denominator)          : 44

Covered  (handler + model present)        : 9   (work_packages, projects, users,
                                                 statuses, types, priorities, roles,
                                                 versions, memberships)
Partial  (some surfaces, no full model)   : 6   (activities, attachments, categories,
                                                 queries, relations, watchers)
In progress (News vertical, this sprint)  : 1   (news — model + repo present, handler
                                                 pending, not yet exported in lib.rs)
Absent                                    : 28  (of 44 domain resources)

Coverage % (covered / all 53 dirs)        : 17.0%   (9 / 53)
Coverage % (covered / 44 domain only)     : 20.5%   (9 / 44)
Coverage % (covered + partial / 53)       : 28.3%   (15 / 53)
Coverage % (covered + partial / 44)       : 34.1%   (15 / 44)
```

**Caveats.**
- 9 of the 53 directories are utility/shared infrastructure (formatters, error bodies,
  schemas, OAuth, render helpers), not addressable domain resources; the 44-resource
  denominator is the fairer breadth measure.
- The seed ships a **`time_entries`** handler + repo, but `time_entries` is *not* a
  top-level `lib/api/v3/` directory in this OpenProject checkout (it is a module-based
  resource under `modules/`), so it is not counted in the 53. It is genuine extra
  coverage that the directory census does not credit.
- `activities` coverage is delivered partly via a `journals` handler/repo (journals
  back the activity feed); both are counted toward the single `activities` resource.
- **News** is the vertical being stood up this sprint: a model (`op-models/src/news.rs`)
  and repo (`op-db/src/news.rs`) exist, but the model is not yet re-exported in
  `op-models/src/lib.rs` and there is no `handlers/news.rs` — hence "partial / in
  progress." When its handler lands, covered rises to 10 (≈18.9% of 53, ≈22.7% of 44).

## What This Means

- **Breadth is low, depth is high.** The seed fully covers only ~9 of 44 domain
  resources (~20%), but those nine (work packages, projects, users, memberships,
  statuses, types, priorities, roles, versions) are the load-bearing core of any
  project-management workload — and several carry full model + repo + HAL representer
  stacks, not just stubs.
- **The measured figure validates the informal estimate.** "6–18%" was a guess; the
  real number is **17% fully covered (9/53), 28% if partials count**, or **20.5% / 34%**
  against domain resources only. The lower bound of the old guess was pessimistic; the
  upper bound (~18%) lands almost exactly on the strict covered figure.
- **Representers lag handlers.** Only 4 resources (work_packages, projects, users,
  queries) have dedicated HAL representers, versus 17 handler modules — HAL output
  fidelity is the narrowest surface and a likely source of API-contract drift.
- **The long tail is collaboration + admin surface.** The 28 absent domain resources
  are dominated by high-value collaboration features (wiki_pages, news, notifications,
  groups, shares, views, favorites) and configuration/admin (custom_fields,
  custom_actions, configuration, capabilities, principals). Breadth growth from here
  means leaving the WP/project core and entering these verticals — News first.
- **Next increment is cheap and visible.** News already has model + repo scaffolding;
  finishing its handler and export converts a "partial" to "covered" and is the
  intended headline win of this sprint.
