# Klickwege structure parity: OpenProject work packages

Status: inventory. Nothing here is a design ratification, and nothing was run.
Every row is a reading of source files, cited by `file:line`. Where a fact could
not be read it says UNVERIFIED.

## 1. Scope and method

The click paths come from a harvest of the 21 feature specs under
`spec/features/work_packages/` in `/home/user/adaworldapi/openproject` (149 `it`
examples). Each distinct path is one row below; examples that walk the same path
are grouped and each is cited. The route column is read from
`config/routes.rb` or, where a spec hides its URL inside a page object, from
`spec/support/pages/work_packages/`. The Redmine column names the ERB that
fixes the field ergonomics of that screen. The nexgen column names the route in
`crates/op-server/src/main.rs` and the handler that serves it.

This is a structure map: which screen a click lands on, and whether nexgen has
that screen. It does not check behaviour, data, or permissions beyond what the
cited lines show.

Status values:

- SERVED: the route exists and renders that screen.
- PARTIAL: the route exists, and a named step, field, or control is missing.
- MISSING: no HTML route serves the screen.
- OUT-OF-SCOPE: the path needs a JS-only or Angular-only interaction (inline
  autosave, drag and drop, popup widget). The reason is given in the row.

Citation prefixes (all paths are under the given root):

| Prefix | Root |
|---|---|
| `spec:` | `/home/user/adaworldapi/openproject/spec/features/work_packages/` (`nav` navigation, `idx` work_package_index, `edit` edit_work_package, `chg` edit_on_change_work_package_status_permission, `ver` edit_on_assign_version_permission, `cancel` cancel_editing, `copy` copy, `d/…` details/, `n/…` new/) |
| `po:` | `/home/user/adaworldapi/openproject/spec/support/pages/work_packages/` |
| `OPR:` | `/home/user/adaworldapi/openproject/config/routes.rb` |
| `RM:` | `/home/user/redmine/app/views/issues/` (ERB) |
| `RMR:` | `/home/user/redmine/config/routes.rb` |
| `main:` `board:` `nav:` `layout:` | `/home/user/nexgen-klickwege/crates/op-server/src/{main,board,nav,layout}.rs` |
| `api:` | `/home/user/nexgen-klickwege/crates/op-api/src/routes.rs` (JSON only, nested under `/api/v3` at `api:14`) |

nexgen's HTML routes are exactly these (`main:268-282`): `/`, `/work_packages/:id`,
`/work_packages/:id/edit` (GET and POST), `/projects`, `/projects/:id`,
`/projects/:id/edit` (GET and POST). Everything else below is either absent or
reachable only as JSON through `/api/v3` (`main:296-333`).

## 2. Click-path table


### 2a. Reaching and reading work packages

| # | Click path (user steps) | OpenProject route/screen | Redmine ERB analogue | nexgen route + renderer | Status |
|---|---|---|---|---|---|
| 1 | From the home page click "Work packages" in the main menu; see the list titled "All open", or the empty-state heading `spec:idx:45-57`, `spec:nav:61-64` | `/work_packages`: `OPR:1396` (asserted `spec:idx:50`) | `RM:index.html.erb:20` (title h2), `:29-30` (no-data paragraph) | `/` at `main:268` renders `board::board_page` (`board:413`) through `render_list`, rows grouped by status (`board:452-560`). The menu entry is "Board" (`nav:274`) | PARTIAL. No `/work_packages` route (`main:272` has only `:id`). No "All open" filter: it loads every status, closed included (`board:422`). Page title is "OpenProject RS — Board" (`board:440`). Empty-state heading UNVERIFIED |
| 2 | Open a project, click "Work packages" in the project sidebar; see that project's list `spec:idx:62-81`, `spec:nav:98-107` | `/projects/:project_id/work_packages`: `OPR:709` (asserted `spec:idx:73`) | `RM:index.html.erb:20-35` (same view, project-scoped) | `/projects/:id` at `main:278` (`board:1311`) stacks the project's work-package list under its detail (`board:1337-1344`, `board:1359-1431`), without the project column (`board:200-216`). Sidebar is global only: Board and Projects (`layout:125-151`) | PARTIAL. No `/projects/:id/work_packages` route and no project-scoped sidebar entry |
| 3 | Open a project overview; the content heading is the project name `spec:idx:63-67`, `spec:nav:162` | `project_path` helper. The route for it is UNVERIFIED: `OPR:558` excludes `:show` from `resources :projects` | not read (no project views were in the input list) | `/projects/:id` at `main:278`, `board:1311-1350`; headline is the escaped project name (`board:1498`) through `render_detail` | SERVED. Heading level UNVERIFIED |
| 4 | In a list click a work package's id or subject (also double-click a row, open from search or My page) and land on its full page `spec:nav:75-80`, `:140-144`, `:216-218`, `:245-247`; browser title carries the subject once `spec:nav:286-294` | `/work_packages/:id(/:tab)`: `OPR:1392`; project form `OPR:720` (`po:full_work_package.rb:109-114`) | `RM:show.html.erb:1-180` | `/work_packages/:id` at `main:272`, `board::work_package_detail` (`board:606-695`). List links come from `IdLink`/`PrimaryLink` cells (`board:487`, `board:516-527`; stacked list `board:1379`, `board:1394-1405`). `<title>` is `Work package #id — subject`, subject escaped once (`board:692`, `esc` at `board:568`) | SERVED. Double-click is JS and not modelled; the links are plain anchors |
| 5 | Read the work package's attributes: status, priority, assignee, category, version, dates, estimated and remaining time, % done, description `spec:edit:161-173`, `spec:copy:118-123` | the full page (row 4) | `RM:show.html.erb:73-108` (two columns), `helpers/issues_helper.rb:326-363` (`IssueFieldsRows`) | detail skin `WP_DETAIL_ORDER` (`board:160-172`): status, project, type, priority, author, assignee, start, due, estimated hours, % done, description, one flat table | PARTIAL. Missing: category, version (`targetVersions`), responsible, remaining time, spent time. Dates show as two rows (`board:724-731`), not one combined date |
| 6 | Visit a work package id that does not exist and see an error flash `spec:nav:150-157` | `/work_packages/:id`, `/projects/:identifier/work_packages/:id` with flash `notice_file_not_found` | none read | `/work_packages/:id` returns a page saying "Work package #N not found." (`board:620-623`) as a bare `Html`, so the HTTP status is 200 and there is no flash. `/projects/:ident/work_packages/:id` is not routed | PARTIAL |
| 7 | In a list click a row; a details pane opens beside the list; switch to full screen; go back `spec:nav:68-90`, `:116-137`; `spec:d/refresh:45-52`; `split_view.close` in `spec:n/filter:244-246` | `/work_packages/details/:work_package_id(/:tab)`: `OPR:1318`; project `OPR:723` (`po:split_work_package.rb:71-77`) | none: Redmine has the full page only (`RM:show.html.erb`) | none | MISSING |
| 8 | On the full page switch tab: activity, relations, files; the activity tab reads "Status changed from X to Y", "Assignee set to …" `spec:nav:126-132`, `:142`, `:248-250`; `spec:edit:141-143`, `:175-177`, `:183-185`, `:214-219`; `spec:copy:128-134` | `/projects/:p/work_packages/:id/activity` and `/relations` (`spec:nav:126`, `:131`; `OPR:720`) | `RM:show.html.erb:123-127` (attachments), `:140-145` + `RM:_relations.html.erb:1-27` (relations), `:151-153` + `RM:tabs/_history.html.erb:1-20` (history) | no HTML tab. JSON only: `/api/v3/work_packages/:id/activities` (`api:59`) and `/relations` (`api:48`) | MISSING |
| 9 | Open a saved query from the sidebar or by id; list shows the query title and filtered rows; a malformed query shows an error `spec:nav:104-112`, `:161-170`, `:211-230`, `:257-269`; `spec:n/filter:63-67` | `…/work_packages?query_id=N` (`po:work_packages_table.rb:53-55`); error case `?query_id=…&query_props=…` (`spec:nav:258-266`) | `RM:_sidebar.html.erb:4` (`render_sidebar_queries`), `RM:index.html.erb:20-22` (query title), `:24-26` (query form), `:28` (`@query.valid?`) | none. `board_page` takes no query parameters (`board:413`). JSON `/api/v3/queries` exists (`api:153-164`) | MISSING |
| 10 | Search for a term, double-click a result row, land on the full page `spec:nav:172-181` | `(/projects/:project_id)/search?q=&filter=work_packages&scope=all`: `OPR:483` | `app/views/search/index.html.erb` exists; contents not read | none | MISSING |
| 11 | Open My page, click a work package id in its table `spec:nav:184-196` | `/my/page`: `modules/my_page/config/routes.rb:29-30` | not read | none | MISSING |
| 12 | Click the logo and land on the home page `spec:cancel:65-77` | `/` is `homescreen#index`: `OPR:31` | not read | brand link `/` (`board:1703`) lands on the board, not a home screen. Menu is Board and Projects (`nav:274`), with no "All projects" entry | PARTIAL |
| 13 | From the list open the avatar menu, click Administration, leave the work-package app `spec:nav:274-283` | admin pages outside the work-package SPA | none | none, and nothing to leave: nexgen has no SPA shell | OUT-OF-SCOPE: asserts the Angular body class `router--work-packages-base` is gone (`spec:nav:283`) |

### 2b. Editing a work package

| # | Click path (user steps) | OpenProject route/screen | Redmine ERB analogue | nexgen route + renderer | Status |
|---|---|---|---|---|---|
| 14 | Change subject, type, dates, assignee, estimated time, priority, version, category, status, description and see the new values `spec:edit:147-177` (also `:132-145`) | inline edit on the full page (row 4); no separate edit URL (`OPR:1400` maps `/edit` to `show`) | `RM:_edit.html.erb:1-80` (form), `RM:_form.html.erb:28-47` (subject, description), `RM:_attributes.html.erb:5-92` (status, priority, assignee, category, version / parent, dates, hours, done ratio) | `/work_packages/:id/edit` GET and POST (`main:273-276`): `work_package_edit_form` (`board:842`), `render_wp_edit` through `render_form` (`board:899-1094`), `work_package_update` redirects to the detail page (`board:1101-1122`). Form fields `WP_FORM_ORDER` (`board:177-189`) | PARTIAL. Form lacks responsible, version, category, remaining time and custom fields. No success toast: the handler only redirects (`board:1122`). Whether `update` writes a journal entry is UNVERIFIED (op-db not read) |
| 15 | Set an assignee, then clear it; the field reads "-" `spec:edit:180-205`; placeholder users assignable `spec:edit:207-219` | inline edit (row 14) | `RM:_attributes.html.erb:25-33` (select with blank option) | `assigned_to_id` select is optional, with the blank option emitted by the select template (`board:892-898`, `board:1002-1011`). An empty value is passed through as `None` and clears the assignee (`board:1146-1148`, `board:1160`). Detail shows "—" for none (`board:677`) | SERVED. Placeholder users as options UNVERIFIED: only `UserRepository::find_all` feeds the select (`board:874-877`). The journal line "Assignee set to" belongs to row 8 |
| 16 | Activate one field, type, press Enter to save, see "Successful update", focus-out must not close the editor `spec:edit:264-286`, `spec:nav:219-230` | inline edit widgets on the full page | none: Redmine has no per-field editor | none; the whole-form POST of row 14 is the nearest path | OUT-OF-SCOPE: per-field Angular editor with XHR autosave and a toast |
| 17 | Change the type; custom fields shown change; a required custom field blocks the save `spec:edit:222-262` | inline edit (row 14) | `RM:_form.html.erb:18-26` (tracker select with `updateIssueFrom`), `RM:_attributes.html.erb:96-98` (custom field values) | `type_id` select only (`board:982-991`). No custom fields in the basis (`board:78-90`) | MISSING |
| 18 | As a user with only `change_work_package_status`, only status is editable; with only `assign_versions`, only version; admin without roles still edits `spec:chg:73-85`, `:124-135`; `spec:ver:71-91`; `spec:edit:127-145` | same screen, fields rendered read-only per permission | `RM:_attributes.html.erb:5`, `:21`, `:25`, `:49` (each field gated by `safe_attribute?`) | render-side seam exists: `skin()` ANDs `viewfilter::current_rbac_field_mask()` into the field mask (`board:147-152`), but the provider is `AnonymousRbac`, which grants every position (`viewfilter.rs:128-130`). `parse_wp_update` applies every posted field (`board:1149-1187`) | PARTIAL. A render mask is not a write gate; no role store; no read-only field state |
| 19 | Change status through the status control; an invalid workflow transition is rejected with an error and the old status stays `spec:chg:74-85`, `:110-122`, `:127-133` | inline status control (row 14) | `RM:_attributes.html.erb:5-9` (`@allowed_statuses`), `:18` (read-only fallback) | status `<select>` lists every status (`board:916-922`, `board:972-981`). A failed update returns a "Save failed" page (`board:1123-1129`) | PARTIAL. Options are not workflow-filtered. Whether `update` rejects an invalid transition is UNVERIFIED |
| 20 | Open a work package with a closed status and a closed version; clicking status shows a warning and the control is disabled `spec:d/closed:49-61` | full page status button | `RM:_attributes.html.erb:5`, `:18` (status shown as plain text when no allowed statuses) | none: the status select is always rendered (`board:972-981`) | MISSING |
| 21 | Open the status, "More", or type menu from the keyboard; arrows move; Escape returns focus to the button `spec:chg:88-108`; `spec:d/toolbar:84-108`; `spec:d/milestone:63-70` | Primer menu components on the full page | none | native `<select>` and anchors only | OUT-OF-SCOPE: focus management of the Primer menu widget |
| 22 | On an open edit form press the Cancel button; nothing is saved and you leave `spec:cancel:130-136` | `/work_packages/new`, `/work_packages/create_new`, project variants (`spec:cancel:39-46`) | `RM:_edit.html.erb:79-80` (cancel link to the issue) | edit form has a Cancel link back to the detail page (`board:947`, `board:1084-1085`) | PARTIAL. Only the edit form; the four create paths are not routed |
| 23 | Leave an open form with unsaved text; the browser asks to confirm unless the user turned the warning off `spec:cancel:79-127`, `:139-168`, `:170-188` | all form pages | none | none | OUT-OF-SCOPE: browser `beforeunload` dialog driven by the `warn_on_leaving_unsaved` preference (`spec:cancel:171-173`) |
| 24 | Set or clear start and due date as plain values; see "start - due" or "no start date" `spec:d/date:70-86`, `:213-235`, `:255-272` | combined date field on the full page | `RM:_attributes.html.erb:72-84` (two `date_field`s) | two `<input type=date>` fields (`board:1012-1029`); parse at `board:1161-1164` | PARTIAL. No duration field and no combined display. A malformed date parses to `None` and silently clears the value (`board:1161-1164`) |
| 25 | Use the datepicker: today link, focus switching, duration errors, manual vs automatic scheduling banners, working-days toggle, parent/child banners, "Show relations" opens a hierarchy table with timeline `spec:d/date:89-187`, `:237-252`, `:275-286`, `:336-457`; `spec:d/workdays:58-72`; `spec:n/datepick:53-81` | datepicker modal on the full page | none: Redmine has plain date fields only (row 24) | none | OUT-OF-SCOPE: flatpickr modal, scheduling engine and Gantt timeline need JS |

### 2c. Creating, copying, and acting on a work package

nexgen has no create form at all. The only create path is JSON `POST /api/v3/work_packages` (`api:43`); no HTML route reaches it.

| # | Click path (user steps) | OpenProject route/screen | Redmine ERB analogue | nexgen route + renderer | Status |
|---|---|---|---|---|---|
| 26 | In a project open the create page, pick a type, enter subject (and description), Save; see "Successful creation" and the new page; create a second one `spec:n/new:114-149`, `:299-341`; `spec:n/required:82-106` | `/projects/:project_id/work_packages/new`: `OPR:718` (`po:full_work_package_create.rb:51-59`) | `RM:new.html.erb:1-45`, `RM:_form.html.erb:18-47`, `RM:_attributes.html.erb:5-92`; route `RMR:180-183` (`projects/:project_id/issues/new`) | none | MISSING |
| 27 | From the global create page choose a project in the form, then type and subject; the project list only offers projects that have the chosen type `spec:n/descr:107-117`; `spec:n/new:343-356`, `:381-396`, `:435-448` | `/work_packages/new`: `OPR:1300` (`only: %i[index show new]`); `spec:n/descr:108` | `RM:_form.html.erb:12-16` (project select on the new form) | none | MISSING |
| 28 | In a list press "Create" and pick a type (or inline-create a row); the new form starts with attributes from the active filters; on save the table reloads and selects the new row; milestone type shows a date `spec:n/new:231-296`, `:310-337`; `spec:n/filter:93-178`, `:187-276`; `spec:d/milestone:58-61` | `/projects/:p/work_packages/create_new`: `OPR:727` (`po:split_work_package_create.rb:47-55`); after save `/details/:id` (`spec:n/new:293`) | `RM:index.html.erb:2-4` (new-issue link; type is chosen in the form, `RM:_form.html.erb:18-26`) | none | MISSING |
| 29 | Open the row context menu, choose "Create new child"; the form is prefilled with the parent's dates and shows a "Parent: subject" breadcrumb `spec:n/new:504-528` | `…/work_packages/new?parent_id=` (`po:full_work_package_create.rb:55-56`); split create `?parent_id` (`po:split_work_package_create.rb:51-53`) | `RM:_subtasks.html.erb:2` (`link_to_new_subtask`), `RM:_attributes.html.erb:65-70` (parent field); context menu `app/views/context_menus/issues.html.erb` | none. `WorkPackageRow` carries `parent_id` (`board:1183`) but nothing creates children | MISSING |
| 30 | Submit a create with the subject blank, a per-type required custom field empty, or without add permission; an error shows and nothing is saved `spec:n/new:152-165`, `:451-462`, `:465-488`; `spec:n/required:82-95` | create pages (rows 26-28) | `RM:new.html.erb:7` (`error_messages_for`) | none for create. Update failure shows only a generic "Save failed" page (`board:1123-1129`) | MISSING |
| 31 | While creating, switch the type: entered values are kept, the type's default description template fills in, an automatic-subject type pre-fills the subject `spec:n/new:167-178`; `spec:n/descr:58-100`; `spec:n/autosubj:45-56` | create pages (rows 26-28) | `RM:_form.html.erb:18-21` (tracker select reloads the form via `updateIssueFrom`) | none | MISSING |
| 32 | On the full page choose "Duplicate", edit the copy, Save; the new work package keeps assignee, version, priority and relates to the original `spec:copy:97-135`, `:137-150`, `:153-170` | `/projects/:p/work_packages/:id/copy`: `OPR:717` (`po:full_work_package_create.rb:52-54`); global `OPR:1391` | `RM:_action_menu.html.erb:8-9`, `RM:new.html.erb:8`, `:14-31` (copy options); `RMR:180` (`issues/:copy_from/copy`); `app/views/context_menus/issues.html.erb:169-170` | none | MISSING |
| 33 | From the menu choose "Duplicate in another project"; land on the move page with `copy=true&ids[]=` `spec:d/ctx:43-50` | `/work_packages/move/new`: `OPR:1312` | `app/views/context_menus/issues.html.erb:173-174` (bulk edit with `copy=1`) | none | MISSING |
| 34 | From the menu choose "Copy link to clipboard" (short URL `/wp/:id`) or "Copy work package ID" `spec:d/ctx:52-59`; `spec:d/toolbar:99-106` | `/wp/*rest` redirects to `/work_packages/:id`: `OPR:73-77` | `RM:_action_menu.html.erb:11` (`copy_object_url_link`), `app/views/context_menus/issues.html.erb:168` | none. No `/wp/:id` route | MISSING. The clipboard write is JS; the part nexgen could serve is the short-URL redirect |
| 35 | Click "Watch", then "Unwatch", then "Watch" on the work package toolbar `spec:d/toolbar:48-66` | toolbar button on the split/full view | `RM:_action_menu.html.erb:7` (`watcher_link`), `RM:show.html.erb:164-172` (watchers sidebar) | no HTML control. JSON: `POST`/`DELETE /api/v3/work_packages/:id/watch` (`api:54-55`), `GET …/watching` (`api:53`) | MISSING. Note `spec:d/refresh:45-53` is titled "toggles the watch state" but its body asserts a drag handle only |
| 36 | Open "More", choose "Delete"; a confirm dialog takes focus; Cancel returns focus `spec:d/toolbar:110-121` | toolbar More menu | `RM:_action_menu.html.erb:12-14` (delete link with confirm), `app/views/context_menus/issues.html.erb:176-178` | no HTML control. JSON `DELETE /api/v3/work_packages/:id` (`api:46`) | MISSING |
| 37 | Open "More"; "Log time" shows with permission; "Start timer" shows only on a mobile viewport; none without permission `spec:d/timer:48-91`; `spec:d/toolbar:91` | toolbar More menu | `RM:_action_menu.html.erb:5-6` (log time). No timer analogue in that file | no HTML control. A `time_entries` JSON handler module is imported (`api:11`); its routes were not read | MISSING |
| 38 | On the create page open "Settings", choose "Configure form"; land on the type's form configuration `spec:n/new:530-559` | `/types/:id/form_configuration/edit` (`spec:n/new:557`; route `OPR:201`, enclosing scope not read, so the prefix is UNVERIFIED) | none read | none | MISSING |
| 39 | Activate the assignee or responsible field; the options show other users' emails only with the permission `spec:edit:288-346` | ng-select autocompleter inside inline edit | none | plain `<select>` labelled with `full_name()` only (`board:937-943`), so emails never appear | OUT-OF-SCOPE: autocompleter widget with permission-dependent labels |
| 40 | In the list see a drag-and-drop handle on each row `spec:d/refresh:50-52` | work-package table row | none | none | OUT-OF-SCOPE: drag and drop |

## 3. Field-view ergonomics

What the Redmine screens fix, read from the ERB:

- The show page splits the attributes in two columns inside one `splitcontent`
  (`RM:show.html.erb:73-108`, `helpers/issues_helper.rb:326-353`). Left: status,
  priority, assignee, category, version. Right: start date, due date, % done,
  estimated time, spent time. Each cell is label plus value in a `div.attribute`
  (`helpers/issues_helper.rb:355-361`).
- A field is hidden when the tracker disables it
  (`@issue.disabled_core_fields.include?(...)`, `RM:show.html.erb:77-102`), and
  category and version are also hidden when there is nothing to choose
  (`RM:show.html.erb:83-88`). The edit form gates each input with
  `safe_attribute?` in the same two-column layout (`RM:_attributes.html.erb:3-93`).
- Custom fields continue the same split: the first half goes left, the rest
  right (`helpers/issues_helper.rb:371-382`, `render_half_width_custom_fields_rows`);
  full-width fields follow the description (`RM:show.html.erb:129`).

How OGAR addresses the same thing. `render_field_view` renders a
`WideFieldMask`-projected `ClassView` as an addressed screen in which every
field carries `data-field-pos`, its mask bit index and so its layout address
(`/home/user/OGAR/crates/ogar-render-askama/src/field_view.rs:12-18`,
`:109-124`). Read against Redmine:

| Redmine | OGAR addressed fieldview |
|---|---|
| `disabled_core_fields` hides a field | the mask: a position that is not set is not rendered. In nexgen the mask is already minted as rbac ∩ present ∩ view (`board:147-152`) |
| left or right column | a layout concern of the renderer, not of the field: the field keeps one position whichever column it lands in |
| "hide category/version when empty" | the presence mask is the seam for it (`present`, `board:142-146`), which today is the full basis, so nothing is hidden for emptiness yet |
| per-field `safe_attribute?` on the form | the same mask, evaluated for write; today only the render side exists (row 18) |

Status of the move in nexgen: PR `claude/wp-fieldview` is moving the detail page
onto `render_field_view`. It had not landed when this was written: this branch
has no `render_field_view` or `data-field-pos` in `crates/` (searched; a search
only locates, so this is a statement about this tree and not about the PR). The
detail page still goes through `render_detail` with a flat column list
(`board:702-833`). The attribute gaps in row 5 (category, version, responsible,
remaining time, spent time) are positions missing from the basis
(`WP_EXTRA_FIELDS`, `board:78-90`), which is a basis change and not a layout
change.

## 4. Counts and next work

Rows: 40. By status:

| Status | Rows | Count |
|---|---|---|
| SERVED | 3, 4, 15 | 3 |
| PARTIAL | 1, 2, 5, 6, 12, 14, 18, 19, 22, 24 | 10 |
| MISSING | 7, 8, 9, 10, 11, 17, 20, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38 | 20 |
| OUT-OF-SCOPE | 13, 16, 21, 23, 25, 39, 40 | 7 |

The 20 MISSING rows share few causes: no create form (rows 26-31, 32, 33), no
tabs, search or saved queries (8-11), and thin JSON-only actions with no HTML
control (35-37).

Next five worth building, ranked. Each lists the smallest route or handler it
needs. Any new screen must also join `SCREEN_UNIVERSE` (`nav:55`) and the
menu-rooted edges (`nav:170-191`), otherwise the boot klickweg check warns
(`main:59-93`).

1. Create form (rows 26, 27, 30; unlocks 28, 29, 31, 32). `GET`/`POST
   /work_packages/new` with an optional `project_id`, reusing the form skin and
   field list of `render_wp_edit` (`board:899-1094`) on an empty row, then
   redirecting to `/work_packages/:id`. A repository create method is
   UNVERIFIED (not read); the JSON handler at `api:43` suggests one exists.
2. List aliases (rows 1, 2, 6). `GET /work_packages` and `GET
   /projects/:id/work_packages` as two route arms over the existing list
   rendering (`board:452`, `board:1359`), with a default filter on non-closed
   statuses and the title "All open". In the same change return
   `StatusCode::NOT_FOUND` for the not-found page (`board:620-623`).
3. Copy and short URL (rows 32, 34). `GET /work_packages/:id/copy` is the
   create form (item 1) prefilled from the source row; `GET /wp/:id` is a
   one-line redirect to `/work_packages/:id`.
4. Activity and relations tabs (row 8). `GET /work_packages/:id/activity` and
   `/relations`, read-only, over the same repositories the JSON handlers use
   (`api:48`, `api:59`) and `render_list`.
5. Watch and delete as plain forms (rows 35, 36). `POST
   /work_packages/:id/watch` and `POST /work_packages/:id/delete`, thin
   wrappers over what `api:46` and `api:54-55` already call, with a
   confirmation page for delete.

## 5. UNVERIFIED

- Row 1: whether `render_list` emits an empty-state heading.
- Row 3: the OpenProject route behind `project_path` (`OPR:558` excludes `:show`);
  the heading level `render_detail` emits.
- Rows 14, 19: whether `WorkPackageRepository::update` writes a journal entry or
  rejects an invalid workflow transition (op-db not read).
- Row 15: whether placeholder users are offered as assignees.
- Row 37: the routes behind the imported `time_entries` JSON handlers.
- Row 38: the scope enclosing `OPR:201`, so the URL prefix of the form
  configuration page.
- Redmine analogues for the project overview (row 3), search (row 10), My page
  (row 11) and the admin form configuration (row 38) were not read.
- Page-object visits that the harvest did not expand (`visit!`, `visit_index`)
  were resolved only where `po:` files were read; `visit_index` of
  `WorkPackagesPage` (`spec:d/toolbar:43-46`) was not.
